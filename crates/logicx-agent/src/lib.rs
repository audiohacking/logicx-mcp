mod agent;
mod connection;
mod ollama;

pub use agent::run_agent;
pub use connection::{check_ollama_connection, check_ollama_connection_with_events};
pub use ollama::OllamaClient;
pub use logicx_core::OllamaConnectionReport;
