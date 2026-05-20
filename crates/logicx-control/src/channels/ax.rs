use crate::channels::{ChannelHealth, ChannelResult, Params};
use crate::macos;
use crate::notes;
use crate::smf;

pub struct AxChannel;

impl AxChannel {
    pub fn health() -> ChannelHealth {
        if !macos::is_logic_running() {
            return ChannelHealth::unavailable("Logic Pro is not running");
        }
        if macos::is_ax_trusted() {
            return ChannelHealth::healthy(
                "Native AX tempo control ready (double-click control bar slider)",
            );
        }
        if macos::automation_system_events_ok() {
            return ChannelHealth::healthy("System Events UI control ready");
        }
        if macos::automation_logic_ok() {
            return ChannelHealth {
                available: true,
                ready: false,
                detail: format!(
                    "Grant Accessibility to {} for tempo control",
                    logicx_core::runtime::automation_settings_app_name()
                ),
            };
        }
        ChannelHealth::unavailable(format!(
            "Grant Accessibility to {} in System Settings",
            logicx_core::runtime::automation_settings_app_name()
        ))
    }

    pub fn execute(operation: &str, params: &Params) -> ChannelResult {
        match operation {
            "transport.play" => macos::transport_play().into(),
            "transport.stop" => macos::transport_stop().into(),
            "transport.set_tempo" => {
                let Some(tempo) = params
                    .get("bpm")
                    .or_else(|| params.get("tempo"))
                    .and_then(|s| s.parse().ok())
                    .filter(|t| (5.0..=999.0).contains(t))
                else {
                    return ChannelResult::err("set_tempo requires tempo 5-999");
                };
                macos::transport_set_tempo(tempo).into()
            }
            "transport.goto_position" => {
                let Some(bar) = params
                    .get("bar")
                    .and_then(|s| s.parse().ok())
                    .or_else(|| {
                        params
                            .get("position")
                            .and_then(|p| p.split('.').next()?.parse().ok())
                    })
                else {
                    return ChannelResult::err("goto_position requires bar");
                };
                macos::transport_goto_bar(bar).into()
            }
            "transport.toggle_cycle" => macos::run_ax_script("toggle_cycle").into(),
            "transport.toggle_metronome" => macos::run_ax_script("toggle_metronome").into(),
            "transport.toggle_count_in" => macos::run_ax_script("toggle_count_in").into(),
            "transport.record" => macos::run_ax_script("record").into(),
            "transport.set_cycle_range" => {
                let start = params.get("start").and_then(|s| s.parse().ok());
                let end = params.get("end").and_then(|s| s.parse().ok());
                match (start, end) {
                    (Some(s), Some(e)) => macos::set_cycle_range(s, e).into(),
                    _ => ChannelResult::err("set_cycle_range requires start and end"),
                }
            }
            "track.create_instrument" => macos::create_instrument_track().into(),
            "track.create_audio" => macos::create_audio_track().into(),
            "track.create_drummer" => macos::create_drummer_track().into(),
            "track.create_external_midi" => macos::create_external_midi_track().into(),
            "track.delete" => macos::delete_track().into(),
            "track.rename" => {
                let Some(index) = params.get("index") else {
                    return ChannelResult::err("rename requires index");
                };
                let Some(name) = params.get("name") else {
                    return ChannelResult::err("rename requires name");
                };
                macos::rename_track(index, name).into()
            }
            "track.select" => {
                let Some(index) = params.get("index") else {
                    return ChannelResult::err("select requires index");
                };
                macos::select_track(index).into()
            }
            "track.set_mute" | "track.set_solo" | "track.set_arm" => {
                let Some(index) = params.get("index") else {
                    return ChannelResult::err("requires index");
                };
                let enabled = params
                    .get("enabled")
                    .map(|s| s != "false" && s != "0")
                    .unwrap_or(true);
                let op = operation.rsplit('.').next().unwrap_or("");
                macos::set_track_toggle(index, op, enabled).into()
            }
            "track.arm_only" => {
                let Some(index) = params.get("index") else {
                    return ChannelResult::err("arm_only requires index");
                };
                macos::arm_only(index).into()
            }
            "track.set_instrument" => {
                let Some(index) = params.get("index") else {
                    return ChannelResult::err("set_instrument requires index");
                };
                let Some(path) = params.get("path") else {
                    return ChannelResult::err("set_instrument requires path");
                };
                macos::set_instrument(index, path).into()
            }
            "library.list" | "library.scan_all" => macos::scan_library().into(),
            "library.resolve_path" => {
                let Some(path) = params.get("path") else {
                    return ChannelResult::err("resolve_path requires path");
                };
                macos::resolve_library_path(path).into()
            }
            "plugin.scan_presets" => macos::scan_plugin_presets().into(),
            "midi.import_file" => {
                let Some(path) = params.get("path") else {
                    return ChannelResult::err("midi.import_file requires path");
                };
                macos::import_midi_file(path).into()
            }
            "nav.get_markers" => macos::get_markers().into(),
            "nav.rename_marker" => {
                let Some(name) = params.get("name") else {
                    return ChannelResult::err("rename_marker requires name");
                };
                let Some(new_name) = params.get("new_name") else {
                    return ChannelResult::err("rename_marker requires new_name");
                };
                macos::rename_marker(name, new_name).into()
            }
            "project.get_info" => macos::project_info().into(),
            "project.save_as" => {
                let Some(path) = params.get("path") else {
                    return ChannelResult::err("save_as requires path");
                };
                macos::save_as_dialog(path).into()
            }
            "region.get_regions" => macos::get_regions().into(),
            "track.get_tracks" => macos::get_tracks().into(),
            _ => ChannelResult::err(format!("Unsupported AX operation: {operation}")),
        }
    }

    pub fn record_sequence(params: &Params) -> ChannelResult {
        let notes_str = params.get("notes").map(String::as_str).unwrap_or("");
        let events = match notes::parse_notes(notes_str) {
            Ok(e) if !e.is_empty() => e,
            Ok(_) => return ChannelResult::err("record_sequence requires notes"),
            Err(e) => return ChannelResult::err(e),
        };
        if events.len() > 512 {
            return ChannelResult::err(format!(
                "record_sequence max 512 events (got {})",
                events.len()
            ));
        }

        let bar = params.get("bar").and_then(|s| s.parse().ok()).unwrap_or(4);
        let tempo = params.get("tempo").and_then(|s| s.parse().ok()).unwrap_or(120.0);
        let event_count = events.len();

        let path = match smf::write_temp_file(&events, tempo, bar) {
            Ok(p) => p,
            Err(e) => return ChannelResult::err(format!("SMF write failed: {e}")),
        };

        let goto = macos::transport_goto_bar(1);
        if !goto.success {
            let _ = std::fs::remove_file(&path);
            return ChannelResult::err(format!(
                "record_sequence: goto bar 1 failed: {}",
                goto.error.unwrap_or_default()
            ));
        }

        let import = macos::import_midi_file(path.to_string_lossy().as_ref());
        let _ = std::fs::remove_file(&path);

        match import {
            logicx_core::HonestResult {
                success: true,
                verified,
                reason,
                error: None,
                detail: _,
            } => ChannelResult::Success {
                message: "ok".into(),
                verified,
                reason,
                detail: Some(serde_json::json!({
                    "method": "record_sequence",
                    "events": event_count,
                    "bars": bar,
                    "tempo": tempo,
                    "project": macos::front_project_name(),
                    "note": "MIDI region imported at bar 1 in the current project",
                })),
            },
            other => other.into(),
        }
    }
}

impl From<logicx_core::HonestResult> for ChannelResult {
    fn from(h: logicx_core::HonestResult) -> Self {
        if h.success {
            ChannelResult::Success {
                message: "ok".into(),
                verified: h.verified,
                reason: h.reason,
                detail: h.detail,
            }
        } else {
            ChannelResult::err(h.error.unwrap_or_else(|| "failed".into()))
        }
    }
}
