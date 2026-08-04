use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use tokio::sync::mpsc;

use crate::{
    controller::{BlockInput, Controller, ControllerError, ControllerStatus},
    filestore::BlockFileStore,
};

const STATUS_IDLE: u8 = 0;
const STATUS_RUNNING: u8 = 1;
const STATUS_STOPPING: u8 = 2;
const STATUS_STOPPED: u8 = 3;

fn status_to_string(status: u8) -> String {
    match status {
        STATUS_RUNNING => "running".to_string(),
        STATUS_STOPPING => "stopping".to_string(),
        STATUS_STOPPED => "done".to_string(),
        _ => "init".to_string(),
    }
}

pub struct ShellController {
    block_id: String,
    conn_name: String,
    status: Arc<AtomicU8>,
    exit_code: Arc<AtomicU8>,
    input_tx: Arc<Mutex<Option<mpsc::Sender<BlockInput>>>>,
    file_store: Arc<BlockFileStore>,
    shell_path: String,
}

impl ShellController {
    pub fn new(
        block_id: String,
        file_store: Arc<BlockFileStore>,
        shell_path: Option<String>,
    ) -> Self {
        let shell = shell_path.unwrap_or_else(detect_shell);
        Self {
            block_id,
            conn_name: "local".to_string(),
            status: Arc::new(AtomicU8::new(STATUS_IDLE)),
            exit_code: Arc::new(AtomicU8::new(255)),
            input_tx: Arc::new(Mutex::new(None)),
            file_store,
            shell_path: shell,
        }
    }
}

#[async_trait::async_trait]
impl Controller for ShellController {
    async fn start(&self) -> Result<(), ControllerError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| ControllerError::Shell(e.to_string()))?;

        let cmd = CommandBuilder::new(&self.shell_path);
        let mut child =
            pair.slave.spawn_command(cmd).map_err(|e| ControllerError::Shell(e.to_string()))?;

        self.status.store(STATUS_RUNNING, Ordering::SeqCst);

        let (tx, mut rx) = mpsc::channel::<BlockInput>(256);
        *self.input_tx.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);

        let mut reader =
            pair.master.try_clone_reader().map_err(|e| ControllerError::Shell(e.to_string()))?;
        let block_id = self.block_id.clone();
        let file_store = self.file_store.clone();

        // Reader task
        tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        file_store.append(&block_id, &buf[..n]);
                    }
                    Err(_) => break, // Error
                }
            }
        });

        // Writer task
        let mut writer =
            pair.master.take_writer().map_err(|e| ControllerError::Shell(e.to_string()))?;
        let master = pair.master;

        tokio::task::spawn_blocking(move || {
            while let Some(input) = rx.blocking_recv() {
                match input {
                    BlockInput::Data(bytes) => {
                        let _ = writer.write_all(&bytes);
                        let _ = writer.flush();
                    }
                    BlockInput::Resize { rows, cols } => {
                        let _ =
                            master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
                    }
                    BlockInput::Signal(_) => {}
                }
            }
        });

        // Waiter task
        let status = self.status.clone();
        let exit_code = self.exit_code.clone();

        tokio::task::spawn_blocking(move || {
            if let Ok(exit_status) = child.wait() {
                let code = exit_status.exit_code();
                exit_code.store(((code as i32) & 0xFF) as u8, Ordering::SeqCst);
            }
            status.store(STATUS_STOPPED, Ordering::SeqCst);
        });

        Ok(())
    }

    async fn stop(&self, graceful: bool) -> Result<(), ControllerError> {
        let mut tx_guard = self.input_tx.lock().unwrap_or_else(|e| e.into_inner());
        *tx_guard = None; // Drop the sender to close the channel

        if !graceful {
            // Best effort without OS specific kill
        }

        let _ = self
            .status
            .compare_exchange(STATUS_RUNNING, STATUS_STOPPING, Ordering::SeqCst, Ordering::Relaxed);
        self.status.store(STATUS_STOPPED, Ordering::SeqCst);
        Ok(())
    }

    fn runtime_status(&self) -> ControllerStatus {
        let exit_code = self.exit_code.load(Ordering::SeqCst);
        ControllerStatus {
            block_id: self.block_id.clone(),
            status: status_to_string(self.status.load(Ordering::SeqCst)),
            conn_name: self.conn_name.clone(),
            exit_code: if exit_code == 255 { None } else { Some(exit_code as i32) },
        }
    }

    fn conn_name(&self) -> &str {
        &self.conn_name
    }

    async fn send_input(&self, input: BlockInput) -> Result<(), ControllerError> {
        let tx = {
            let guard = self.input_tx.lock().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };

        if let Some(tx) = tx {
            tx.send(input).await.map_err(|_| ControllerError::NotRunning(self.block_id.clone()))?;
            Ok(())
        } else {
            Err(ControllerError::NotRunning(self.block_id.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_shell() {
        let shell = detect_shell();
        assert!(!shell.is_empty());
    }

    #[test]
    fn test_controller_init() {
        let store = Arc::new(BlockFileStore::new());
        let controller = ShellController::new("blk1".to_string(), store, None);
        let status = controller.runtime_status();
        assert_eq!(status.status, "init");
        assert_eq!(status.conn_name, "local");
        assert_eq!(status.block_id, "blk1");
        assert_eq!(status.exit_code, None);
    }

    #[test]
    fn test_controller_init_with_custom_shell() {
        let store = Arc::new(BlockFileStore::new());
        let controller =
            ShellController::new("blk1".to_string(), store, Some("/bin/sh".to_string()));
        assert_eq!(controller.conn_name(), "local");
        let status = controller.runtime_status();
        assert_eq!(status.status, "init");
    }

    #[tokio::test]
    async fn test_send_input_before_start() {
        let store = Arc::new(BlockFileStore::new());
        let controller = ShellController::new("blk1".to_string(), store, None);
        let result = controller.send_input(BlockInput::Data(b"hello".to_vec())).await;
        assert!(result.is_err());
        match result {
            Err(ControllerError::NotRunning(id)) => assert_eq!(id, "blk1"),
            _ => panic!("expected NotRunning error"),
        }
    }

    #[tokio::test]
    async fn test_stop_sets_status_done() {
        let store = Arc::new(BlockFileStore::new());
        let controller = ShellController::new("blk1".to_string(), store, None);
        controller.stop(false).await.unwrap();
        let status = controller.runtime_status();
        assert_eq!(status.status, "done");
    }

    #[tokio::test]
    async fn test_stop_graceful_vs_ungraceful() {
        let store = Arc::new(BlockFileStore::new());
        let controller = ShellController::new("blk1".to_string(), store, None);
        controller.stop(true).await.unwrap();
        let status = controller.runtime_status();
        assert_eq!(status.status, "done");

        let store2 = Arc::new(BlockFileStore::new());
        let controller2 = ShellController::new("blk2".to_string(), store2, None);
        controller2.stop(false).await.unwrap();
        let status2 = controller2.runtime_status();
        assert_eq!(status2.status, "done");
    }

    #[tokio::test]
    #[ignore]
    async fn test_integration_shell() {
        let store = Arc::new(BlockFileStore::new());
        let controller = ShellController::new("blk1".to_string(), store.clone(), None);
        controller.start().await.unwrap();

        controller.send_input(BlockInput::Data(b"echo hello\n".to_vec())).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        let output = store.read_all("blk1").unwrap_or_default();
        let output_str = String::from_utf8_lossy(&output);
        assert!(output_str.contains("hello"));

        controller.send_input(BlockInput::Data(b"exit\n".to_vec())).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        let status = controller.runtime_status();
        assert_eq!(status.status, "done");
    }

    #[tokio::test]
    async fn test_start_invalid_shell_path() {
        let store = Arc::new(BlockFileStore::new());
        let controller =
            ShellController::new("blk1".to_string(), store, Some("/nonexistent/shell".to_string()));
        let result = controller.start().await;
        assert!(result.is_err());
        match result {
            Err(ControllerError::Shell(_)) => {}
            _ => panic!("expected Shell error"),
        }
    }

    #[tokio::test]
    async fn test_send_input_after_stop() {
        let store = Arc::new(BlockFileStore::new());
        let controller = ShellController::new("blk1".to_string(), store, None);
        controller.stop(true).await.unwrap();
        let result = controller.send_input(BlockInput::Data(b"hello".to_vec())).await;
        assert!(result.is_err());
        match result {
            Err(ControllerError::NotRunning(_)) => {}
            _ => panic!("expected NotRunning error"),
        }
    }
}
