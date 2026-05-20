//! Runtime context: standalone app vs AU plugin inside a DAW host.

use std::path::PathBuf;

/// Path of the running executable (Logic Pro when hosted as AU).
pub fn host_executable() -> String {
    std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "?".into())
}

/// True when loaded inside a DAW host (Logic Pro AU), not the standalone `.app`.
pub fn hosted_in_daw() -> bool {
    #[cfg(target_os = "macos")]
    {
        let lower = host_executable().to_lowercase();
        if lower.contains("logicx-mcp-standalone") || lower.contains("logicx_mcp_standalone") {
            return false;
        }
        // Logic Pro loads AU v2 plugins in AUHostingServiceXPC, not in its own binary.
        lower.contains("logic pro")
            || lower.contains("auhostingservice")
            || lower.contains("auhosting")
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// True when the current process lives inside a `.app` bundle (companion app bridge).
pub fn running_in_app_bundle() -> bool {
    app_bundle_path().is_some()
}

/// Path to the enclosing `.app` bundle, if any.
pub fn app_bundle_path() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|exe| {
        exe.ancestors().find_map(|p| {
            if p.extension().and_then(|e| e.to_str()) == Some("app") {
                Some(p.to_path_buf())
            } else {
                None
            }
        })
    })
}

/// Process name shown in System Settings → Privacy.
/// Uses the `.app` display name when running inside a bundle (required for Automation UI).
pub fn permission_subject() -> String {
    app_bundle_path()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        })
        .unwrap_or_else(|| "LogicX MCP".into())
}

/// Name that appears under System Settings → Automation (always the companion app).
pub fn automation_settings_app_name() -> &'static str {
    "LogicX MCP"
}

pub fn support_dir() -> PathBuf {
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support/LogicX MCP"))
        .unwrap_or_else(|_| PathBuf::from("/tmp/LogicX MCP"))
}

/// Known install locations for the companion app (user dev install vs pkg).
pub fn companion_app_paths() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    vec![
        PathBuf::from(format!("{home}/Applications/LogicX MCP.app")),
        PathBuf::from("/Applications/LogicX MCP.app"),
    ]
}

/// First companion `.app` found on disk, if any.
pub fn installed_companion_app() -> Option<PathBuf> {
    companion_app_paths().into_iter().find(|p| p.is_dir())
}

/// Embedded `logicx-control-bridge` inside the companion `.app` (preferred launcher).
pub fn companion_app_embedded_bridge() -> Option<PathBuf> {
    installed_companion_app().map(|app| app.join("Contents/MacOS/logicx-control-bridge"))
}

/// `logicx-mcp-standalone --control-bridge` inside the installed companion app.
pub fn companion_app_bridge_executable() -> Option<PathBuf> {
    installed_companion_app().map(|app| app.join("Contents/MacOS/logicx-mcp-standalone"))
}

/// Human-readable install hint for errors and the debug panel.
pub fn companion_app_install_hint() -> String {
    match installed_companion_app() {
        Some(p) => format!("Companion app found at: {}", p.display()),
        None => {
            let home = std::env::var("HOME").unwrap_or_else(|_| "~".into());
            format!(
                "Companion app not found. Install with ./scripts/install-au.sh (→ {home}/Applications/LogicX MCP.app) \
                 or sudo installer -pkg release-artefacts/LogicX-MCP-macOS-Installer.pkg -target / (→ /Applications/LogicX MCP.app)"
            )
        }
    }
}

/// Embedded `logicx-control-bridge` next to the AU component (user + system Library).
pub fn control_bridge_binary_candidates() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    [
        format!(
            "{home}/Library/Audio/Plug-Ins/Components/LogicX MCP.component/Contents/MacOS/logicx-control-bridge"
        ),
        "/Library/Audio/Plug-Ins/Components/LogicX MCP.component/Contents/MacOS/logicx-control-bridge"
            .into(),
        format!("{home}/Applications/LogicX MCP.app/Contents/MacOS/logicx-control-bridge"),
        "/Applications/LogicX MCP.app/Contents/MacOS/logicx-control-bridge".into(),
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect()
}
