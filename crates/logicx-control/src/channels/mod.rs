mod applescript;
mod ax;
pub mod cgevent;
mod coremidi;
pub mod keycmd;
mod mcu;
pub mod router;
pub mod scripter;

pub use cgevent::CgEventChannel;

pub use ax::AxChannel;
pub use router::{ChannelRouter, channel_result_to_honest, operation_for_tool};

use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelId {
    Accessibility,
    Mcu,
    CoreMidi,
    MidiKeyCommands,
    CgEvent,
    AppleScript,
    Scripter,
}

impl ChannelId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accessibility => "accessibility",
            Self::Mcu => "mcu",
            Self::CoreMidi => "core_midi",
            Self::MidiKeyCommands => "midi_key_commands",
            Self::CgEvent => "cg_event",
            Self::AppleScript => "applescript",
            Self::Scripter => "scripter",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChannelHealth {
    pub available: bool,
    pub ready: bool,
    pub detail: String,
}

impl ChannelHealth {
    pub fn healthy(detail: impl Into<String>) -> Self {
        Self {
            available: true,
            ready: true,
            detail: detail.into(),
        }
    }

    pub fn unavailable(detail: impl Into<String>) -> Self {
        Self {
            available: false,
            ready: false,
            detail: detail.into(),
        }
    }

    /// Port published but awaiting one-time operator validation (e.g. MIDI Learn).
    pub fn manual_validation_required(detail: impl Into<String>) -> Self {
        Self {
            available: true,
            ready: false,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ChannelResult {
    Success {
        message: String,
        verified: Option<bool>,
        reason: Option<String>,
        detail: Option<Value>,
    },
    Error(String),
}

impl ChannelResult {
    pub fn ok(msg: impl Into<String>) -> Self {
        Self::Success {
            message: msg.into(),
            verified: Some(false),
            reason: Some("readback_unavailable".into()),
            detail: None,
        }
    }

    pub fn ok_verified(detail: Value) -> Self {
        Self::Success {
            message: "ok".into(),
            verified: Some(true),
            reason: None,
            detail: Some(detail),
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self::Error(msg.into())
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
}

pub type Params = HashMap<String, String>;

pub fn json_params(params: &Value) -> Params {
    let mut map = Params::new();
    let Some(obj) = params.as_object() else {
        return map;
    };
    for (k, v) in obj {
        match v {
            Value::String(s) => {
                map.insert(k.clone(), s.clone());
            }
            Value::Number(n) => {
                map.insert(k.clone(), n.to_string());
            }
            Value::Bool(b) => {
                map.insert(k.clone(), b.to_string());
            }
            _ => {}
        }
    }
    map
}

pub fn param_f64(params: &Value, keys: &[&str], default: f64) -> f64 {
    for key in keys {
        if let Some(v) = params.get(*key).and_then(|v| v.as_f64()) {
            return v;
        }
    }
    default
}

pub fn param_u64(params: &Value, key: &str) -> Option<u64> {
    params.get(key).and_then(|v| v.as_u64())
}

pub fn param_str<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(|v| v.as_str())
}

pub fn req_param<'a>(params: &'a Params, key: &str) -> Result<&'a String, ChannelResult> {
    params
        .get(key)
        .ok_or_else(|| ChannelResult::err(format!("requires {key}")))
}

pub fn req_param_err<'a>(
    params: &'a Params,
    key: &str,
    msg: &str,
) -> Result<&'a String, ChannelResult> {
    params.get(key).ok_or_else(|| ChannelResult::err(msg))
}

pub fn normalize_params(operation: &str, mut params: Params) -> Params {
    if !params.contains_key("index") {
        if let Some(track) = params.remove("track") {
            params.insert("index".into(), track);
        }
    }
    if matches!(
        operation,
        "track.set_mute" | "track.set_solo" | "track.set_arm"
    ) && !params.contains_key("enabled")
    {
        if let Some(v) = params.remove("value") {
            params.insert("enabled".into(), v);
        }
    }
    if operation == "transport.set_tempo" {
        if let Some(t) = params.remove("tempo") {
            params.insert("bpm".into(), t);
        }
    }
    if operation == "transport.goto_position" {
        if let Some(bar) = params.remove("bar") {
            params.insert("position".into(), format!("{bar}.1.1.1"));
        }
    }
    params
}

pub fn param_bool(params: &Value, key: &str, default: bool) -> bool {
    params
        .get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

#[cfg(test)]
mod normalize_tests {
    use super::*;

    #[test]
    fn track_alias_becomes_index() {
        let p = Params::from([("track".to_string(), "3".to_string())]);
        let out = normalize_params("track.set_mute", p);
        assert_eq!(out.get("index").map(String::as_str), Some("3"));
        assert!(out.get("track").is_none());
    }

    #[test]
    fn mute_value_alias_becomes_enabled() {
        let mut p = Params::from([("value".to_string(), "true".to_string())]);
        let out = normalize_params("track.set_mute", p);
        assert_eq!(out.get("enabled").map(String::as_str), Some("true"));
    }

    #[test]
    fn tempo_alias_becomes_bpm() {
        let p = Params::from([("tempo".to_string(), "128.5".to_string())]);
        let out = normalize_params("transport.set_tempo", p);
        assert_eq!(out.get("bpm").map(String::as_str), Some("128.5"));
    }

    #[test]
    fn goto_bar_becomes_position() {
        let p = Params::from([("bar".to_string(), "9".to_string())]);
        let out = normalize_params("transport.goto_position", p);
        assert_eq!(out.get("position").map(String::as_str), Some("9.1.1.1"));
    }
}
