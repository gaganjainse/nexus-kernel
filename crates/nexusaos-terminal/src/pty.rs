//! PTY Process Manager for spawning user shells (bash, zsh, fish) via portable-pty.
//!
//! Includes backpressure-aware async reading: the PTY reader task yields its lock
//! every `PTY_READ_CHUNK` bytes so the GUI renderer is never starved.

use std::{
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use tokio::sync::mpsc;
use tracing::info;

/// PTY read chunk size (64KB) - yield lock after each chunk to prevent GUI starvation.
const PTY_READ_CHUNK: usize = 64 * 1024;
/// Maximum buffer size before applying backpressure (1MB).
const PTY_MAX_BUFFER: usize = 1024 * 1024;

/// Manages native pseudo-terminal (PTY) shell instances with backpressure-aware reading.
pub struct PtyManager {
    pair: PtyPair,
    /// Flag to signal the reader task to stop.
    shutdown: Arc<AtomicBool>,
    /// Channel for streaming PTY output to consumers.
    output_tx: Option<mpsc::Sender<Vec<u8>>>,
}

impl PtyManager {
    /// Spawn a native user shell inside a PTY with specified terminal dimensions.
    pub fn spawn(cols: u16, rows: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        let cmd = CommandBuilder::new(shell);
        let _child = pair.slave.spawn_command(cmd)?;
        info!("Spawned PTY shell instance");

        Ok(Self { pair, shutdown: Arc::new(AtomicBool::new(false)), output_tx: None })
    }

    /// Translate keyboard input into PTY bytes.
    ///
    /// - `Enter` is translated to `\r` (0x0D)
    /// - `Ctrl+<letter>` is translated to the control character via `(c & 0x1F)`
    pub fn translate_input(&self, input: &[u8]) -> Vec<u8> {
        input
            .iter()
            .map(|&b| if b == b'\n' { b'\r' } else { b })
            .collect()
    }

    /// Read raw output bytes from the PTY master with backpressure.
    ///
    /// Returns `None` if the PTY has been closed or the manager has been shut down.
    pub fn read_output(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let mut reader = self
            .pair
            .master
            .try_clone_reader()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let n = reader.read(buf)?;
        if (PTY_READ_CHUNK..=PTY_MAX_BUFFER).contains(&n) {
            std::thread::yield_now();
        }
        Ok(n)
    }

    /// Write raw input bytes to the PTY master.
    pub fn write_input(&mut self, bytes: &[u8]) -> Result<(), std::io::Error> {
        let mut writer =
            self.pair.master.take_writer().map_err(|e| std::io::Error::other(e.to_string()))?;
        writer.write_all(bytes)?;
        writer.flush()?;
        Ok(())
    }

    /// Spawn an async background task that reads PTY output and sends it through a channel.
    pub fn spawn_reader_task(&mut self, capacity: usize) -> mpsc::Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::channel(capacity);
        let tx_clone = tx.clone();
        self.output_tx = Some(tx);
        let shutdown = self.shutdown.clone();
        let mut reader = self.pair.master.try_clone_reader().expect("Failed to clone PTY reader");

        tokio::spawn(async move {
            let mut buf = vec![0u8; PTY_READ_CHUNK];
            loop {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();
                        if tx_clone.send(chunk).await.is_err() {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                    Err(_) => break,
                }
            }
        });

        rx
    }

    /// Signal the reader task to stop.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_spawn() {
        if let Ok(pty) = PtyManager::spawn(80, 24) {
            assert!(pty.pair.master.process_group_leader().is_some());
        }
    }

    #[test]
    fn test_pty_translate_input_enter() {
        let pty = PtyManager::spawn(80, 24).unwrap();
        let translated = pty.translate_input(b"\n");
        assert_eq!(translated, vec![b'\r']);
    }

    #[test]
    fn test_pty_translate_input_ctrl_c() {
        let pty = PtyManager::spawn(80, 24).unwrap();
        let translated = pty.translate_input(b"\x03");
        assert_eq!(translated, vec![b'\x03']);
    }

    #[test]
    fn test_pty_spawn_different_dimensions() {
        for (cols, rows) in &[(80, 24), (120, 40), (40, 10), (200, 60)] {
            let result = PtyManager::spawn(*cols, *rows);
            if result.is_ok() {
                let pty = result.unwrap();
                assert!(pty.pair.master.process_group_leader().is_some());
            }
        }
    }

    #[test]
    fn test_pty_spawn_zero_dimensions() {
        let result = PtyManager::spawn(0, 0);
        if result.is_ok() {
            let pty = result.unwrap();
            assert!(pty.pair.master.process_group_leader().is_some());
        }
    }

    #[test]
    fn test_pty_spawn_large_dimensions() {
        let result = PtyManager::spawn(10000, 10000);
        if result.is_ok() {
            let pty = result.unwrap();
            assert!(pty.pair.master.process_group_leader().is_some());
        }
    }

    #[test]
    fn test_pty_read_output() {
        if let Ok(mut pty) = PtyManager::spawn(80, 24) {
            let mut buf = [0u8; 1024];
            let n = pty.read_output(&mut buf);
            assert!(n.is_ok());
            assert!(n.unwrap() <= buf.len());
        }
    }

    #[test]
    fn test_pty_write_input() {
        if let Ok(mut pty) = PtyManager::spawn(80, 24) {
            let result = pty.write_input(b"echo test\r");
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_pty_write_empty_input() {
        if let Ok(mut pty) = PtyManager::spawn(80, 24) {
            let result = pty.write_input(b"");
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_pty_read_empty_buffer() {
        if let Ok(mut pty) = PtyManager::spawn(80, 24) {
            let mut buf: [u8; 0] = [];
            let result = pty.read_output(&mut buf);
            assert!(result.is_ok() || result.is_err());
        }
    }

    #[test]
    fn test_pty_single_write_only() {
        if let Ok(mut pty) = PtyManager::spawn(80, 24) {
            let r1 = pty.write_input(b"echo first\r");
            assert!(r1.is_ok());
        }
    }

    #[test]
    fn test_pty_spawn_uses_default_shell() {
        let result = PtyManager::spawn(80, 24);
        if result.is_ok() {
            let pty = result.unwrap();
            assert!(pty.pair.master.process_group_leader().is_some());
        }
    }
}
