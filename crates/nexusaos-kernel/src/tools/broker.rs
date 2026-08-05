use std::{collections::HashMap, sync::Arc, time::Duration};

use tracing::{error, info, warn};

use super::executor::{ToolExecutor, ToolRequest, ToolResult};
use crate::{
    error::ToolError,
    policy::{PolicyDecision, PolicyEngine},
};

/// The result of a broker dispatch.
#[derive(Debug)]
pub enum BrokerResult {
    /// Tool executed successfully.
    Completed(ToolResult),
    /// Tool requires confirmation before execution.
    RequiresConfirmation(String),
    /// Tool was denied by policy.
    Denied(String),
}

/// Dispatches tool calls through policy checks and logging.
pub struct ToolBroker {
    executors: HashMap<String, Arc<dyn ToolExecutor>>,
    policy: Arc<PolicyEngine>,
    default_executor: Option<Arc<dyn ToolExecutor>>,
    max_tool_seconds: u64,
    event_store: Option<Arc<dyn crate::storage::EventStore>>,
    capabilities: Option<Arc<crate::capability::CapabilitySet>>,
}

impl ToolBroker {
    pub fn new(policy: Arc<PolicyEngine>) -> Self {
        Self {
            executors: HashMap::new(),
            policy,
            default_executor: None,
            max_tool_seconds: 300,
            event_store: None,
            capabilities: None,
        }
    }

    /// Set the maximum tool execution timeout in seconds.
    pub fn with_timeout(mut self, max_tool_seconds: u64) -> Self {
        self.max_tool_seconds = max_tool_seconds;
        self
    }

    /// Set the event store for tool invocation/result events.
    pub fn with_event_store(mut self, event_store: Arc<dyn crate::storage::EventStore>) -> Self {
        self.event_store = Some(event_store);
        self
    }

    /// Set the capability set for authorization checks.
    pub fn with_capabilities(
        mut self,
        capabilities: Arc<crate::capability::CapabilitySet>,
    ) -> Self {
        self.capabilities = Some(capabilities);
        self
    }

    /// Register a tool executor.
    pub fn register(&mut self, executor: Arc<dyn ToolExecutor>) {
        let name = executor.name().to_string();
        self.executors.insert(name, executor);
    }

    /// Set the default executor used when no specific executor is registered for a tool.
    pub fn set_default_executor(&mut self, executor: Arc<dyn ToolExecutor>) {
        self.default_executor = Some(executor);
    }

    /// Execute a tool call with policy checks.
    /// Returns PolicyDecision if confirmation is needed, or executes if allowed.
    pub async fn execute(&self, request: &ToolRequest) -> Result<BrokerResult, ToolError> {
        let action = request
            .arguments
            .get("action")
            .and_then(|v| v.as_str())
            .map(|a| format!("{}.{}", request.tool_name, a))
            .unwrap_or_else(|| format!("{}.execute", request.tool_name));
        let decision = self.policy.evaluate(&action);

        info!(tool = %request.tool_name, action = %action, "Tool execution requested");

        match decision {
            PolicyDecision::Deny(reason) => {
                warn!(tool = %request.tool_name, reason = %reason, "Tool execution denied by policy");
                Ok(BrokerResult::Denied(reason))
            }
            PolicyDecision::RequireConfirmation(reason) => {
                info!(tool = %request.tool_name, reason = %reason, "Tool execution requires confirmation");
                Ok(BrokerResult::RequiresConfirmation(reason))
            }
            PolicyDecision::Allow => {
                let executor = self
                    .executors
                    .get(&request.tool_name)
                    .or(self.default_executor.as_ref())
                    .ok_or_else(|| {
                        error!(tool = %request.tool_name, "Tool not found");
                        ToolError::NotFound { name: request.tool_name.clone() }
                    })?;

                // Capability check
                if let Some(caps) = &self.capabilities {
                    let required = format!("tool.{}", request.tool_name);
                    if !caps.has_capability(&required) {
                        return Ok(BrokerResult::Denied(format!(
                            "Missing capability: {}",
                            required
                        )));
                    }
                }

                info!(tool = %request.tool_name, "Executing tool");
                let result = tokio::time::timeout(
                    Duration::from_secs(self.max_tool_seconds),
                    executor.execute(request),
                )
                .await
                .map_err(|_| ToolError::Timeout {
                    name: request.tool_name.clone(),
                    timeout_secs: self.max_tool_seconds,
                })??;
                info!(tool = %request.tool_name, success = %result.success, "Tool execution completed");
                Ok(BrokerResult::Completed(result))
            }
        }
    }

    /// List available tools.
    pub fn available_tools(&self) -> Vec<String> {
        self.executors.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde_json::json;

    use super::*;
    use crate::policy::{PolicyRule, TrustTier};

    struct DummyTool;
    #[async_trait]
    impl ToolExecutor for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "dummy"
        }
        fn is_destructive(&self) -> bool {
            false
        }
        async fn execute(&self, _req: &ToolRequest) -> Result<ToolResult, ToolError> {
            Ok(ToolResult { success: true, output: "ok".to_string(), data: None })
        }
    }

    #[tokio::test]
    async fn test_broker() -> Result<(), Box<dyn std::error::Error>> {
        let rule = PolicyRule {
            name: "allow-dummy".to_string(),
            action_pattern: "dummy.execute".to_string(),
            decision: "allow".to_string(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Basic);

        let mut broker = ToolBroker::new(Arc::new(policy));
        broker.register(Arc::new(DummyTool));

        let req = ToolRequest { tool_name: "dummy".to_string(), arguments: json!({}) };

        let res = broker.execute(&req).await?;
        match res {
            BrokerResult::Completed(tr) => assert_eq!(tr.output, "ok"),
            other => return Err(format!("expected Completed, got {other:?}").into()),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_deny_policy() -> Result<(), Box<dyn std::error::Error>> {
        let rule = PolicyRule {
            name: "deny-dummy".to_string(),
            action_pattern: "dummy.execute".to_string(),
            decision: "deny".to_string(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Basic);
        let mut broker = ToolBroker::new(Arc::new(policy));
        broker.register(Arc::new(DummyTool));

        let req = ToolRequest { tool_name: "dummy".to_string(), arguments: json!({}) };
        let res = broker.execute(&req).await?;
        match res {
            BrokerResult::Denied(reason) => {
                assert!(reason.contains("deny") || reason.contains("Deny"))
            }
            other => return Err(format!("expected Denied, got {other:?}").into()),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_require_confirmation_policy() -> Result<(), Box<dyn std::error::Error>> {
        let rule = PolicyRule {
            name: "confirm-dummy".to_string(),
            action_pattern: "dummy.execute".to_string(),
            decision: "require_confirmation".to_string(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Basic);
        let mut broker = ToolBroker::new(Arc::new(policy));
        broker.register(Arc::new(DummyTool));

        let req = ToolRequest { tool_name: "dummy".to_string(), arguments: json!({}) };
        let res = broker.execute(&req).await?;
        match res {
            BrokerResult::RequiresConfirmation(reason) => assert!(!reason.is_empty()),
            other => return Err(format!("expected RequiresConfirmation, got {other:?}").into()),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_unknown_tool() -> Result<(), Box<dyn std::error::Error>> {
        let rule = PolicyRule {
            name: "allow-all".to_string(),
            action_pattern: "*".to_string(),
            decision: "allow".to_string(),
            trust_tier: 0,
            description: None,
        };
        let policy = PolicyEngine::new(vec![rule], TrustTier::Basic);
        let mut broker = ToolBroker::new(Arc::new(policy));
        broker.register(Arc::new(DummyTool));

        let req = ToolRequest { tool_name: "nonexistent".to_string(), arguments: json!({}) };
        let Err(err) = broker.execute(&req).await else {
            return Err("expected the broker to fail".into());
        };
        match err {
            ToolError::NotFound { name } => assert_eq!(name, "nonexistent"),
            other => return Err(format!("expected NotFound, got {other:?}").into()),
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_broker_available_tools() {
        let policy = PolicyEngine::deny_all();
        let mut broker = ToolBroker::new(Arc::new(policy));
        broker.register(Arc::new(DummyTool));

        struct AnotherTool;
        #[async_trait]
        impl ToolExecutor for AnotherTool {
            fn name(&self) -> &str {
                "another"
            }
            fn description(&self) -> &str {
                "another tool"
            }
            fn is_destructive(&self) -> bool {
                false
            }
            async fn execute(&self, _req: &ToolRequest) -> Result<ToolResult, ToolError> {
                Ok(ToolResult { success: true, output: "ok".into(), data: None })
            }
        }

        broker.register(Arc::new(AnotherTool));
        let tools = broker.available_tools();
        assert_eq!(tools.len(), 2);
        assert!(tools.contains(&"dummy".to_string()));
        assert!(tools.contains(&"another".to_string()));
    }

    #[tokio::test]
    async fn test_broker_no_registered_tools() {
        let policy = PolicyEngine::deny_all();
        let broker = ToolBroker::new(Arc::new(policy));
        assert!(broker.available_tools().is_empty());
    }

    #[tokio::test]
    async fn test_broker_deny_all_engine() -> Result<(), Box<dyn std::error::Error>> {
        let policy = PolicyEngine::deny_all();
        let mut broker = ToolBroker::new(Arc::new(policy));
        broker.register(Arc::new(DummyTool));

        let req = ToolRequest { tool_name: "dummy".to_string(), arguments: json!({}) };
        let res = broker.execute(&req).await?;
        match res {
            BrokerResult::Denied(_) => {}
            other => {
                return Err(format!("expected Denied with deny_all policy, got {other:?}").into())
            }
        }
        Ok(())
    }
}
