use crate::channels::{ChannelHealth, ChannelResult, Params};
use crate::macos;
use core_graphics::event::{CGEvent, CGEventFlags, CGKeyCode};
use once_cell::sync::Lazy;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use std::process::Command;

struct Shortcut {
    key: CGKeyCode,
    flags: CGEventFlags,
}

impl Shortcut {
    fn key(code: CGKeyCode) -> Self {
        Self {
            key: code,
            flags: CGEventFlags::empty(),
        }
    }

    fn cmd(code: CGKeyCode) -> Self {
        Self {
            key: code,
            flags: CGEventFlags::CGEventFlagCommand,
        }
    }

    fn cmd_shift(code: CGKeyCode) -> Self {
        Self {
            key: code,
            flags: CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagShift,
        }
    }

    fn cmd_option(code: CGKeyCode) -> Self {
        Self {
            key: code,
            flags: CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagAlternate,
        }
    }
}

fn key_map() -> &'static [(&'static str, Shortcut)] {
    static MAP: Lazy<Vec<(&'static str, Shortcut)>> = Lazy::new(|| {
        vec![
            ("transport.play", Shortcut::key(49)),
            ("transport.stop", Shortcut::key(49)),
            ("transport.record", Shortcut::key(15)),
            ("transport.pause", Shortcut::key(49)),
            ("transport.rewind", Shortcut::key(123)),
            ("transport.fast_forward", Shortcut::key(124)),
            ("transport.toggle_cycle", Shortcut::key(8)),
            ("transport.toggle_metronome", Shortcut::key(40)),
            ("edit.undo", Shortcut::cmd(6)),
            ("edit.redo", Shortcut::cmd_shift(6)),
            ("edit.cut", Shortcut::cmd(7)),
            ("edit.copy", Shortcut::cmd(8)),
            ("edit.paste", Shortcut::cmd(9)),
            ("edit.delete", Shortcut::key(51)),
            ("edit.select_all", Shortcut::cmd(0)),
            ("edit.split", Shortcut::cmd(17)),
            ("edit.join", Shortcut::cmd(38)),
            ("edit.quantize", Shortcut::key(44)),
            ("edit.bounce_in_place", Shortcut::cmd_option(11)),
            ("view.toggle_mixer", Shortcut::key(7)),
            ("view.toggle_piano_roll", Shortcut::key(35)),
            ("view.toggle_library", Shortcut::key(16)),
            ("view.toggle_inspector", Shortcut::key(34)),
            ("project.new", Shortcut::cmd(45)),
            ("project.save", Shortcut::cmd(1)),
            ("project.save_as", Shortcut::cmd_shift(1)),
            ("project.close", Shortcut::cmd(13)),
            ("track.create_audio", Shortcut::cmd_option(0)),
            ("track.create_instrument", Shortcut::cmd_option(1)),
            ("track.duplicate", Shortcut::cmd(2)),
            ("track.delete", Shortcut::cmd(51)),
            ("nav.create_marker", Shortcut::cmd_option(39)),
            ("nav.zoom_to_fit", Shortcut::key(6)),
            ("automation.toggle_view", Shortcut::key(0)),
        ]
    });
    MAP.as_slice()
}

pub struct CgEventChannel;

impl CgEventChannel {
    pub fn registered_operations() -> Vec<&'static str> {
        key_map().iter().map(|(op, _)| *op).collect()
    }

    pub fn has_shortcut(operation: &str) -> bool {
        key_map().iter().any(|(op, _)| *op == operation)
    }

    pub fn health() -> ChannelHealth {
        if logic_pid().is_some() {
            ChannelHealth::healthy("CGEvent ready")
        } else if macos::is_logic_running() {
            ChannelHealth::unavailable("Cannot determine Logic Pro PID")
        } else {
            ChannelHealth::unavailable("Logic Pro is not running")
        }
    }

    pub fn execute(operation: &str, params: &Params) -> ChannelResult {
        let Some(pid) = logic_pid() else {
            return ChannelResult::err("Logic Pro is not running");
        };

        if operation == "transport.set_tempo" {
            let Some(tempo) = params
                .get("bpm")
                .or_else(|| params.get("tempo"))
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|t| (5.0..=999.0).contains(t))
            else {
                return ChannelResult::err("set_tempo requires tempo 5-999");
            };
            return Self::set_tempo(tempo).into();
        }

        if operation == "transport.goto_position" {
            let position = params
                .get("position")
                .cloned()
                .unwrap_or_else(|| "1.1.1.1".into());
            return goto_position_cg(&position, pid);
        }

        if operation == "nav.goto_bar" || operation == "nav.set_zoom" {
            // handled via transport or params elsewhere
        }

        for (op, shortcut) in key_map() {
            if *op == operation {
                if post_key(shortcut, pid) {
                    return ChannelResult::ok(format!("CGEvent sent for {operation}"));
                }
                return ChannelResult::err(format!("Failed to post CGEvent for {operation}"));
            }
        }

        if operation.starts_with("view.toggle_") {
            return Self::execute(
                match operation.strip_prefix("view.toggle_").unwrap_or("") {
                    "mixer" => "view.toggle_mixer",
                    "piano_roll" => "view.toggle_piano_roll",
                    "library" => "view.toggle_library",
                    "inspector" => "view.toggle_inspector",
                    "automation" => "automation.toggle_view",
                    other => return ChannelResult::err(format!("Unknown view toggle: {other}")),
                },
                params,
            );
        }

        if operation == "nav.toggle_view" {
            let view = params.get("view").map(String::as_str).unwrap_or("mixer");
            let mapped = match view {
                "mixer" => "view.toggle_mixer",
                "piano_roll" => "view.toggle_piano_roll",
                "library" => "view.toggle_library",
                "inspector" => "view.toggle_inspector",
                "automation" => "automation.toggle_view",
                other => return ChannelResult::err(format!("Unknown view: {other}")),
            };
            return Self::execute(mapped, params);
        }

        ChannelResult::err(format!("No CGEvent mapping for {operation}"))
    }

    /// Fallback when System Events automation is unavailable but Logic + Accessibility are granted.
    pub fn set_tempo(tempo: f64) -> logicx_core::HonestResult {
        let Some(pid) = logic_pid() else {
            return logicx_core::HonestResult::failed("Logic Pro is not running");
        };
        set_tempo_cg(tempo, pid)
    }
}

fn logic_pid() -> Option<i32> {
    let output = Command::new("/usr/bin/pgrep")
        .args(["-x", "Logic Pro"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .and_then(|l| l.trim().parse().ok())
}

fn post_key(shortcut: &Shortcut, pid: i32) -> bool {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        return false;
    };
    let Ok(down) = CGEvent::new_keyboard_event(source.clone(), shortcut.key, true) else {
        return false;
    };
    let Ok(up) = CGEvent::new_keyboard_event(source, shortcut.key, false) else {
        return false;
    };
    down.set_flags(shortcut.flags);
    up.set_flags(shortcut.flags);
    down.post_to_pid(pid);
    up.post_to_pid(pid);
    true
}

fn set_tempo_cg(tempo: f64, pid: i32) -> logicx_core::HonestResult {
    use serde_json::json;

    let _ = macos::run_osascript_output(r#"tell application "Logic Pro" to activate"#);
    std::thread::sleep(std::time::Duration::from_millis(450));
    let _ = post_key(&Shortcut::key(53), pid);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let _ = post_key(&Shortcut::key(53), pid);
    std::thread::sleep(std::time::Duration::from_millis(150));

    if !post_key(&Shortcut::cmd_option(17), pid) {
        return logicx_core::HonestResult::failed("Failed to open Tempo settings (Option+Command+T)");
    }
    std::thread::sleep(std::time::Duration::from_millis(700));

    if !post_key(&Shortcut::cmd(0), pid) {
        return logicx_core::HonestResult::failed("Failed to select tempo field");
    }
    std::thread::sleep(std::time::Duration::from_millis(80));

    let tempo_str = if (tempo - tempo.round()).abs() < f64::EPSILON {
        format!("{}", tempo.round() as u32)
    } else {
        format!("{tempo:.2}")
    };

    for ch in tempo_str.chars() {
        let Some(sc) = char_shortcut(ch) else {
            return logicx_core::HonestResult::failed(format!("Unsupported tempo character: {ch}"));
        };
        std::thread::sleep(std::time::Duration::from_millis(30));
        if !post_key(&sc, pid) {
            return logicx_core::HonestResult::failed("Failed to type tempo value");
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(120));
    if !post_key(&Shortcut::key(36), pid) {
        return logicx_core::HonestResult::failed("Failed to confirm tempo");
    }
    std::thread::sleep(std::time::Duration::from_millis(250));
    let _ = post_key(&Shortcut::key(53), pid);

    logicx_core::HonestResult {
        success: true,
        verified: Some(false),
        reason: Some("readback_unavailable".into()),
        error: None,
        detail: Some(json!({
            "requested": tempo,
            "via": "cgevent_fallback",
            "note": "Enable Automation → System Events on logicx-control-bridge for verified tempo control",
            "message": format!("CGEvent set tempo to {tempo_str}"),
        })),
    }
}

fn goto_position_cg(position: &str, pid: i32) -> ChannelResult {
    let open = Shortcut::key(44);
    if !post_key(&open, pid) {
        return ChannelResult::err("Failed to open goto dialog");
    }
    for ch in position.chars() {
        let Some(sc) = char_shortcut(ch) else {
            return ChannelResult::err(format!("Unsupported position character: {ch}"));
        };
        std::thread::sleep(std::time::Duration::from_millis(20));
        if !post_key(&sc, pid) {
            return ChannelResult::err("Failed to type position");
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(20));
    if post_key(&Shortcut::key(36), pid) {
        ChannelResult::ok(format!("CGEvent goto position {position}"))
    } else {
        ChannelResult::err("Failed to confirm goto position")
    }
}

fn char_shortcut(ch: char) -> Option<Shortcut> {
    Some(match ch {
        '0' => Shortcut::key(29),
        '1' => Shortcut::key(18),
        '2' => Shortcut::key(19),
        '3' => Shortcut::key(20),
        '4' => Shortcut::key(21),
        '5' => Shortcut::key(23),
        '6' => Shortcut::key(22),
        '7' => Shortcut::key(26),
        '8' => Shortcut::key(28),
        '9' => Shortcut::key(25),
        '.' => Shortcut::key(47),
        ':' => Shortcut {
            key: 41,
            flags: CGEventFlags::CGEventFlagShift,
        },
        _ => return None,
    })
}
