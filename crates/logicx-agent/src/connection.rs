//! Ollama connectivity diagnostics for the plugin debug panel.

use logicx_core::{AgentSettings, OllamaConnectionReport, runtime};
use std::sync::mpsc::Sender;

use crate::ollama::{OllamaClient, OllamaError};

pub fn check_ollama_connection(settings: &AgentSettings, build_id: &str) -> OllamaConnectionReport {
    let mut debug = Vec::new();
    let host_exe = runtime::host_executable();
    let in_daw = runtime::hosted_in_daw();

    log(&mut debug, format!("build_id={build_id}"));
    log(&mut debug, format!("host_exe={host_exe}"));
    log(
        &mut debug,
        format!("hosted_in_daw={in_daw} transport=direct+curl (same client as standalone)"),
    );
    log(
        &mut debug,
        format!(
            "ollama_url={} model={}",
            settings.ollama_base_url, settings.model
        ),
    );

    if in_daw {
        log(
            &mut debug,
            "NOTE: AU runs in AUHostingServiceXPC — Logic control delegates to LogicX MCP.app (embedded logicx-control-bridge).",
        );
        log(
            &mut debug,
            format!(
                "Grant Accessibility to \"{}\" in System Settings. System Events is optional for tempo. {}",
                runtime::automation_settings_app_name(),
                runtime::companion_app_install_hint()
            ),
        );
        if let Some(status) = logicx_control::bridge::bridge_status() {
            log(
                &mut debug,
                format!(
                    "bridge: subject={} in_app={} ax={} tempo_ready={}",
                    status.permission_subject,
                    status.running_in_app_bundle,
                    status.accessibility,
                    status.tempo_control_ready
                ),
            );
            log(&mut debug, format!("bridge_exe={}", status.host_exe));
            if !status.running_in_app_bundle {
                log(
                    &mut debug,
                    "bridge STALE: bare logicx-control-bridge detected — will restart via LogicX MCP.app on next tool call.",
                );
            }
        } else {
            log(&mut debug, "bridge: not running (starts automatically on first control tool call)");
        }
    }

    // Probe curl itself before Ollama.
    match std::process::Command::new("/usr/bin/curl")
        .args(["--version"])
        .output()
    {
        Ok(o) if o.status.success() => {
            let ver = String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("curl")
                .to_string();
            log(&mut debug, format!("curl_ok: {ver}"));
        }
        Ok(o) => log(
            &mut debug,
            format!(
                "curl_version_failed exit={}",
                o.status.code().unwrap_or(-1)
            ),
        ),
        Err(e) => log(&mut debug, format!("curl_spawn_failed: {e}")),
    }

    let client = OllamaClient::new(&settings.ollama_base_url, &settings.model);
    match client.list_models() {
        Ok(models) => {
            log(
                &mut debug,
                format!(
                    "list_models OK: {} model(s) [{}]",
                    models.len(),
                    models.join(", ")
                ),
            );
            OllamaConnectionReport {
                connected: true,
                in_daw,
                url: settings.ollama_base_url.clone(),
                model: settings.model.clone(),
                host_exe,
                build_id: build_id.to_string(),
                model_count: Some(models.len()),
                error: None,
                debug,
            }
        }
        Err(e) => {
            let detail = format_connection_error(&e, &mut debug);
            log(&mut debug, format!("FAILED: {detail}"));
            OllamaConnectionReport {
                connected: false,
                in_daw,
                url: settings.ollama_base_url.clone(),
                model: settings.model.clone(),
                host_exe,
                build_id: build_id.to_string(),
                model_count: None,
                error: Some(detail),
                debug,
            }
        }
    }
}

pub fn check_ollama_connection_with_events(
    settings: AgentSettings,
    build_id: String,
    events: Sender<logicx_core::UiAgentEvent>,
) {
    let _ = events.send(logicx_core::UiAgentEvent::Debug {
        line: format!("── connectivity check (build {build_id}) ──"),
    });

    let report = check_ollama_connection(&settings, &build_id);
    for line in &report.debug {
        let _ = events.send(logicx_core::UiAgentEvent::Debug {
            line: line.clone(),
        });
    }
    let _ = events.send(logicx_core::UiAgentEvent::Connection { report });
}

fn format_connection_error(err: &OllamaError, debug: &mut Vec<String>) -> String {
    match err {
        OllamaError::Http(msg) => {
            log(debug, format!("http_error: {msg}"));
            msg.clone()
        }
        OllamaError::Parse(msg) => {
            log(debug, format!("parse_error: {msg}"));
            format!("parse error: {msg}")
        }
        OllamaError::Api(msg) => {
            log(debug, format!("api_error: {msg}"));
            format!("Ollama API: {msg}")
        }
    }
}

fn log(debug: &mut Vec<String>, line: impl Into<String>) {
    let line = line.into();
    eprintln!("[LogicX MCP] {line}");
    debug.push(line);
}
