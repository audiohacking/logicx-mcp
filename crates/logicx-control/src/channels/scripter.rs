use crate::approvals;
use crate::channels::{ChannelHealth, ChannelResult, Params};
use crate::midi::engine::MidiEngine;
use once_cell::sync::Lazy;
use std::sync::Arc;

static ENGINE: Lazy<Arc<MidiEngine>> =
    Lazy::new(|| Arc::new(MidiEngine::new("LogicX-MCP-Scripter")));

/// Wire byte for MIDI channel 16 (1-based).
const SCRIPTER_CH: u8 = 15;

pub fn cc_for_param(param: i32) -> Option<u8> {
    if (0..18).contains(&param) {
        Some(102 + param as u8)
    } else {
        None
    }
}

pub fn midi_value(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 127.0).round() as u8
}

pub struct ScripterChannel;

impl ScripterChannel {
    pub fn health() -> ChannelHealth {
        if ENGINE.is_active() {
            ChannelHealth::healthy(
                "Scripter CC bridge active — insert Scripter MIDI FX on target track (channel 16, CC 102–119)",
            )
        } else {
            ChannelHealth::unavailable("Scripter port not started")
        }
    }

    pub fn execute(operation: &str, params: &Params) -> ChannelResult {
        if !approvals::is_approved("scripter") {
            return ChannelResult::err(
                "scripter requires operator approval — run logic_system approve_channel",
            );
        }
        let _ = ENGINE.start();
        match operation {
            "mixer.set_plugin_param" | "plugin.set_param" => {
                if params.contains_key("insert") {
                    let insert = params.get("insert").map(String::as_str).unwrap_or("0");
                    if insert != "0" {
                        return ChannelResult::err(
                            "plugin.set_param only supports insert 0 (Scripter MIDI FX slot)",
                        );
                    }
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
                ChannelResult::ok(format!("Scripter CC {cc}={cc_value} on CH16"))
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
}
