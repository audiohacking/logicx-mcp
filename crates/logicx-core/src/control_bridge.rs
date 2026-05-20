//! Unix-socket IPC for delegating Logic control to a companion process (logic-pro-mcp model).
//!
//! AU plugins run in `AUHostingServiceXPC_*` where System Events automation fails.
//! The bridge runs as `LogicX MCP.app` with proper TCC grants.

use crate::ToolInvocation;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn support_dir() -> PathBuf {
    crate::runtime::support_dir()
}

pub fn socket_path() -> PathBuf {
    support_dir().join("control.sock")
}

pub fn pid_path() -> PathBuf {
    support_dir().join("control-bridge.pid")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BridgeContext {
    /// AU plugin delegated this request — always edit the front Logic project.
    #[serde(default)]
    pub in_logic_plugin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BridgeRequest {
    Ping,
    Execute {
        invocation: ToolInvocation,
        #[serde(default)]
        context: BridgeContext,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeResponse {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl BridgeResponse {
    pub fn success(result: String) -> Self {
        Self {
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: None,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeStatus {
    pub pong: bool,
    pub host_exe: String,
    pub permission_subject: String,
    pub running_in_app_bundle: bool,
    pub accessibility: bool,
    pub tempo_control_ready: bool,
}

impl BridgeStatus {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| r#"{"pong":true}"#.into())
    }

    pub fn parse(raw: &str) -> Option<Self> {
        serde_json::from_str(raw.trim()).ok()
    }
}
