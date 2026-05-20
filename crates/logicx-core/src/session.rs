//! Control session context — AU plugin vs standalone companion vs bridge.
//!
//! When LogicX MCP runs inside Logic (AU → bridge delegation), every operation
//! targets the **current front project**. Never infer "no project" from failed
//! AppleScript in the companion process.

use crate::runtime;
use std::cell::Cell;

thread_local! {
    static IN_LOGIC_PLUGIN: Cell<bool> = const { Cell::new(false) };
}

/// Set for the duration of a bridge `Execute` RPC when the AU plugin delegated control.
pub fn set_in_logic_plugin_session(active: bool) {
    IN_LOGIC_PLUGIN.with(|c| c.set(active));
}

pub fn in_logic_plugin_session() -> bool {
    IN_LOGIC_PLUGIN.with(|c| c.get())
}

/// True when control originates from or targets the Logic project hosting the plugin.
pub fn targets_current_logic_project() -> bool {
    runtime::hosted_in_daw() || in_logic_plugin_session()
}

/// Destructive project lifecycle ops must not run while embedded in Logic.
pub fn blocks_project_lifecycle(command: &str) -> bool {
    targets_current_logic_project()
        && matches!(command, "new" | "open" | "close" | "quit")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_blocked_only_for_destructive_commands_in_plugin_session() {
        set_in_logic_plugin_session(true);
        assert!(blocks_project_lifecycle("new"));
        assert!(blocks_project_lifecycle("open"));
        assert!(blocks_project_lifecycle("close"));
        assert!(blocks_project_lifecycle("quit"));
        assert!(!blocks_project_lifecycle("save"));
        assert!(!blocks_project_lifecycle("bounce"));
        set_in_logic_plugin_session(false);
        assert!(!blocks_project_lifecycle("new"));
    }
}
