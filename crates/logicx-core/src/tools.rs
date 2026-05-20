use serde_json::{Value, json};

/// Ollama-compatible tool definitions for the 8 Logic Pro dispatchers.
pub fn ollama_tool_definitions() -> Vec<Value> {
    vec![
        dispatcher_tool(
            "logic_transport",
            "Control Logic Pro transport: play, stop, record, tempo, playhead, cycle, metronome.",
            transport_commands(),
        ),
        dispatcher_tool(
            "logic_tracks",
            "Manage tracks, mute/solo/arm, MIDI composition (record_sequence), instruments, library.",
            tracks_commands(),
        ),
        dispatcher_tool(
            "logic_mixer",
            "Mixer control: volume, pan, master volume, plugin parameters via Scripter.",
            mixer_commands(),
        ),
        dispatcher_tool(
            "logic_midi",
            "Send MIDI notes, chords, CC, sysex, MMC transport commands.",
            midi_commands(),
        ),
        dispatcher_tool(
            "logic_edit",
            "Edit operations: undo, cut, copy, paste, quantize, split, join.",
            edit_commands(),
        ),
        dispatcher_tool(
            "logic_navigate",
            "Navigate arrangement: goto bar, markers, zoom, toggle views.",
            navigate_commands(),
        ),
        dispatcher_tool(
            "logic_project",
            "Project lifecycle: new, open, save, bounce, quit. Destructive ops need confirmed:true.",
            project_commands(),
        ),
        dispatcher_tool(
            "logic_system",
            "System health, permissions, cache refresh, command help.",
            system_commands(),
        ),
    ]
}

fn dispatcher_tool(name: &str, description: &str, command_enum: &[&str]) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "required": ["command"],
                "properties": {
                    "command": {
                        "type": "string",
                        "enum": command_enum,
                        "description": "Sub-command to execute"
                    },
                    "params": {
                        "type": "object",
                        "description": "Command-specific parameters",
                        "additionalProperties": true
                    }
                }
            }
        }
    })
}

fn transport_commands() -> &'static [&'static str] {
    &[
        "play",
        "stop",
        "record",
        "pause",
        "rewind",
        "fast_forward",
        "toggle_cycle",
        "toggle_metronome",
        "toggle_count_in",
        "set_tempo",
        "goto_position",
        "set_cycle_range",
        "capture_recording",
    ]
}

fn tracks_commands() -> &'static [&'static str] {
    &[
        "select",
        "create_audio",
        "create_instrument",
        "create_drummer",
        "create_external_midi",
        "delete",
        "duplicate",
        "rename",
        "mute",
        "solo",
        "arm",
        "arm_only",
        "record_sequence",
        "set_automation",
        "set_instrument",
        "list_library",
        "scan_library",
        "resolve_path",
        "scan_plugin_presets",
    ]
}

fn mixer_commands() -> &'static [&'static str] {
    &[
        "set_volume",
        "set_pan",
        "set_master_volume",
        "set_plugin_param",
        "set_send",
        "toggle_eq",
    ]
}

fn midi_commands() -> &'static [&'static str] {
    &[
        "send_note",
        "send_chord",
        "send_cc",
        "send_program_change",
        "send_pitch_bend",
        "send_aftertouch",
        "send_sysex",
        "step_input",
        "create_virtual_port",
        "mmc_play",
        "mmc_stop",
        "mmc_record",
        "mmc_record_exit",
        "mmc_pause",
        "mmc_locate",
        "play_sequence",
    ]
}

fn edit_commands() -> &'static [&'static str] {
    &[
        "undo",
        "redo",
        "cut",
        "copy",
        "paste",
        "delete",
        "select_all",
        "split",
        "join",
        "quantize",
        "bounce_in_place",
        "normalize",
        "duplicate",
        "toggle_step_input",
    ]
}

fn navigate_commands() -> &'static [&'static str] {
    &[
        "goto_bar",
        "goto_marker",
        "create_marker",
        "delete_marker",
        "rename_marker",
        "toggle_view",
        "set_zoom",
        "zoom_to_fit",
    ]
}

fn project_commands() -> &'static [&'static str] {
    &[
        "new", "open", "save", "save_as", "close", "bounce", "launch", "quit", "is_running",
        "get_regions",
    ]
}

fn system_commands() -> &'static [&'static str] {
    &[
        "health",
        "permissions",
        "refresh_cache",
        "refresh",
        "read_resource",
        "help",
        "approve_channel",
        "list_approvals",
        "restart_bridge",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_tools_defined() {
        assert_eq!(ollama_tool_definitions().len(), 8);
    }
}
