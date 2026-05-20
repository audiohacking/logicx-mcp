use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::System,
            content: content.into(),
            tool_name: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            content: content.into(),
            tool_name: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            content: content.into(),
            tool_name: None,
        }
    }

    pub fn tool(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Tool,
            content: content.into(),
            tool_name: Some(name.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HonestResult {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

impl HonestResult {
    pub fn confirmed(detail: impl Into<String>) -> Self {
        Self {
            success: true,
            verified: Some(true),
            reason: None,
            error: None,
            detail: Some(serde_json::Value::String(detail.into())),
        }
    }

    pub fn uncertain(reason: impl Into<String>) -> Self {
        Self {
            success: true,
            verified: Some(false),
            reason: Some(reason.into()),
            error: None,
            detail: None,
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            success: false,
            verified: Some(false),
            reason: None,
            error: Some(error.into()),
            detail: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentSettings {
    pub ollama_base_url: String,
    pub model: String,
    pub max_tool_rounds: u32,
}

impl AgentSettings {
    pub fn default_local() -> Self {
        Self {
            ollama_base_url: "http://127.0.0.1:11434".into(),
            model: "qwen3.5".into(),
            max_tool_rounds: 12,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiAgentEvent {
    Status {
        text: String,
    },
    Assistant {
        content: String,
    },
    ToolStarted {
        name: String,
        arguments: serde_json::Value,
    },
    ToolFinished {
        name: String,
        result: String,
    },
    Error {
        message: String,
    },
    Done,
}
