mod ax_native;
mod ax_ops;
mod cg_input;
mod import;
mod permissions;
mod project;
mod transport;
mod tracks;

use logicx_core::HonestResult;
use std::process::Command;

pub use import::import_midi_file;
pub use permissions::{
    automation_logic_ok, automation_ok, automation_system_events_ok, is_ax_trusted,
    prime_automation_prompts,
};
pub use project::{front_project_name, has_open_project};
pub use transport::{read_transport_state, transport_goto_bar, transport_play, transport_set_tempo, transport_stop};
pub use tracks::{create_audio_track, create_instrument_track};

pub use ax_native::dialog_present;
pub use ax_ops::{
    arm_only, create_drummer_track, create_external_midi_track, delete_track, get_markers,
    get_regions, get_tracks, project_info, rename_marker, rename_track, resolve_library_path,
    run_ax_script, save_as_dialog, scan_library, scan_plugin_presets, select_track,
    set_cycle_range, set_instrument, set_track_toggle, verify_track_delta,
};

pub fn is_logic_running() -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-x", "Logic Pro"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn activate_logic() -> Result<(), String> {
    run_osascript(r#"tell application "Logic Pro" to activate"#)
}

/// Run AppleScript; returns stderr on failure.
pub fn run_osascript(script: &str) -> Result<(), String> {
    let output = run_osascript_output(script)?;
    if output.status.starts_with("OK") || output.status.is_empty() {
        Ok(())
    } else if output.status.starts_with("ERR:") {
        Err(output.status.trim_start_matches("ERR:").trim().to_string())
    } else {
        Ok(())
    }
}

pub struct ScriptOutput {
    pub status: String,
    pub stdout: String,
}

pub fn run_osascript_output(script: &str) -> Result<ScriptOutput, String> {
    if !is_logic_running() {
        return Err("Logic Pro is not running".into());
    }
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(ScriptOutput {
            status: stdout.clone(),
            stdout,
        })
    } else {
        Err(if stderr.is_empty() {
            stdout
        } else {
            format!("{stderr}; {stdout}")
        })
    }
}

pub fn is_permission_error(err: &str) -> bool {
    err.contains("assistive access")
        || err.contains("not allowed to send keystrokes")
        || err.contains("privilege violation")
        || err.contains("1002")
        || err.contains("-10004")
        || err.contains("not allowed assistive")
}

pub fn permission_failed(op: &str, err: String) -> HonestResult {
    let subject = logicx_core::runtime::permission_subject();
    HonestResult {
        success: false,
        verified: Some(false),
        reason: Some("permission_required".into()),
        error: Some(format!(
            "{op}: enable Accessibility for \"{subject}\" in System Settings → Privacy & Security → Accessibility"
        )),
        detail: Some(serde_json::json!({
            "osascript_error": err,
            "permission_subject": subject,
            "host_exe": logicx_core::runtime::host_executable(),
        })),
    }
}

pub fn map_script_error(op: &str, err: String) -> HonestResult {
    if is_permission_error(&err) {
        permission_failed(op, err)
    } else {
        HonestResult::failed(err)
    }
}

pub fn honest_from_script(
    op: &str,
    ok_detail: &str,
    script_result: Result<ScriptOutput, String>,
) -> HonestResult {
    match script_result {
        Ok(out) => HonestResult {
            success: true,
            verified: Some(false),
            reason: Some("readback_unavailable".into()),
            error: None,
            detail: Some(serde_json::json!({
                "via": "applescript_ax",
                "result": out.status,
                "note": ok_detail
            })),
        },
        Err(e) => map_script_error(op, e),
    }
}

pub fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}
