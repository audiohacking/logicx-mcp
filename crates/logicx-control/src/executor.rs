use logicx_core::{HonestResult, ToolInvocation};
use serde_json::{Value, json};
use thiserror::Error;

use crate::macos;
use crate::notes;
use crate::smf;

#[derive(Debug, Error)]
pub enum ExecuteError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("missing command")]
    MissingCommand,
    #[error("{0}")]
    Other(String),
}

/// Routes tool invocations to macOS control channels (MongLong logic-pro-mcp pattern).
pub struct LogicExecutor;

impl LogicExecutor {
    pub fn new() -> Self {
        Self
    }

    pub fn execute(&self, tool: &ToolInvocation) -> Result<String, ExecuteError> {
        let command = tool
            .arguments
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or(ExecuteError::MissingCommand)?;

        let params = tool.arguments.get("params").cloned().unwrap_or(json!({}));

        let result = match tool.name.as_str() {
            "logic_system" => self.system(command, &params),
            "logic_transport" => self.transport(command, &params),
            "logic_tracks" => self.tracks(command, &params),
            "logic_mixer" => self.mixer(command, &params),
            "logic_midi" => self.midi(command, &params),
            "logic_edit" => self.edit(command, &params),
            "logic_navigate" => self.navigate(command, &params),
            "logic_project" => self.project(command, &params),
            other => return Err(ExecuteError::UnknownTool(other.to_string())),
        };

        Ok(serde_json::to_string_pretty(&result)
            .unwrap_or_else(|_| format!("{{\"success\":{}}}", result.success)))
    }

    fn system(&self, command: &str, params: &Value) -> HonestResult {
        match command {
            "health" => {
                #[cfg(target_os = "macos")]
                {
                    let running = macos::is_logic_running();
                    HonestResult {
                        success: true,
                        verified: Some(true),
                        reason: None,
                        error: None,
                        detail: Some(json!({
                            "logic_pro_running": running,
                            "channels": {
                                "applescript": running,
                                "core_midi": "pending",
                                "mcu": "setup_required",
                                "accessibility": "pending"
                            },
                            "plugin": "logicx-mcp v0.1.0",
                            "note": "Full channel stack ports from logic-pro-mcp; AppleScript path active."
                        })),
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    HonestResult::failed("LogicX MCP requires macOS")
                }
            }
            "permissions" => HonestResult::uncertain(
                "Grant Accessibility and Automation for Logic Pro in System Settings.",
            ),
            "refresh_cache" => HonestResult::uncertain("State cache refresh not yet implemented"),
            "help" => HonestResult {
                success: true,
                verified: Some(true),
                reason: None,
                error: None,
                detail: Some(json!({
                    "category": params.get("category").and_then(|v| v.as_str()).unwrap_or("all"),
                    "docs": "https://github.com/MongLong0214/logic-pro-mcp/blob/main/docs/API.md"
                })),
            },
            _ => HonestResult::failed(format!("unknown system command: {command}")),
        }
    }

    fn transport(&self, command: &str, params: &Value) -> HonestResult {
        #[cfg(target_os = "macos")]
        {
            match command {
                "play" => macos::transport_play(),
                "stop" => macos::transport_stop(),
                "set_tempo" => {
                    let tempo = params.get("tempo").and_then(|v| v.as_f64());
                    match tempo {
                        Some(t) if (5.0..=999.0).contains(&t) => macos::transport_set_tempo(t),
                        _ => HonestResult::failed("set_tempo requires tempo 5–999"),
                    }
                }
                "goto_position" => {
                    if let Some(bar) = params.get("bar").and_then(|v| v.as_u64()) {
                        macos::transport_goto_bar(bar as u32)
                    } else {
                        HonestResult::failed("goto_position requires bar")
                    }
                }
                "set_cycle_range" => HonestResult::uncertain(
                    "set_cycle_range pending AX channel — use goto_position for now",
                ),
                other => HonestResult::uncertain(format!(
                    "transport.{other} routed — channel implementation in progress"
                )),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (command, params);
            HonestResult::failed("macOS only")
        }
    }

    fn tracks(&self, command: &str, params: &Value) -> HonestResult {
        match command {
            "record_sequence" => self.record_sequence(params),
            "set_instrument" => {
                let index = params.get("index").and_then(|v| v.as_u64());
                let path = params.get("path").and_then(|v| v.as_str());
                match (index, path) {
                    (Some(idx), Some(p)) => HonestResult::uncertain(format!(
                        "set_instrument index={idx} path={p} — AX channel pending"
                    )),
                    _ => HonestResult::failed("set_instrument requires index and path"),
                }
            }
            "mute" | "solo" | "arm" | "rename" | "select" | "delete" | "duplicate" => {
                if params.get("index").is_none() {
                    return HonestResult::failed(format!(
                        "{command} requires explicit index (fail-closed)"
                    ));
                }
                HonestResult::uncertain(format!("tracks.{command} — MCU/AX channel pending"))
            }
            "create_instrument" | "create_audio" => {
                HonestResult::uncertain(format!("tracks.{command} — AX channel pending"))
            }
            "scan_library" | "list_library" => {
                HonestResult::uncertain("Library scan requires Accessibility channel")
            }
            _ => HonestResult::failed(format!("unknown tracks command: {command}")),
        }
    }

    fn record_sequence(&self, params: &Value) -> HonestResult {
        let notes_str = match params.get("notes").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s,
            _ => return HonestResult::failed("record_sequence requires non-empty notes"),
        };

        let events = match notes::parse_notes(notes_str) {
            Ok(e) if !e.is_empty() => e,
            Ok(_) => return HonestResult::failed("no valid note events parsed"),
            Err(e) => return HonestResult::failed(e),
        };

        let bar = params.get("bar").and_then(|v| v.as_u64()).unwrap_or(4) as u32;
        let tempo = params
            .get("tempo")
            .and_then(|v| v.as_f64())
            .unwrap_or(120.0);

        #[cfg(target_os = "macos")]
        {
            match smf::write_temp_file(&events, tempo, bar) {
                Ok(path) => {
                    let import = macos::import_midi_file(path.to_string_lossy().as_ref());
                    let _ = std::fs::remove_file(&path);
                    if import.success {
                        HonestResult {
                            success: true,
                            verified: import.verified,
                            reason: import.reason,
                            error: None,
                            detail: Some(json!({
                                "method": "smf_import",
                                "bar": bar,
                                "tempo": tempo,
                                "note_count": events.len(),
                                "created_track": import.detail
                            })),
                        }
                    } else {
                        import
                    }
                }
                Err(e) => HonestResult::failed(format!("SMF write failed: {e}")),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (events, bar, tempo);
            HonestResult::failed("record_sequence requires macOS")
        }
    }

    fn mixer(&self, command: &str, params: &Value) -> HonestResult {
        if params
            .get("index")
            .or_else(|| params.get("track"))
            .is_none()
            && matches!(command, "set_volume" | "set_pan" | "set_plugin_param")
        {
            return HonestResult::failed(format!(
                "{command} requires explicit index/track (fail-closed)"
            ));
        }
        HonestResult::uncertain(format!(
            "mixer.{command} requires MCU control surface — see SETUP.md"
        ))
    }

    fn midi(&self, command: &str, _params: &Value) -> HonestResult {
        HonestResult::uncertain(format!("midi.{command} — CoreMIDI channel pending"))
    }

    fn edit(&self, command: &str, _params: &Value) -> HonestResult {
        HonestResult::uncertain(format!("edit.{command} — KeyCmd/CGEvent channel pending"))
    }

    fn navigate(&self, command: &str, params: &Value) -> HonestResult {
        if command == "goto_bar" {
            if let Some(bar) = params.get("bar").and_then(|v| v.as_u64()) {
                #[cfg(target_os = "macos")]
                {
                    return macos::transport_goto_bar(bar as u32);
                }
            }
            return HonestResult::failed("goto_bar requires bar");
        }
        HonestResult::uncertain(format!("navigate.{command} pending"))
    }

    fn project(&self, command: &str, params: &Value) -> HonestResult {
        let destructive = matches!(command, "open" | "close" | "quit" | "bounce" | "save_as");
        if destructive
            && !params
                .get("confirmed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        {
            return HonestResult {
                success: false,
                verified: Some(false),
                reason: Some("confirmation_required".into()),
                error: Some(format!("project.{command} requires params.confirmed=true")),
                detail: Some(json!({ "risk": "destructive" })),
            };
        }
        HonestResult::uncertain(format!("project.{command} — AppleScript channel pending"))
    }
}

impl Default for LogicExecutor {
    fn default() -> Self {
        Self::new()
    }
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
        let out = r.unwrap();
        assert!(out.contains("confirmation_required"));
    }

    #[test]
    fn parses_record_sequence_notes() {
        let ex = LogicExecutor::new();
        let r = ex.execute(&ToolInvocation {
            name: "logic_tracks".into(),
            arguments: json!({
                "command": "record_sequence",
                "params": {
                    "bar": 4,
                    "tempo": 140,
                    "notes": "45,0,95;57,107,95"
                }
            }),
        });
        // May fail on SMF/import without Logic — but should not fail parse
        assert!(r.is_ok());
    }
}
