use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OllamaError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("Ollama: {0}")]
    Api(String),
}

/// HTTP client for Ollama — identical code path in standalone and AU plugin.
///
/// macOS note: an AU is a dylib inside Logic Pro's process, not a separate app.
/// We use `/usr/bin/curl` (same as standalone). If Logic blocks network from
/// plugins you will see "Operation not permitted" in the debug log; the fix is
/// to run `LogicX MCP.app` alongside Logic (companion mode — coming next).
#[derive(Clone)]
pub struct OllamaClient {
    base_url: String,
    model: String,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn chat(&self, request: ChatRequest) -> Result<ChatResponse, OllamaError> {
        let url = format!("{}/api/chat", self.base_url);
        let body = ChatRequestBody {
            model: self.model.clone(),
            messages: request.messages,
            tools: request.tools,
            stream: false,
            options: Some(ChatOptions {
                temperature: request.temperature.unwrap_or(0.2),
                num_predict: request.max_tokens,
            }),
        };
        let json = serde_json::to_string(&body).map_err(|e| OllamaError::Parse(e.to_string()))?;
        let text = http_post(&url, &json)?;
        if let Ok(err) = serde_json::from_str::<OllamaErrorBody>(&text)
            && let Some(msg) = err.error
        {
            return Err(OllamaError::Api(msg));
        }
        serde_json::from_str(&text).map_err(|e| OllamaError::Parse(format!("{e}: {text}")))
    }

    pub fn list_models(&self) -> Result<Vec<String>, OllamaError> {
        let url = format!("{}/api/tags", self.base_url);
        let text = http_get(&url)?;
        let body: TagsResponse =
            serde_json::from_str(&text).map_err(|e| OllamaError::Parse(e.to_string()))?;
        Ok(body.models.into_iter().map(|m| m.name).collect())
    }

    pub fn ping(&self) -> Result<(), OllamaError> {
        http_get(&format!("{}/api/tags", self.base_url)).map(|_| ())
    }
}

pub struct ChatRequest {
    pub messages: Vec<OllamaMessage>,
    pub tools: Option<Vec<Value>>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OllamaToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaToolCall {
    pub function: OllamaFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaFunctionCall {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub message: OllamaMessage,
    #[serde(default)]
    pub done: bool,
}

#[derive(Serialize)]
struct ChatRequestBody {
    model: String,
    messages: Vec<OllamaMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<ChatOptions>,
}

#[derive(Serialize)]
struct ChatOptions {
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

#[derive(Deserialize)]
struct TagsResponse {
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
}

#[derive(Deserialize)]
struct OllamaErrorBody {
    error: Option<String>,
}

pub fn http_get(url: &str) -> Result<String, OllamaError> {
    eprintln!("[LogicX MCP] GET {url}");
    let output = Command::new("/usr/bin/curl")
        .args(["-sfS", "--max-time", "30", url])
        .output()
        .map_err(|e| OllamaError::Http(format!("curl failed to run: {e}")))?;

    if !output.status.success() {
        return Err(curl_failure(&output, url));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn http_post(url: &str, json_body: &str) -> Result<String, OllamaError> {
    eprintln!("[LogicX MCP] POST {url} ({} bytes)", json_body.len());
    let mut child = Command::new("/usr/bin/curl")
        .args([
            "-sfS",
            "--max-time",
            "300",
            "-X",
            "POST",
            "-H",
            "Content-Type: application/json",
            "-d",
            "@-",
            url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| OllamaError::Http(format!("curl failed to run: {e}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(json_body.as_bytes())
            .map_err(|e| OllamaError::Http(e.to_string()))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| OllamaError::Http(e.to_string()))?;

    if !output.status.success() {
        return Err(curl_failure(&output, url));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn curl_failure(output: &std::process::Output, url: &str) -> OllamaError {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let code = output.status.code().unwrap_or(-1);

    let hint = if stderr.contains("Connection refused") || stderr.contains("Failed to connect") {
        format!(" — is Ollama running at {url}? Try: ollama serve")
    } else if stderr.contains("Operation not permitted") || stderr.contains("access denied") {
        " — BLOCKER: Logic Pro blocked network from this plugin. \
         AU plugins run inside Logic's process and cannot open sockets like a standalone app. \
         Workaround: run LogicX MCP.app alongside Logic (companion mode)."
            .to_string()
    } else if stderr.contains("Could not resolve host") {
        " — check Ollama URL in settings (⚙)".to_string()
    } else {
        String::new()
    };

    OllamaError::Http(format!("curl exit {code}: {stderr}{hint}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_base_url_trailing_slash() {
        let c = OllamaClient::new("http://127.0.0.1:11434/", "qwen3.5");
        assert_eq!(c.base_url, "http://127.0.0.1:11434");
    }
}
