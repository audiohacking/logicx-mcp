use crate::channels::applescript::AppleScriptChannel;
use crate::channels::ax::AxChannel;
use crate::channels::cgevent::CgEventChannel;
use crate::channels::coremidi::CoreMidiChannel;
use crate::channels::mcu::McuChannel;
use crate::channels::scripter::ScripterChannel;
use crate::channels::{ChannelHealth, ChannelId, ChannelResult, Params};
use crate::midi::engine::MidiEngine;
use crate::midi::mcu_feedback;
use logicx_core::HonestResult;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

static CORE_MIDI: Lazy<Arc<MidiEngine>> =
    Lazy::new(|| Arc::new(MidiEngine::new(crate::channels::coremidi::PORT_NAME)));
static MCU_MIDI: Lazy<Arc<MidiEngine>> =
    Lazy::new(|| Arc::new(MidiEngine::new(crate::midi::mcu_protocol::PORT_NAME)));
static KEYCMD_MIDI: Lazy<Arc<MidiEngine>> =
    Lazy::new(|| Arc::new(MidiEngine::new(crate::channels::keycmd::PORT_NAME)));

pub struct ChannelRouter {
    core: CoreMidiChannel,
    mcu: McuChannel,
    started: bool,
}

impl ChannelRouter {
    pub fn global() -> &'static Mutex<ChannelRouter> {
        static ROUTER: Lazy<Mutex<ChannelRouter>> = Lazy::new(|| {
            Mutex::new(ChannelRouter {
                core: CoreMidiChannel::new(Arc::clone(&CORE_MIDI)),
                mcu: McuChannel::new(Arc::clone(&MCU_MIDI)),
                started: false,
            })
        });
        &ROUTER
    }

    pub fn ensure_started(&mut self) {
        if self.started {
            return;
        }
        let _ = CORE_MIDI.start();
        let _ = MCU_MIDI.start();
        let _ = KEYCMD_MIDI.start();
        let _ = crate::channels::scripter::ensure_started();
        mcu_feedback::ensure_started(crate::midi::mcu_state::McuStateCache::global());
        mcu_feedback::send_device_query(&MCU_MIDI);
        self.started = true;
    }

    pub fn route(&self, operation: &str, params: Params) -> ChannelResult {
        route_chain(
            operation,
            &params,
            |ch| self.health(ch),
            |ch, op, p| self.execute(ch, op, p),
        )
    }

    fn execute(&self, channel: ChannelId, operation: &str, params: &Params) -> ChannelResult {
        match channel {
            ChannelId::Accessibility => AxChannel::execute(operation, params),
            ChannelId::Mcu => self.mcu.execute(operation, params),
            ChannelId::CoreMidi => self.core.execute(operation, params),
            ChannelId::MidiKeyCommands => {
                crate::channels::keycmd::KeyCmdChannel::execute(operation, params)
            }
            ChannelId::CgEvent => CgEventChannel::execute(operation, params),
            ChannelId::AppleScript => AppleScriptChannel::execute(operation, params),
            ChannelId::Scripter => ScripterChannel::execute(operation, params),
        }
    }

    pub fn health_report(&self) -> HashMap<String, ChannelHealth> {
        let mut report = HashMap::new();
        for id in [
            ChannelId::Accessibility,
            ChannelId::Mcu,
            ChannelId::CoreMidi,
            ChannelId::MidiKeyCommands,
            ChannelId::CgEvent,
            ChannelId::AppleScript,
            ChannelId::Scripter,
        ] {
            report.insert(id.as_str().into(), self.health(id));
        }
        report
    }

    fn health(&self, channel: ChannelId) -> ChannelHealth {
        match channel {
            ChannelId::Accessibility => AxChannel::health(),
            ChannelId::Mcu => self.mcu.health(),
            ChannelId::CoreMidi => self.core.health(),
            ChannelId::MidiKeyCommands => crate::channels::keycmd::KeyCmdChannel::health(),
            ChannelId::CgEvent => CgEventChannel::health(),
            ChannelId::AppleScript => AppleScriptChannel::health(),
            ChannelId::Scripter => ScripterChannel::health(),
        }
    }
}

/// Ops exempt from runtime-readiness gate (`health.ready == false`).
/// KeyCmd `*.keycmd` send ops must still deliver bytes while awaiting MIDI Learn.
pub fn bypass_readiness_ops() -> &'static [&'static str] {
    &[
        "midi.send_cc.keycmd",
        "midi.send_note.keycmd",
        "midi.send_chord.keycmd",
        "midi.send_program_change.keycmd",
        "midi.send_pitch_bend.keycmd",
        "midi.send_aftertouch.keycmd",
        "midi.play_sequence.keycmd",
    ]
}

fn port_unavailable_envelope(operation: &str, hint: &str) -> String {
    use logicx_core::{HonestError, encode_state_c};
    let mut extras = serde_json::Map::new();
    extras.insert(
        "operation".into(),
        serde_json::Value::String(operation.into()),
    );
    encode_state_c(HonestError::PortUnavailable, None, Some(hint), extras)
}

/// Pure routing chain (used by production router and mock tests).
pub fn route_chain(
    operation: &str,
    params: &Params,
    health: impl Fn(ChannelId) -> ChannelHealth,
    execute: impl Fn(ChannelId, &str, &Params) -> ChannelResult,
) -> ChannelResult {
    let chain = routing_table().get(operation).cloned().unwrap_or_default();

    if chain.is_empty() {
        return ChannelResult::ok(format!("No channel required for {operation}"));
    }

    let is_bypass = bypass_readiness_ops().contains(&operation);
    let mut last_error = "No channels available".to_string();
    for channel in chain {
        let h = health(channel);
        if !h.available {
            if is_bypass {
                return ChannelResult::err(port_unavailable_envelope(operation, &h.detail));
            }
            last_error = format!("{}: {}", channel.as_str(), h.detail);
            continue;
        }
        if !h.ready && !is_bypass && !is_operator_gated(channel) {
            last_error = format!("{} is not runtime-ready: {}", channel.as_str(), h.detail);
            continue;
        }
        let result = execute(channel, operation, params);
        if result.is_success() {
            if should_fallback_on_unverified(operation, &result) {
                last_error = "previous channel succeeded but tempo not verified".into();
                continue;
            }
            return result;
        }
        if let ChannelResult::Error(msg) = &result {
            if is_terminal_error(msg) {
                return result;
            }
            last_error = msg.clone();
        }
    }
    ChannelResult::err(format!(
        "All channels exhausted for {operation}. Last: {last_error}"
    ))
}

fn should_fallback_on_unverified(operation: &str, result: &ChannelResult) -> bool {
    operation == "transport.set_tempo"
        && matches!(
            result,
            ChannelResult::Success {
                verified: Some(false),
                ..
            }
        )
}

pub fn is_terminal_error(msg: &str) -> bool {
    if logicx_core::is_terminal_state_c(msg) {
        return true;
    }
    msg.contains("requires explicit")
        || msg.contains("not_implemented")
        || msg.contains("permission_required")
        || msg.contains("invalid")
        || msg.contains("element_not_found")
        || msg.contains("element not found")
        || msg.contains("no track at index")
        || msg.contains("confirmation_required")
        || msg.contains("blocked_in_logic_plugin")
        || msg.contains("blocked while LogicX MCP")
}

fn is_operator_gated(channel: ChannelId) -> bool {
    matches!(channel, ChannelId::Scripter | ChannelId::MidiKeyCommands)
}

/// Ops excluded from the production contract — fail before channel health gates.
pub fn is_not_implemented_op(operation: &str) -> bool {
    matches!(
        operation,
        "mixer.set_send"
            | "mixer.toggle_eq"
            | "mixer.reset_strip"
            | "mixer.set_output"
            | "mixer.set_input"
            | "mixer.set_output_volume"
    )
}

pub fn routing_table() -> &'static HashMap<&'static str, Vec<ChannelId>> {
    static TABLE: Lazy<HashMap<&'static str, Vec<ChannelId>>> = Lazy::new(|| {
        let mut m = HashMap::new();
        macro_rules! ins {
            ($op:expr, $($ch:ident),+ $(,)?) => {
                m.insert($op, vec![$(ChannelId::$ch),+]);
            };
        }

        ins!("transport.play", Accessibility, Mcu, CoreMidi, CgEvent);
        ins!(
            "transport.stop",
            Accessibility,
            Mcu,
            CoreMidi,
            CgEvent,
            AppleScript
        );
        ins!(
            "transport.record",
            Accessibility,
            Mcu,
            CoreMidi,
            CgEvent,
            AppleScript
        );
        ins!("transport.pause", CoreMidi, CgEvent);
        ins!("transport.rewind", Mcu, CoreMidi, CgEvent);
        ins!("transport.fast_forward", Mcu, CoreMidi, CgEvent);
        ins!(
            "transport.toggle_cycle",
            Accessibility,
            Mcu,
            MidiKeyCommands,
            CgEvent
        );
        ins!(
            "transport.toggle_metronome",
            Accessibility,
            MidiKeyCommands,
            CgEvent
        );
        ins!("transport.set_tempo", Accessibility);
        ins!(
            "transport.goto_position",
            Accessibility,
            Mcu,
            CoreMidi,
            CgEvent
        );
        ins!("transport.set_cycle_range", Accessibility);
        ins!(
            "transport.toggle_count_in",
            Accessibility,
            MidiKeyCommands,
            CgEvent
        );
        ins!("transport.capture_recording", MidiKeyCommands, CgEvent);

        ins!("track.select", Accessibility, Mcu);
        ins!(
            "track.create_audio",
            Accessibility,
            MidiKeyCommands,
            CgEvent
        );
        ins!(
            "track.create_instrument",
            Accessibility,
            MidiKeyCommands,
            CgEvent
        );
        ins!(
            "track.create_drummer",
            Accessibility,
            MidiKeyCommands,
            CgEvent
        );
        ins!(
            "track.create_external_midi",
            Accessibility,
            MidiKeyCommands,
            CgEvent
        );
        ins!("track.delete", Accessibility, MidiKeyCommands, CgEvent);
        ins!("track.rename", Accessibility);
        ins!("track.set_mute", Accessibility, Mcu, CgEvent);
        ins!("track.set_solo", Accessibility, Mcu, CgEvent);
        ins!("track.set_arm", Accessibility, Mcu, CgEvent);
        ins!("track.arm_only", Accessibility);
        ins!("track.set_automation", Mcu);
        ins!("track.set_instrument", Accessibility);
        ins!("track.duplicate", MidiKeyCommands, CgEvent);
        ins!("library.list", Accessibility);
        ins!("library.scan_all", Accessibility);
        ins!("library.resolve_path", Accessibility);
        ins!("plugin.scan_presets", Accessibility);
        ins!("midi.import_file", Accessibility);

        ins!("mixer.set_volume", Mcu);
        ins!("mixer.set_pan", Mcu);
        ins!("mixer.set_master_volume", Mcu);
        ins!("mixer.set_plugin_param", Scripter);

        ins!("midi.send_note", CoreMidi);
        ins!("midi.send_chord", CoreMidi);
        ins!("midi.send_cc", CoreMidi);
        ins!("midi.send_program_change", CoreMidi);
        ins!("midi.send_pitch_bend", CoreMidi);
        ins!("midi.send_aftertouch", CoreMidi);
        ins!("midi.send_sysex", CoreMidi);
        ins!("midi.play_sequence", CoreMidi);
        ins!("midi.step_input", CoreMidi);
        ins!("midi.list_ports", CoreMidi);
        ins!("midi.get_input_state", CoreMidi);
        ins!("midi.create_virtual_port", CoreMidi);

        ins!("mmc.play", CoreMidi);
        ins!("mmc.stop", CoreMidi);
        ins!("mmc.record_strobe", CoreMidi);
        ins!("mmc.record_exit", CoreMidi);
        ins!("mmc.locate", CoreMidi);
        ins!("mmc.pause", CoreMidi);

        ins!("edit.undo", MidiKeyCommands, CgEvent);
        ins!("edit.redo", MidiKeyCommands, CgEvent);
        ins!("edit.cut", MidiKeyCommands, CgEvent);
        ins!("edit.copy", MidiKeyCommands, CgEvent);
        ins!("edit.paste", MidiKeyCommands, CgEvent);
        ins!("edit.delete", MidiKeyCommands, CgEvent);
        ins!("edit.select_all", MidiKeyCommands, CgEvent);
        ins!("edit.split", MidiKeyCommands, CgEvent);
        ins!("edit.join", MidiKeyCommands, CgEvent);
        ins!("edit.quantize", MidiKeyCommands, CgEvent);
        ins!("edit.bounce_in_place", MidiKeyCommands, CgEvent);
        ins!("edit.normalize", MidiKeyCommands);
        ins!("edit.duplicate", MidiKeyCommands);
        ins!("edit.toggle_step_input", MidiKeyCommands);

        ins!("nav.goto_marker", MidiKeyCommands);
        ins!("nav.create_marker", MidiKeyCommands, CgEvent);
        ins!("nav.delete_marker", MidiKeyCommands);
        ins!("nav.rename_marker", Accessibility);
        ins!("nav.get_markers", Accessibility);
        ins!("nav.zoom_to_fit", MidiKeyCommands, CgEvent);
        ins!("nav.set_zoom_level", MidiKeyCommands);

        ins!("view.toggle_mixer", MidiKeyCommands, CgEvent);
        ins!("view.toggle_piano_roll", MidiKeyCommands, CgEvent);
        ins!("view.toggle_library", MidiKeyCommands, CgEvent);
        ins!("view.toggle_inspector", MidiKeyCommands, CgEvent);
        ins!("automation.toggle_view", MidiKeyCommands, CgEvent);

        ins!("project.new", AppleScript, CgEvent);
        ins!("project.open", AppleScript);
        ins!("project.save", MidiKeyCommands, CgEvent, AppleScript);
        ins!("project.save_as", Accessibility, AppleScript);
        ins!("project.close", AppleScript, CgEvent);
        ins!("project.bounce", MidiKeyCommands);
        ins!("project.launch", AppleScript);
        ins!("project.quit", AppleScript);
        ins!("project.get_info", Accessibility);

        ins!("region.get_regions", Accessibility);
        ins!("track.get_tracks", Accessibility);

        ins!("transport.get_state", Accessibility, Mcu);
        ins!("track.get_selected", Accessibility, Mcu);
        ins!("track.set_color", Accessibility);
        ins!("track.create_stack", Accessibility, MidiKeyCommands);

        ins!("mixer.get_state", Mcu, Accessibility);
        ins!("mixer.set_send", Mcu);
        ins!("mixer.set_output", Accessibility);
        ins!("mixer.set_input", Accessibility);
        ins!("mixer.get_channel_strip", Mcu, Accessibility);
        ins!("mixer.set_output_volume", Mcu);
        ins!("mixer.get_bus_routing", Accessibility);
        ins!("mixer.toggle_eq", Mcu, Accessibility);
        ins!("mixer.reset_strip", Mcu, Accessibility);

        ins!("midi.send_note.keycmd", MidiKeyCommands);
        ins!("midi.send_chord.keycmd", MidiKeyCommands);
        ins!("midi.send_cc.keycmd", MidiKeyCommands);
        ins!("midi.send_program_change.keycmd", MidiKeyCommands);
        ins!("midi.send_pitch_bend.keycmd", MidiKeyCommands);
        ins!("midi.send_aftertouch.keycmd", MidiKeyCommands);
        ins!("midi.play_sequence.keycmd", MidiKeyCommands);

        ins!("view.toggle_score_editor", MidiKeyCommands, CgEvent);
        ins!("view.toggle_step_editor", MidiKeyCommands, CgEvent);
        ins!("view.toggle_smart_controls", MidiKeyCommands);
        ins!("view.toggle_automation", MidiKeyCommands);
        ins!("view.toggle_plugin_windows", MidiKeyCommands);

        ins!("region.select", Accessibility);
        ins!("region.select_last", Accessibility);
        ins!("region.move_to_playhead", Accessibility);
        ins!("region.loop", Accessibility);
        ins!("region.set_name", Accessibility);
        ins!("region.move", Accessibility);
        ins!("region.resize", Accessibility);

        ins!("plugin.list", Accessibility);
        ins!("plugin.set_param", Scripter);

        ins!("automation.get_mode", Accessibility);
        ins!("automation.set_mode", Mcu, MidiKeyCommands, CgEvent);
        ins!("automation.get_parameter", Accessibility);

        ins!("note.up_semitone", MidiKeyCommands, CgEvent);
        ins!("note.down_semitone", MidiKeyCommands, CgEvent);
        ins!("note.up_octave", MidiKeyCommands, CgEvent);
        ins!("note.down_octave", MidiKeyCommands, CgEvent);

        m.insert("system.health", vec![]);
        m.insert("system.cache_state", vec![]);
        m.insert("system.refresh", vec![]);
        m.insert("system.permissions", vec![]);
        m.insert("project.is_running", vec![]);

        m
    });
    &TABLE
}

pub fn channel_result_to_honest(result: ChannelResult) -> HonestResult {
    match result {
        ChannelResult::Success {
            message,
            verified,
            reason,
            detail,
        } => {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&message)
                && v.get("success").is_some()
                && let Ok(h) = serde_json::from_value::<HonestResult>(v)
            {
                return h;
            }
            HonestResult {
                success: true,
                verified,
                reason,
                error: None,
                detail: detail.or_else(|| Some(serde_json::json!({ "message": message }))),
            }
        }
        ChannelResult::Error(e) => {
            if logicx_core::is_terminal_state_c(&e)
                && let Ok(v) = serde_json::from_str::<serde_json::Value>(&e)
                && let Ok(h) = serde_json::from_value::<HonestResult>(v)
            {
                return h;
            }
            HonestResult::failed(e)
        }
    }
}

pub fn operation_for_tool(tool: &str, command: &str) -> String {
    match tool {
        "logic_transport" => format!("transport.{command}"),
        "logic_tracks" => match command {
            "mute" => "track.set_mute".into(),
            "solo" => "track.set_solo".into(),
            "arm" => "track.set_arm".into(),
            "arm_only" => "track.arm_only".into(),
            "set_automation" => "track.set_automation".into(),
            "list_library" => "library.list".into(),
            "scan_library" => "library.scan_all".into(),
            "scan_plugin_presets" => "plugin.scan_presets".into(),
            "resolve_path" => "library.resolve_path".into(),
            _ => format!("track.{command}"),
        },
        "logic_mixer" => format!("mixer.{command}"),
        "logic_midi" if command == "mmc_record" => "mmc.record_strobe".into(),
        "logic_midi" if command == "mmc_record_exit" => "mmc.record_exit".into(),
        "logic_midi" if command.starts_with("mmc_") => command.replacen("mmc_", "mmc.", 1),
        "logic_midi" => format!("midi.{command}"),
        "logic_edit" => format!("edit.{command}"),
        "logic_navigate" if command == "goto_bar" => "transport.goto_position".into(),
        "logic_navigate" if command == "toggle_view" => "view.toggle_mixer".into(), // remapped in channel
        "logic_navigate" if command == "set_zoom" => "nav.set_zoom_level".into(),
        "logic_navigate" => format!("nav.{command}"),
        "logic_project" if command == "get_regions" => "region.get_regions".into(),
        "logic_project" if command == "is_running" => "project.is_running".into(),
        "logic_project" => format!("project.{command}"),
        _ => format!("{tool}.{command}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::ChannelId;

    #[test]
    fn tracks_command_aliases_match_reference() {
        assert_eq!(operation_for_tool("logic_tracks", "mute"), "track.set_mute");
        assert_eq!(operation_for_tool("logic_tracks", "solo"), "track.set_solo");
        assert_eq!(operation_for_tool("logic_tracks", "arm"), "track.set_arm");
        assert_eq!(
            operation_for_tool("logic_tracks", "list_library"),
            "library.list"
        );
        assert_eq!(
            operation_for_tool("logic_tracks", "scan_plugin_presets"),
            "plugin.scan_presets"
        );
        assert_eq!(
            operation_for_tool("logic_midi", "mmc_record_exit"),
            "mmc.record_exit"
        );
    }

    #[test]
    fn set_tempo_ax_only() {
        let chain = routing_table().get("transport.set_tempo").unwrap();
        assert!(chain.iter().all(|c| *c == ChannelId::Accessibility));
    }

    #[test]
    fn mixer_mcu_only() {
        let chain = routing_table().get("mixer.set_volume").unwrap();
        assert_eq!(chain.as_slice(), &[ChannelId::Mcu]);
    }

    fn healthy() -> ChannelHealth {
        ChannelHealth::healthy("mock ok")
    }

    fn unavailable() -> ChannelHealth {
        ChannelHealth::unavailable("mock unavailable")
    }

    fn manual_validation() -> ChannelHealth {
        ChannelHealth::manual_validation_required("manual validation required")
    }

    #[test]
    fn bypass_readiness_ops_exactly_seven_keycmd_suffixes() {
        assert_eq!(super::bypass_readiness_ops().len(), 7);
    }

    #[test]
    fn routing_table_keycmd_suffixes_in_bypass_set() {
        let bypass: std::collections::HashSet<_> =
            super::bypass_readiness_ops().iter().copied().collect();
        for (op, chain) in super::routing_table().iter() {
            if op.starts_with("midi.") && op.ends_with(".keycmd") {
                assert!(
                    bypass.contains(*op),
                    "{op} missing from bypass_readiness_ops"
                );
                assert_eq!(chain.as_slice(), &[ChannelId::MidiKeyCommands]);
            }
        }
    }

    #[test]
    fn mock_bypass_op_routes_through_manual_validation_keycmd() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        let result = super::route_chain(
            "midi.send_cc.keycmd",
            &Params::from([
                ("controller".to_string(), "74".to_string()),
                ("value".to_string(), "127".to_string()),
            ]),
            |ch| {
                if ch == ChannelId::MidiKeyCommands {
                    manual_validation()
                } else {
                    healthy()
                }
            },
            |ch, op, _| {
                calls.borrow_mut().push((ch, op.to_string()));
                ChannelResult::ok("keycmd send")
            },
        );
        assert!(result.is_success());
        assert_eq!(
            *calls.borrow(),
            vec![(
                ChannelId::MidiKeyCommands,
                "midi.send_cc.keycmd".to_string()
            )]
        );
    }

    #[test]
    fn mock_bypass_op_port_unavailable_is_terminal() {
        let result = super::route_chain(
            "midi.send_cc.keycmd",
            &Params::new(),
            |_| unavailable(),
            |_, _, _| ChannelResult::ok("should not run"),
        );
        assert!(!result.is_success());
        if let ChannelResult::Error(msg) = &result {
            assert!(logicx_core::is_terminal_state_c(msg));
            let v: serde_json::Value = serde_json::from_str(msg).unwrap();
            assert_eq!(v["error"], "port_unavailable");
            assert_eq!(v["operation"], "midi.send_cc.keycmd");
        } else {
            panic!("expected error envelope");
        }
    }

    #[test]
    fn mock_non_bypass_skips_manual_validation_keycmd() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        let result = super::route_chain(
            "edit.undo",
            &Params::new(),
            |ch| {
                if ch == ChannelId::MidiKeyCommands {
                    manual_validation()
                } else {
                    healthy()
                }
            },
            |ch, _, _| {
                calls.borrow_mut().push(ch);
                if ch == ChannelId::MidiKeyCommands {
                    ChannelResult::err(
                        "midi_key_commands requires operator approval — run logic_system approve_channel",
                    )
                } else if ch == ChannelId::CgEvent {
                    ChannelResult::ok("cg undo")
                } else {
                    ChannelResult::err("unexpected")
                }
            },
        );
        assert!(result.is_success());
        assert_eq!(
            *calls.borrow(),
            vec![ChannelId::MidiKeyCommands, ChannelId::CgEvent]
        );
    }

    #[test]
    fn mock_mixer_routes_to_mcu_only() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        let result = super::route_chain(
            "mixer.set_volume",
            &Params::from([
                ("index".to_string(), "0".to_string()),
                ("volume".to_string(), "0.7".to_string()),
            ]),
            |_| healthy(),
            |ch, _op, _| {
                calls.borrow_mut().push(ch);
                if ch == ChannelId::Mcu {
                    ChannelResult::ok("mock mcu")
                } else {
                    ChannelResult::err("unexpected channel")
                }
            },
        );
        assert!(result.is_success());
        assert_eq!(*calls.borrow(), vec![ChannelId::Mcu]);
    }

    #[test]
    fn mock_mixer_no_fallback_when_mcu_unavailable() {
        let result = super::route_chain(
            "mixer.set_volume",
            &Params::new(),
            |ch| {
                if ch == ChannelId::Mcu {
                    unavailable()
                } else {
                    healthy()
                }
            },
            |_, _, _| ChannelResult::ok("should not run"),
        );
        assert!(!result.is_success());
    }

    #[test]
    fn mock_edit_undo_falls_back_from_keycmd_to_cgevent() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        let result = super::route_chain(
            "edit.undo",
            &Params::new(),
            |ch| {
                if ch == ChannelId::MidiKeyCommands {
                    unavailable()
                } else {
                    healthy()
                }
            },
            |ch, _, _| {
                calls.borrow_mut().push(ch);
                if ch == ChannelId::CgEvent {
                    ChannelResult::ok("cg undo")
                } else {
                    ChannelResult::err("keycmd fail")
                }
            },
        );
        assert!(result.is_success());
        assert_eq!(*calls.borrow(), vec![ChannelId::CgEvent]);
    }

    #[test]
    fn mock_terminal_state_c_does_not_fall_through() {
        use logicx_core::{HonestError, encode_state_c};
        use std::cell::Cell;
        let terminal = encode_state_c(
            HonestError::ElementNotFound,
            None,
            Some("no track at index 99999"),
            Default::default(),
        );
        let mcu_calls = Cell::new(0);
        let result = super::route_chain(
            "track.set_mute",
            &Params::from([
                ("index".to_string(), "99999".to_string()),
                ("enabled".to_string(), "true".to_string()),
            ]),
            |_| healthy(),
            |ch, _, _| {
                if ch == ChannelId::Accessibility {
                    ChannelResult::err(terminal.clone())
                } else {
                    mcu_calls.set(mcu_calls.get() + 1);
                    ChannelResult::ok("mcu lie")
                }
            },
        );
        assert!(!result.is_success());
        assert_eq!(mcu_calls.get(), 0);
    }

    #[test]
    fn mock_non_terminal_ax_write_falls_through() {
        use std::cell::Cell;
        let mcu_calls = Cell::new(0);
        let result = super::route_chain(
            "track.set_mute",
            &Params::from([
                ("index".to_string(), "0".to_string()),
                ("enabled".to_string(), "true".to_string()),
            ]),
            |_| healthy(),
            |ch, _, _| {
                if ch == ChannelId::Accessibility {
                    ChannelResult::err("ax_write_failed: focus stolen".to_string())
                } else {
                    mcu_calls.set(mcu_calls.get() + 1);
                    ChannelResult::ok("mcu ok")
                }
            },
        );
        assert!(result.is_success());
        assert_eq!(mcu_calls.get(), 1);
    }

    #[test]
    fn transport_pause_routes_coremidi_first() {
        let chain = routing_table().get("transport.pause").unwrap();
        assert_eq!(chain.first(), Some(&ChannelId::CoreMidi));
    }

    #[test]
    fn channel_result_parses_state_b_envelope() {
        use logicx_core::{HonestReason, encode_state_b};
        use serde_json::Map;
        let mut extras = Map::new();
        extras.insert("operation".into(), serde_json::json!("project.save"));
        let envelope = encode_state_b(HonestReason::ReadbackUnavailable, extras);
        let result = super::channel_result_to_honest(ChannelResult::Success {
            message: envelope,
            verified: Some(false),
            reason: Some("readback_unavailable".into()),
            detail: None,
        });
        assert!(result.success);
        assert_eq!(result.verified, Some(false));
        assert_eq!(result.reason.as_deref(), Some("readback_unavailable"));
    }

    #[test]
    fn mock_set_tempo_ax_only_no_mcu() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        let result = super::route_chain(
            "transport.set_tempo",
            &Params::from([("bpm".to_string(), "128.5".to_string())]),
            |_| healthy(),
            |ch, _, _| {
                calls.borrow_mut().push(ch);
                ChannelResult::Success {
                    message: "ok".into(),
                    verified: Some(true),
                    reason: None,
                    detail: None,
                }
            },
        );
        assert!(result.is_success());
        assert_eq!(*calls.borrow(), vec![ChannelId::Accessibility]);
    }
}
