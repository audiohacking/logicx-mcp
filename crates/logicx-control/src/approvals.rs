//! Operator approval gates (logic-pro-mcp parity) for KeyCmd + Scripter channels.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ApprovalFile {
    approved_channels: HashSet<String>,
}

fn path() -> PathBuf {
    logicx_core::runtime::support_dir().join("operator-approvals.json")
}

const OPERATOR_CHANNELS: &[&str] = &["midi_key_commands", "scripter"];

pub fn is_approved(channel: &str) -> bool {
    load().approved_channels.contains(channel)
}

pub fn approve(channel: &str) -> Result<(), String> {
    if !OPERATOR_CHANNELS.contains(&channel) {
        let automation_app = logicx_core::runtime::automation_settings_app_name();
        return Err(format!(
            "approve_channel only gates KeyCmd/Scripter ({OPERATOR_CHANNELS:?}). \
             It does not grant macOS permissions — \"{channel}\" is not an operator channel. \
             For tempo: enable Accessibility for \"{automation_app}\". \
             For Automation fallbacks: System Settings → Automation → \"{automation_app}\"."
        ));
    }
    let mut data = load();
    data.approved_channels.insert(channel.to_string());
    save(&data)
}

pub fn list() -> Vec<String> {
    let mut v: Vec<_> = load().approved_channels.into_iter().collect();
    v.sort();
    v
}

fn load() -> ApprovalFile {
    let p = path();
    if !p.is_file() {
        return ApprovalFile::default();
    }
    fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(data: &ApprovalFile) -> Result<(), String> {
    let p = path();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    fs::write(&p, json).map_err(|e| e.to_string())
}
