//! Delegate control to a companion process when the AU XPC host lacks System Events.

use logicx_core::ToolInvocation;
use std::path::PathBuf;

/// True when control should run in the companion app (AU XPC host — logic-pro-mcp process model).
pub fn should_delegate() -> bool {
    #[cfg(target_os = "macos")]
    {
        logicx_core::runtime::hosted_in_daw()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ();
        false
    }
}

#[cfg(not(target_os = "macos"))]
pub fn run_server() {}

#[cfg(not(target_os = "macos"))]
pub fn ensure_running() -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn bridge_status() -> Option<logicx_core::control_bridge::BridgeStatus> {
    None
}

#[cfg(not(target_os = "macos"))]
pub fn execute_remote(_: &ToolInvocation) -> Result<String, String> {
    Err("macOS only".into())
}

#[cfg(not(target_os = "macos"))]
pub fn kill_stale_bridges() {}

#[cfg(not(target_os = "macos"))]
pub fn reconcile_bridge() -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn kill_stale_bridges() {
    macos_impl::kill_stale_bridges();
}

/// Kill stale/legacy bridge processes, then ensure the companion-app bridge is running.
#[cfg(target_os = "macos")]
pub fn reconcile_bridge() -> Result<(), String> {
    if !should_delegate() {
        return Ok(());
    }
    macos_impl::reconcile_bridge()
}

#[cfg(target_os = "macos")]
pub fn bridge_status() -> Option<logicx_core::control_bridge::BridgeStatus> {
    macos_impl::bridge_status()
}

#[cfg(target_os = "macos")]
mod macos_impl {
    use super::*;
    use logicx_core::control_bridge::{BridgeRequest, BridgeResponse, BridgeStatus, socket_path};
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Duration;

    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::Mutex;

    static EXECUTE_LOCK: Mutex<()> = Mutex::new(());

    pub fn run_server() {
        crate::macos::prime_automation_prompts();

        let dir = logicx_core::control_bridge::support_dir();
        let _ = fs::create_dir_all(&dir);

        let socket = socket_path();
        let _ = fs::remove_file(&socket);

        let listener = match std::os::unix::net::UnixListener::bind(&socket) {
            Ok(l) => l,
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("logicx-control-bridge: bind {} failed: {e}", socket.display());
                std::process::exit(1);
            }
        };

        let _ = fs::write(
            logicx_core::control_bridge::pid_path(),
            std::process::id().to_string(),
        );

        eprintln!(
            "logicx-control-bridge: listening on {} (pid {}, exe={})",
            socket.display(),
            std::process::id(),
            logicx_core::runtime::host_executable()
        );

        for stream in listener.incoming() {
            match stream {
                Ok(s) => {
                    thread::spawn(|| {
                        let _ = handle_client(s);
                    });
                }
                Err(e) => eprintln!("logicx-control-bridge: accept error: {e}"),
            }
        }

        let _ = fs::remove_file(&socket);
        let _ = fs::remove_file(logicx_core::control_bridge::pid_path());
    }

    fn handle_client(mut stream: UnixStream) -> std::io::Result<()> {
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut line = String::new();
        reader.read_line(&mut line)?;

        let response = match serde_json::from_str::<BridgeRequest>(line.trim()) {
            Ok(BridgeRequest::Ping) => {
                let ax = crate::macos::is_ax_trusted();
                let status = BridgeStatus {
                    pong: true,
                    host_exe: logicx_core::runtime::host_executable(),
                    permission_subject: logicx_core::runtime::permission_subject(),
                    running_in_app_bundle: logicx_core::runtime::running_in_app_bundle(),
                    accessibility: ax,
                    tempo_control_ready: ax,
                };
                BridgeResponse::success(status.to_json())
            }
            Ok(BridgeRequest::Execute { invocation, context }) => {
                let _guard = EXECUTE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
                logicx_core::diagnostic_log::append_bridge_log(format!(
                    "execute {} {:?}",
                    invocation.name,
                    invocation
                        .arguments
                        .get("command")
                        .and_then(|v| v.as_str())
                ));
                logicx_core::session::set_in_logic_plugin_session(context.in_logic_plugin);
                let result = catch_unwind(AssertUnwindSafe(|| {
                    let executor = crate::LogicExecutor::new();
                    executor.execute_local(&invocation)
                }));
                logicx_core::session::set_in_logic_plugin_session(false);
                let response = match result {
                    Ok(Ok(json)) => {
                        logicx_core::diagnostic_log::append_bridge_log(format!(
                            "execute ok {} bytes",
                            json.len()
                        ));
                        BridgeResponse::success(json)
                    }
                    Ok(Err(e)) => {
                        logicx_core::diagnostic_log::append_bridge_log(format!("execute err: {e}"));
                        BridgeResponse::failure(e.to_string())
                    }
                    Err(_) => {
                        logicx_core::diagnostic_log::append_bridge_log("execute PANIC");
                        BridgeResponse::failure(
                            "bridge handler panicked during execute (see bridge.log in Application Support/LogicX MCP)".to_string(),
                        )
                    }
                };
                response
            }
            Err(e) => BridgeResponse::failure(format!("invalid request JSON: {e}")),
        };

        let out = serde_json::to_string(&response).unwrap_or_else(|e| {
            format!(r#"{{"ok":false,"error":"encode error: {e}"}}"#)
        });
        stream.write_all(out.as_bytes())?;
        stream.write_all(b"\n")?;
        stream.flush()?;
        Ok(())
    }

    pub fn execute_remote(invocation: &ToolInvocation) -> Result<String, String> {
        ensure_running()?;
        match rpc_execute(invocation) {
            Ok(json) => Ok(json),
            Err(e) if is_rpc_disconnect(&e) => {
                kill_stale_bridges();
                ensure_running()?;
                rpc_execute(invocation)
            }
            Err(e) => Err(format!("control bridge: {e}")),
        }
    }

    fn rpc_execute(invocation: &ToolInvocation) -> Result<String, String> {
        let req = BridgeRequest::Execute {
            invocation: invocation.clone(),
            context: logicx_core::control_bridge::BridgeContext {
                in_logic_plugin: true,
            },
        };
        let line = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        let resp = rpc(&line)?;
        if resp.ok {
            resp.result.ok_or_else(|| "bridge returned ok without result".into())
        } else {
            Err(resp.error.unwrap_or_else(|| "bridge error".into()))
        }
    }

    fn is_rpc_disconnect(err: &str) -> bool {
        err.contains("EOF while parsing")
            || err.contains("parse error: EOF")
            || err.contains("connection reset")
            || err.contains("broken pipe")
    }

    pub fn ensure_running() -> Result<(), String> {
        if let Ok(resp) = rpc_ping() {
            if resp.ok {
                let raw = resp.result.unwrap_or_default();
                if !needs_bridge_restart(&raw) {
                    return Ok(());
                }
            }
        }

        kill_stale_bridges();
        let tried = spawn_bridge()?;

        for _ in 0..40 {
            thread::sleep(Duration::from_millis(100));
            if let Ok(resp) = rpc_ping() {
                if resp.ok {
                    return Ok(());
                }
            }
        }

        Err(format!(
            "control bridge did not start after trying: {}. \
             Grant Accessibility to \"{}\" in System Settings → Privacy & Security → Accessibility. \
             For Automation (optional), use System Settings → Automation → {} (not logicx-control-bridge). \
             {}",
            tried.join(", "),
            logicx_core::runtime::automation_settings_app_name(),
            logicx_core::runtime::automation_settings_app_name(),
            logicx_core::runtime::companion_app_install_hint()
        ))
    }

    fn spawn_bridge() -> Result<Vec<String>, String> {
        if let Ok(p) = std::env::var("LOGICX_CONTROL_BRIDGE_BIN") {
            let path = PathBuf::from(&p);
            if path.is_file() {
                launch_bridge_binary(&path)?;
                return Ok(vec![path.display().to_string()]);
            }
        }

        let mut tried = Vec::new();

        // Prefer bridge binary inside companion `.app` (inherits app bundle ID for TCC).
        if let Some(app) = logicx_core::runtime::installed_companion_app() {
            let embedded = app.join("Contents/MacOS/logicx-control-bridge");
            if embedded.is_file() {
                tried.push(embedded.display().to_string());
                launch_bridge_binary(&embedded)?;
                return Ok(tried);
            }

            // Secondary: standalone host with --control-bridge (requires sync_standalone_host in install).
            if let Some(exe) = logicx_core::runtime::companion_app_bridge_executable() {
                if exe.is_file() {
                    let label = format!("{} --control-bridge", exe.display());
                    tried.push(label);
                    launch_app_bridge(&exe)?;
                    return Ok(tried);
                }
            }
        }

        for path in logicx_core::runtime::control_bridge_binary_candidates() {
            if path.is_file() {
                tried.push(path.display().to_string());
                launch_bridge_binary(&path)?;
                return Ok(tried);
            }
        }

        Err(format!(
            "control bridge not found. Run ./scripts/install-au.sh or install the .pkg. {}",
            logicx_core::runtime::companion_app_install_hint()
        ))
    }

    fn launch_app_bridge(exe: &std::path::Path) -> Result<(), String> {
        let log_path = logicx_core::control_bridge::support_dir().join("bridge.log");
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok()
            .map(|f| Stdio::from(f));

        let mut cmd = Command::new(exe);
        cmd.arg("--control-bridge");
        cmd.stdin(Stdio::null()).stdout(Stdio::null());
        if let Some(stderr) = log_file {
            cmd.stderr(stderr);
        } else {
            cmd.stderr(Stdio::null());
        }
        cmd.spawn()
            .map_err(|e| format!("failed to spawn {} --control-bridge: {e}", exe.display()))?;
        Ok(())
    }

    fn launch_bridge_binary(path: &std::path::Path) -> Result<(), String> {
        let log_path = logicx_core::control_bridge::support_dir().join("bridge.log");
        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok()
            .map(|f| Stdio::from(f));

        let mut cmd = Command::new(path);
        cmd.stdin(Stdio::null()).stdout(Stdio::null());
        if let Some(stderr) = log_file {
            cmd.stderr(stderr);
        } else {
            cmd.stderr(Stdio::null());
        }
        cmd.spawn()
            .map_err(|e| format!("failed to spawn {}: {e}", path.display()))?;
        Ok(())
    }

    pub fn reconcile_bridge() -> Result<(), String> {
        if logicx_core::runtime::companion_app_bridge_executable().is_some() {
            match bridge_status() {
                Some(status) if status.running_in_app_bundle => {}
                _ => kill_stale_bridges(),
            }
        } else if rpc_ping().is_err() {
            kill_stale_bridge_binaries_only();
        }
        ensure_running()
    }

    /// Kill every control-bridge process and remove IPC files.
    pub fn kill_stale_bridges() {
        let mut pids = Vec::new();
        pids.extend(pids_matching("logicx-control-bridge"));
        pids.extend(pids_matching("logicx-mcp-standalone --control-bridge"));
        if let Ok(pid_str) = fs::read_to_string(logicx_core::control_bridge::pid_path()) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                pids.push(pid);
            }
        }
        pids.sort_unstable();
        pids.dedup();
        terminate_pids(&pids);
        cleanup_bridge_artifacts();
    }

    /// Kill bare `logicx-control-bridge` binaries only (leave app-hosted bridge running).
    fn kill_stale_bridge_binaries_only() {
        let pids = pids_matching("logicx-control-bridge");
        terminate_pids(&pids);
    }

    fn pids_matching(pattern: &str) -> Vec<i32> {
        Command::new("/usr/bin/pgrep")
            .args(["-f", pattern])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|line| line.trim().parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn process_alive(pid: i32) -> bool {
        Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn terminate_pids(pids: &[i32]) {
        let alive: Vec<i32> = pids.iter().copied().filter(|p| process_alive(*p)).collect();
        if alive.is_empty() {
            return;
        }
        for pid in &alive {
            let _ = Command::new("/bin/kill").arg(pid.to_string()).status();
        }
        thread::sleep(Duration::from_millis(300));
        for pid in &alive {
            if process_alive(*pid) {
                let _ = Command::new("/bin/kill")
                    .arg("-9")
                    .arg(pid.to_string())
                    .status();
            }
        }
        thread::sleep(Duration::from_millis(100));
    }

    fn cleanup_bridge_artifacts() {
        let _ = fs::remove_file(socket_path());
        let _ = fs::remove_file(logicx_core::control_bridge::pid_path());
    }

    fn needs_bridge_restart(raw: &str) -> bool {
        if logicx_core::runtime::companion_app_bridge_executable().is_none() {
            return false;
        }
        match BridgeStatus::parse(raw) {
            Some(status) => !status.running_in_app_bundle,
            None => true, // legacy bare logicx-control-bridge
        }
    }

    fn rpc_ping() -> Result<BridgeResponse, String> {
        let line = serde_json::to_string(&BridgeRequest::Ping).map_err(|e| e.to_string())?;
        rpc(&line)
    }

    pub fn bridge_status() -> Option<BridgeStatus> {
        let resp = rpc_ping().ok()?;
        if !resp.ok {
            return None;
        }
        BridgeStatus::parse(&resp.result?)
    }

    fn rpc(request_line: &str) -> Result<BridgeResponse, String> {
        let socket = socket_path();
        let mut stream = UnixStream::connect(&socket)
            .map_err(|e| format!("connect {} failed: {e}", socket.display()))?;
        stream
            .write_all(request_line.as_bytes())
            .map_err(|e| e.to_string())?;
        stream.write_all(b"\n").map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        serde_json::from_str(line.trim()).map_err(|e| format!("bridge response parse error: {e}"))
    }
}

#[cfg(target_os = "macos")]
pub use macos_impl::{ensure_running, execute_remote, run_server};
