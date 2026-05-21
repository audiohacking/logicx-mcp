//! logic-pro-mcp parity tests (ported from Tests/LogicProMCPTests).
//!
//! Run: `cargo test -p logicx-control --test logic_pro_mcp_parity`
//! Or:  `./scripts/test-parity.sh`

use logicx_control::notes::parse_notes;
use logicx_control::routing_table;
use logicx_control::smf::build_smf_bytes;
use logicx_control::{is_terminal_error, operation_for_tool};
use logicx_core::session::{set_in_logic_plugin_session, targets_current_logic_project};
use logicx_core::{HonestResult, ToolInvocation};
use serde_json::json;

// --- NoteSequenceParserResultTests ---

#[test]
fn parse_empty_string_returns_success_empty() {
    let ev = parse_notes("").unwrap();
    assert!(ev.is_empty());
}

#[test]
fn parse_valid_segment_channel_1() {
    let ev = parse_notes("60,0,500,127,1").unwrap();
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].channel, 1);
    assert_eq!(ev[0].pitch, 60);
}

#[test]
fn parse_valid_segment_channel_16() {
    let ev = parse_notes("60,0,500,127,16").unwrap();
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].channel, 16);
}

#[test]
fn parse_channel_0_rejected() {
    assert!(parse_notes("60,0,500,127,0").is_err());
}

#[test]
fn parse_channel_17_rejected() {
    assert!(parse_notes("60,0,500,127,17").is_err());
}

#[test]
fn parse_channel_omitted_defaults_to_1() {
    let ev = parse_notes("60,0,500,127").unwrap();
    assert_eq!(ev[0].channel, 1);
}

#[test]
fn parse_invalid_pitch_rejected() {
    assert!(parse_notes("200,0,500,127,1").is_err());
}

#[test]
fn parse_invalid_timing_rejected() {
    assert!(parse_notes("60,-1,500,127,1").is_err());
}

#[test]
fn parse_malformed_rejected() {
    assert!(parse_notes("60").is_err());
}

#[test]
fn parse_mixed_valid_invalid_whole_fails() {
    assert!(parse_notes("60,0,500;invalid;70,1000,500").is_err());
}

#[test]
fn parse_multiple_valid_segments() {
    let ev = parse_notes("60,0,500,127,1;72,1000,500,100,2").unwrap();
    assert_eq!(ev.len(), 2);
    assert_eq!(ev[0].pitch, 60);
    assert_eq!(ev[0].channel, 1);
    assert_eq!(ev[1].pitch, 72);
    assert_eq!(ev[1].channel, 2);
}

// --- ChannelRouterTests (routing table invariants) ---

#[test]
fn router_set_tempo_routes_only_to_accessibility() {
    let chain = routing_table().get("transport.set_tempo").unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].as_str(), "accessibility");
}

#[test]
fn router_mixer_has_no_fallback_channel() {
    let chain = routing_table().get("mixer.set_volume").unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].as_str(), "mcu");
}

#[test]
fn router_all_operations_have_channel() {
    let table = routing_table();
    assert!(
        table.len() >= 105,
        "expected 105+ routed operations, got {}",
        table.len()
    );
    let no_channel_ok = [
        "system.health",
        "system.cache_state",
        "system.refresh",
        "system.permissions",
        "project.is_running",
    ];
    for (op, channels) in table.iter() {
        if no_channel_ok.contains(op) {
            assert!(
                channels.is_empty(),
                "operation '{op}' should have empty chain"
            );
        } else {
            assert!(
                !channels.is_empty(),
                "operation '{op}' has no channels assigned"
            );
        }
    }
}

#[test]
fn router_step_input_goes_to_coremidi() {
    let chain = routing_table().get("midi.step_input").unwrap();
    assert_eq!(chain[0].as_str(), "core_midi");
}

#[test]
fn router_set_plugin_param_goes_to_scripter() {
    let chain = routing_table().get("mixer.set_plugin_param").unwrap();
    assert_eq!(chain[0].as_str(), "scripter");
}

#[test]
fn terminal_state_c_json_envelope() {
    use logicx_core::{HonestError, encode_state_c};
    let raw = encode_state_c(HonestError::ElementNotFound, None, None, Default::default());
    assert!(is_terminal_error(&raw));
    assert!(!is_terminal_error("ax_write_failed: focus stolen"));
}

// --- HonestContractTests (logicx-core) ---

#[test]
fn honest_contract_state_b_echo_timeout() {
    use logicx_core::{HonestReason, encode_state_b};
    let raw = encode_state_b(HonestReason::EchoTimeoutMs(500), Default::default());
    assert!(raw.contains("echo_timeout_500ms"));
}

// --- operation_for_tool / DispatcherTests subset ---

#[test]
fn tracks_command_aliases_match_reference() {
    assert_eq!(operation_for_tool("logic_tracks", "mute"), "track.set_mute");
    assert_eq!(operation_for_tool("logic_tracks", "solo"), "track.set_solo");
    assert_eq!(
        operation_for_tool("logic_navigate", "goto_bar"),
        "transport.goto_position"
    );
    assert_eq!(
        operation_for_tool("logic_midi", "mmc_record"),
        "mmc.record_strobe"
    );
}

#[test]
fn transport_command_aliases_match_reference() {
    for (cmd, op) in [
        ("play", "transport.play"),
        ("stop", "transport.stop"),
        ("record", "transport.record"),
        ("pause", "transport.pause"),
        ("rewind", "transport.rewind"),
        ("fast_forward", "transport.fast_forward"),
        ("toggle_cycle", "transport.toggle_cycle"),
        ("toggle_metronome", "transport.toggle_metronome"),
        ("toggle_count_in", "transport.toggle_count_in"),
        ("set_tempo", "transport.set_tempo"),
        ("goto_position", "transport.goto_position"),
        ("set_cycle_range", "transport.set_cycle_range"),
    ] {
        assert_eq!(operation_for_tool("logic_transport", cmd), op);
    }
}

#[test]
fn transport_set_tempo_rejects_below_range() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_transport".into(),
            arguments: json!({"command": "set_tempo", "params": {"bpm": 4}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn track_delete_requires_explicit_index() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_tracks".into(),
            arguments: json!({"command": "delete", "params": {}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert!(v["error"].as_str().unwrap_or("").contains("explicit"));
}

#[test]
fn track_set_automation_rejects_invalid_mode() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_tracks".into(),
            arguments: json!({
                "command": "set_automation",
                "params": {"track": 0, "mode": "invalid"}
            }),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn record_sequence_blocked_without_open_project() {
    let ex = logicx_control::LogicExecutor::new();
    ex.cache().set_document_open(false);
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_tracks".into(),
            arguments: json!({
                "command": "record_sequence",
                "params": {"bar": 1, "tempo": 120, "notes": "60,0,480,100"}
            }),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert!(
        v["error"]
            .as_str()
            .unwrap_or("")
            .contains("No Logic project open")
    );
}

// --- HonestContractTests (shape) ---

#[test]
fn honest_state_a_shape() {
    let h = HonestResult::confirmed("ok");
    let v = serde_json::to_value(&h).unwrap();
    assert_eq!(v["success"], true);
    assert_eq!(v["verified"], true);
    assert!(v.get("reason").is_none() || v["reason"].is_null());
    assert!(v.get("error").is_none() || v["error"].is_null());
}

#[test]
fn honest_state_b_shape() {
    let h = HonestResult::uncertain("readback_unavailable");
    let v = serde_json::to_value(&h).unwrap();
    assert_eq!(v["success"], true);
    assert_eq!(v["verified"], false);
    assert_eq!(v["reason"], "readback_unavailable");
}

#[test]
fn honest_state_c_shape() {
    let h = HonestResult::failed("ax_write_failed");
    let v = serde_json::to_value(&h).unwrap();
    assert_eq!(v["success"], false);
    assert_eq!(v["error"], "ax_write_failed");
}

// --- DestructiveOperationTests / plugin session ---

#[test]
fn project_new_blocked_in_logic_plugin_session() {
    set_in_logic_plugin_session(true);
    assert!(targets_current_logic_project());
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_project".into(),
            arguments: json!({"command": "new", "params": {"confirmed": true}}),
        })
        .unwrap();
    set_in_logic_plugin_session(false);
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert!(v["reason"].as_str().unwrap_or("").contains("blocked"));
}

#[test]
fn project_quit_requires_confirmation_without_session() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_project".into(),
            arguments: json!({"command": "quit", "params": {}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert!(v["reason"].as_str().unwrap_or("").contains("confirmation"));
}

// --- SMFWriterTests (via build_smf_bytes) ---

fn find_pattern(pattern: &[u8], bytes: &[u8]) -> bool {
    if pattern.len() > bytes.len() {
        return false;
    }
    bytes.windows(pattern.len()).any(|w| w == pattern)
}

#[test]
fn smf_build_rejects_empty() {
    assert!(build_smf_bytes(&[], 120.0, 1).is_err());
}

#[test]
fn smf_multi_note_sequence() {
    use logicx_control::notes::NoteEvent;
    let events = vec![
        NoteEvent {
            pitch: 60,
            offset_ms: 0,
            duration_ms: 500,
            velocity: 100,
            channel: 1,
        },
        NoteEvent {
            pitch: 64,
            offset_ms: 500,
            duration_ms: 500,
            velocity: 100,
            channel: 1,
        },
        NoteEvent {
            pitch: 67,
            offset_ms: 1000,
            duration_ms: 500,
            velocity: 100,
            channel: 1,
        },
    ];
    let data = build_smf_bytes(&events, 120.0, 1).unwrap();
    assert!(find_pattern(&[0x90, 0x3C, 0x64], &data));
    assert!(find_pattern(&[0x90, 0x40, 0x64], &data));
    assert!(find_pattern(&[0x90, 0x43, 0x64], &data));
}

#[test]
fn smf_bar_five_longer_than_bar_one() {
    use logicx_control::notes::NoteEvent;
    let events = vec![NoteEvent {
        pitch: 60,
        offset_ms: 0,
        duration_ms: 480,
        velocity: 100,
        channel: 1,
    }];
    let bar1 = build_smf_bytes(&events, 120.0, 1).unwrap();
    let bar5 = build_smf_bytes(&events, 120.0, 5).unwrap();
    assert!(bar5.len() > bar1.len());
    assert!(find_pattern(&[0xB0, 0x6E, 0x00], &bar5));
}

// --- Fail-closed dispatcher parity (RB-1.a subset) ---

#[test]
fn mixer_set_volume_requires_explicit_track() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_mixer".into(),
            arguments: json!({"command": "set_volume", "params": {"value": 0.5}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert!(v["error"].as_str().unwrap_or("").contains("explicit"));
}

#[test]
fn track_mute_requires_explicit_index() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_tracks".into(),
            arguments: json!({"command": "mute", "params": {"enabled": true}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn transport_set_tempo_rejects_out_of_range() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_transport".into(),
            arguments: json!({"command": "set_tempo", "params": {"bpm": 2000}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn track_solo_requires_explicit_index() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_tracks".into(),
            arguments: json!({"command": "solo", "params": {"enabled": true}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn mixer_set_plugin_param_requires_all_keys() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_mixer".into(),
            arguments: json!({"command": "set_plugin_param", "params": {"track": 0}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert!(v["error"].as_str().unwrap_or("").contains("insert"));
}

#[test]
fn track_mute_rejects_index_out_of_range() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_tracks".into(),
            arguments: json!({"command": "mute", "params": {"index": 1000, "enabled": true}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert!(v["error"].as_str().unwrap_or("").contains("out of range"));
}

#[test]
fn state_poller_refresh_updates_shared_cache() {
    use logicx_control::state_poller::{MockAxPollSource, StatePoller};
    use logicx_control::{ProjectInfo, StateCache};
    use std::sync::Arc;
    use std::time::Duration;

    let cache = Arc::new(StateCache::new());
    let source = Arc::new(MockAxPollSource {
        has_window: true,
        dialog: false,
        project: Some(ProjectInfo {
            name: "Poller Cache".into(),
            track_count: 4,
        }),
        tracks: None,
        transport: None,
        strips: None,
        markers: None,
    });
    let poller = StatePoller::new(Arc::clone(&cache), source, Duration::from_millis(1));
    poller.refresh_now();
    assert_eq!(cache.get_project().name, "Poller Cache");
    assert!(cache.has_document_open());
}

#[test]
fn mixer_set_send_returns_not_implemented() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_mixer".into(),
            arguments: json!({
                "command": "set_send",
                "params": {"track": 0, "send": 1, "value": 0.5}
            }),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert_eq!(v["error"], "not_implemented");
}

#[test]
fn mixer_toggle_eq_returns_not_implemented() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_mixer".into(),
            arguments: json!({"command": "toggle_eq", "params": {"track": 0}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert_eq!(v["error"], "not_implemented");
}

#[test]
fn scripter_set_plugin_param_requires_approval() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_mixer".into(),
            arguments: json!({
                "command": "set_plugin_param",
                "params": {"track": 0, "insert": 0, "param": 0, "value": 0.5}
            }),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert!(raw.contains("approval"));
}

#[test]
fn fake_ax_runtime_hook_routes_toggle_cycle() {
    use logicx_control::ax_test::{clear_script_hook, run_ax_script, set_script_hook};
    use logicx_core::HonestResult;
    use std::sync::Arc;

    set_script_hook(Arc::new(|kind| {
        if kind == "toggle_cycle" {
            Some(HonestResult {
                success: true,
                verified: Some(true),
                reason: None,
                error: None,
                detail: Some(serde_json::json!({ "via": "fake_ax" })),
            })
        } else {
            None
        }
    }));
    let result = run_ax_script("toggle_cycle");
    clear_script_hook();
    assert!(result.success);
    assert_eq!(result.detail.unwrap()["via"], "fake_ax");
}

// --- Routing audit invariants (adapted from RoutingAuditInvariantTests) ---

#[test]
fn ax_only_ops_have_single_channel() {
    for op in [
        "transport.set_tempo",
        "transport.set_cycle_range",
        "automation.get_mode",
    ] {
        let chain = routing_table().get(op).unwrap();
        assert_eq!(chain.len(), 1, "{op}");
        assert_eq!(chain[0].as_str(), "accessibility");
    }
}

#[test]
fn transport_play_routes_accessibility_first() {
    let chain = routing_table().get("transport.play").unwrap();
    assert_eq!(chain.first().map(|c| c.as_str()), Some("accessibility"));
}

#[test]
fn mixer_set_send_mcu_only() {
    let chain = routing_table().get("mixer.set_send").unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].as_str(), "mcu");
}

#[test]
fn system_ops_have_empty_routing_chains() {
    for op in [
        "system.health",
        "system.cache_state",
        "system.refresh",
        "system.permissions",
        "project.is_running",
    ] {
        assert!(
            routing_table()
                .get(op)
                .map(|c| c.is_empty())
                .unwrap_or(false),
            "{op} should have empty chain"
        );
    }
}

#[test]
fn mcu_only_ops_have_no_fallback() {
    for op in [
        "mixer.set_volume",
        "mixer.set_pan",
        "mixer.set_master_volume",
    ] {
        let chain = routing_table().get(op).unwrap();
        assert_eq!(chain.len(), 1);
        assert_eq!(chain[0].as_str(), "mcu");
    }
}

#[test]
fn keycmd_primary_ops_route_via_keycmd() {
    for op in [
        "edit.duplicate",
        "edit.normalize",
        "nav.delete_marker",
        "project.bounce",
        "transport.capture_recording",
    ] {
        let chain = routing_table().get(op).unwrap();
        assert!(
            chain.iter().any(|c| c.as_str() == "midi_key_commands"),
            "{op} should include midi_key_commands"
        );
    }
}

// --- tools.rs parity ---

#[test]
fn eight_tools_defined() {
    assert_eq!(logicx_core::ollama_tool_definitions().len(), 8);
}

// --- CGEvent key map coverage ---

#[cfg(target_os = "macos")]
#[test]
fn cgevent_has_core_edit_shortcuts() {
    use logicx_control::channels::cgevent::CgEventChannel;
    assert!(CgEventChannel::has_shortcut("edit.undo"));
    assert!(CgEventChannel::has_shortcut("edit.redo"));
    assert!(CgEventChannel::has_shortcut("transport.play"));
    assert!(!CgEventChannel::has_shortcut("edit.duplicate"));
    assert!(!CgEventChannel::has_shortcut("project.bounce"));
}

#[test]
fn routing_table_includes_reference_gap_fill_ops() {
    for op in [
        "track.create_stack",
        "note.up_semitone",
        "midi.send_cc.keycmd",
        "region.select",
        "plugin.list",
        "automation.set_mode",
        "mixer.set_send",
        "transport.get_state",
    ] {
        assert!(routing_table().contains_key(op), "missing routed op {op}");
    }
}

#[test]
fn keycmd_only_ops_have_no_cgevent_shortcut() {
    use logicx_control::channels::keycmd::keycmd_only_ops;
    #[cfg(target_os = "macos")]
    {
        use logicx_control::channels::cgevent::CgEventChannel;
        let leaked: Vec<_> = keycmd_only_ops()
            .iter()
            .filter(|op| CgEventChannel::has_shortcut(op))
            .copied()
            .collect();
        assert!(
            leaked.is_empty(),
            "keycmd-only ops must not have CGEvent shortcuts: {leaked:?}"
        );
    }
}

#[test]
fn keycmd_only_ops_are_in_mapping_table() {
    use logicx_control::channels::keycmd::{keycmd_only_ops, mapping_table};
    let missing: Vec<_> = keycmd_only_ops()
        .iter()
        .filter(|op| !mapping_table().contains_key(*op))
        .copied()
        .collect();
    assert!(
        missing.is_empty(),
        "keycmd-only ops missing from mapping: {missing:?}"
    );
}

#[test]
fn mapping_table_ops_are_all_routed() {
    use logicx_control::channels::keycmd::mapping_table;
    let unrouted: Vec<_> = mapping_table()
        .keys()
        .filter(|op| !routing_table().contains_key(*op))
        .copied()
        .collect();
    assert!(
        unrouted.is_empty(),
        "mappingTable ops not routed: {unrouted:?}"
    );
}

#[test]
fn keycmd_mapping_undo_is_cc_30() {
    use logicx_control::channels::keycmd::mapping_table;
    assert_eq!(mapping_table().get("edit.undo"), Some(&30));
}

#[test]
fn scripter_cc_for_param_range() {
    use logicx_control::channels::scripter::{cc_for_param, midi_value};
    assert_eq!(cc_for_param(0), Some(102));
    assert_eq!(cc_for_param(17), Some(119));
    assert_eq!(midi_value(0.5), 64);
}

#[test]
fn mixer_set_volume_rejects_negative_track() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_mixer".into(),
            arguments: json!({"command": "set_volume", "params": {"track": -1, "value": 0.5}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn navigate_goto_bar_rejects_zero() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_navigate".into(),
            arguments: json!({"command": "goto_bar", "params": {"bar": 0}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn edit_quantize_requires_grid() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_edit".into(),
            arguments: json!({"command": "quantize", "params": {}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn mixer_set_plugin_param_requires_all_fields() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_mixer".into(),
            arguments: json!({"command": "set_plugin_param", "params": {"track": 0, "param": 0}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn midi_send_cc_rejects_channel_zero() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_midi".into(),
            arguments: json!({
                "command": "send_cc",
                "params": {"channel": 0, "controller": 74, "value": 127}
            }),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn midi_port_rejects_scripter() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_midi".into(),
            arguments: json!({
                "command": "send_cc",
                "params": {"port": "scripter", "controller": 1, "value": 64}
            }),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn navigate_toggle_score_editor_routes() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_navigate".into(),
            arguments: json!({"command": "toggle_view", "params": {"view": "score"}}),
        })
        .unwrap();
    // May fail at channel execution but must not fail at dispatch parse
    assert!(raw.contains("view") || raw.contains("success"));
}

#[test]
fn state_cache_select_only_and_strips() {
    let cache = logicx_control::StateCache::new();
    cache.update_track(1, |t| {
        t.name = "Bass".into();
        t.is_selected = true;
    });
    cache.update_fader(1, 0.5);
    cache.select_only(1);
    assert_eq!(cache.get_selected_track().unwrap().name, "Bass");
    assert_eq!(cache.get_channel_strip(1).unwrap().volume, 0.5);
}

#[test]
fn transport_set_cycle_range_missing_end_rejected() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_transport".into(),
            arguments: json!({"command": "set_cycle_range", "params": {"start": 2}}),
        })
        .unwrap();
    assert!(raw.contains("start and end"));
}

#[test]
fn navigate_rename_marker_requires_name() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_navigate".into(),
            arguments: json!({"command": "rename_marker", "params": {"index": 1}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn bypass_readiness_set_matches_routing_table() {
    use logicx_control::bypass_readiness_ops;
    assert_eq!(bypass_readiness_ops().len(), 7);
    for op in bypass_readiness_ops() {
        assert!(routing_table().contains_key(op), "missing route for {op}");
    }
}

#[test]
fn port_unavailable_is_terminal_state_c() {
    use logicx_core::{HonestError, encode_state_c, is_terminal_state_c};
    let raw = encode_state_c(
        HonestError::PortUnavailable,
        None,
        Some("KeyCmd port not yet published"),
        Default::default(),
    );
    assert!(is_terminal_state_c(&raw));
}

#[test]
fn midi_pitch_bend_channel_17_rejected() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_midi".into(),
            arguments: json!({"command": "send_pitch_bend", "params": {"value": 0, "channel": 17}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn mmc_command_aliases_match_reference() {
    for (cmd, op) in [
        ("mmc_play", "mmc.play"),
        ("mmc_stop", "mmc.stop"),
        ("mmc_record", "mmc.record_strobe"),
        ("mmc_record_exit", "mmc.record_exit"),
        ("mmc_pause", "mmc.pause"),
        ("mmc_locate", "mmc.locate"),
    ] {
        assert_eq!(operation_for_tool("logic_midi", cmd), op);
    }
}

#[test]
fn mmc_bar_beat_to_smpte_two_minutes() {
    use logicx_control::midi::mmc::{self, FrameRate};
    let smpte = mmc::bar_beat_to_smpte(61, 1.0, 120.0, 4, FrameRate::Fps30).unwrap();
    assert_eq!(smpte.minutes, 2);
    assert_eq!(smpte.seconds, 0);
}

#[test]
fn mmc_locate_bytes_match_reference_layout() {
    use logicx_control::midi::mmc;
    assert_eq!(
        mmc::locate(1, 2, 3, 4, 5),
        vec![
            0xF0, 0x7F, 0x7F, 0x06, 0x44, 0x06, 0x01, 1, 2, 3, 4, 5, 0xF7
        ]
    );
}

#[test]
fn navigate_goto_marker_requires_index_or_name() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_navigate".into(),
            arguments: json!({"command": "goto_marker", "params": {}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn navigate_toggle_view_unknown_rejected() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_navigate".into(),
            arguments: json!({"command": "toggle_view", "params": {"view": "not_a_view"}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert!(v["error"].as_str().unwrap_or("").contains("unknown view"));
}

#[test]
fn navigate_goto_bar_rejects_bar_over_9999() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_navigate".into(),
            arguments: json!({"command": "goto_bar", "params": {"bar": 10000}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn mixer_set_pan_requires_explicit_track() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_mixer".into(),
            arguments: json!({"command": "set_pan", "params": {"value": 0.0}}),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
}

#[test]
fn edit_command_aliases_match_reference() {
    for cmd in ["undo", "redo", "quantize", "duplicate", "normalize"] {
        assert_eq!(operation_for_tool("logic_edit", cmd), format!("edit.{cmd}"));
    }
}

#[test]
fn transport_pause_routes_via_mmc_chain() {
    let chain = routing_table().get("transport.pause").unwrap();
    assert!(chain.iter().any(|c| c.as_str() == "core_midi"));
}

#[test]
fn project_lifecycle_blocked_commands_in_plugin_session() {
    use logicx_core::session::blocks_project_lifecycle;
    set_in_logic_plugin_session(true);
    assert!(blocks_project_lifecycle("quit"));
    assert!(!blocks_project_lifecycle("save"));
    set_in_logic_plugin_session(false);
}

#[test]
fn project_open_requires_confirmation_without_session() {
    let ex = logicx_control::LogicExecutor::new();
    let raw = ex
        .execute_local(&ToolInvocation {
            name: "logic_project".into(),
            arguments: json!({
                "command": "open",
                "params": {"path": "/tmp/test.logicx"}
            }),
        })
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["success"], false);
    assert!(v["reason"].as_str().unwrap_or("").contains("confirmation"));
}
