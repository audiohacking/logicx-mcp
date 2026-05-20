use logicx_core::HonestResult;
use std::process::Command;

pub fn is_logic_running() -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-x", "Logic Pro"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn transport_play() -> HonestResult {
    if let Ok(()) = run_osascript(
        r#"tell application "System Events" to tell process "Logic Pro" to keystroke "." using {command down}"#,
    ) {
        return HonestResult::uncertain("play via keystroke");
    }
    keystroke_via_logic(" ")
}

pub fn transport_stop() -> HonestResult {
    keystroke_via_logic(" ")
}

pub fn transport_set_tempo(tempo: f64) -> HonestResult {
    let script = format!(
        r#"tell application "Logic Pro"
            set tempo to {tempo}
        end tell"#
    );
    match run_osascript(&script) {
        Ok(()) => HonestResult::uncertain(
            "tempo set via AppleScript — verify in transport; AX/MCU path preferred",
        ),
        Err(e) => HonestResult::failed(e),
    }
}

pub fn transport_goto_bar(bar: u32) -> HonestResult {
    let script = format!(
        r#"tell application "Logic Pro"
            set the bar to {bar}
            set playhead to the bar
        end tell"#
    );
    match run_osascript(&script) {
        Ok(()) => HonestResult::uncertain(format!("goto bar {bar} via AppleScript")),
        Err(e) => HonestResult::failed(e),
    }
}

pub fn import_midi_file(path: &str) -> HonestResult {
    let escaped = path.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"tell application "Logic Pro"
            activate
            open POSIX file "{escaped}"
        end tell"#
    );
    match run_osascript(&script) {
        Ok(()) => HonestResult::uncertain(format!("opened MIDI file {path} — verify new track")),
        Err(e) => HonestResult::failed(e),
    }
}

fn keystroke_via_logic(key: &str) -> HonestResult {
    let script = format!(
        r#"tell application "Logic Pro" to activate
        tell application "System Events" to tell process "Logic Pro" to keystroke "{key}""#
    );
    match run_osascript(&script) {
        Ok(()) => HonestResult::uncertain("transport keystroke sent"),
        Err(e) => HonestResult::failed(e),
    }
}

fn run_osascript(script: &str) -> Result<(), String> {
    if !is_logic_running() {
        return Err("Logic Pro is not running".into());
    }
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
