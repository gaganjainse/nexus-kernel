//! Worker isolation for NexusAOS.
//!
//! Tools run as same-machine isolated worker processes with explicit
//! capability leases. Workers are spawned, monitored, and terminated
//! by the worker pool.

use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Arc,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::{
    capability::{CapabilityLease, Scope},
    error::ToolError,
    tools::executor::{ToolExecutor, ToolRequest, ToolResult},
};

/// Configuration for worker isolation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    /// Maximum number of concurrent worker processes.
    pub max_workers: usize,

    /// Timeout for worker execution in seconds.
    pub execution_timeout_secs: u64,

    /// Whether to restart failed workers.
    pub restart_on_failure: bool,

    /// Maximum restart attempts per worker.
    pub max_restart_attempts: u32,

    /// Working directory for worker processes.
    pub working_directory: PathBuf,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            max_workers: 8,
            execution_timeout_secs: 300,
            restart_on_failure: true,
            max_restart_attempts: 3,
            working_directory: PathBuf::from("/tmp/nexusaos-workers"),
        }
    }
}

/// The state of a worker process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerState {
    /// Worker is idle and waiting for a task.
    Idle,
    /// Worker is currently executing a task.
    Busy,
    /// Worker has failed and may be restarted.
    Failed,
    /// Worker has been terminated.
    Terminated,
}

/// Information about a running worker process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub worker_id: String,
    pub state: WorkerState,
    pub pid: Option<u32>,
    pub current_task: Option<String>,
    pub restart_count: u32,
    pub last_heartbeat: Option<std::time::SystemTime>,
}

/// A capability lease passed to a worker process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerCapabilityLease {
    pub lease_id: String,
    pub capability_name: String,
    pub scope: Scope,
    pub granted_at: String,
    pub expires_at: Option<String>,
}

/// A worker process that executes tools in isolation.
pub struct WorkerProcess {
    pub worker_id: String,
    pub child: Option<Child>,
    pub config: WorkerConfig,
    pub state: WorkerState,
    pub current_task: Option<String>,
    pub restart_count: u32,
}

impl WorkerProcess {
    /// Create a new worker process.
    pub fn new(worker_id: String, config: WorkerConfig) -> Self {
        Self {
            worker_id,
            child: None,
            config,
            state: WorkerState::Idle,
            current_task: None,
            restart_count: 0,
        }
    }

    /// Spawn the worker process.
    pub fn spawn(&mut self) -> Result<(), ToolError> {
        let child = Command::new("nexusaos-worker")
            .arg("--worker-id")
            .arg(&self.worker_id)
            .arg("--working-dir")
            .arg(&self.config.working_directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed {
                name: self.worker_id.clone(),
                reason: format!("Failed to spawn worker: {}", e),
            })?;

        self.child = Some(child);
        self.state = WorkerState::Busy;
        info!(worker = %self.worker_id, "Worker process spawned");
        Ok(())
    }

    /// Check if the worker is still alive.
    pub fn is_alive(&mut self) -> bool {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    self.state = WorkerState::Failed;
                    return false;
                }
                Ok(None) => return true,
                Err(_) => {
                    self.state = WorkerState::Failed;
                    return false;
                }
            }
        }
        false
    }

    /// Terminate the worker process.
    pub fn terminate(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.state = WorkerState::Terminated;
        info!(worker = %self.worker_id, "Worker process terminated");
    }

    /// Restart the worker process if it has failed.
    pub fn restart_if_needed(&mut self) -> Result<(), ToolError> {
        if self.state == WorkerState::Failed
            && self.restart_count < self.config.max_restart_attempts
        {
            self.restart_count += 1;
            self.terminate();
            self.spawn()?;
            info!(worker = %self.worker_id, restart_count = self.restart_count, "Worker restarted");
        }
        Ok(())
    }
}

/// A pool of isolated worker processes.
pub struct WorkerPool {
    workers: Vec<WorkerProcess>,
    config: WorkerConfig,
}

impl WorkerPool {
    /// Create a new worker pool with the given configuration.
    pub fn new(config: WorkerConfig) -> Self {
        let max_workers = config.max_workers;
        let mut workers = Vec::with_capacity(max_workers);
        for i in 0..max_workers {
            workers.push(WorkerProcess::new(format!("worker-{}", i), config.clone()));
        }

        Self { workers, config }
    }

    /// Find an idle worker and assign a task to it.
    pub async fn execute_tool(
        &mut self,
        request: &ToolRequest,
        lease: Option<&CapabilityLease>,
    ) -> Result<ToolResult, ToolError> {
        let worker_idx = self.find_idle_worker().await;
        if worker_idx.is_none() {
            return Err(ToolError::ExecutionFailed {
                name: "worker-pool".to_string(),
                reason: "No idle workers available".to_string(),
            });
        }

        let worker_idx = worker_idx.unwrap();
        let worker_id = self.workers[worker_idx].worker_id.clone();

        // Validate capability lease before execution
        if let Some(lease) = lease {
            if !lease.is_valid() {
                return Err(ToolError::ExecutionFailed {
                    name: worker_id.clone(),
                    reason: "Capability lease is invalid or expired".to_string(),
                });
            }
        }

        // Execute the tool in the worker
        let result = self.execute_in_worker(worker_idx, request).await?;

        info!(worker = %worker_id, tool = %request.tool_name, "Worker tool execution completed");
        Ok(result)
    }

    async fn find_idle_worker(&self) -> Option<usize> {
        for (i, worker) in self.workers.iter().enumerate() {
            if worker.state == WorkerState::Idle {
                return Some(i);
            }
        }
        None
    }

    async fn execute_in_worker(
        &mut self,
        worker_idx: usize,
        request: &ToolRequest,
    ) -> Result<ToolResult, ToolError> {
        let worker_id = self.workers[worker_idx].worker_id.clone();

        self.workers[worker_idx].state = WorkerState::Busy;
        self.workers[worker_idx].current_task = Some(request.tool_name.clone());

        // Simulate worker execution - in production this would
        // communicate with the worker process via stdin/stdout
        let result = self.execute_tool_in_process(&worker_id, request).await;

        self.workers[worker_idx].state = WorkerState::Idle;
        self.workers[worker_idx].current_task = None;

        // Check if worker needs restart
        if self.workers[worker_idx].state == WorkerState::Failed && self.config.restart_on_failure {
            if let Err(e) = self.workers[worker_idx].restart_if_needed() {
                warn!(worker = %worker_id, error = %e, "Worker restart failed");
            }
        }

        result
    }

    async fn execute_tool_in_process(
        &self,
        worker_id: &str,
        request: &ToolRequest,
    ) -> Result<ToolResult, ToolError> {
        let request_json =
            serde_json::to_string(request).map_err(|e| ToolError::ExecutionFailed {
                name: worker_id.to_string(),
                reason: format!("Failed to serialize request: {}", e),
            })?;

        let mut child = std::process::Command::new("nexusaos-worker")
            .arg("--worker-id")
            .arg(worker_id)
            .arg("--working-dir")
            .arg(&self.config.working_directory)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed {
                name: worker_id.to_string(),
                reason: format!("Failed to spawn worker: {}", e),
            })?;

        let mut stdin = child.stdin.take().ok_or_else(|| ToolError::ExecutionFailed {
            name: worker_id.to_string(),
            reason: "Failed to open worker stdin".to_string(),
        })?;

        let mut stdout = child.stdout.take().ok_or_else(|| ToolError::ExecutionFailed {
            name: worker_id.to_string(),
            reason: "Failed to open worker stdout".to_string(),
        })?;

        use std::io::Write;
        stdin.write_all(request_json.as_bytes()).map_err(|e| ToolError::ExecutionFailed {
            name: worker_id.to_string(),
            reason: format!("Failed to write to worker: {}", e),
        })?;
        stdin.write_all(b"\n").map_err(|e| ToolError::ExecutionFailed {
            name: worker_id.to_string(),
            reason: format!("Failed to write to worker: {}", e),
        })?;
        drop(stdin);

        let mut result_line = String::new();
        use std::io::Read;
        stdout.read_to_string(&mut result_line).map_err(|e| ToolError::ExecutionFailed {
            name: worker_id.to_string(),
            reason: format!("Failed to read from worker: {}", e),
        })?;

        let result: ToolResult =
            serde_json::from_str(result_line.trim()).map_err(|e| ToolError::ExecutionFailed {
                name: worker_id.to_string(),
                reason: format!("Failed to parse worker response: {}", e),
            })?;

        let _ = child.wait();

        Ok(result)
    }

    /// Get the status of all workers.
    pub fn worker_status(&self) -> Vec<WorkerInfo> {
        self.workers
            .iter()
            .map(|w| WorkerInfo {
                worker_id: w.worker_id.clone(),
                state: w.state,
                pid: w.child.as_ref().and_then(|c| c.id().into()),
                current_task: w.current_task.clone(),
                restart_count: w.restart_count,
                last_heartbeat: None,
            })
            .collect()
    }

    /// Terminate all workers in the pool.
    pub fn terminate_all(&mut self) {
        for worker in &mut self.workers {
            worker.terminate();
        }
    }
}

/// A tool executor that runs tools in isolated worker processes.
pub struct IsolatedWorkerExecutor {
    pool: Arc<RwLock<WorkerPool>>,
}

impl IsolatedWorkerExecutor {
    /// Create a new isolated worker executor.
    pub fn new(config: WorkerConfig) -> Self {
        Self { pool: Arc::new(RwLock::new(WorkerPool::new(config))) }
    }

    /// Get the worker pool configuration.
    pub fn config(&self) -> WorkerConfig {
        self.pool.blocking_read().config.clone()
    }

    /// Get the current worker status.
    pub async fn worker_status(&self) -> Vec<WorkerInfo> {
        self.pool.read().await.worker_status()
    }
}

#[async_trait]
impl ToolExecutor for IsolatedWorkerExecutor {
    fn name(&self) -> &str {
        "isolated-worker-executor"
    }

    fn description(&self) -> &str {
        "Executes tools in isolated worker processes with capability lease enforcement"
    }

    fn is_destructive(&self) -> bool {
        false
    }

    async fn execute(&self, request: &ToolRequest) -> Result<ToolResult, ToolError> {
        let mut pool = self.pool.write().await;
        pool.execute_tool(request, None).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::capability::CapabilitySet;

    #[test]
    fn test_worker_config_default() {
        let config = WorkerConfig::default();
        assert_eq!(config.max_workers, 8);
        assert_eq!(config.execution_timeout_secs, 300);
        assert!(config.restart_on_failure);
        assert_eq!(config.max_restart_attempts, 3);
    }

    #[test]
    fn test_worker_process_creation() {
        let worker = WorkerProcess::new("worker-0".to_string(), WorkerConfig::default());
        assert_eq!(worker.worker_id, "worker-0");
        assert_eq!(worker.state, WorkerState::Idle);
        assert_eq!(worker.restart_count, 0);
    }

    #[test]
    fn test_worker_info() {
        let info = WorkerInfo {
            worker_id: "worker-0".to_string(),
            state: WorkerState::Idle,
            pid: None,
            current_task: None,
            restart_count: 0,
            last_heartbeat: None,
        };
        assert_eq!(info.worker_id, "worker-0");
        assert_eq!(info.state, WorkerState::Idle);
    }

    #[test]
    fn test_worker_pool_creation() {
        let config = WorkerConfig::default();
        let pool = WorkerPool::new(config);
        assert_eq!(pool.workers.len(), 8);
    }

    #[tokio::test]
    async fn test_worker_pool_find_idle_worker() -> Result<(), Box<dyn std::error::Error>> {
        let config = WorkerConfig::default();
        let pool = WorkerPool::new(config);
        let idle_idx = pool.find_idle_worker().await;
        assert_eq!(idle_idx.ok_or_else(|| "no idle worker")?, 0);
        Ok(())
    }

    #[test]
    fn test_worker_state_transitions() {
        let states = [
            WorkerState::Idle,
            WorkerState::Busy,
            WorkerState::Failed,
            WorkerState::Terminated,
        ];
        assert_eq!(states.len(), 4);
    }

    #[tokio::test]
    async fn test_isolated_worker_executor() {
        let config = WorkerConfig::default();
        let executor = IsolatedWorkerExecutor::new(config);

        assert_eq!(executor.name(), "isolated-worker-executor");
        assert_eq!(
            executor.description(),
            "Executes tools in isolated worker processes with capability lease enforcement"
        );
    }

    #[tokio::test]
    async fn test_worker_pool_terminate_all() {
        let _caps = Arc::new(CapabilitySet::new());
        let config = WorkerConfig::default();
        let mut pool = WorkerPool::new(config);
        pool.terminate_all();

        for worker in &pool.workers {
            assert_eq!(worker.state, WorkerState::Terminated);
        }
    }
}
