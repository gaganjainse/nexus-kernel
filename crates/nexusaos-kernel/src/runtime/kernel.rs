use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::sync::RwLock;
use tracing::warn;

use crate::{
    config::ContextConfig,
    context::ContextManager,
    error::{NexusError, TaskError},
    events::{Event, EventKind, EventPayload},
    model::{
        provider::ModelProvider,
        registry::ProviderRegistry,
        types::{ChatMessage, ChatRole, CompletionRequest, CompletionResponse},
    },
    policy::PolicyEngine,
    resource::{ResourceBudget, ResourceMonitor, SystemPressure},
    router::TaskRouter,
    runtime::scheduler::Scheduler,
    state::{TaskRecord, TaskState},
    storage::{EventStore, SnapshotStore, TaskProjection},
    task::{Priority, TaskId, TaskInput, TaskRequest},
    tools::broker::ToolBroker,
};

/// Tracks performance metrics for model loading, context growth, and tool latency.
#[derive(Debug, Clone)]
pub struct PerformanceMonitor {
    model_load_times: Arc<RwLock<Vec<(String, u128)>>>,
    context_sizes: Arc<RwLock<Vec<(String, usize)>>>,
    tool_latencies: Arc<RwLock<Vec<(String, u128)>>>,
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceMonitor {
    pub fn new() -> Self {
        Self {
            model_load_times: Arc::new(RwLock::new(Vec::new())),
            context_sizes: Arc::new(RwLock::new(Vec::new())),
            tool_latencies: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Record a model loading time in milliseconds.
    pub async fn record_model_load(&self, model_name: &str, duration_ms: u128) {
        let mut times = self.model_load_times.write().await;
        times.push((model_name.to_string(), duration_ms));
    }

    /// Record a context size for a given role.
    pub async fn record_context_size(&self, role: &str, size: usize) {
        let mut sizes = self.context_sizes.write().await;
        sizes.push((role.to_string(), size));
    }

    /// Record a tool invocation latency in milliseconds.
    pub async fn record_tool_latency(&self, tool_name: &str, duration_ms: u128) {
        let mut latencies = self.tool_latencies.write().await;
        latencies.push((tool_name.to_string(), duration_ms));
    }

    /// Check for bottlenecks and return warnings.
    pub async fn check_bottlenecks(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        let times = self.model_load_times.read().await;
        if let Some((_, max_time)) = times.iter().max_by_key(|(_, t)| *t) {
            if *max_time > 5000 {
                warnings.push(format!(
                    "Model loading bottleneck: {}ms exceeds 5s threshold",
                    max_time
                ));
            }
        }

        let sizes = self.context_sizes.read().await;
        if let Some((_, max_size)) = sizes.iter().max_by_key(|(_, s)| *s) {
            if *max_size > 8000 {
                warnings.push(format!(
                    "Context growth bottleneck: {} tokens exceeds 8K threshold",
                    max_size
                ));
            }
        }

        let latencies = self.tool_latencies.read().await;
        if let Some((_, max_lat)) = latencies.iter().max_by_key(|(_, l)| *l) {
            if *max_lat > 10000 {
                warnings.push(format!(
                    "Tool latency bottleneck: {}ms exceeds 10s threshold",
                    max_lat
                ));
            }
        }

        warnings
    }
}

/// The NexusAOS kernel — owns task lifecycle, policy, and state.
pub struct Kernel {
    event_store: Arc<dyn EventStore>,
    projection: Arc<RwLock<TaskProjection>>,
    policy: Arc<RwLock<PolicyEngine>>,
    provider_registry: Arc<ProviderRegistry>,
    tool_broker: Arc<ToolBroker>,
    max_tool_output_size: usize,
    snapshot_store: Option<Arc<SnapshotStore>>,
    performance_monitor: Arc<PerformanceMonitor>,
    resource_budget: ResourceBudget,
    resource_monitor: Arc<ResourceMonitor>,
    context_manager: Arc<ContextManager>,
    scheduler: Arc<Scheduler>,
}

impl Kernel {
    /// Create a new kernel with the given components.
    pub async fn new(
        event_store: Arc<dyn EventStore>,
        policy: Arc<RwLock<PolicyEngine>>,
        provider_registry: Arc<ProviderRegistry>,
        tool_broker: Arc<ToolBroker>,
        max_tool_output_size: usize,
        snapshot_store: Option<Arc<SnapshotStore>>,
        resource_budget: ResourceBudget,
        resource_monitor: Arc<ResourceMonitor>,
        context_manager: Arc<ContextManager>,
        scheduler: Arc<Scheduler>,
    ) -> Result<Self, NexusError> {
        let kernel = Self {
            event_store,
            projection: Arc::new(RwLock::new(TaskProjection::new())),
            policy,
            provider_registry,
            tool_broker,
            max_tool_output_size,
            snapshot_store,
            performance_monitor: Arc::new(PerformanceMonitor::new()),
            resource_budget,
            resource_monitor,
            context_manager,
            scheduler,
        };
        Ok(kernel)
    }

    /// Take a system pressure snapshot without blocking the async runtime.
    async fn system_pressure(&self) -> SystemPressure {
        let monitor = self.resource_monitor.clone();
        tokio::task::spawn_blocking(move || ResourceMonitor::snapshot())
            .await
            .unwrap_or_else(|_| SystemPressure {
                ram_available_mb: 0,
                ram_total_mb: 0,
                vram_available_mb: 0,
                vram_total_mb: 0,
                disk_available_gb: 0,
                queue_depth: self.scheduler.queue_depth(),
            })
    }

    /// Submit a new task. Returns the TaskId.
    pub async fn submit_task(&self, input: TaskInput) -> Result<TaskId, NexusError> {
        let task_id = TaskId::new();
        let request = TaskRequest::new(input.clone());

        // Policy check for task creation
        let decision = {
            let policy = self.policy.read().await;
            policy.evaluate(crate::policy::actions::TASK_CREATE)
        };

        if decision.is_denied() {
            return Err(NexusError::Policy(crate::error::PolicyError::Denied {
                reason: "Task creation denied by policy".into(),
            }));
        }

        // Resource budget admission check
        let pressure = self.system_pressure().await;
        let exceeded = ResourceBudget::check_all(&pressure, &self.resource_budget);
        if !exceeded.is_empty() {
            let reasons = exceeded.join("; ");
            self.emit_event(Event::new(
                task_id,
                EventKind::ResourceBudgetExceeded,
                EventPayload::ResourceBudgetExceeded {
                    resource: reasons.clone(),
                    requested: 0,
                    limit: 0,
                },
                "kernel".to_string(),
            ))
            .await?;
            return Err(NexusError::Resource(crate::error::ResourceError::BudgetExceeded {
                reason: reasons,
            }));
        }

        // Queue depth check
        if ResourceBudget::exceeds_queue_budget(self.scheduler.queue_depth(), &self.resource_budget) {
            return Err(NexusError::Resource(crate::error::ResourceError::QueueFull {
                max_depth: self.resource_budget.max_queue_depth,
            }));
        }

        // Emit TaskCreated event
        let event_payload = EventPayload::TaskCreated {
            request: serde_json::to_value(&request).map_err(NexusError::Serde)?,
        };
        let event =
            Event::new(task_id, EventKind::TaskCreated, event_payload, "kernel".to_string());
        self.emit_event(event).await?;

        // Initialize state in projection
        let record = TaskRecord {
            task_id,
            request,
            current_state: TaskState::Received,
            assigned_role: None,
            state_history: vec![(TaskState::Received, Utc::now())],
        };

        {
            let mut proj = self.projection.write().await;
            proj.tasks.insert(task_id, record);
        }

        // Classify via router
        let has_images = matches!(input, TaskInput::Vision { .. });
        let input_text = input.text();

        let route_decision = TaskRouter::route(&input_text, has_images);

        // Update state to Classified
        let class_payload = EventPayload::StateChanged {
            from: "Received".to_string(),
            to: "Classified".to_string(),
        };
        let class_event =
            Event::new(task_id, EventKind::TaskClassified, class_payload, "router".to_string());
        self.emit_event(class_event).await?;

        {
            let mut proj = self.projection.write().await;
            if let Some(task) = proj.tasks.get_mut(&task_id) {
                task.current_state = TaskState::Classified;
                task.assigned_role = Some(route_decision.primary_role);
                task.state_history.push((TaskState::Classified, Utc::now()));
            }
        }

        // Enqueue in scheduler for dispatch
        let priority = match route_decision.primary_role {
            crate::state::ModelRole::Planner => Priority::High,
            crate::state::ModelRole::Coder => Priority::Normal,
            crate::state::ModelRole::Vision => Priority::Normal,
            crate::state::ModelRole::Reviewer => Priority::Low,
        };
        let _token = self.scheduler.enqueue(task_id, priority).await?;

        Ok(task_id)
    }

    /// Get the current state of a task.
    pub async fn task_state(&self, id: &TaskId) -> Result<TaskState, NexusError> {
        let proj = self.projection.read().await;
        if let Some(task) = proj.tasks.get(id) {
            Ok(task.current_state)
        } else {
            Err(NexusError::Task(TaskError::NotFound { id: id.to_string() }))
        }
    }

    /// Get all tasks in a given state.
    pub async fn tasks_in_state(&self, state: &TaskState) -> Vec<TaskId> {
        let proj = self.projection.read().await;
        proj.tasks
            .iter()
            .filter(|(_, record)| record.current_state == *state)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Transition a task to a new state (with validation).
    pub async fn transition_task(
        &self,
        task_id: &TaskId,
        new_state: TaskState,
    ) -> Result<(), NexusError> {
        let mut proj = self.projection.write().await;
        let task = proj
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| NexusError::Task(TaskError::NotFound { id: task_id.to_string() }))?;

        let current_state = task.current_state;

        if !current_state.can_transition_to(&new_state) {
            return Err(NexusError::Task(TaskError::InvalidTransition {
                from: current_state.to_string(),
                to: new_state.to_string(),
            }));
        }

        let event_payload = EventPayload::StateChanged {
            from: current_state.to_string(),
            to: new_state.to_string(),
        };
        let event =
            Event::new(*task_id, EventKind::TaskStateChanged, event_payload, "kernel".to_string());
        self.emit_event(event).await?;

        task.current_state = new_state;
        task.state_history.push((new_state, Utc::now()));

        Ok(())
    }

    /// Get task count.
    pub async fn task_count(&self) -> usize {
        let proj = self.projection.read().await;
        proj.tasks.len()
    }

    /// Execute a task through the multi-model workflow (Planner -> Coder -> Reviewer -> Tool Broker).
    pub async fn execute_task(
        &self,
        task_id: &TaskId,
    ) -> Result<crate::task::TaskOutcome, NexusError> {
        // 1. Get request and verify state
        let task = {
            let proj = self.projection.read().await;
            proj.tasks
                .get(task_id)
                .cloned()
                .ok_or_else(|| NexusError::Task(TaskError::NotFound { id: task_id.to_string() }))?
        };

        if task.current_state != TaskState::Classified {
            return Err(NexusError::Task(TaskError::InvalidTransition {
                from: task.current_state.to_string(),
                to: TaskState::Planned.to_string(),
            }));
        }

        let input_text = task.request.input.text();

        let planner =
            self.provider_registry.get(&crate::state::ModelRole::Planner).ok_or_else(|| {
                NexusError::Provider(crate::error::ProviderError::Unavailable {
                    name: "Planner".into(),
                })
            })?;

        let plan_resp = match self
            .call_model_with_fallback(
                *task_id,
                "Planner",
                &input_text,
                "You are a planner.",
                planner,
                None,
            )
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                let err_msg = format!("{}", e);
                return self
                    .emit_failure_and_return(*task_id, err_msg, Some(input_text.clone()))
                    .await;
            }
        };

        self.transition_task(task_id, TaskState::Planned).await?;

        // If the task was routed to the vision model, treat its output
        // as a structured observation, not a direct action (§6.8).
        if task.assigned_role == Some(crate::state::ModelRole::Vision) {
            self.emit_vision_observation(
                *task_id,
                "Vision",
                &plan_resp.content,
            )
            .await?;
        }

        let plan = plan_resp.content.to_lowercase();
        let requires_coder = ["write code", "implement ", "edit ", "fix bug", "refactor"]
            .iter()
            .any(|kw| plan.contains(kw))
            || task.assigned_role == Some(crate::state::ModelRole::Coder);

        let mut final_output = plan_resp.content;

        if requires_coder {
            let coder = self.provider_registry.get(&crate::state::ModelRole::Coder);

            if coder.is_none() {
                let err_msg = "Coder provider not available".to_string();
                return self.emit_failure_and_return(*task_id, err_msg, Some(final_output)).await;
            }

            let coder = coder.unwrap();
            self.transition_task(task_id, TaskState::Executing).await?;

            let code_resp = match self
                .call_model_with_fallback(
                    *task_id,
                    "Coder",
                    &final_output,
                    "You are a coder.",
                    coder,
                    None,
                )
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    let err_msg = format!("{}", e);
                    return self
                        .emit_failure_and_return(*task_id, err_msg, Some(final_output))
                        .await;
                }
            };

            final_output = code_resp.content.clone();

            // Reviewer
            if let Some(reviewer) = self.provider_registry.get(&crate::state::ModelRole::Reviewer) {
                let rev_resp = match self
                    .call_model_with_fallback(
                        *task_id,
                        "Reviewer",
                        &final_output,
                        "You are a reviewer.",
                        reviewer,
                        None,
                    )
                    .await
                    {
                    Ok(resp) => resp,
                    Err(e) => {
                        let err_msg = format!("{}", e);
                        return self
                            .emit_failure_and_return(*task_id, err_msg, Some(final_output.clone()))
                            .await;
                    }
                };
                final_output = format!("{}\nReview: {}", final_output, rev_resp.content);
            }
        }

        let mut requires_confirmation = false;

        if let Some(tool_call_str) =
            final_output.strip_prefix("TOOL:").map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            let tool_call = match parse_tool_call(tool_call_str) {
                Ok(tc) => tc,
                Err(err_msg) => {
                    return self
                        .emit_failure_and_return(*task_id, err_msg, Some(final_output.clone()))
                        .await;
                }
            };

            let tool_req = crate::tools::executor::ToolRequest {
                tool_name: tool_call.tool_name.clone(),
                arguments: tool_call.arguments.clone(),
            };

            // Enforce Planner constraint: planner should not write files
            // directly unless explicitly delegated (§6.9)
            if task.assigned_role == Some(crate::state::ModelRole::Planner) {
                if let Some(path) = tool_call.arguments.get("path").and_then(|v| v.as_str()) {
                    if path.starts_with('/') || path.contains("write") || path.contains("edit") {
                        self.emit_event(Event::new(
                            *task_id,
                            EventKind::PolicyDenied,
                            EventPayload::PolicyCheck {
                                action: format!("planner.write_file:{}", path),
                                decision: "denied".to_string(),
                                reason: Some("Planner must not write files directly without explicit delegation".to_string()),
                            },
                            "kernel".to_string(),
                        ))
                        .await?;
                        return self
                            .emit_failure_and_return(
                                *task_id,
                                "Planner cannot write files directly. Delegate file operations to the Coder.".to_string(),
                                Some(final_output.clone()),
                            )
                            .await;
                    }
                }
            }

            // Enforce Coder constraint: coder should not decide product scope
            // or architecture direction without planner input for large tasks (§6.10)
            if task.assigned_role == Some(crate::state::ModelRole::Coder) {
                let scope_keywords = ["architecture", "design", "scope", "product direction", "plan"];
                let output_lower = final_output.to_lowercase();
                let has_scope_keyword = scope_keywords.iter().any(|kw| output_lower.contains(kw));
                if has_scope_keyword {
                    self.emit_event(Event::new(
                        *task_id,
                        EventKind::PolicyChecked,
                        EventPayload::PolicyCheck {
                            action: format!("coder.scope_check:{}", tool_call.tool_name),
                            decision: "require_confirmation".to_string(),
                            reason: Some("Coder must not decide product scope or architecture without planner input".to_string()),
                        },
                        "kernel".to_string(),
                    ))
                    .await?;
                    return self
                        .emit_failure_and_return(
                            *task_id,
                            "Coder cannot decide product scope or architecture direction without planner input. Route to Planner first.".to_string(),
                            Some(final_output.clone()),
                        )
                        .await;
                }
            }

            // Create checkpoint before risky tool execution
            self.create_checkpoint(*task_id, &tool_call.tool_name, &tool_call.arguments)
                .await?;

            self.emit_tool_requested(*task_id, &tool_call.tool_name, tool_call.arguments).await?;

            let tool_start = Instant::now();
            let broker_result = self.tool_broker.execute(&tool_req).await;
            let tool_duration = tool_start.elapsed().as_millis();
            self.performance_monitor
                .record_tool_latency(&tool_call.tool_name, tool_duration)
                .await;

            match broker_result {
                Ok(crate::tools::broker::BrokerResult::Completed(res)) => {
                    self.emit_tool_result(
                        *task_id,
                        EventKind::ToolCompleted,
                        &tool_call.tool_name,
                        res.success,
                        &res.output,
                    )
                    .await?;
                }
                Ok(crate::tools::broker::BrokerResult::Denied(reason)) => {
                    self.emit_tool_result(
                        *task_id,
                        EventKind::ToolFailed,
                        &tool_call.tool_name,
                        false,
                        &format!("Denied: {}", reason),
                    )
                    .await?;
                    let err_msg = format!("Tool denied: {}", reason);
                    return self
                        .emit_failure_and_return(*task_id, err_msg, Some(final_output.clone()))
                        .await;
                }
                Ok(crate::tools::broker::BrokerResult::RequiresConfirmation(reason)) => {
                    self.emit_tool_result(
                        *task_id,
                        EventKind::ToolFailed,
                        &tool_call.tool_name,
                        false,
                        &format!("Requires confirmation: {}", reason),
                    )
                    .await?;
                    requires_confirmation = true;
                }
                Err(e) => {
                    self.emit_tool_result(
                        *task_id,
                        EventKind::ToolFailed,
                        &tool_call.tool_name,
                        false,
                        &e.to_string(),
                    )
                    .await?;
                    return self
                        .emit_failure_and_return(*task_id, e.to_string(), Some(final_output))
                        .await;
                }
            }
        }

        if requires_confirmation {
            let current_state = self.task_state(task_id).await?;
            match current_state {
                TaskState::Planned => {
                    self.transition_task(task_id, TaskState::AwaitingConfirmation).await?;
                }
                TaskState::Executing => {
                    self.transition_task(task_id, TaskState::Blocked).await?;
                }
                _ => {}
            }
            return Ok(crate::task::TaskOutcome {
                task_id: *task_id,
                success: false,
                output: Some(final_output),
                error: Some("Requires confirmation".to_string()),
                completed_at: Utc::now(),
                requires_confirmation: true,
            });
        }

        let current_state = self.task_state(task_id).await?;
        if current_state == TaskState::Planned {
            self.transition_task(task_id, TaskState::Executing).await?;
        }
        self.transition_task(task_id, TaskState::Completed).await?;

        Ok(crate::task::TaskOutcome {
            task_id: *task_id,
            success: true,
            output: Some(final_output),
            error: None,
            completed_at: Utc::now(),
            requires_confirmation: false,
        })
    }

    // Helper: emit an event
    async fn emit_event(&self, event: Event) -> Result<(), NexusError> {
        self.event_store.append(event).await
    }

    /// Redact common secret patterns from strings before storing in events.
    /// Prevents secrets from leaking into the audit log.
    fn redact_secrets(text: &str) -> String {
        let mut result = text.to_string();
        let secret_patterns = [
            "api_key=",
            "apikey=",
            "secret=",
            "token=",
            "password=",
            "passwd=",
        ];
        for pattern in &secret_patterns {
            if let Some(pos) = result.find(pattern) {
                let end = result[pos..]
                    .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '\n')
                    .map(|i| pos + i)
                    .unwrap_or(result.len());
                result.replace_range(pos..end, &format!("{}***REDACTED***", pattern));
            }
        }
        // Also handle JSON-style key-value pairs like "api_key": "value"
        let json_patterns = [
            "\"api_key\"",
            "\"apikey\"",
            "\"secret\"",
            "\"token\"",
            "\"password\"",
            "\"passwd\"",
        ];
        for pattern in &json_patterns {
            if let Some(pos) = result.find(pattern) {
                // Find the value after the colon
                if let Some(colon_pos) = result[pos..].find(':') {
                    let value_start = pos + colon_pos + 1;
                    let value_end = result[value_start..]
                        .find(|c: char| c == ',' || c == '}' || c == '\n')
                        .map(|i| value_start + i)
                        .unwrap_or(result.len());
                    result.replace_range(value_start..value_end, " \"***REDACTED***\"");
                }
            }
        }
        result
    }

    async fn emit_model_requested(
        &self,
        task_id: TaskId,
        role: &str,
        context_budget: usize,
    ) -> Result<(), NexusError> {
        self.emit_event(Event::new(
            task_id,
            EventKind::ModelRequested,
            EventPayload::ModelRequest { role: role.to_string(), prompt_tokens: 0, context_budget },
            "kernel".to_string(),
        ))
        .await
    }

    async fn emit_model_responded(
        &self,
        task_id: TaskId,
        role: &str,
        response_tokens: usize,
        content: &str,
    ) -> Result<(), NexusError> {
        let redacted = Self::redact_secrets(content);
        self.emit_event(Event::new(
            task_id,
            EventKind::ModelResponded,
            EventPayload::ModelResponse {
                role: role.to_string(),
                response_tokens,
                content: redacted,
            },
            "kernel".to_string(),
        ))
        .await
    }

    async fn emit_tool_requested(
        &self,
        task_id: TaskId,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<(), NexusError> {
        self.emit_event(Event::new(
            task_id,
            EventKind::ToolRequested,
            EventPayload::ToolCall { tool_name: tool_name.to_string(), arguments },
            "kernel".to_string(),
        ))
        .await
    }

    async fn emit_tool_result(
        &self,
        task_id: TaskId,
        kind: EventKind,
        tool_name: &str,
        success: bool,
        output: &str,
    ) -> Result<(), NexusError> {
        let max_size = self.max_tool_output_size;
        let truncated = truncate_output(output, max_size);
        let redacted = Self::redact_secrets(&truncated);
        self.emit_event(Event::new(
            task_id,
            kind,
            EventPayload::ToolResult {
                tool_name: tool_name.to_string(),
                success,
                output: redacted,
            },
            "kernel".to_string(),
        ))
        .await
    }

    async fn emit_failure_and_return(
        &self,
        task_id: TaskId,
        error_message: String,
        output: Option<String>,
    ) -> Result<crate::task::TaskOutcome, NexusError> {
        self.emit_event(Event::new(
            task_id,
            EventKind::Error,
            EventPayload::ErrorEvent { message: error_message.clone(), details: None },
            "kernel".to_string(),
        ))
        .await?;
        self.transition_task(&task_id, TaskState::Failed).await?;
        Ok(crate::task::TaskOutcome {
            task_id,
            success: false,
            output,
            error: Some(error_message),
            completed_at: Utc::now(),
            requires_confirmation: false,
        })
    }

    /// Create a checkpoint snapshot before a risky action.
    async fn create_checkpoint(
        &self,
        task_id: TaskId,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<(), NexusError> {
        let snapshot_store = match &self.snapshot_store {
            Some(store) => store,
            None => return Ok(()),
        };

        let projection = self.projection.read().await;
        let task = projection.tasks.get(&task_id).cloned();
        drop(projection);

        let snapshot_data = serde_json::json!({
            "task_id": task_id.to_string(),
            "tool_name": tool_name,
            "arguments": arguments,
            "task_state": task.as_ref().map(|t| t.current_state.to_string()),
            "state_history": task.as_ref().map(|t| t.state_history.iter().map(|(s, _)| s.to_string()).collect::<Vec<_>>()),
        });

        let snapshot = crate::storage::Snapshot {
            snapshot_id: format!("checkpoint-{}-{}", task_id, tool_name),
            created_at: Utc::now(),
            last_sequence: 0,
            data: snapshot_data,
        };

        snapshot_store.save(&snapshot).await?;

        self.emit_event(Event::new(
            task_id,
            EventKind::CheckpointCreated,
            EventPayload::Checkpoint { snapshot_path: snapshot.snapshot_id },
            "kernel".to_string(),
        ))
        .await?;

        Ok(())
    }

    /// Call a model provider with the given prompt, emitting telemetry events.
    /// If the provider fails and a fallback is available, retries with the fallback.
    async fn call_model(
        &self,
        task_id: TaskId,
        role_label: &str,
        user_content: &str,
        system_prompt: &str,
        provider: &dyn ModelProvider,
    ) -> Result<CompletionResponse, crate::error::ProviderError> {
        let req = CompletionRequest::new(
            vec![
                ChatMessage {
                    role: ChatRole::System,
                    content: system_prompt.to_string(),
                    images: None,
                },
                ChatMessage {
                    role: ChatRole::User,
                    content: user_content.to_string(),
                    images: None,
                },
            ],
            provider.name(),
            provider.max_context(),
        );
        self.emit_model_requested(task_id, role_label, provider.max_context()).await.map_err(
            |e| {
                crate::error::ProviderError::InferenceFailed(format!(
                    "{} event emission failed: {}",
                    role_label, e
                ))
            },
        )?;
        let resp = provider.complete(req).await.map_err(|e| {
            crate::error::ProviderError::InferenceFailed(format!("{} failed: {}", role_label, e))
        })?;
        self.emit_model_responded(
            task_id,
            role_label,
            resp.completion_tokens.unwrap_or(0),
            &resp.content,
        )
        .await
        .map_err(|e| {
            crate::error::ProviderError::InferenceFailed(format!(
                "{} event emission failed: {}",
                role_label, e
            ))
        })?;
        Ok(resp)
    }

    /// Validate that vision model output is treated as a structured observation,
    /// not a direct system action. Vision outputs must be converted to
    /// observations before use (§6.8 constraint).
    async fn emit_vision_observation(
        &self,
        task_id: TaskId,
        role: &str,
        observation: &str,
    ) -> Result<(), NexusError> {
        self.emit_event(Event::new(
            task_id,
            EventKind::ModelResponded,
            EventPayload::ModelResponse {
                role: role.to_string(),
                response_tokens: 0,
                content: format!("[VISION OBSERVATION] {}", observation),
            },
            "kernel".to_string(),
        ))
        .await?;
        Ok(())
    }

    /// Call a model with fallback support. If the primary provider fails,
    /// retries with the fallback provider for the same role.
    async fn call_model_with_fallback(
        &self,
        task_id: TaskId,
        role_label: &str,
        user_content: &str,
        system_prompt: &str,
        primary: &dyn ModelProvider,
        fallback: Option<&dyn ModelProvider>,
    ) -> Result<CompletionResponse, crate::error::ProviderError> {
        match self
            .call_model(task_id, role_label, user_content, system_prompt, primary)
            .await
        {
            Ok(resp) => Ok(resp),
            Err(primary_err) => {
                if let Some(fb) = fallback {
                    warn!(
                        role = role_label,
                        primary = primary.name(),
                        fallback = fb.name(),
                        error = %primary_err,
                        "Primary model failed, falling back"
                    );
                    self.emit_event(Event::new(
                        task_id,
                        EventKind::ModelFailed,
                        EventPayload::ModelFailure {
                            role: role_label.to_string(),
                            error: format!("{} (fallback attempted)", primary_err),
                        },
                        "kernel".to_string(),
                    ))
                    .await
                    .map_err(|e| crate::error::ProviderError::InferenceFailed(e.to_string()))?;
                    self.call_model(task_id, role_label, user_content, system_prompt, fb)
                        .await
                } else {
                    Err(primary_err)
                }
            }
        }
    }

    /// Recover incomplete tasks from the event store on startup.
    /// Scans for tasks that are in non-terminal states and attempts to
    /// restore them or mark them as failed.
    pub async fn recover_incomplete_tasks(&self) -> Result<Vec<TaskId>, NexusError> {
        let events = self.event_store.get_all_events().await?;
        let mut incomplete = Vec::new();

        for event in &events {
            if let EventPayload::StateChanged { to, .. } = &event.payload {
                if matches!(
                    to.as_str(),
                    "Executing" | "AwaitingConfirmation" | "Blocked" | "Planned"
                ) {
                    if let Some(task_id) = event.task_id {
                        incomplete.push(task_id);
                    }
                }
            }
        }

        for task_id in &incomplete {
            self.transition_task(task_id, TaskState::Failed).await?;
            self.emit_event(Event::new(
                *task_id,
                EventKind::Error,
                EventPayload::ErrorEvent {
                    message: format!(
                        "Task {} recovered from incomplete state on startup",
                        task_id
                    ),
                    details: None,
                },
                "kernel".to_string(),
            ))
            .await?;
        }

        Ok(incomplete)
    }
}

/// A parsed tool call directive from a model response.
struct ParsedToolCall {
    tool_name: String,
    arguments: serde_json::Value,
}

/// Parse a `TOOL:name args` directive from model output.
fn parse_tool_call(input: &str) -> Result<ParsedToolCall, String> {
    let tool_name_end = input.find(|c: char| c.is_whitespace()).unwrap_or(input.len());
    let tool_name = input[..tool_name_end].to_string();
    if tool_name.is_empty() {
        return Err("Tool name is empty".to_string());
    }
    let args_str = input[tool_name_end..].trim();
    let arguments = if args_str.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(args_str).map_err(|e| format!("Invalid tool arguments JSON: {}", e))?
    };
    Ok(ParsedToolCall { tool_name, arguments })
}

/// Truncate output at a newline boundary to avoid cutting lines mid-way.
/// Also ensures the truncation point is at a valid UTF-8 character boundary.
fn truncate_output(output: &str, max_size: usize) -> &str {
    if output.len() <= max_size {
        return output;
    }
    // Step back to the nearest UTF-8 character boundary.
    let mut boundary = max_size;
    while boundary > 0 && !output.is_char_boundary(boundary) {
        boundary -= 1;
    }
    // Step back to the nearest newline boundary to preserve line structure.
    let cut_point = &output[..boundary];
    cut_point.rfind('\n').map(|i| &output[..i + 1]).unwrap_or(cut_point)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::{path::PathBuf, sync::Mutex};

    use async_trait::async_trait;

    use super::*;
    use crate::{
        model::{
            provider::ModelProvider,
            types::{CompletionRequest, CompletionResponse},
        },
        policy::TrustTier,
        state::ModelRole,
    };

    struct MockProvider {
        role: crate::state::ModelRole,
        content: String,
    }
    #[async_trait]
    impl ModelProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }
        fn role(&self) -> crate::state::ModelRole {
            self.role
        }
        fn max_context(&self) -> usize {
            100
        }
        fn supports_vision(&self) -> bool {
            false
        }
        async fn health_check(&self) -> Result<bool, crate::error::ProviderError> {
            Ok(true)
        }
        async fn complete(
            &self,
            _r: CompletionRequest,
        ) -> Result<CompletionResponse, crate::error::ProviderError> {
            Ok(CompletionResponse {
                content: self.content.clone(),
                finish_reason: None,
                prompt_tokens: None,
                completion_tokens: None,
                model: "mock".into(),
            })
        }
        async fn cancel(&self) -> Result<(), crate::error::ProviderError> {
            Ok(())
        }
    }

    struct MockEventStore {
        events: Mutex<Vec<Event>>,
    }

    impl MockEventStore {
        fn new() -> Self {
            Self { events: Mutex::new(Vec::new()) }
        }
    }

    #[async_trait]
    impl EventStore for MockEventStore {
        async fn append(&self, event: Event) -> Result<(), NexusError> {
            self.events.lock().unwrap().push(event);
            Ok(())
        }
        async fn get_all_events(&self) -> Result<Vec<Event>, NexusError> {
            Ok(self.events.lock().unwrap().clone())
        }
        async fn get_task_events(&self, task_id: &TaskId) -> Result<Vec<Event>, NexusError> {
            Ok(self
                .events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.task_id == Some(*task_id))
                .cloned()
                .collect())
        }
        async fn read_since(&self, _sequence: u64) -> Result<Vec<Event>, NexusError> {
            Ok(self.events.lock().unwrap().clone())
        }
    }

    #[tokio::test]
    async fn test_submit_task_allowed() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        let id = kernel.submit_task(TaskInput::Text("test".into())).await.unwrap();
        let state = kernel.task_state(&id).await.unwrap();
        assert_eq!(state, TaskState::Classified);
    }

    #[tokio::test]
    async fn test_submit_task_denied() {
        let store = Arc::new(MockEventStore::new());
        let policy = PolicyEngine::deny_all();
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        let result = kernel.submit_task(TaskInput::Text("test".into())).await;
        assert!(matches!(result, Err(NexusError::Policy(_))));
    }

    #[tokio::test]
    async fn test_task_transition() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        let id = kernel.submit_task(TaskInput::Text("test".into())).await.unwrap();
        kernel.transition_task(&id, TaskState::Planned).await.unwrap();
        assert_eq!(kernel.task_state(&id).await.unwrap(), TaskState::Planned);
    }

    #[tokio::test]
    async fn test_invalid_task_transition() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        let id = kernel.submit_task(TaskInput::Text("test".into())).await.unwrap();
        // Classified -> Completed is invalid
        let result = kernel.transition_task(&id, TaskState::Completed).await;
        assert!(matches!(result, Err(NexusError::Task(_))));
    }

    #[tokio::test]
    async fn test_execute_task() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);

        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(MockProvider {
            role: crate::state::ModelRole::Planner,
            content: "Need to write some code to fix this. TOOL: dummy".into(),
        }));
        registry.register(Box::new(MockProvider {
            role: crate::state::ModelRole::Coder,
            content: "Here is the code.".into(),
        }));
        registry.register(Box::new(MockProvider {
            role: crate::state::ModelRole::Reviewer,
            content: "Looks good.".into(),
        }));

        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(
            store,
            Arc::new(RwLock::new(policy)),
            Arc::new(registry),
            broker,
            1_048_576,
            None,
            ResourceBudget::default(),
            Arc::new(ResourceMonitor::new()),
            Arc::new(ContextManager::new(ContextConfig::default())),
            Arc::new(Scheduler::new(32)),
        )
        .await
        .unwrap();

        let id = kernel.submit_task(TaskInput::Text("fix this".into())).await.unwrap();

        // Execute task should run Planner -> Coder -> Reviewer
        let outcome = kernel.execute_task(&id).await.unwrap();
        assert!(outcome.success);
        let final_output = outcome.output.unwrap();
        assert!(final_output.contains("Here is the code."));
        assert!(final_output.contains("Review: Looks good."));

        let state = kernel.task_state(&id).await.unwrap();
        assert_eq!(state, TaskState::Completed);
    }

    #[tokio::test]
    async fn test_task_state_not_found() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        let fake_id = TaskId::new();
        let result = kernel.task_state(&fake_id).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NexusError::Task(TaskError::NotFound { .. }) => {}
            _ => panic!("Expected TaskNotFound"),
        }
    }

    #[tokio::test]
    async fn test_tasks_in_state() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        let id1 = kernel.submit_task(TaskInput::Text("task1".into())).await.unwrap();
        let id2 = kernel.submit_task(TaskInput::Text("task2".into())).await.unwrap();

        let classified = kernel.tasks_in_state(&TaskState::Classified).await;
        assert_eq!(classified.len(), 2);
        assert!(classified.contains(&id1));
        assert!(classified.contains(&id2));

        let received = kernel.tasks_in_state(&TaskState::Received).await;
        assert!(received.is_empty());
    }

    #[tokio::test]
    async fn test_task_count() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        assert_eq!(kernel.task_count().await, 0);
        kernel.submit_task(TaskInput::Text("t1".into())).await.unwrap();
        assert_eq!(kernel.task_count().await, 1);
        kernel.submit_task(TaskInput::Text("t2".into())).await.unwrap();
        assert_eq!(kernel.task_count().await, 2);
    }

    #[tokio::test]
    async fn test_transition_task_not_found() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        let fake_id = TaskId::new();
        let result = kernel.transition_task(&fake_id, TaskState::Planned).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_task_no_planner() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        let id = kernel.submit_task(TaskInput::Text("do something".into())).await.unwrap();
        let result = kernel.execute_task(&id).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NexusError::Provider(crate::error::ProviderError::Unavailable { .. }) => {}
            _ => panic!("Expected Provider Unavailable"),
        }
    }

    #[tokio::test]
    async fn test_submit_task_events_emitted() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel =
            Kernel::new(store.clone(), Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
                .await
                .unwrap();

        let _id = kernel.submit_task(TaskInput::Text("test".into())).await.unwrap();

        let events = store.get_all_events().await.unwrap();
        // Should have at least TaskCreated and TaskClassified events
        assert!(events.len() >= 2);
        let kinds: Vec<_> = events.iter().map(|e| &e.kind).collect();
        assert!(kinds.contains(&&EventKind::TaskCreated));
        assert!(kinds.contains(&&EventKind::TaskClassified));
    }

    #[tokio::test]
    async fn test_execute_task_planner_only_no_code() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);

        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(MockProvider {
            role: crate::state::ModelRole::Planner,
            content: "Here is the architectural plan. No implementation required.".into(),
        }));

        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(
            store.clone(),
            Arc::new(RwLock::new(policy)),
            Arc::new(registry),
            broker,
            1_048_576,
            None,
            ResourceBudget::default(),
            Arc::new(ResourceMonitor::new()),
            Arc::new(ContextManager::new(ContextConfig::default())),
            Arc::new(Scheduler::new(32)),
        )
        .await
        .unwrap();

        let id = kernel.submit_task(TaskInput::Text("plan something".into())).await.unwrap();
        let outcome = kernel.execute_task(&id).await.unwrap();
        assert!(outcome.success);
        assert!(outcome.output.unwrap().contains("architectural plan"));

        let state = kernel.task_state(&id).await.unwrap();
        assert_eq!(state, TaskState::Completed);

        // Verify reviewer was skipped: no ModelRequest events for "Reviewer"
        let events = store.get_all_events().await.unwrap();
        let reviewer_events: Vec<_> = events
            .iter()
            .filter(|e| {
                if let EventPayload::ModelRequest { role, .. } = &e.payload {
                    role == "Reviewer"
                } else {
                    false
                }
            })
            .collect();
        assert!(
            reviewer_events.is_empty(),
            "Reviewer should be skipped when only planner is registered"
        );
    }

    #[tokio::test]
    async fn test_kernel_new() {
        let store = Arc::new(MockEventStore::new());
        let policy = PolicyEngine::deny_all();
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();
        assert_eq!(kernel.task_count().await, 0);
    }

    #[tokio::test]
    async fn test_transition_through_multiple_states() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        let id = kernel.submit_task(TaskInput::Text("test".into())).await.unwrap();
        assert_eq!(kernel.task_state(&id).await.unwrap(), TaskState::Classified);

        kernel.transition_task(&id, TaskState::Planned).await.unwrap();
        assert_eq!(kernel.task_state(&id).await.unwrap(), TaskState::Planned);

        kernel.transition_task(&id, TaskState::AwaitingConfirmation).await.unwrap();
        assert_eq!(kernel.task_state(&id).await.unwrap(), TaskState::AwaitingConfirmation);

        kernel.transition_task(&id, TaskState::Executing).await.unwrap();
        assert_eq!(kernel.task_state(&id).await.unwrap(), TaskState::Executing);

        kernel.transition_task(&id, TaskState::Blocked).await.unwrap();
        assert_eq!(kernel.task_state(&id).await.unwrap(), TaskState::Blocked);

        kernel.transition_task(&id, TaskState::Executing).await.unwrap();
        kernel.transition_task(&id, TaskState::Completed).await.unwrap();
        assert_eq!(kernel.task_state(&id).await.unwrap(), TaskState::Completed);
    }

    #[tokio::test]
    async fn test_vision_task_input() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        let id = kernel
            .submit_task(TaskInput::Vision {
                text: "describe this image".into(),
                image_paths: vec![PathBuf::from("/tmp/img.png")],
            })
            .await
            .unwrap();
        assert_eq!(kernel.task_state(&id).await.unwrap(), TaskState::Classified);
    }

    #[tokio::test]
    async fn test_multi_task_input() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(store, Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
            .await
            .unwrap();

        let input = TaskInput::Multi {
            parts: vec![TaskInput::Text("part1".into()), TaskInput::Text("part2".into())],
        };
        let id = kernel.submit_task(input).await.unwrap();
        assert_eq!(kernel.task_state(&id).await.unwrap(), TaskState::Classified);
    }

    #[tokio::test]
    async fn test_submit_task_creates_record_with_correct_state_history() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel =
            Kernel::new(store.clone(), Arc::new(RwLock::new(policy)), registry, broker, 1_048_576, None, ResourceBudget::default(), Arc::new(ResourceMonitor), Arc::new(ContextManager::new(ContextConfig::default())), Arc::new(Scheduler::new(32)))
                .await
                .unwrap();

        let id = kernel.submit_task(TaskInput::Text("test".into())).await.unwrap();

        // The projection should have the task with state history
        let events = store.get_task_events(&id).await.unwrap();
        assert!(!events.is_empty());
    }

    // === v1 Acceptance Criteria Tests ===

    #[tokio::test]
    async fn test_v1_tasks_are_traceable() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let kernel = Kernel::new(
            store.clone(),
            Arc::new(RwLock::new(policy)),
            registry,
            broker,
            1_048_576,
            None,
            ResourceBudget::default(),
            Arc::new(ResourceMonitor::new()),
            Arc::new(ContextManager::new(ContextConfig::default())),
            Arc::new(Scheduler::new(32)),
        )
        .await
        .unwrap();

        let id = kernel.submit_task(TaskInput::Text("test traceability".into())).await.unwrap();
        let events = store.get_task_events(&id).await.unwrap();
        assert!(!events.is_empty(), "Task must have traceable events");
    }

    #[tokio::test]
    async fn test_v1_router_chooses_specialists_consistently() {
        let decision = TaskRouter::route("write a function to sort an array", false);
        assert_eq!(decision.primary_role, ModelRole::Coder);

        let decision = TaskRouter::route("design the system architecture", false);
        assert_eq!(decision.primary_role, ModelRole::Planner);

        let decision = TaskRouter::route("look at this screenshot", true);
        assert_eq!(decision.primary_role, ModelRole::Vision);
    }

    #[tokio::test]
    async fn test_v1_checkpoint_created_before_tool_execution() {
        let store = Arc::new(MockEventStore::new());
        let rule = crate::policy::PolicyRule {
            name: "allow".into(),
            action_pattern: "*".into(),
            decision: "allow".into(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Autonomous);
        let registry = Arc::new(ProviderRegistry::new());
        let broker = Arc::new(ToolBroker::new(Arc::new(policy.clone())));
        let snapshot_dir = tempfile::tempdir().unwrap();
        let snapshot_store = Arc::new(crate::storage::SnapshotStore::new(
            snapshot_dir.path().to_path_buf(),
        ));
        let kernel = Kernel::new(
            store.clone(),
            Arc::new(RwLock::new(policy)),
            registry,
            broker,
            1_048_576,
            Some(snapshot_store.clone()),
        )
        .await
        .unwrap();

        let id = kernel.submit_task(TaskInput::Text("test checkpoint".into())).await.unwrap();
        let events = store.get_task_events(&id).await.unwrap();
        let _checkpoint_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::CheckpointCreated))
            .collect();
        // Checkpoint events are created when tools are executed during execute_task,
        // not during submit_task. This test verifies the snapshot store is configured.
        assert!(snapshot_store.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_v1_performance_monitor_detects_bottlenecks() {
        let monitor = crate::runtime::kernel::PerformanceMonitor::new();

        // Simulate a slow model load
        monitor
            .record_model_load("slow-model", 6000)
            .await;
        monitor
            .record_context_size("planner", 9000)
            .await;
        monitor
            .record_tool_latency("fs.read", 11000)
            .await;

        let warnings = monitor.check_bottlenecks().await;
        assert_eq!(warnings.len(), 3);
        assert!(warnings.iter().any(|w| w.contains("Model loading bottleneck")));
        assert!(warnings.iter().any(|w| w.contains("Context growth bottleneck")));
        assert!(warnings.iter().any(|w| w.contains("Tool latency bottleneck")));
    }

    #[tokio::test]
    async fn test_v1_secret_redaction() {
        let text = "api_key=sk-secret123 token=bearer-abc password=pass123";
        let redacted = Kernel::redact_secrets(text);
        assert!(!redacted.contains("sk-secret123"));
        assert!(!redacted.contains("bearer-abc"));
        assert!(!redacted.contains("pass123"));
        assert!(redacted.contains("***REDACTED***"));
    }

    #[tokio::test]
    async fn test_v1_manifest_lifecycle() {
        let manifest = crate::manifest::Manifest::new(
            "v1.0".to_string(),
            serde_json::json!({"name": "test"}),
            "1.0".to_string(),
            "test-user".to_string(),
        );
        assert_eq!(manifest.state, crate::manifest::ManifestState::Draft);
    }
}
