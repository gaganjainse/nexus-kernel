use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum RpcId {
    Num(i64),
    Str(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    pub id: Option<RpcId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
    pub id: Option<RpcId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_rpc_request() -> Result<(), Box<dyn std::error::Error>> {
        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            method: "ping".into(),
            params: None,
            id: Some(RpcId::Str("1".into())),
        };
        assert_eq!(req.method, "ping");
        Ok(())
    }

    #[test]
    fn test_rpc_request_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            method: "subtract".into(),
            params: Some(json!({"minuend": 23, "subtrahend": 42})),
            id: Some(RpcId::Str("3".into())),
        };
        let json_str = serde_json::to_string(&req)?;
        let decoded: RpcRequest = serde_json::from_str(&json_str)?;
        assert_eq!(decoded.jsonrpc, "2.0");
        assert_eq!(decoded.method, "subtract");
        assert_eq!(decoded.params, Some(json!({"minuend": 23, "subtrahend": 42})));
        assert_eq!(decoded.id, Some(RpcId::Str("3".into())));
        Ok(())
    }

    #[test]
    fn test_rpc_request_empty_method() -> Result<(), Box<dyn std::error::Error>> {
        let req = RpcRequest { jsonrpc: "2.0".into(), method: "".into(), params: None, id: None };
        assert!(req.method.is_empty());
        assert!(req.id.is_none());
        Ok(())
    }

    #[test]
    fn test_rpc_request_none_params_and_id() -> Result<(), Box<dyn std::error::Error>> {
        let req =
            RpcRequest { jsonrpc: "2.0".into(), method: "update".into(), params: None, id: None };
        assert!(req.params.is_none());
        assert!(req.id.is_none());
        Ok(())
    }

    #[test]
    fn test_rpc_request_clone() -> Result<(), Box<dyn std::error::Error>> {
        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            method: "clone_test".into(),
            params: Some(json!([1, 2, 3])),
            id: Some(RpcId::Str("99".into())),
        };
        let cloned = req.clone();
        assert_eq!(cloned.method, req.method);
        assert_eq!(cloned.params, req.params);
        assert_eq!(cloned.id, req.id);
        Ok(())
    }

    #[test]
    fn test_rpc_request_with_array_params() -> Result<(), Box<dyn std::error::Error>> {
        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            method: "sum".into(),
            params: Some(json!([1, 2, 3])),
            id: Some(RpcId::Num(1)),
        };
        let params = req.params.as_ref().ok_or("params should be Some")?;
        assert!(params.is_array());
        assert_eq!(params.as_array().ok_or("params should be array")?.len(), 3);
        Ok(())
    }

    #[test]
    fn test_rpc_request_integer_id() -> Result<(), Box<dyn std::error::Error>> {
        let req = RpcRequest {
            jsonrpc: "2.0".into(),
            method: "ping".into(),
            params: None,
            id: Some(RpcId::Num(42)),
        };
        let json_str = serde_json::to_string(&req)?;
        let decoded: RpcRequest = serde_json::from_str(&json_str)?;
        assert_eq!(decoded.id, Some(RpcId::Num(42)));
        Ok(())
    }

    #[test]
    fn test_rpc_response_with_result() -> Result<(), Box<dyn std::error::Error>> {
        let resp = RpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(json!("success")),
            error: None,
            id: Some(RpcId::Str("1".into())),
        };
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        assert_eq!(resp.result.ok_or("result should be Some")?, json!("success"));
        Ok(())
    }

    #[test]
    fn test_rpc_response_with_error() -> Result<(), Box<dyn std::error::Error>> {
        let resp = RpcResponse {
            jsonrpc: "2.0".into(),
            result: None,
            error: Some(RpcError { code: -32601, message: "Method not found".into() }),
            id: Some(RpcId::Str("1".into())),
        };
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        assert_eq!(resp.error.ok_or("error should be Some")?.code, -32601);
        Ok(())
    }

    #[test]
    fn test_rpc_response_both_result_and_error() -> Result<(), Box<dyn std::error::Error>> {
        let resp = RpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(json!("data")),
            error: Some(RpcError { code: 0, message: "ok".into() }),
            id: Some(RpcId::Str("1".into())),
        };
        assert!(resp.result.is_some());
        assert!(resp.error.is_some());
        Ok(())
    }

    #[test]
    fn test_rpc_response_none_id() -> Result<(), Box<dyn std::error::Error>> {
        let resp =
            RpcResponse { jsonrpc: "2.0".into(), result: Some(json!(null)), error: None, id: None };
        assert!(resp.id.is_none());
        Ok(())
    }

    #[test]
    fn test_rpc_response_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let resp = RpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(json!({"key": "value"})),
            error: None,
            id: Some(RpcId::Str("req-1".into())),
        };
        let json_str = serde_json::to_string(&resp)?;
        let decoded: RpcResponse = serde_json::from_str(&json_str)?;
        assert_eq!(decoded.jsonrpc, "2.0");
        assert_eq!(decoded.result, Some(json!({"key": "value"})));
        assert_eq!(decoded.id, Some(RpcId::Str("req-1".into())));
        Ok(())
    }

    #[test]
    fn test_rpc_error_various_codes() -> Result<(), Box<dyn std::error::Error>> {
        let parse_error = RpcError { code: -32700, message: "Parse error".into() };
        assert_eq!(parse_error.code, -32700);

        let invalid_request = RpcError { code: -32600, message: "Invalid Request".into() };
        assert_eq!(invalid_request.code, -32600);

        let method_not_found = RpcError { code: -32601, message: "Method not found".into() };
        assert_eq!(method_not_found.code, -32601);

        let internal_error = RpcError { code: -32603, message: "Internal error".into() };
        assert_eq!(internal_error.code, -32603);
        Ok(())
    }

    #[test]
    fn test_rpc_error_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let err = RpcError { code: -32000, message: "Server error".into() };
        let json_str = serde_json::to_string(&err)?;
        let decoded: RpcError = serde_json::from_str(&json_str)?;
        assert_eq!(decoded.code, -32000);
        assert_eq!(decoded.message, "Server error");
        Ok(())
    }

    #[test]
    fn test_rpc_error_clone() -> Result<(), Box<dyn std::error::Error>> {
        let err = RpcError { code: 1, message: "test".into() };
        let cloned = err.clone();
        assert_eq!(cloned.code, err.code);
        assert_eq!(cloned.message, err.message);
        Ok(())
    }

    #[test]
    fn test_rpc_response_clone() -> Result<(), Box<dyn std::error::Error>> {
        let resp = RpcResponse {
            jsonrpc: "2.0".into(),
            result: Some(json!(true)),
            error: None,
            id: Some(RpcId::Str("1".into())),
        };
        let cloned = resp.clone();
        assert_eq!(cloned.jsonrpc, resp.jsonrpc);
        assert_eq!(cloned.result, resp.result);
        Ok(())
    }
}
