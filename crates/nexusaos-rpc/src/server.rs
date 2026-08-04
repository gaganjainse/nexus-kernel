use std::{path::PathBuf, sync::Arc};

use tokio::net::UnixListener;

use crate::handler::RpcHandler;

pub struct RpcServer {
    handler: Arc<RpcHandler>,
    socket_path: PathBuf,
}

impl RpcServer {
    pub fn new(handler: Arc<RpcHandler>, socket_path: PathBuf) -> Self {
        Self { handler, socket_path }
    }

    pub async fn run(&self) -> Result<(), std::io::Error> {
        tokio::fs::remove_file(&self.socket_path).await.ok();
        let listener = UnixListener::bind(&self.socket_path)?;

        loop {
            let (stream, _addr) = listener.accept().await?;
            let handler = self.handler.clone();
            // Spawn a task to handle each connection
            tokio::spawn(async move {
                if let Err(e) = handler.handle_connection(stream).await {
                    eprintln!("[RPC] Connection error: {}", e);
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use nexusaos_waveobj::store::WaveStore;
    use nexusaos_wps::broker::Broker;

    use super::*;

    #[tokio::test]
    async fn test_server_new() -> Result<(), Box<dyn std::error::Error>> {
        let broker = Broker::new(10);
        let store = Arc::new(WaveStore::open_in_memory()?);
        let handler = Arc::new(RpcHandler::new(broker, store));
        let server = RpcServer::new(handler, PathBuf::from("/tmp/test_server.sock"));
        let socket_str = server.socket_path.to_str().ok_or("socket path should be valid UTF-8")?;
        assert_eq!(socket_str, "/tmp/test_server.sock");
    Ok(())
    }

    #[tokio::test]
    async fn test_server_new_with_relative_path() -> Result<(), Box<dyn std::error::Error>> {
        let broker = Broker::new(10);
        let store = Arc::new(WaveStore::open_in_memory()?);
        let handler = Arc::new(RpcHandler::new(broker, store));
        let server = RpcServer::new(handler, PathBuf::from("relative/path.sock"));
        assert_eq!(server.socket_path, PathBuf::from("relative/path.sock"));
    Ok(())
    }

    #[tokio::test]
    async fn test_server_new_with_empty_path() -> Result<(), Box<dyn std::error::Error>> {
        let broker = Broker::new(10);
        let store = Arc::new(WaveStore::open_in_memory()?);
        let handler = Arc::new(RpcHandler::new(broker, store));
        let server = RpcServer::new(handler, PathBuf::from(""));
        assert_eq!(server.socket_path, PathBuf::from(""));
    Ok(())
    }

    #[tokio::test]
    async fn test_server_socket_path_stored_correctly() -> Result<(), Box<dyn std::error::Error>> {
        let broker = Broker::new(10);
        let store = Arc::new(WaveStore::open_in_memory()?);
        let handler = Arc::new(RpcHandler::new(broker, store));
        let path = PathBuf::from("/var/run/nexusaos/rpc.sock");
        let server = RpcServer::new(handler, path.clone());
        assert_eq!(server.socket_path, path);
    Ok(())
    }

    #[tokio::test]
    async fn test_server_run_creates_listener() -> Result<(), Box<dyn std::error::Error>> {
        use tempfile::TempDir;
        let temp_dir = TempDir::new()?;
        let socket_path = temp_dir.path().join("test.sock");

        let broker = Broker::new(10);
        let store = Arc::new(WaveStore::open_in_memory()?);
        let handler = Arc::new(RpcHandler::new(broker, store));
        let server = RpcServer::new(handler, socket_path.clone());

        let server_handle = tokio::spawn(async move {
            let result = server.run().await;
            result
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        server_handle.abort();
        let _ = tokio::fs::remove_file(&socket_path).await;
    Ok(())
    }
}
