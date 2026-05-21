use crate::approvals;
use crate::channels::{ChannelHealth, ChannelResult, Params};
use crate::midi::engine::MidiEngine;
use once_cell::sync::Lazy;
use serde_json::json;
use std::sync::Arc;

pub const PORT_NAME: &str = "LogicX-MCP-Scripter";

static ENGINE: Lazy<Arc<MidiEngine>> = Lazy::new(|| Arc::new(MidiEngine::new(PORT_NAME)));

/// Wire byte for MIDI channel 16 (1-based).
const SCRIPTER_CH: u8 = 15;
const CC_BASE: u8 = 102;

pub fn ensure_started() -> Result<(), String> {
    ENGINE.start().map_err(|e| e.to_string())
}

pub fn cc_for_param(param: i32) -> Option<u8> {
    if (0..18).contains(&param) {
        Some(CC_BASE + param as u8)
    } else {
        None
    }
}

pub fn midi_value(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 127.0).round() as u8
}

pub fn manual_validation_detail() -> &'static str {
    "Insert Scripter MIDI FX on the target track (insert 0). Map CC 102–119 on channel 16 to plugin parameters 0–17. \
     Run logic_system approve_channel with channel=scripter after manual validation."
}

pub struct ScripterChannel;

impl ScripterChannel {
    pub fn health() -> ChannelHealth {
        if !ENGINE.is_active()
            && let Err(e) = ensure_started()
        {
            return ChannelHealth::unavailable(format!("Scripter port not started: {e}"));
        }
        if !ENGINE.is_active() {
            return ChannelHealth::unavailable("Scripter port not started");
        }
        if approvals::is_approved("scripter") {
            return ChannelHealth::healthy(format!(
                "Scripter CC bridge on '{PORT_NAME}' — operator approved (CC {CC_BASE}–{} CH16)",
                CC_BASE + 17
            ));
        }
        ChannelHealth::manual_validation_required(format!(
            "Scripter port '{PORT_NAME}' active — {detail}",
            detail = manual_validation_detail(),
        ))
    }

    pub fn execute(operation: &str, params: &Params) -> ChannelResult {
        if !approvals::is_approved("scripter") {
            return ChannelResult::err(
                "scripter requires operator approval — run logic_system approve_channel",
            );
        }
        if let Err(e) = ensure_started() {
            return ChannelResult::err(e);
        }
        match operation {
            "mixer.set_plugin_param" | "plugin.set_param" => {
                let insert = params.get("insert").map(String::as_str).unwrap_or("0");
                if insert != "0" {
                    return ChannelResult::err(
                        "plugin.set_param only supports insert 0 (Scripter MIDI FX slot)",
                    );
                }
                let param_index = match params
                    .get("param")
                    .or_else(|| params.get("index"))
                    .and_then(|s| s.parse::<i32>().ok())
                {
                    Some(v) => v,
                    None => return ChannelResult::err("set_plugin_param requires param 0-17"),
                };
                let Some(cc) = cc_for_param(param_index) else {
                    return ChannelResult::err(format!("param {param_index} out of range (0-17)"));
                };
                let value = match params.get("value").and_then(|s| s.parse::<f64>().ok()) {
                    Some(v) => v,
                    None => return ChannelResult::err("set_plugin_param requires value 0-1"),
                };
                let cc_value = midi_value(value);
                if ENGINE.send_cc(SCRIPTER_CH, cc, cc_value).is_err() {
                    return ChannelResult::err(format!(
                        "Failed to send Scripter param {param_index}"
                    ));
                }
                ChannelResult::Success {
                    message: format!(
                        "Scripter param {param_index} set to {value} (CC {cc} val {cc_value})"
                    ),
                    verified: Some(false),
                    reason: Some("readback_unavailable".into()),
                    detail: Some(json!({
                        "param": param_index,
                        "value": value,
                        "cc": cc,
                        "cc_value": cc_value,
                        "insert": 0,
                        "channel": 16,
                        "port": PORT_NAME,
                    })),
                }
            }
            _ => ChannelResult::err(format!(
                "Scripter only handles plugin.set_param / mixer.set_plugin_param, not {operation}"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn param_to_cc() {
        assert_eq!(cc_for_param(0), Some(102));
        assert_eq!(cc_for_param(17), Some(119));
    }

    #[test]
    fn param_range() {
        for i in 0..18 {
            assert_eq!(cc_for_param(i), Some(102 + i as u8));
        }
    }

    #[test]
    fn value_normalize() {
        assert_eq!(midi_value(0.5), 64);
        assert_eq!(midi_value(0.0), 0);
        assert_eq!(midi_value(1.0), 127);
    }

    #[test]
    fn out_of_range_param() {
        assert_eq!(cc_for_param(18), None);
        assert_eq!(cc_for_param(-1), None);
    }

    #[test]
    fn port_name_matches_setup_doc() {
        assert_eq!(PORT_NAME, "LogicX-MCP-Scripter");
    }
}
