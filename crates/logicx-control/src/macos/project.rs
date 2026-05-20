use super::{is_logic_running, run_osascript_output};
use logicx_core::session::targets_current_logic_project;

/// The project LogicX MCP should edit right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTarget {
    /// AU / delegated bridge — always the front Logic project.
    CurrentLogicProject,
    /// Standalone control with Logic running.
    LogicFrontProject,
    /// Logic not running.
    None,
}

pub fn project_target() -> ProjectTarget {
    if targets_current_logic_project() {
        return ProjectTarget::CurrentLogicProject;
    }
    if !is_logic_running() {
        return ProjectTarget::None;
    }
    if applescript_has_document() {
        ProjectTarget::LogicFrontProject
    } else {
        // Logic is running; treat as current project even if AppleScript is unavailable
        // (companion app may lack Automation → Logic Pro while AX still works).
        ProjectTarget::LogicFrontProject
    }
}

pub fn has_open_project() -> bool {
    !matches!(project_target(), ProjectTarget::None)
}

pub fn front_project_name() -> Option<String> {
    if matches!(project_target(), ProjectTarget::None) {
        return None;
    }
    applescript_front_document_name().or_else(|| {
        if targets_current_logic_project() {
            Some("current project".into())
        } else {
            Some("Logic Pro project".into())
        }
    })
}

fn applescript_has_document() -> bool {
    matches!(
        run_osascript_output(
            r#"tell application "Logic Pro"
    if not running then return "not_running"
    if (count of documents) > 0 then return "yes"
    return "no"
end tell"#
        ),
        Ok(out) if out.status == "yes"
    )
}

fn applescript_front_document_name() -> Option<String> {
    run_osascript_output(
        r#"tell application "Logic Pro"
    if (count of documents) > 0 then return name of front document
    return ""
end tell"#,
    )
    .ok()
    .map(|o| o.status)
    .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use logicx_core::session::{set_in_logic_plugin_session, targets_current_logic_project};

    #[test]
    fn plugin_session_targets_current_project() {
        set_in_logic_plugin_session(true);
        assert!(targets_current_logic_project());
        assert!(has_open_project());
        set_in_logic_plugin_session(false);
    }
}
