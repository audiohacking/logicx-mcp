use crate::macos::{
    automation_logic_ok, automation_system_events_ok, honest_from_script, is_ax_trusted,
    is_permission_error, map_script_error, run_osascript_output, sleep_ms,
};
use logicx_core::HonestResult;

pub fn transport_play() -> HonestResult {
    let _ = run_osascript_output(r#"tell application "Logic Pro" to activate"#);
    sleep_ms(200);
    if is_ax_trusted() {
        if let Some(result) = crate::macos::ax_native::transport_play() {
            return result;
        }
    }
    transport_play_system_events()
}

fn transport_play_system_events() -> HonestResult {
    let script = r#"
on transportPlay()
    tell application "Logic Pro" to activate
    delay 0.15
    tell application "System Events"
        tell process "Logic Pro"
            set frontmost to true
            try
                repeat with cb in every checkbox of window 1
                    set n to name of cb
                    if n is "Play" or n is "재생" then
                        if value of cb is 0 then
                            click cb
                            return "OK"
                        else
                            return "OK_ALREADY"
                        end if
                    end if
                end repeat
            end try
            keystroke " "
            return "OK_SPACE"
        end tell
    end tell
end transportPlay
return transportPlay()
"#;
    honest_from_script("transport.play", "play via control-bar checkbox or space", run_osascript_output(script))
}

pub fn transport_stop() -> HonestResult {
    let _ = run_osascript_output(r#"tell application "Logic Pro" to activate"#);
    sleep_ms(200);
    if is_ax_trusted() {
        if let Some(result) = crate::macos::ax_native::transport_stop() {
            return result;
        }
    }
    transport_stop_system_events()
}

fn transport_stop_system_events() -> HonestResult {
    let script = r#"
on transportStop()
    tell application "Logic Pro" to activate
    delay 0.15
    tell application "System Events"
        tell process "Logic Pro"
            set frontmost to true
            try
                repeat with cb in every checkbox of window 1
                    if name of cb is in {"Record", "녹음"} then
                        if value of cb is 1 then click cb
                    end if
                end repeat
            end try
            try
                repeat with cb in every checkbox of window 1
                    if name of cb is in {"Play", "재생"} then
                        if value of cb is 1 then
                            click cb
                            return "OK"
                        else
                            return "OK_ALREADY"
                        end if
                    end if
                end repeat
            end try
            keystroke " "
            return "OK_SPACE"
        end tell
    end tell
end transportStop
return transportStop()
"#;
    honest_from_script("transport.stop", "stop via control-bar checkbox or space", run_osascript_output(script))
}

pub fn transport_set_tempo(tempo: f64) -> HonestResult {
    let _ = run_osascript_output(r#"tell application "Logic Pro" to activate"#);
    sleep_ms(250);

    if is_ax_trusted() {
        if let Some(result) = crate::macos::ax_native::set_tempo(tempo) {
            return result;
        }
    }

    if automation_system_events_ok() {
        return transport_set_tempo_system_events(tempo);
    }

    if is_ax_trusted() && automation_logic_ok() {
        return crate::channels::CgEventChannel::set_tempo(tempo);
    }

    crate::macos::permission_failed(
        "transport.set_tempo",
        format!(
            "Enable Accessibility for {} (primary tempo path). \
             Optional: Automation → {} for AppleScript fallback. \
             Current: ax={} logic={} system_events={}",
            logicx_core::runtime::automation_settings_app_name(),
            logicx_core::runtime::automation_settings_app_name(),
            is_ax_trusted(),
            automation_logic_ok(),
            automation_system_events_ok()
        ),
    )
}

fn transport_set_tempo_system_events(tempo: f64) -> HonestResult {
    let tempo_str = if (tempo - tempo.round()).abs() < f64::EPSILON {
        format!("{}", tempo.round() as u32)
    } else {
        format!("{tempo:.2}")
    };
    let script = format!(
        r#"
on setTempo()
    set targetTempo to {tempo}
    tell application "Logic Pro" to activate
    delay 0.35
    tell application "System Events"
        tell process "Logic Pro"
            set frontmost to true
            delay 0.15
            try
                perform action "AXRaise" of window 1
            end try
            -- Steal focus from plugin UI: click the main arrange area.
            try
                click group 1 of window 1
            on error
                try
                    click window 1
                end try
            end try
            delay 0.15

            -- Method 1: control-bar tempo slider (most reliable when present).
            try
                repeat with sl in every slider of window 1
                    set d to description of sl
                    if d contains "tempo" or d contains "Tempo" or d contains "BPM" or d contains "템포" then
                        set value of sl to targetTempo
                        delay 0.15
                        return "OK:" & (value of sl as text)
                    end if
                end repeat
            end try

            -- Method 2: tempo text field in control bar / transport.
            try
                repeat with tf in every text field of window 1
                    set d to description of tf
                    if d contains "tempo" or d contains "Tempo" or d contains "BPM" or d contains "템포" then
                        click tf
                        delay 0.05
                        keystroke "a" using command down
                        delay 0.05
                        keystroke "{tempo_str}"
                        delay 0.05
                        key code 36
                        delay 0.15
                        return "OK:" & (value of tf as text)
                    end if
                end repeat
            end try

            -- Method 3: Tempo & Project Settings (Option+Command+T).
            key code 17 using {{command down, option down}}
            delay 0.7
            keystroke "a" using command down
            delay 0.05
            keystroke "{tempo_str}"
            delay 0.1
            key code 36
            delay 0.25
            key code 53
            delay 0.15

            -- Readback from tempo slider if visible after dialog.
            try
                repeat with sl in every slider of window 1
                    set d to description of sl
                    if d contains "tempo" or d contains "Tempo" or d contains "BPM" or d contains "템포" then
                        return "OK:" & (value of sl as text)
                    end if
                end repeat
            end try
            return "OK_UNVERIFIED"
        end tell
    end tell
end setTempo
return setTempo()
"#
    );

    match run_osascript_output(&script) {
        Ok(out) if out.status.starts_with("OK:") => {
            let actual = out
                .status
                .trim_start_matches("OK:")
                .trim()
                .parse::<f64>()
                .unwrap_or(-1.0);
            let verified = (actual - tempo).abs() <= 1.0;
            HonestResult {
                success: true,
                verified: Some(verified),
                reason: if verified {
                    None
                } else {
                    Some("tempo_mismatch".into())
                },
                error: None,
                detail: Some(serde_json::json!({
                    "requested": tempo,
                    "observed": actual,
                    "via": "system_events",
                    "raw": out.status,
                })),
            }
        }
        Ok(out) if out.status.starts_with("OK") => HonestResult {
            success: true,
            verified: Some(false),
            reason: Some("readback_unavailable".into()),
            error: None,
            detail: Some(serde_json::json!({
                "requested": tempo,
                "via": "system_events",
                "raw": out.status,
            })),
        },
        Ok(out) => HonestResult::failed(format!("set_tempo: {}", out.status)),
        Err(e) => map_script_error("transport.set_tempo", e),
    }
}

pub fn transport_goto_bar(bar: u32) -> HonestResult {
    let _ = run_osascript_output(r#"tell application "Logic Pro" to activate"#);
    sleep_ms(250);

    if is_ax_trusted() {
        if let Some(result) = crate::macos::ax_native::goto_bar(bar) {
            return result;
        }
    }

    transport_goto_bar_system_events(bar)
}

fn transport_goto_bar_system_events(bar: u32) -> HonestResult {
    // Primary: Navigate → Go To → Position dialog (works on existing projects with regions).
    let dialog_script = format!(
        r#"
on gotoDialog()
    tell application "Logic Pro" to activate
    delay 0.2
    tell application "System Events"
        tell process "Logic Pro"
            try
                set mi to menu item "위치…" of menu 1 of menu item "이동" of menu 1 of menu bar item "탐색" of menu bar 1
            on error
                try
                    set mi to menu item "Position…" of menu 1 of menu item "Go To" of menu 1 of menu bar item "Navigate" of menu bar 1
                on error errMsg
                    return "ERR:DIALOG_MENU:" & errMsg
                end try
            end try
            if not (enabled of mi) then
                return "ERR:DIALOG_DISABLED"
            end if
            click mi
            set dialogReady to false
            repeat 30 times
                delay 0.1
                try
                    set _ to first window whose name is "위치로 이동"
                    set dialogReady to true
                    exit repeat
                end try
                try
                    set _ to first window whose name is "Go to Position"
                    set dialogReady to true
                    exit repeat
                end try
            end repeat
            if not dialogReady then
                return "ERR:DIALOG_NOT_READY"
            end if
            keystroke "a" using command down
            delay 0.05
            keystroke "{bar}"
            delay 0.05
            keystroke return
            delay 0.2
            return "OK_DIALOG"
        end tell
    end tell
end gotoDialog
return gotoDialog()
"#
    );

    match run_osascript_output(&dialog_script) {
        Ok(out) if out.status.starts_with("OK") => {
            return HonestResult {
                success: true,
                verified: Some(false),
                reason: Some("readback_unavailable".into()),
                error: None,
                detail: Some(serde_json::json!({
                    "requested": format!("{bar}.1.1.1"),
                    "via": "goto_dialog",
                    "result": out.status
                })),
            };
        }
        Ok(out) if out.status.contains("DIALOG_DISABLED") || out.status.contains("DIALOG_MENU") => {
            // Fallback: control-bar bar slider (clamps to project length).
        }
        Ok(out) => {
            return HonestResult::failed(format!("goto_position dialog: {}", out.status));
        }
        Err(e) => {
            if is_permission_error(&e) {
                return crate::macos::map_script_error("transport.goto_position", e);
            }
            // Non-permission errors fall through to slider fallback.
        }
    }

    sleep_ms(200);
    let slider_script = format!(
        r#"
on gotoSlider()
    tell application "Logic Pro" to activate
    delay 0.15
    tell application "System Events"
        tell process "Logic Pro"
            set frontmost to true
            try
                repeat with sl in every slider of window 1
                    set d to description of sl
                    if d contains "bar" or d contains "Bar" or d contains "마디" then
                        set value of sl to {bar}
                        return "OK_SLIDER"
                    end if
                end repeat
            end try
            return "ERR:NO_SLIDER"
        end tell
    end tell
end gotoSlider
return gotoSlider()
"#
    );

    honest_from_script(
        "transport.goto_position",
        &format!("goto bar {bar} via control-bar slider fallback"),
        run_osascript_output(&slider_script),
    )
}

pub fn read_transport_state() -> HonestResult {
    if is_ax_trusted() {
        if let Some(result) = crate::macos::ax_native::read_transport_state() {
            return result;
        }
    }
    HonestResult {
        success: false,
        verified: None,
        reason: Some("ax_unavailable".into()),
        error: Some(format!(
            "transport.get_state requires Accessibility for {}",
            logicx_core::runtime::automation_settings_app_name()
        )),
        detail: None,
    }
}
