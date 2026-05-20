use crate::macos::{run_osascript_output, sleep_ms, is_ax_trusted};
use logicx_core::HonestResult;

fn create_track(korean: &str, english: &str) -> HonestResult {
    let script = format!(
        r#"
on createTrack()
    tell application "Logic Pro" to activate
    delay 0.2
    tell application "System Events"
        tell process "Logic Pro"
            set frontmost to true
            try
                click menu item "{korean}" of menu 1 of menu bar item "트랙" of menu bar 1
                return "OK_KR"
            on error
                try
                    click menu item "{english}" of menu 1 of menu bar item "Track" of menu bar 1
                    return "OK_EN"
                on error errMsg
                    return "ERR:" & errMsg
                end try
            end try
        end tell
    end tell
end createTrack
return createTrack()
"#
    );

    match run_osascript_output(&script) {
        Ok(out) if out.status.starts_with("OK") => {
            sleep_ms(400);
            // Confirm "New Track" dialog if it appears (Logic 12).
            let _ = run_osascript_output(
                r#"tell application "System Events" to key code 36"#,
            );
            sleep_ms(800);
            HonestResult {
                success: true,
                verified: Some(false),
                reason: Some("readback_unavailable".into()),
                error: None,
                detail: Some(serde_json::json!({
                    "menu_clicked": english,
                    "via": "track_menu",
                    "result": out.status,
                    "note": "Track count not verified — check Logic arrange window"
                })),
            }
        }
        Ok(out) => HonestResult::failed(out.status),
        Err(e) => crate::macos::map_script_error("track.create", e),
    }
}

pub fn create_instrument_track() -> HonestResult {
    let _ = run_osascript_output(r#"tell application "Logic Pro" to activate"#);
    sleep_ms(250);
    if is_ax_trusted() {
        if let Some(result) = crate::macos::ax_native::create_software_instrument_track() {
            return result;
        }
    }
    create_track("새로운 소프트웨어 악기 트랙", "New Software Instrument Track")
}

pub fn create_audio_track() -> HonestResult {
    create_track("새로운 오디오 트랙", "New Audio Track")
}
