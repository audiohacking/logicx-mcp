use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
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

#[derive(Clone)]
pub struct OllamaClient {
    base_url: String,
    model: String,
    http: Client,
}

impl OllamaClient {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .expect("reqwest client");
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            model: model.into(),
            http,
        }
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

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .map_err(|e| OllamaError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            return Err(OllamaError::Api(format!("{status}: {text}")));
        }

        resp.json::<ChatResponse>()
            .map_err(|e| OllamaError::Parse(e.to_string()))
    }

    pub fn list_models(&self) -> Result<Vec<String>, OllamaError> {
        let url = format!("{}/api/tags", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .map_err(|e| OllamaError::Http(e.to_string()))?;
        let body: TagsResponse = resp.json().map_err(|e| OllamaError::Parse(e.to_string()))?;
        Ok(body.models.into_iter().map(|m| m.name).collect())
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
