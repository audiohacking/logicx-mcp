use serde::{Deserialize, Serialize};

/// Result of an Ollama connectivity probe (shown in plugin UI).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OllamaConnectionReport {
    pub connected: bool,
    /// Running inside Logic Pro (AU) vs standalone `.app`.
    pub in_daw: bool,
    pub url: String,
    pub model: String,
    pub host_exe: String,
    pub build_id: String,
    pub model_count: Option<usize>,
    pub error: Option<String>,
    pub debug: Vec<String>,
}

impl OllamaConnectionReport {
    pub fn summary(&self) -> String {
        if self.connected {
            let mode = if self.in_daw {
                "direct (in Logic)"
            } else {
                "direct"
            };
            let models = self
                .model_count
                .map(|n| format!(", {n} models"))
                .unwrap_or_default();
            format!("Ollama OK ({mode}{models})")
        } else {
            self.error
                .clone()
                .unwrap_or_else(|| "Ollama unreachable".into())
        }
    }
}
