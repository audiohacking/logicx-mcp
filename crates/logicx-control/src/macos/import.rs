use crate::macos::cg_input;
use crate::macos::project::{front_project_name, has_open_project};
use crate::macos::{is_ax_trusted, map_script_error, run_osascript_output, sleep_ms};
use logicx_core::HonestResult;
use std::path::Path;

/// Import MIDI into the **current** project via File → Import → MIDI File.
/// Never uses `open POSIX file` (that creates a new project).
pub fn import_midi_file(path: &str) -> HonestResult {
    if !Path::new(path).exists() {
        return HonestResult::failed(format!("MIDI file not found: {path}"));
    }

    if !has_open_project() {
        return HonestResult::failed(
            "Logic Pro is not running — start Logic and open a project, then retry.",
        );
    }

    let _ = run_osascript_output(r#"tell application "Logic Pro" to activate"#);
    sleep_ms(300);

    if is_ax_trusted()
        && let Some(result) = import_midi_native(path)
    {
        return result;
    }

    import_midi_system_events(path)
}

fn import_midi_native(path: &str) -> Option<HonestResult> {
    let opened = crate::macos::ax_native::open_import_midi_file_dialog()?;
    if !opened.success {
        return Some(opened);
    }

    sleep_ms(800);
    cg_input::press_cmd_shift_g();
    sleep_ms(400);
    cg_input::type_string(path);
    sleep_ms(300);
    cg_input::press_return();
    sleep_ms(400);
    cg_input::press_return();
    sleep_ms(1500);

    let _ = crate::macos::ax_native::click_dialog_button(
        &["Import", "가져오기"],
        &["Import", "가져오기"],
    );
    sleep_ms(2000);

    let _ = crate::macos::ax_native::click_dialog_button(
        &["Import", "가져오기", "Tempo"],
        &["No", "아니요"],
    );
    sleep_ms(500);

    Some(import_success_detail(path))
}

fn import_midi_system_events(path: &str) -> HonestResult {
    if !has_open_project() {
        return HonestResult::failed("Logic Pro is not running");
    }

    let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"
on importMIDI()
    tell application "Logic Pro" to activate
    delay 0.3
    tell application "System Events"
        tell process "Logic Pro"
            try
                click menu item "MIDI 파일…" of menu 1 of menu item "가져오기" of menu 1 of menu bar item "파일" of menu bar 1
            on error
                try
                    click menu item "MIDI File…" of menu 1 of menu item "Import" of menu 1 of menu bar item "File" of menu bar 1
                on error errMsg
                    return "ERR:MENU:" & errMsg
                end try
            end try
        end tell
        delay 1.5
        keystroke "G" using {{command down, shift down}}
        delay 0.5
        keystroke "{escaped}"
        delay 0.3
        keystroke return
        delay 0.3
        keystroke return
        delay 1.5
        tell process "Logic Pro"
            try
                set importDlg to first window whose name is "가져오기"
                click button "가져오기" of UI element 1 of importDlg
            on error
                try
                    set importDlg to first window whose name is "Import"
                    click button "Import" of UI element 1 of importDlg
                on error errMsg
                    return "ERR:IMPORT_BTN:" & errMsg
                end try
            end try
        end tell
        delay 2.0
        tell process "Logic Pro"
            try
                set tempoDlg to first window whose subrole is "AXDialog"
                try
                    click button "아니요" of tempoDlg
                on error
                    try
                        click button "No" of tempoDlg
                    end try
                end try
            end try
        end tell
    end tell
    return "OK"
end importMIDI
return importMIDI()
"#
    );

    match run_osascript_output(&script) {
        Ok(out) if out.status == "OK" => import_success_detail(path),
        Ok(out) => HonestResult::failed(out.status),
        Err(e) => map_script_error("midi.import_file", e),
    }
}

fn import_success_detail(path: &str) -> HonestResult {
    sleep_ms(300);
    HonestResult {
        success: true,
        verified: Some(false),
        reason: Some("readback_unavailable".into()),
        error: None,
        detail: Some(serde_json::json!({
            "method": "import_into_current_project",
            "project": front_project_name(),
            "note_count_hint": Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_string()),
            "note": "MIDI imported at playhead into the open project — verify region in arrange",
        })),
    }
}
