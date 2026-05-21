use crate::approvals;
use crate::channels::{ChannelHealth, ChannelResult, Params};
use crate::midi::engine::MidiEngine;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;

pub const PORT_NAME: &str = "LogicProMCP-KeyCmd";
/// Wire byte for MIDI channel 16 (1-based).
const MIDI_CH: u8 = 15;

static ENGINE: Lazy<Arc<MidiEngine>> = Lazy::new(|| Arc::new(MidiEngine::new(PORT_NAME)));

/// Operation → CC# mapping (logic-pro-mcp PRD §4.11).
pub fn mapping_table() -> &'static HashMap<&'static str, u8> {
    static TABLE: Lazy<HashMap<&'static str, u8>> = Lazy::new(|| {
        HashMap::from([
            ("track.create_audio", 20),
            ("track.create_instrument", 21),
            ("track.create_external_midi", 22),
            ("track.duplicate", 23),
            ("track.delete", 24),
            ("track.create_stack", 25),
            ("track.create_drummer", 26),
            ("edit.undo", 30),
            ("edit.redo", 31),
            ("edit.cut", 32),
            ("edit.copy", 33),
            ("edit.paste", 34),
            ("edit.select_all", 35),
            ("edit.bounce_in_place", 37),
            ("nav.goto_marker", 38),
            ("nav.create_marker", 39),
            ("edit.quantize", 40),
            ("edit.join", 43),
            ("edit.toggle_step_input", 44),
            ("nav.delete_marker", 45),
            ("nav.zoom_to_fit", 46),
            ("nav.set_zoom_level", 47),
            ("view.toggle_step_editor", 48),
            ("view.toggle_mixer", 50),
            ("view.toggle_piano_roll", 51),
            ("view.toggle_smart_controls", 54),
            ("view.toggle_library", 55),
            ("view.toggle_inspector", 56),
            ("view.toggle_automation", 57),
            ("view.toggle_plugin_windows", 58),
            ("view.toggle_score_editor", 59),
            ("project.save", 60),
            ("project.save_as", 61),
            ("project.bounce", 62),
            ("transport.toggle_cycle", 72),
            ("transport.capture_recording", 73),
            ("automation.set_mode", 84),
            ("automation.toggle_view", 85),
            ("note.up_semitone", 90),
            ("note.down_semitone", 91),
            ("note.up_octave", 92),
            ("note.down_octave", 93),
            ("edit.delete", 94),
            ("edit.split", 95),
            ("edit.normalize", 96),
            ("edit.duplicate", 97),
            ("transport.toggle_metronome", 98),
            ("transport.toggle_count_in", 99),
        ])
    });
    &TABLE
}

/// Ops with no working non-keycmd fallback (SETUP.md §4.1 / RoutingAuditInvariantTests).
pub fn keycmd_only_ops() -> &'static [&'static str] {
    &[
        "edit.duplicate",
        "edit.normalize",
        "edit.toggle_step_input",
        "nav.goto_marker",
        "nav.delete_marker",
        "nav.set_zoom_level",
        "project.bounce",
        "transport.capture_recording",
        "automation.set_mode",
        "note.up_semitone",
        "note.down_semitone",
        "note.up_octave",
        "note.down_octave",
        "view.toggle_smart_controls",
        "view.toggle_plugin_windows",
        "view.toggle_automation",
        "track.create_stack",
    ]
}

pub fn manual_validation_detail_suffix() -> &'static str {
    "Manual MIDI Learn required — see docs/SETUP.md §4. \
     Effectively keycmd-only (no working non-keycmd fallback on Logic 12.2): \
     edit.duplicate, edit.normalize, edit.toggle_step_input, \
     nav.goto_marker, nav.delete_marker, nav.set_zoom_level, \
     project.bounce, transport.capture_recording. \
     Other preset ops have an AX/MCU/AppleScript/CGEvent fallback and do not require keycmd binding. \
     Orphans (in mappingTable + routingTable but no MCP tool exposes a call path): \
     automation.set_mode, note.up_semitone, note.up_octave, note.down_semitone, note.down_octave, \
     view.toggle_smart_controls, view.toggle_plugin_windows, view.toggle_automation (CC 57; distinct from automation.toggle_view CC 85), \
     track.create_stack."
}

pub fn send_key_command(engine: &MidiEngine, cc: u8) -> Result<(), String> {
    engine
        .send_cc(MIDI_CH, cc, 0x7F)
        .map_err(|e| e.to_string())?;
    engine
        .send_cc(MIDI_CH, cc, 0x00)
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub struct KeyCmdChannel;

impl KeyCmdChannel {
    pub fn health() -> ChannelHealth {
        if !ENGINE.is_active() {
            return ChannelHealth::unavailable("KeyCmd port not started");
        }
        if crate::approvals::is_approved("midi_key_commands") {
            return ChannelHealth::healthy(format!(
                "KeyCmd port '{PORT_NAME}' active — operator approved"
            ));
        }
        ChannelHealth::manual_validation_required(format!(
            "KeyCmd port '{PORT_NAME}' active — {detail}",
            detail = manual_validation_detail_suffix(),
        ))
    }

    pub fn execute(operation: &str, params: &Params) -> ChannelResult {
        let _ = params;
        if !approvals::is_approved("midi_key_commands") {
            return ChannelResult::err(
                "midi_key_commands requires operator approval — run logic_system approve_channel",
            );
        }
        if operation.ends_with(".keycmd") && operation.starts_with("midi.") {
            return Self::execute_direct_send(operation, params);
        }
        let Some(cc) = mapping_table().get(operation).copied() else {
            return ChannelResult::err(format!("No MIDI Key Command mapping for: {operation}"));
        };
        if let Err(e) = ENGINE.start() {
            return ChannelResult::err(e.to_string());
        }
        if let Err(e) = send_key_command(&ENGINE, cc) {
            return ChannelResult::err(format!("Failed to send key command for {operation}: {e}"));
        }
        ChannelResult::ok(format!(
            "Key command triggered: {operation} (CC {cc} CH 16)"
        ))
    }

    fn execute_direct_send(operation: &str, params: &Params) -> ChannelResult {
        if let Err(e) = ENGINE.start() {
            return ChannelResult::err(e.to_string());
        }
        match operation {
            "midi.send_cc.keycmd" => {
                let ch = params
                    .get("channel")
                    .and_then(|s| s.parse::<u8>().ok())
                    .unwrap_or(0);
                let Some(controller) = params.get("controller").and_then(|s| s.parse::<u8>().ok())
                else {
                    return ChannelResult::err("requires controller");
                };
                let value = params
                    .get("value")
                    .and_then(|s| s.parse::<u8>().ok())
                    .unwrap_or(127);
                send_bytes(&[0xB0 | (ch & 0x0F), controller, value], operation)
            }
            "midi.send_note.keycmd" => {
                let ch = params
                    .get("channel")
                    .and_then(|s| s.parse::<u8>().ok())
                    .unwrap_or(0);
                let Some(note) = params.get("note").and_then(|s| s.parse::<u8>().ok()) else {
                    return ChannelResult::err("requires note");
                };
                let velocity = params
                    .get("velocity")
                    .and_then(|s| s.parse::<u8>().ok())
                    .unwrap_or(100);
                send_bytes(&[0x90 | (ch & 0x0F), note, velocity], operation)
            }
            _ => ChannelResult::err(format!("Unknown midi.*.keycmd operation: {operation}")),
        }
    }
}

fn send_bytes(bytes: &[u8], operation: &str) -> ChannelResult {
    if ENGINE.send_bytes(bytes).is_err() {
        return ChannelResult::err(format!("Failed to send key command for {operation}"));
    }
    ChannelResult::ok(format!("Direct keycmd send: {operation}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_undo_create_audio_mixer() {
        let m = mapping_table();
        assert_eq!(m.get("edit.undo"), Some(&30));
        assert_eq!(m.get("track.create_audio"), Some(&20));
        assert_eq!(m.get("view.toggle_mixer"), Some(&50));
    }

    #[test]
    fn mapping_all_ccs_unique() {
        let m = mapping_table();
        let values: Vec<u8> = m.values().copied().collect();
        let unique: std::collections::HashSet<u8> = values.iter().copied().collect();
        assert_eq!(values.len(), unique.len(), "duplicate CC in mapping table");
    }

    #[test]
    fn mapping_count_at_least_30() {
        assert!(mapping_table().len() >= 30);
    }

    #[test]
    fn keycmd_only_list_nonempty() {
        assert!(keycmd_only_ops().len() >= 8);
    }

    #[test]
    fn health_detail_mentions_keycmd_only_ops() {
        let detail = manual_validation_detail_suffix();
        for op in [
            "edit.duplicate",
            "project.bounce",
            "transport.capture_recording",
        ] {
            assert!(detail.contains(op), "detail missing {op}");
        }
        assert!(detail.len() < 1024);
    }
}
