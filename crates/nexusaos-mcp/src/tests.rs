//! Unit tests for MCP capability checking.

use nexusaos_kernel::capability::{Capability, CapabilitySet, Scope};
use serde_json::json;

use crate::check_mcp_capabilities;

#[cfg(test)]
mod mcp_capability_tests {

    use super::*;

    #[test]
    fn test_no_tool_capability_denied() {
        let capabilities = CapabilitySet::new();
        let arguments = json!({ "path": "/tmp/test.txt" });
        assert!(!check_mcp_capabilities(&capabilities, "filesystem", &arguments));
    }

    #[test]
    fn test_empty_arguments_denied() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(
            Capability {
                name: "filesystem".to_string(),
                scope: Scope::Global,
                description: "filesystem access".to_string(),
            },
            "test".to_string(),
            None,
        );
        let arguments = json!({});
        assert!(!check_mcp_capabilities(&capabilities, "filesystem", &arguments));
    }

    #[test]
    fn test_path_scope_allows_matching_path() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(
            Capability {
                name: "filesystem".to_string(),
                scope: Scope::Path(std::path::PathBuf::from("/workspace")),
                description: "workspace access".to_string(),
            },
            "test".to_string(),
            None,
        );
        let arguments = json!({ "path": "/workspace/test.rs" });
        assert!(check_mcp_capabilities(&capabilities, "filesystem", &arguments));
    }

    #[test]
    fn test_path_scope_denies_non_matching_path() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(
            Capability {
                name: "filesystem".to_string(),
                scope: Scope::Path(std::path::PathBuf::from("/workspace")),
                description: "workspace access".to_string(),
            },
            "test".to_string(),
            None,
        );
        let arguments = json!({ "path": "/etc/passwd" });
        assert!(!check_mcp_capabilities(&capabilities, "filesystem", &arguments));
    }

    #[test]
    fn test_command_scope_allows_matching_command() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(
            Capability {
                name: "terminal".to_string(),
                scope: Scope::Command("echo".to_string()),
                description: "echo command".to_string(),
            },
            "test".to_string(),
            None,
        );
        let arguments = json!({ "command": "echo hello" });
        assert!(check_mcp_capabilities(&capabilities, "terminal", &arguments));
    }

    #[test]
    fn test_command_scope_denies_non_matching_command() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(
            Capability {
                name: "terminal".to_string(),
                scope: Scope::Command("echo".to_string()),
                description: "echo command".to_string(),
            },
            "test".to_string(),
            None,
        );
        let arguments = json!({ "command": "rm -rf /" });
        assert!(!check_mcp_capabilities(&capabilities, "terminal", &arguments));
    }

    #[test]
    fn test_global_scope_allows_any_path() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(
            Capability {
                name: "filesystem".to_string(),
                scope: Scope::Global,
                description: "global filesystem access".to_string(),
            },
            "test".to_string(),
            None,
        );
        let arguments = json!({ "path": "/etc/passwd" });
        assert!(check_mcp_capabilities(&capabilities, "filesystem", &arguments));
    }

    #[test]
    fn test_null_argument_value_denied() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(
            Capability {
                name: "filesystem".to_string(),
                scope: Scope::Path(std::path::PathBuf::from("/workspace")),
                description: "workspace access".to_string(),
            },
            "test".to_string(),
            None,
        );
        let arguments = json!({ "path": null });
        assert!(!check_mcp_capabilities(&capabilities, "filesystem", &arguments));
    }

    #[test]
    fn test_wrong_type_argument_denied() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(
            Capability {
                name: "filesystem".to_string(),
                scope: Scope::Path(std::path::PathBuf::from("/workspace")),
                description: "workspace access".to_string(),
            },
            "test".to_string(),
            None,
        );
        let arguments = json!({ "path": 123 });
        assert!(!check_mcp_capabilities(&capabilities, "filesystem", &arguments));
    }

    #[test]
    fn test_path_traversal_with_normalization() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(
            Capability {
                name: "filesystem".to_string(),
                scope: Scope::Path(std::path::PathBuf::from("/workspace")),
                description: "workspace access".to_string(),
            },
            "test".to_string(),
            None,
        );
        let arguments = json!({ "path": "/workspace/../../../etc/passwd" });
        // Lexical normalization resolves "/workspace/../../../etc/passwd" to "/etc/passwd",
        // which does NOT start with "/workspace", so this is correctly denied.
        assert!(!check_mcp_capabilities(&capabilities, "filesystem", &arguments));
    }

    #[test]
    fn test_capability_isolation() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(
            Capability {
                name: "filesystem".to_string(),
                scope: Scope::Path(std::path::PathBuf::from("/workspace")),
                description: "filesystem access".to_string(),
            },
            "test".to_string(),
            None,
        );
        let terminal_args = json!({ "command": "echo test" });
        assert!(!check_mcp_capabilities(&capabilities, "terminal", &terminal_args));

        let fs_args = json!({ "path": "/workspace/test.txt" });
        assert!(check_mcp_capabilities(&capabilities, "filesystem", &fs_args));
    }

    #[test]
    fn test_multiple_scopes_any_match() {
        let mut capabilities = CapabilitySet::new();
        capabilities.grant(
            Capability {
                name: "multi_tool".to_string(),
                scope: Scope::Path(std::path::PathBuf::from("/allowed")),
                description: "path access".to_string(),
            },
            "test".to_string(),
            None,
        );
        capabilities.grant(
            Capability {
                name: "multi_tool".to_string(),
                scope: Scope::Command("safe_cmd".to_string()),
                description: "command access".to_string(),
            },
            "test".to_string(),
            None,
        );

        let path_args = json!({ "path": "/allowed/file.txt" });
        assert!(check_mcp_capabilities(&capabilities, "multi_tool", &path_args));

        let cmd_args = json!({ "command": "safe_cmd arg1" });
        assert!(check_mcp_capabilities(&capabilities, "multi_tool", &cmd_args));
    }
}