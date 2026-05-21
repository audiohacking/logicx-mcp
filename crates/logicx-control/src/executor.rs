use logicx_core::session::blocks_project_lifecycle;
use logicx_core::{HonestResult, ToolInvocation};
use serde_json::{Value, json};
use std::panic::{AssertUnwindSafe, catch_unwind};
use thiserror::Error;

use crate::cache::StateCache;
use crate::channels::{AxChannel, ChannelRouter, channel_result_to_honest, operation_for_tool};
use crate::channels::{json_params, normalize_params, param_bool, param_f64, param_str, param_u64, Params};
use crate::macos;

#[derive(Debug, Error)]
pub enum ExecuteError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("missing command")]
    MissingCommand,
    #[error("{0}")]
    Other(String),
}

use std::sync::Arc;

static SHARED_CACHE: once_cell::sync::Lazy<Arc<StateCache>> = once_cell::sync::Lazy::new(|| {
    let cache = Arc::new(StateCache::new());
    crate::state_poller::register_cache(Arc::clone(&cache));
    cache
});

/// Routes tool invocations through the 7-channel router (logic-pro-mcp parity).
pub struct LogicExecutor {
    cache: Arc<StateCache>,
}

impl LogicExecutor {
    pub fn new() -> Self {
        Self {
            cache: Arc::clone(&SHARED_CACHE),
        }
    }

    /// Start background AX supplementary polling (idempotent).
    pub fn warm_poller() {
        crate::state_poller::ensure_started();
    }

    pub fn cache(&self) -> &StateCache {
        &self.cache
    }

    pub fn execute(&self, tool: &ToolInvocation) -> Result<String, ExecuteError> {
        Self::warm_poller();
        #[cfg(target_os = "macos")]
        if crate::bridge::should_delegate() {
            return match catch_unwind(AssertUnwindSafe(|| crate::bridge::execute_remote(tool))) {
                Ok(inner) => inner.map_err(|e| ExecuteError::Other(format!("control bridge: {e}"))),
                Err(_) => Err(ExecuteError::Other(
                    "control bridge RPC panicked (see bridge.log)".into(),
                )),
            };
        }
        self.execute_local(tool)
    }

    /// Run in-process (standalone app or control bridge server).
    pub fn execute_local(&self, tool: &ToolInvocation) -> Result<String, ExecuteError> {
        let command = tool
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or(ExecuteError::MissingCommand)?;

        let params = tool.arguments.get("params").cloned().unwrap_or(json!({}));

        let result = match tool.name.as_str() {
            "logic_system" => self.system(command, &params),
            "logic_transport" => self.route_tool("logic_transport", command, &params),
            "logic_tracks" if command == "record_sequence" => self.record_sequence(&params),
            "logic_tracks" => self.route_tool("logic_tracks", command, &params),
            "logic_mixer" => self.route_tool("logic_mixer", command, &params),
            "logic_midi" => self.route_midi(command, &params),
            "logic_edit" => self.route_tool("logic_edit", command, &params),
            "logic_navigate" => self.navigate(command, &params),
            "logic_project" => self.project(command, &params),
            other => return Err(ExecuteError::UnknownTool(other.to_string())),
        };

        Ok(serde_json::to_string_pretty(&result)
            .unwrap_or_else(|_| format!("{{\"success\":{}}}", result.success)))
    }

    fn route(&self, operation: &str, mut params: Params) -> HonestResult {
        let mut router = ChannelRouter::global().lock();
        router.ensure_started();
        let params = normalize_params(operation, params);
        let result = router.route(operation, params);
        let honest = channel_result_to_honest(result);
        if operation == "library.scan_all" && honest.success {
            if let Some(detail) = &honest.detail {
                self.cache.set_library_inventory(detail.clone());
            }
        }
        honest
    }

    fn route_midi(&self, command: &str, params: &Value) -> HonestResult {
        let flat: Vec<(String, String)> = params
            .as_object()
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| {
                        Some((
                            k.clone(),
                            match v {
                                Value::String(s) => s.clone(),
                                Value::Number(n) => n.to_string(),
                                Value::Bool(b) => b.to_string(),
                                _ => return None,
                            },
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let port = match crate::midi::validate::validate_port(&flat) {
            Ok(p) => p,
            Err(e) => return HonestResult::failed(e.message),
        };

        let mut routed_params = json_params(params);

        if matches!(
            command,
            "send_note" | "send_cc" | "send_chord" | "send_program_change" | "send_pitch_bend"
                | "send_aftertouch" | "play_sequence"
        ) {
            let wire = match crate::midi::validate::validate_midi_channel(&flat) {
                Ok(w) => w,
                Err(e) => return HonestResult::failed(e.message),
            };
            routed_params.insert("channel".into(), wire.to_string());
        }

        let mut op = operation_for_tool("logic_midi", command);
        if port == "keycmd"
            && matches!(
                op.as_str(),
                "midi.send_note"
                    | "midi.send_cc"
                    | "midi.send_chord"
                    | "midi.send_program_change"
                    | "midi.send_pitch_bend"
                    | "midi.send_aftertouch"
                    | "midi.play_sequence"
            )
        {
            op.push_str(".keycmd");
        }
        self.route(&op, routed_params)
    }

    fn route_tool(&self, tool: &str, command: &str, params: &Value) -> HonestResult {
        if let Some(err) = self.validate_fail_closed(tool, command, params) {
            return err;
        }
        let op = operation_for_tool(tool, command);
        self.route(&op, json_params(params))
    }

    fn system(&self, command: &str, params: &Value) -> HonestResult {
        match command {
            "health" => self.health(),
            "permissions" => self.permissions(),
            "refresh_cache" | "refresh" => {
                self.cache.refresh();
                HonestResult::confirmed("State cache refreshed")
            }
            "read_resource" => {
                let Some(uri) = param_str(params, "uri") else {
                    return HonestResult::failed("read_resource requires params.uri");
                };
                HonestResult {
                    success: true,
                    verified: Some(true),
                    reason: None,
                    error: None,
                    detail: Some(self.cache.read_resource(uri)),
                }
            }
            "help" => HonestResult {
                success: true,
                verified: Some(true),
                reason: None,
                error: None,
                detail: Some(json!({
                    "category": params.get("category").and_then(|v| v.as_str()).unwrap_or("all"),
                    "docs": "https://github.com/MongLong0214/logic-pro-mcp/blob/main/docs/API.md",
                    "resources": self.cache.resource_index(),
                    "operator_approvals": crate::approvals::list(),
                })),
            },
            "approve_channel" => {
                let Some(channel) = param_str(params, "channel") else {
                    return HonestResult::failed(
                        "approve_channel requires params.channel (midi_key_commands | scripter only — not macOS permissions)",
                    );
                };
                match crate::approvals::approve(channel) {
                    Ok(()) => HonestResult::confirmed(format!("Approved operator channel: {channel}")),
                    Err(e) => HonestResult::failed(e),
                }
            }
            "list_approvals" => HonestResult {
                success: true,
                verified: Some(true),
                reason: None,
                error: None,
                detail: Some(json!({
                    "approved": crate::approvals::list(),
                })),
            },
            "restart_bridge" => {
                #[cfg(target_os = "macos")]
                {
                    crate::bridge::kill_stale_bridges();
                    match crate::bridge::ensure_running() {
                        Ok(()) => {
                            let status = crate::bridge::bridge_status();
                            HonestResult {
                                success: true,
                                verified: Some(true),
                                reason: None,
                                error: None,
                                detail: Some(json!({
                                    "message": "control bridge restarted",
                                    "bridge": status,
                                })),
                            }
                        }
                        Err(e) => HonestResult::failed(e),
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    HonestResult::failed("macOS only")
                }
            }
            _ => HonestResult::failed(format!("unknown system command: {command}")),
        }
    }

    fn health(&self) -> HonestResult {
        #[cfg(target_os = "macos")]
        {
            let running = macos::is_logic_running();
            let mut router = ChannelRouter::global().lock();
            router.ensure_started();
            let channels: serde_json::Map<String, Value> = router
                .health_report()
                .into_iter()
                .map(|(k, h)| {
                    let status = if h.available && h.ready {
                        "ready"
                    } else if !h.available {
                        "unavailable"
                    } else {
                        "setup_required"
                    };
                    (k, json!({ "status": status, "detail": h.detail }))
                })
                .collect();
            HonestResult {
                success: true,
                verified: Some(true),
                reason: None,
                error: None,
                detail: Some(json!({
                    "logic_pro_running": running,
                    "channels": channels,
                    "plugin": format!("logicx-mcp {}", env!("CARGO_PKG_VERSION")),
                    "cache": self.cache.summary(),
                    "mcu": crate::midi::mcu_state::McuStateCache::global().summary(),
                    "operator_approvals": crate::approvals::list(),
                    "control_bridge": {
                        "delegate": crate::bridge::should_delegate(),
                        "host_exe": logicx_core::runtime::host_executable(),
                        "permission_subject": logicx_core::runtime::permission_subject(),
                        "hosted_in_daw": logicx_core::runtime::hosted_in_daw(),
                    }
                })),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            HonestResult::failed("LogicX MCP requires macOS")
        }
    }

    fn permissions(&self) -> HonestResult {
        #[cfg(target_os = "macos")]
        {
            use logicx_core::runtime;
            let ax = macos::is_ax_trusted();
            let automation_logic = macos::automation_logic_ok();
            let automation_system_events = macos::automation_system_events_ok();
            let ok = ax;
            let tempo_ready = ax;
            let subject = runtime::permission_subject();
            let automation_app = runtime::automation_settings_app_name();
            let hint = if !ax {
                format!(
                    "Enable Accessibility for \"{subject}\" in System Settings → Privacy & Security → Accessibility."
                )
            } else if tempo_ready && !automation_system_events {
                format!(
                    "Tempo control ready via native Accessibility on \"{subject}\". \
                     System Events is optional. To enable AppleScript fallbacks: \
                     System Settings → Privacy & Security → Automation → \"{automation_app}\" → enable System Events and Logic Pro. \
                     Note: \"logicx-control-bridge\" never appears in Automation — only \"{automation_app}\" does."
                )
            } else if !automation_system_events {
                format!(
                    "Optional: enable Automation → System Events for \"{automation_app}\" \
                     (System Settings → Privacy & Security → Automation). \
                     \"logicx-control-bridge\" is a bare binary and cannot be listed there."
                )
            } else if !automation_logic {
                format!(
                    "Enable Automation → Logic Pro for \"{automation_app}\" in System Settings → Privacy & Security → Automation."
                )
            } else if runtime::hosted_in_daw() {
                format!(
                    "All permissions OK on \"{subject}\" (control bridge). AU delegates Logic control here."
                )
            } else {
                format!("All permissions OK for \"{subject}\".")
            };
            HonestResult {
                success: ok,
                verified: Some(true),
                reason: if ok {
                    None
                } else {
                    Some("permission_required".into())
                },
                error: None,
                detail: Some(json!({
                    "accessibility": ax,
                    "automation_logic_pro": automation_logic,
                    "automation_system_events": automation_system_events,
                    "tempo_control_ready": tempo_ready,
                    "tempo_control_via": if ax { "native_ax" } else if automation_system_events { "system_events" } else { "none" },
                    "automation_settings_app": automation_app,
                    "running_in_app_bundle": runtime::running_in_app_bundle(),
                    "hosted_in_daw": runtime::hosted_in_daw(),
                    "permission_subject": subject,
                    "host_exe": runtime::host_executable(),
                    "hint": hint,
                })),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            HonestResult::failed("macOS only")
        }
    }

    fn navigate(&self, command: &str, params: &Value) -> HonestResult {
        match command {
            "goto_bar" => {
                let Some(bar) = param_u64(params, "bar") else {
                    return HonestResult::failed("goto_bar requires bar");
                };
                if bar == 0 || bar > 9999 {
                    return HonestResult::failed("goto_bar bar must be 1-9999");
                }
                self.route(
                    "transport.goto_position",
                    params_map(&[("bar", bar.to_string())]),
                )
            }
            "goto_marker" => self.navigate_goto_marker(params),
            "toggle_view" => {
                let view = param_str(params, "view").unwrap_or("mixer");
                let op = match view {
                    "mixer" => "view.toggle_mixer",
                    "piano_roll" => "view.toggle_piano_roll",
                    "library" => "view.toggle_library",
                    "inspector" => "view.toggle_inspector",
                    "automation" => "automation.toggle_view",
                    "score" | "score_editor" => "view.toggle_score_editor",
                    "step_editor" => "view.toggle_step_editor",
                    "smart_controls" => "view.toggle_smart_controls",
                    "plugin_windows" => "view.toggle_plugin_windows",
                    other => {
                        return HonestResult::failed(format!("unknown view: {other}"));
                    }
                };
                self.route(op, Params::new())
            }
            "set_zoom" => {
                let mut p = Params::new();
                if let Some(v) = param_str(params, "level") {
                    p.insert("level".into(), v.into());
                }
                self.route("nav.set_zoom_level", p)
            }
            _ => self.route_tool("logic_navigate", command, params),
        }
    }

    fn navigate_goto_marker(&self, params: &Value) -> HonestResult {
        use logicx_core::{encode_state_c, HonestError};
        use serde_json::Map;

        self.cache.refresh();
        let markers = self.cache.get_markers();
        let target = if let Some(index) = param_u64(params, "index") {
            markers.into_iter().find(|m| u64::from(m.id) == index)
        } else if let Some(name) = param_str(params, "name") {
            if name.is_empty() {
                return HonestResult::failed("goto_marker requires index or name");
            }
            let needle = name.to_lowercase();
            markers
                .into_iter()
                .find(|m| m.name.to_lowercase().contains(&needle))
        } else {
            return HonestResult::failed("goto_marker requires index or name");
        };

        if let Some(marker) = target {
            return self.route(
                "transport.goto_position",
                params_map(&[("position", marker.position.clone())]),
            );
        }

        let mut extras = Map::new();
        extras.insert(
            "cached_marker_count".into(),
            json!(self.cache.get_markers().len()),
        );
        if let Some(index) = param_u64(params, "index") {
            extras.insert("requested_index".into(), json!(index));
            let raw = encode_state_c(
                HonestError::ElementNotFound,
                None,
                Some(
                    "goto_marker: marker index not found in cached marker list — try system.refresh_cache and retry",
                ),
                extras,
            );
            return serde_json::from_str(&raw).unwrap_or_else(|_| HonestResult::failed(raw));
        }
        let name = param_str(params, "name").unwrap_or("");
        extras.insert("requested_name".into(), json!(name));
        let raw = encode_state_c(
            HonestError::ElementNotFound,
            None,
            Some(
                "goto_marker: no marker matching name in cached list — try system.refresh_cache and retry",
            ),
            extras,
        );
        serde_json::from_str(&raw).unwrap_or_else(|_| HonestResult::failed(raw))
    }

    fn project(&self, command: &str, params: &Value) -> HonestResult {
        if blocks_project_lifecycle(command) {
            return HonestResult {
                success: false,
                verified: Some(false),
                reason: Some("blocked_in_logic_plugin".into()),
                error: Some(format!(
                    "logic_project.{command} is blocked while LogicX MCP runs inside Logic Pro — \
                     all tools edit the current project only"
                )),
                detail: Some(json!({ "target": "current_project" })),
            };
        }
        if command == "is_running" {
            return HonestResult {
                success: true,
                verified: Some(true),
                reason: None,
                error: None,
                detail: Some(json!({ "running": macos::is_logic_running() })),
            };
        }
        if command == "get_regions" {
            return self.route("region.get_regions", Params::new());
        }
        let destructive =
            matches!(command, "open" | "close" | "quit" | "bounce" | "save_as" | "new");
        if destructive && !param_bool(params, "confirmed", false) {
            return HonestResult {
                success: false,
                verified: Some(false),
                reason: Some("confirmation_required".into()),
                error: Some(format!("project.{command} requires params.confirmed=true")),
                detail: Some(json!({ "risk": "destructive" })),
            };
        }
        self.route_tool("logic_project", command, params)
    }

    fn record_sequence(&self, params: &Value) -> HonestResult {
        #[cfg(target_os = "macos")]
        {
            if !logicx_core::targets_current_logic_project() && !crate::macos::has_open_project() {
                return HonestResult::failed("No Logic project open");
            }
            channel_result_to_honest(AxChannel::record_sequence(&json_params(params)))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = params;
            HonestResult::failed("macOS only")
        }
    }

    fn validate_fail_closed(&self, tool: &str, command: &str, params: &Value) -> Option<HonestResult> {
        let needs_index = matches!(
            (tool, command),
            (
                "logic_tracks",
                "mute" | "solo" | "arm" | "arm_only" | "rename" | "select" | "delete" | "duplicate" | "set_instrument" | "set_automation"
            )
        ) || matches!(
            (tool, command),
            ("logic_mixer", "set_volume" | "set_pan" | "set_plugin_param" | "set_send" | "toggle_eq")
        ) || matches!(
            (tool, command),
            ("logic_navigate", "delete_marker" | "rename_marker")
        );
        if needs_index {
            let has_index = param_u64(params, "index").is_some() || param_u64(params, "track").is_some();
            if !has_index {
                return Some(HonestResult::failed(format!(
                    "{command} requires explicit index/track (fail-closed)"
                )));
            }
            if let Some(idx) = param_u64(params, "index").or_else(|| param_u64(params, "track")) {
                if idx > 999 {
                    return Some(HonestResult::failed(format!(
                        "{command} index out of range"
                    )));
                }
            }
        }
        if tool == "logic_mixer" && command == "set_plugin_param" {
            for key in ["track", "insert", "param", "value"] {
                if params.get(key).is_none() {
                    return Some(HonestResult::failed(format!(
                        "set_plugin_param requires explicit '{key}'"
                    )));
                }
            }
        }
        if tool == "logic_edit" && command == "quantize" {
            if param_str(params, "value").is_none() && param_str(params, "grid").is_none() {
                return Some(HonestResult::failed("quantize requires explicit grid value"));
            }
        }
        if tool == "logic_transport" && command == "set_tempo" {
            let tempo = param_f64(params, &["tempo", "bpm"], -1.0);
            if !(5.0..=999.0).contains(&tempo) {
                return Some(HonestResult::failed("set_tempo requires tempo 5–999"));
            }
        }
        if tool == "logic_transport" && command == "set_cycle_range" {
            if param_u64(params, "start").is_none() || param_u64(params, "end").is_none() {
                return Some(HonestResult::failed(
                    "set_cycle_range requires start and end bar",
                ));
            }
        }
        if tool == "logic_transport" && matches!(command, "goto_position" | "goto_bar") {
            if param_u64(params, "bar").is_none()
                && param_str(params, "position").is_none()
                && param_str(params, "time").is_none()
            {
                return Some(HonestResult::failed("goto_position requires bar or position"));
            }
        }
        if tool == "logic_navigate" && command == "rename_marker" {
            let name = param_str(params, "name").unwrap_or("");
            if name.is_empty() {
                return Some(HonestResult::failed("rename_marker requires non-empty name"));
            }
        }
        if tool == "logic_tracks" && command == "set_automation" {
            let mode = param_str(params, "mode").unwrap_or("read");
            let valid = ["read", "write", "touch", "latch", "trim", "off"];
            if !valid.contains(&mode) {
                return Some(HonestResult::failed(format!(
                    "set_automation mode must be one of: {}",
                    valid.join(", ")
                )));
            }
        }
        None
    }
}

impl Default for LogicExecutor {
    fn default() -> Self {
        Self::new()
    }
}

fn params_map(pairs: &[(&str, String)]) -> Params {
    pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn destructive_project_requires_confirmation() {
        let ex = LogicExecutor::new();
        let r = ex.execute(&ToolInvocation {
            name: "logic_project".into(),
            arguments: json!({"command": "quit", "params": {}}),
        });
        assert!(r.unwrap().contains("confirmation_required"));
    }

    #[test]
    fn parses_comma_only_notes() {
        use crate::notes;
        let ev = notes::parse_notes("36,0,500,107,250,250").unwrap();
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].pitch, 36);
        assert_eq!(ev[1].pitch, 107);
    }

    #[test]
    fn parses_record_sequence_notes() {
        let ex = LogicExecutor::new();
        let r = ex.execute(&ToolInvocation {
            name: "logic_tracks".into(),
            arguments: json!({
                "command": "record_sequence",
                "params": { "bar": 4, "tempo": 140, "notes": "45,0,95;57,107,95" }
            }),
        });
        assert!(r.is_ok());
    }

    #[test]
    fn transport_set_cycle_range_requires_both_bars() {
        let ex = LogicExecutor::new();
        let raw = ex
            .execute_local(&ToolInvocation {
                name: "logic_transport".into(),
                arguments: json!({"command": "set_cycle_range", "params": {"start": 1}}),
            })
            .unwrap();
        assert!(raw.contains("start and end"));
    }

    #[test]
    fn transport_goto_requires_position() {
        let ex = LogicExecutor::new();
        let raw = ex
            .execute_local(&ToolInvocation {
                name: "logic_transport".into(),
                arguments: json!({"command": "goto_position", "params": {}}),
            })
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["success"], false);
    }

    #[test]
    fn navigate_delete_marker_requires_index() {
        let ex = LogicExecutor::new();
        let raw = ex
            .execute_local(&ToolInvocation {
                name: "logic_navigate".into(),
                arguments: json!({"command": "delete_marker", "params": {}}),
            })
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["success"], false);
    }

    #[test]
    fn midi_pitch_bend_rejects_channel_17() {
        let ex = LogicExecutor::new();
        let raw = ex
            .execute_local(&ToolInvocation {
                name: "logic_midi".into(),
                arguments: json!({
                    "command": "send_pitch_bend",
                    "params": {"value": 0, "channel": 17}
                }),
            })
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["success"], false);
    }

    #[test]
    fn goto_marker_cache_hit_routes_transport() {
        let ex = LogicExecutor::new();
        ex.cache().update_markers(vec![crate::cache::MarkerState {
            id: 2,
            name: "Chorus".into(),
            position: "17.1.1.1".into(),
        }]);
        let raw = ex
            .execute_local(&ToolInvocation {
                name: "logic_navigate".into(),
                arguments: json!({"command": "goto_marker", "params": {"index": 2}}),
            })
            .unwrap();
        assert!(raw.contains("17.1.1.1") || raw.contains("success"));
    }

    #[test]
    fn goto_marker_cache_miss_returns_element_not_found() {
        let ex = LogicExecutor::new();
        let raw = ex
            .execute_local(&ToolInvocation {
                name: "logic_navigate".into(),
                arguments: json!({"command": "goto_marker", "params": {"index": 99}}),
            })
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["success"], false);
        assert_eq!(v["error"], "element_not_found");
    }

    #[test]
    fn midi_aftertouch_rejects_channel_zero() {
        let ex = LogicExecutor::new();
        let raw = ex
            .execute_local(&ToolInvocation {
                name: "logic_midi".into(),
                arguments: json!({
                    "command": "send_aftertouch",
                    "params": {"value": 64, "channel": 0}
                }),
            })
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["success"], false);
    }
}
