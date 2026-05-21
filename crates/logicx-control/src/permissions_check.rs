//! Lightweight permission snapshot for in-app onboarding (AU + standalone).

use serde::{Deserialize, Serialize};

/// Current macOS permission state for Logic control.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionsSnapshot {
    pub permission_subject: String,
    pub companion_app_installed: bool,
    pub bridge_running: bool,
    pub accessibility: bool,
    pub tempo_control_ready: bool,
    pub automation_logic_pro: bool,
    pub automation_system_events: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl PermissionsSnapshot {
    /// Minimum needed for tempo / native AX control.
    pub fn tempo_ready(&self) -> bool {
        self.tempo_control_ready && self.error.is_none()
    }

    /// All recommended grants (Accessibility + Automation targets).
    pub fn fully_ready(&self) -> bool {
        self.tempo_ready()
            && self.automation_logic_pro
            && self.automation_system_events
            && self.bridge_running
    }

    pub fn show_onboarding(&self) -> bool {
        !self.tempo_ready() || !self.companion_app_installed || !self.bridge_running
    }
}

#[cfg(not(target_os = "macos"))]
pub fn snapshot() -> PermissionsSnapshot {
    PermissionsSnapshot {
        permission_subject: "LogicX MCP".into(),
        companion_app_installed: true,
        bridge_running: true,
        accessibility: true,
        tempo_control_ready: true,
        automation_logic_pro: true,
        automation_system_events: true,
        error: None,
    }
}

#[cfg(target_os = "macos")]
pub fn snapshot() -> PermissionsSnapshot {
    use logicx_core::runtime;

    let subject = runtime::automation_settings_app_name().to_string();
    let companion_app_installed = runtime::installed_companion_app().is_some();

    if runtime::hosted_in_daw() {
        snapshot_via_bridge(subject, companion_app_installed)
    } else {
        snapshot_local(subject, companion_app_installed)
    }
}

#[cfg(target_os = "macos")]
fn snapshot_local(subject: String, companion_app_installed: bool) -> PermissionsSnapshot {
    use crate::macos::{
        automation_logic_ok, automation_system_events_ok, is_ax_trusted, prime_automation_prompts,
    };

    prime_automation_prompts();
    let ax = is_ax_trusted();
    PermissionsSnapshot {
        permission_subject: subject,
        companion_app_installed,
        bridge_running: true,
        accessibility: ax,
        tempo_control_ready: ax,
        automation_logic_pro: automation_logic_ok(),
        automation_system_events: automation_system_events_ok(),
        error: None,
    }
}

#[cfg(target_os = "macos")]
pub fn refresh() -> PermissionsSnapshot {
    use logicx_core::runtime;

    let subject = runtime::automation_settings_app_name().to_string();
    let companion_app_installed = runtime::installed_companion_app().is_some();

    if runtime::hosted_in_daw() {
        if !companion_app_installed {
            return PermissionsSnapshot {
                permission_subject: subject,
                companion_app_installed: false,
                bridge_running: false,
                accessibility: false,
                tempo_control_ready: false,
                automation_logic_pro: false,
                automation_system_events: false,
                error: Some(logicx_core::runtime::companion_app_install_hint()),
            };
        }
        if let Some(status) = crate::bridge::bridge_status() {
            return snapshot_from_bridge_status(status);
        }
        snapshot_via_bridge(subject, companion_app_installed)
    } else {
        snapshot_local(subject, companion_app_installed)
    }
}

#[cfg(not(target_os = "macos"))]
pub fn refresh() -> PermissionsSnapshot {
    snapshot()
}

#[cfg(target_os = "macos")]
fn snapshot_from_bridge_status(
    status: logicx_core::control_bridge::BridgeStatus,
) -> PermissionsSnapshot {
    PermissionsSnapshot {
        permission_subject: status.permission_subject,
        companion_app_installed: true,
        bridge_running: true,
        accessibility: status.accessibility,
        tempo_control_ready: status.tempo_control_ready,
        automation_logic_pro: status.automation_logic_pro,
        automation_system_events: status.automation_system_events,
        error: None,
    }
}

#[cfg(target_os = "macos")]
fn snapshot_via_bridge(subject: String, companion_app_installed: bool) -> PermissionsSnapshot {
    if !companion_app_installed {
        return PermissionsSnapshot {
            permission_subject: subject,
            companion_app_installed: false,
            bridge_running: false,
            accessibility: false,
            tempo_control_ready: false,
            automation_logic_pro: false,
            automation_system_events: false,
            error: Some(logicx_core::runtime::companion_app_install_hint()),
        };
    }

    match crate::bridge::reconcile_bridge() {
        Ok(()) => match crate::bridge::bridge_status() {
            Some(status) => snapshot_from_bridge_status(status),
            None => PermissionsSnapshot {
                permission_subject: subject,
                companion_app_installed: true,
                bridge_running: false,
                accessibility: false,
                tempo_control_ready: false,
                automation_logic_pro: false,
                automation_system_events: false,
                error: Some(
                    "Control bridge did not respond. Quit and relaunch Logic Pro, then try again."
                        .into(),
                ),
            },
        },
        Err(e) => PermissionsSnapshot {
            permission_subject: subject,
            companion_app_installed: true,
            bridge_running: false,
            accessibility: false,
            tempo_control_ready: false,
            automation_logic_pro: false,
            automation_system_events: false,
            error: Some(e),
        },
    }
}

/// Open System Settings → Privacy & Security → Accessibility.
#[cfg(target_os = "macos")]
pub fn open_accessibility_settings() {
    open_settings_url(
        "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Accessibility",
    );
}

/// Open System Settings → Privacy & Security → Automation.
#[cfg(target_os = "macos")]
pub fn open_automation_settings() {
    open_settings_url(
        "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_Automation",
    );
}

#[cfg(not(target_os = "macos"))]
pub fn open_accessibility_settings() {}

#[cfg(not(target_os = "macos"))]
pub fn open_automation_settings() {}

#[cfg(target_os = "macos")]
fn open_settings_url(url: &str) {
    use std::process::Command;
    let _ = Command::new("/usr/bin/open").arg(url).status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tempo_ready_requires_accessibility() {
        let ok = PermissionsSnapshot {
            permission_subject: "LogicX MCP".into(),
            companion_app_installed: true,
            bridge_running: true,
            accessibility: true,
            tempo_control_ready: true,
            automation_logic_pro: false,
            automation_system_events: false,
            error: None,
        };
        assert!(ok.tempo_ready());
        assert!(!ok.show_onboarding());

        let missing_ax = PermissionsSnapshot {
            accessibility: false,
            tempo_control_ready: false,
            ..ok.clone()
        };
        assert!(!missing_ax.tempo_ready());
        assert!(missing_ax.show_onboarding());
    }

    #[test]
    fn fully_ready_includes_automation() {
        let partial = PermissionsSnapshot {
            permission_subject: "LogicX MCP".into(),
            companion_app_installed: true,
            bridge_running: true,
            accessibility: true,
            tempo_control_ready: true,
            automation_logic_pro: false,
            automation_system_events: false,
            error: None,
        };
        assert!(!partial.fully_ready());
        assert!(!partial.show_onboarding());

        let full = PermissionsSnapshot {
            automation_logic_pro: true,
            automation_system_events: true,
            ..partial
        };
        assert!(full.fully_ready());
        assert!(!full.show_onboarding());
    }
}
