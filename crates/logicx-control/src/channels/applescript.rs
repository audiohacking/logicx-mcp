use crate::channels::{ChannelHealth, ChannelResult, Params};
use crate::macos;
use logicx_core::session::blocks_project_lifecycle;
use logicx_core::{HonestReason, encode_state_b};
use serde_json::{Map, Value, json};

pub struct AppleScriptChannel;

impl AppleScriptChannel {
    pub fn health() -> ChannelHealth {
        if macos::is_logic_running() && macos::automation_ok() {
            ChannelHealth::healthy("AppleScript ready")
        } else if macos::is_logic_running() {
            ChannelHealth::unavailable("Automation permission required for Logic Pro")
        } else {
            ChannelHealth::unavailable("Logic Pro is not running")
        }
    }

    pub fn execute(operation: &str, params: &Params) -> ChannelResult {
        if let Some(blocked) = block_project_lifecycle_op(operation) {
            return blocked;
        }
        match operation {
            "project.new" => wrap_mutating(
                run(r#"tell application "Logic Pro"
    activate
    make new document
    return "OK"
end tell"#),
                operation,
                Map::new(),
            ),
            "project.open" => {
                let Some(path) = params.get("path") else {
                    return ChannelResult::err("project.open requires path");
                };
                let escaped = escape(path);
                let mut extras = Map::new();
                extras.insert("path".into(), json!(path));
                wrap_mutating(
                    run(&format!(
                        "do shell script \"open '{escaped}'\"\ntell application \"Logic Pro\" to activate\nreturn \"OK\""
                    )),
                    operation,
                    extras,
                )
            }
            "project.close" => {
                let saving = params.get("saving").map(String::as_str).unwrap_or("yes");
                let mut extras = Map::new();
                extras.insert("saving".into(), json!(saving));
                wrap_mutating(
                    run(&project_close_script(close_saving_clause(saving))),
                    operation,
                    extras,
                )
            }
            "project.save" => wrap_mutating(
                run(r#"tell application "Logic Pro"
    if (count of documents) > 0 then save front document
    return "OK"
end tell"#),
                operation,
                Map::new(),
            ),
            "project.save_as" => {
                let Some(path) = params.get("path") else {
                    return ChannelResult::err("project.save_as requires path");
                };
                let escaped = escape(path);
                let mut extras = Map::new();
                extras.insert("path".into(), json!(path));
                wrap_mutating(
                    run(&format!(
                        r#"tell application "Logic Pro"
    save front document in (POSIX file "{escaped}")
    return "OK"
end tell"#
                    )),
                    operation,
                    extras,
                )
            }
            "project.launch" => run(r#"tell application "Logic Pro" to activate"#),
            "project.quit" => run(r#"tell application "Logic Pro" to quit"#),
            "project.bounce" => run(r#"tell application "Logic Pro" to activate
tell application "System Events" to keystroke "b" using {command down, option down}
return "OK""#),
            "transport.stop" => wrap_mutating(
                run(r#"tell application id "com.apple.logic10" to stop"#),
                operation,
                Map::new(),
            ),
            "transport.record" => wrap_mutating(
                run(r#"tell application id "com.apple.logic10" to record"#),
                operation,
                Map::new(),
            ),
            "transport.play" | "transport.pause" => {
                ChannelResult::err(format!("Unsupported AppleScript operation: {operation}"))
            }
            _ => ChannelResult::err(format!("Unsupported AppleScript operation: {operation}")),
        }
    }
}

fn wrap_mutating(
    result: ChannelResult,
    operation: &str,
    extras: Map<String, Value>,
) -> ChannelResult {
    if !result.is_success() {
        return result;
    }
    let raw = match result {
        ChannelResult::Success { message, .. } => message,
        ChannelResult::Error(_) => return result,
    };
    if is_already_envelope(&raw) {
        return ChannelResult::Success {
            message: raw,
            verified: Some(false),
            reason: Some("readback_unavailable".into()),
            detail: None,
        };
    }
    let mut merged = extras;
    merged.insert("operation".into(), json!(operation));
    merged.insert("method".into(), json!("applescript"));
    merged.insert("raw".into(), json!(raw));
    let envelope = encode_state_b(HonestReason::ReadbackUnavailable, merged);
    ChannelResult::Success {
        message: envelope,
        verified: Some(false),
        reason: Some("readback_unavailable".into()),
        detail: None,
    }
}

fn is_already_envelope(raw: &str) -> bool {
    serde_json::from_str::<Value>(raw)
        .ok()
        .is_some_and(|v| v.get("success").is_some())
}

fn close_saving_clause(saving: &str) -> &'static str {
    match saving {
        "no" | "false" => "saving no",
        "ask" => "saving ask",
        _ => "saving yes",
    }
}

fn project_close_script(clause: &str) -> String {
    format!(
        r#"tell application "Logic Pro"
    if (count of documents) > 0 then
        close front document {clause}
    end if
    return "OK"
end tell"#
    )
}

fn block_project_lifecycle_op(operation: &str) -> Option<ChannelResult> {
    let command = operation.strip_prefix("project.")?;
    if blocks_project_lifecycle(command) {
        return Some(ChannelResult::err(format!(
            "{operation} blocked while LogicX MCP is embedded in Logic Pro (current project only)"
        )));
    }
    None
}

fn run(script: &str) -> ChannelResult {
    match macos::run_osascript_output(script) {
        Ok(out) if out.status.starts_with("OK") || out.status.is_empty() => {
            ChannelResult::ok(out.status)
        }
        Ok(out) => ChannelResult::err(out.status),
        Err(e) => {
            let h = macos::map_script_error("applescript", e);
            ChannelResult::err(h.error.unwrap_or_else(|| "failed".into()))
        }
    }
}

fn escape(path: &str) -> String {
    path.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::{ChannelResult, Params};

    #[test]
    fn escape_doubles_backslashes_and_quotes() {
        assert_eq!(escape(r#"C:\path\file"#), r"C:\\path\\file");
        assert_eq!(escape(r#"/tmp/a"b.logicx"#), r#"/tmp/a\"b.logicx"#);
    }

    #[test]
    fn close_saving_clause_variants() {
        assert_eq!(close_saving_clause("yes"), "saving yes");
        assert_eq!(close_saving_clause("no"), "saving no");
        assert_eq!(close_saving_clause("false"), "saving no");
        assert_eq!(close_saving_clause("ask"), "saving ask");
        assert_eq!(close_saving_clause("anything"), "saving yes");
    }

    #[test]
    fn project_close_script_includes_clause() {
        let script = project_close_script("saving no");
        assert!(script.contains("close front document saving no"));
    }

    #[test]
    fn wrap_mutating_produces_state_b_envelope() {
        let wrapped = wrap_mutating(ChannelResult::ok("OK"), "project.save", Map::new());
        let ChannelResult::Success {
            message,
            verified,
            reason,
            ..
        } = wrapped
        else {
            panic!("expected success");
        };
        assert_eq!(verified, Some(false));
        assert_eq!(reason.as_deref(), Some("readback_unavailable"));
        let v: Value = serde_json::from_str(&message).unwrap();
        assert_eq!(v["success"], true);
        assert_eq!(v["verified"], false);
        assert_eq!(v["reason"], "readback_unavailable");
        assert_eq!(v["operation"], "project.save");
        assert_eq!(v["method"], "applescript");
        assert_eq!(v["raw"], "OK");
    }

    #[test]
    fn wrap_mutating_passes_through_errors() {
        let err = wrap_mutating(ChannelResult::err("boom"), "project.save", Map::new());
        assert!(!err.is_success());
    }

    #[test]
    fn unsupported_operation_fails_fast() {
        let result = AppleScriptChannel::execute("project.export", &Params::new());
        assert!(!result.is_success());
        if let ChannelResult::Error(msg) = result {
            assert!(msg.contains("Unsupported AppleScript operation"));
        } else {
            panic!("expected error");
        }
    }

    #[test]
    fn project_open_requires_path() {
        let result = AppleScriptChannel::execute("project.open", &Params::new());
        assert!(!result.is_success());
        if let ChannelResult::Error(msg) = result {
            assert!(msg.contains("requires path"));
        } else {
            panic!("expected error");
        }
    }

    #[test]
    fn project_lifecycle_blocked_in_plugin_session() {
        use logicx_core::session::set_in_logic_plugin_session;
        set_in_logic_plugin_session(true);
        let mut params = Params::new();
        params.insert("path".into(), "/tmp/test.logicx".into());
        let result = AppleScriptChannel::execute("project.open", &params);
        set_in_logic_plugin_session(false);
        assert!(!result.is_success());
        if let ChannelResult::Error(msg) = result {
            assert!(msg.contains("blocked while LogicX MCP"));
        } else {
            panic!("expected error");
        }
    }
}
