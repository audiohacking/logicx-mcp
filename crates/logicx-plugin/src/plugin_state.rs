use logicx_core::{AgentSettings, ChatMessage};

/// Persistent plugin state: chat history + Ollama settings.
#[derive(truce::State, Clone)]
pub struct PluginState {
    pub ollama_base_url: String,
    pub model: String,
    pub status_line: String,
    pub busy: bool,
    /// JSON-serialized `Vec<ChatMessage>` — State derive supports String fields.
    pub messages_json: String,
    /// `connected` | `disconnected` | `checking`
    pub ollama_status: String,
    pub connection_summary: String,
    pub build_id: String,
}

impl Default for PluginState {
    fn default() -> Self {
        let defaults = AgentSettings::default_local();
        Self {
            ollama_base_url: defaults.ollama_base_url,
            model: defaults.model,
            status_line: String::new(),
            busy: false,
            messages_json: "[]".into(),
            ollama_status: "checking".into(),
            connection_summary: "Checking Ollama…".into(),
            build_id: crate::BUILD_ID.to_string(),
        }
    }
}

impl PluginState {
    pub fn messages(&self) -> Vec<ChatMessage> {
        serde_json::from_str(&self.messages_json).unwrap_or_default()
    }

    fn save_messages(&mut self, messages: &[ChatMessage]) {
        self.messages_json = serde_json::to_string(messages).unwrap_or_else(|_| "[]".to_string());
    }

    pub fn settings(&self) -> AgentSettings {
        AgentSettings {
            ollama_base_url: if self.ollama_base_url.is_empty() {
                AgentSettings::default_local().ollama_base_url
            } else {
                self.ollama_base_url.clone()
            },
            model: if self.model.is_empty() {
                AgentSettings::default_local().model
            } else {
                self.model.clone()
            },
            max_tool_rounds: 12,
        }
    }

    pub fn push_user(&mut self, text: String) {
        let mut m = self.messages();
        m.push(ChatMessage::user(text));
        self.save_messages(&m);
    }

    pub fn push_assistant(&mut self, text: String) {
        let mut m = self.messages();
        m.push(ChatMessage::assistant(text));
        self.save_messages(&m);
    }

    pub fn push_tool_note(&mut self, name: String, summary: String) {
        let mut m = self.messages();
        m.push(ChatMessage::tool(name, summary));
        self.save_messages(&m);
    }

    pub fn clear_chat(&mut self) {
        self.messages_json = "[]".into();
        self.status_line.clear();
    }
}
