use crate::macos::run_osascript_output;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

pub fn is_ax_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { AXIsProcessTrusted() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Probe Automation permission by asking Logic Pro for its name.
pub fn automation_logic_ok() -> bool {
    run_osascript_output(r#"tell application id "com.apple.logic10" to return name"#)
        .map(|o| o.status.contains("Logic"))
        .unwrap_or(false)
}

/// Trigger macOS Automation prompts when running inside `LogicX MCP.app`.
/// Bare `logicx-control-bridge` binaries never appear in Automation settings — only `.app` bundles do.
pub fn prime_automation_prompts() {
    if !logicx_core::runtime::running_in_app_bundle() {
        return;
    }
    if !automation_system_events_ok() {
        let _ = run_osascript_output(
            r#"tell application "System Events" to return "LogicX MCP""#,
        );
    }
    if !automation_logic_ok() {
        let _ = run_osascript_output(r#"tell application id "com.apple.logic10" to return name"#);
    }
}

/// Probe Automation permission for System Events (required for AX menu/keystroke scripts).
pub fn automation_system_events_ok() -> bool {
    run_osascript_output(
        r#"tell application "System Events" to return name of first process whose name is "Logic Pro""#,
    )
    .map(|o| o.status.contains("Logic"))
    .unwrap_or(false)
}

/// Back-compat alias — prefer checking both logic + system events explicitly.
pub fn automation_ok() -> bool {
    automation_logic_ok() && automation_system_events_ok()
}
