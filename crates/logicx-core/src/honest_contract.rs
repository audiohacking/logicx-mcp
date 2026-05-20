//! Honest Contract envelope helpers (logic-pro-mcp parity).

use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HonestReason {
    ReadbackUnavailable,
    EchoTimeoutMs(u32),
    RetryExhausted,
}

impl HonestReason {
    pub fn as_str(self) -> String {
        match self {
            Self::ReadbackUnavailable => "readback_unavailable".into(),
            Self::EchoTimeoutMs(ms) => format!("echo_timeout_{ms}ms"),
            Self::RetryExhausted => "retry_exhausted".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HonestError {
    AxWriteFailed,
    ElementNotFound,
    InvalidParams,
    PermissionRequired,
    PortUnavailable,
}

impl HonestError {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AxWriteFailed => "ax_write_failed",
            Self::ElementNotFound => "element_not_found",
            Self::InvalidParams => "invalid_params",
            Self::PermissionRequired => "permission_required",
            Self::PortUnavailable => "port_unavailable",
        }
    }
}

pub fn encode_state_a(extras: Map<String, Value>) -> String {
    let mut obj = Map::new();
    obj.insert("success".into(), Value::Bool(true));
    obj.insert("verified".into(), Value::Bool(true));
    for (k, v) in extras {
        obj.insert(k, v);
    }
    json_string(&obj)
}

pub fn encode_state_b(reason: HonestReason, extras: Map<String, Value>) -> String {
    let mut obj = Map::new();
    obj.insert("success".into(), Value::Bool(true));
    obj.insert("verified".into(), Value::Bool(false));
    obj.insert("reason".into(), Value::String(reason.as_str()));
    for (k, v) in extras {
        obj.insert(k, v);
    }
    json_string(&obj)
}

pub fn encode_state_c(
    error: HonestError,
    ax_code: Option<i32>,
    hint: Option<&str>,
    extras: Map<String, Value>,
) -> String {
    let mut obj = Map::new();
    obj.insert("success".into(), Value::Bool(false));
    obj.insert("error".into(), Value::String(error.as_str().into()));
    if let Some(code) = ax_code {
        obj.insert("axCode".into(), Value::Number(code.into()));
    }
    if let Some(h) = hint {
        obj.insert("hint".into(), Value::String(h.into()));
    }
    for (k, v) in extras {
        obj.insert(k, v);
    }
    json_string(&obj)
}

/// Merge caller extras into State A/B JSON at top level. State C and invalid JSON pass through.
pub fn add_extras(extras: Map<String, Value>, into: &str) -> String {
    let Ok(mut obj) = serde_json::from_str::<Map<String, Value>>(into) else {
        return into.to_string();
    };
    if obj.get("success") == Some(&Value::Bool(false)) {
        return into.to_string();
    }
    for (k, v) in extras {
        obj.insert(k, v);
    }
    json_string(&obj)
}

pub fn json_string(obj: &Map<String, Value>) -> String {
    let mut keys: Vec<_> = obj.keys().cloned().collect();
    keys.sort();
    let mut sorted = Map::new();
    for k in keys {
        if let Some(v) = obj.get(&k) {
            sorted.insert(k, v.clone());
        }
    }
    serde_json::to_string(&sorted).unwrap_or_else(|_| "{}".into())
}

pub fn is_terminal_state_c(envelope: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(envelope) else {
        return false;
    };
    if v.get("success") != Some(&Value::Bool(false)) {
        return false;
    }
    let Some(err) = v.get("error").and_then(|e| e.as_str()) else {
        return false;
    };
    matches!(
        err,
        "element_not_found"
            | "invalid_params"
            | "permission_required"
            | "port_unavailable"
            | "requires explicit"
    ) || err.contains("requires explicit")
        || err.contains("element_not_found")
        || err.contains("not_implemented")
        || err.contains("confirmation_required")
        || err.contains("blocked_in_logic_plugin")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn state_a_shape() {
        let mut extras = Map::new();
        extras.insert("requested".into(), json!("Piano"));
        extras.insert("observed".into(), json!("Piano"));
        let raw = encode_state_a(extras);
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["success"], true);
        assert_eq!(v["verified"], true);
        assert!(v.get("reason").is_none() || v["reason"].is_null());
        assert!(v.get("error").is_none() || v["error"].is_null());
    }

    #[test]
    fn state_b_echo_timeout() {
        let mut extras = Map::new();
        extras.insert("requested".into(), json!(0.8));
        let raw = encode_state_b(HonestReason::EchoTimeoutMs(500), extras);
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["reason"], "echo_timeout_500ms");
        assert_eq!(v["verified"], false);
    }

    #[test]
    fn state_b_readback_and_retry() {
        let raw = encode_state_b(HonestReason::ReadbackUnavailable, Map::new());
        assert!(raw.contains("readback_unavailable"));
        let raw2 = encode_state_b(HonestReason::RetryExhausted, Map::new());
        assert!(raw2.contains("retry_exhausted"));
    }

    #[test]
    fn state_c_with_ax_code() {
        let mut extras = Map::new();
        extras.insert("requested".into(), json!(7));
        let raw = encode_state_c(
            HonestError::AxWriteFailed,
            Some(-25212),
            Some("permission?"),
            extras,
        );
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["error"], "ax_write_failed");
        assert_eq!(v["axCode"], -25212);
        assert_eq!(v["hint"], "permission?");
        assert!(v.get("verified").is_none() || v["verified"].is_null());
    }

    #[test]
    fn state_c_element_not_found_minimal() {
        let raw = encode_state_c(HonestError::ElementNotFound, None, None, Map::new());
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["error"], "element_not_found");
        assert!(v.get("axCode").is_none() || v["axCode"].is_null());
    }

    #[test]
    fn json_sorted_deterministic() {
        let mut a_map = Map::new();
        a_map.insert("b".into(), json!(1));
        a_map.insert("a".into(), json!(2));
        let mut b_map = Map::new();
        b_map.insert("a".into(), json!(2));
        b_map.insert("b".into(), json!(1));
        assert_eq!(json_string(&a_map), json_string(&b_map));
        assert!(json_string(&a_map).starts_with("{\"a\":"));
    }

    #[test]
    fn add_extras_merges_state_a() {
        let mut base = Map::new();
        base.insert("requested".into(), json!("1.1.1.1"));
        let raw = encode_state_a(base);
        let mut caller = Map::new();
        caller.insert("caller_flag".into(), json!(true));
        let merged = add_extras(caller, &raw);
        let v: Value = serde_json::from_str(&merged).unwrap();
        assert_eq!(v["caller_flag"], true);
        assert_eq!(v["requested"], "1.1.1.1");
    }

    #[test]
    fn add_extras_skips_state_c() {
        let raw = encode_state_c(HonestError::AxWriteFailed, None, Some("permission?"), Map::new());
        let mut caller = Map::new();
        caller.insert("caller_flag".into(), json!(true));
        let merged = add_extras(caller, &raw);
        assert_eq!(merged, raw);
    }

    #[test]
    fn add_extras_invalid_json_passthrough() {
        assert_eq!(
            add_extras(Map::new(), "not json"),
            "not json"
        );
    }

    #[test]
    fn terminal_state_c_port_unavailable() {
        let raw = encode_state_c(
            HonestError::PortUnavailable,
            None,
            Some("KeyCmd port not yet published"),
            Map::new(),
        );
        assert!(is_terminal_state_c(&raw));
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["error"], "port_unavailable");
    }

    #[test]
    fn terminal_state_c_detection() {
        let raw = encode_state_c(HonestError::ElementNotFound, None, None, Map::new());
        assert!(is_terminal_state_c(&raw));
        assert!(!is_terminal_state_c("ax_write_failed: focus stolen"));
    }
}
