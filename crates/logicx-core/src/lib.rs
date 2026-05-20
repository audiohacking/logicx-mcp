pub mod honest_contract;
pub mod connection;
pub mod control_bridge;
pub mod diagnostic_log;
pub mod ollama_proxy;
pub mod runtime;
pub mod session;
pub mod prompt;
pub mod tools;
pub mod types;

pub use honest_contract::{
    add_extras, encode_state_a, encode_state_b, encode_state_c, is_terminal_state_c, json_string,
    HonestError, HonestReason,
};
pub use connection::OllamaConnectionReport;
pub use prompt::SYSTEM_PROMPT;
pub use session::{
    blocks_project_lifecycle, in_logic_plugin_session, set_in_logic_plugin_session,
    targets_current_logic_project,
};
pub use tools::ollama_tool_definitions;
pub use types::*;
