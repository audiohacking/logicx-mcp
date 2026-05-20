//! Unix-socket sidecar protocol for Ollama HTTP proxy (AU network workaround).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub fn support_dir() -> PathBuf {
    crate::runtime::support_dir()
}

pub fn socket_path() -> PathBuf {
    support_dir().join("ollama-proxy.sock")
}

pub fn pid_path() -> PathBuf {
    support_dir().join("ollama-proxy.pid")
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ProxyRequest {
    Ping,
    HttpGet { url: String },
    HttpPost { url: String, body: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ProxyResponse {
    pub fn success(body: impl Into<String>) -> Self {
        Self {
            ok: true,
            body: Some(body.into()),
            error: None,
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            body: None,
            error: Some(error.into()),
        }
    }
}
