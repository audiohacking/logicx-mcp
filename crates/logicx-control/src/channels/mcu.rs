use crate::channels::{ChannelHealth, ChannelResult, Params};
use crate::midi::engine::MidiEngine;
use crate::midi::{
    mcu_protocol::{self, TransportCommand},
    mcu_state,
};
use serde_json::json;
use std::sync::Arc;

pub const PORT_NAME: &str = crate::midi::mcu_protocol::PORT_NAME;

pub struct McuChannel {
    engine: Arc<MidiEngine>,
}

impl McuChannel {
    pub fn new(engine: Arc<MidiEngine>) -> Self {
        Self { engine }
    }

    pub fn health(&self) -> ChannelHealth {
        if !self.engine.is_active() {
            return ChannelHealth::unavailable("MCU port not started");
        }
        let cache = mcu_state::McuStateCache::global();
        let conn = cache.connection();
        if cache.is_feedback_fresh(5) {
            let detail = if conn.registered_as_device {
                format!("MCU feedback active on '{PORT_NAME}'")
            } else {
                "MCU feedback active".into()
            };
            return ChannelHealth::healthy(detail);
        }
        ChannelHealth::unavailable(format!(
            "MCU feedback not detected — Logic Control Surfaces → Mackie Control: Input `{PORT_NAME}`, Output `{}`",
            mcu_protocol::FEEDBACK_PORT_NAME
        ))
    }

    pub fn execute(&self, operation: &str, params: &Params) -> ChannelResult {
        if !self.engine.is_active() {
            return ChannelResult::err("MCU engine not active");
        }

        match operation {
            "transport.play" => send(
                &self.engine,
                mcu_protocol::encode_transport(TransportCommand::Play),
            ),
            "transport.stop" => send(
                &self.engine,
                mcu_protocol::encode_transport(TransportCommand::Stop),
            ),
            "transport.record" => send(
                &self.engine,
                mcu_protocol::encode_transport(TransportCommand::Record),
            ),
            "transport.rewind" => send(
                &self.engine,
                mcu_protocol::encode_transport(TransportCommand::Rewind),
            ),
            "transport.fast_forward" => send(
                &self.engine,
                mcu_protocol::encode_transport(TransportCommand::FastForward),
            ),
            "transport.toggle_cycle" => send(
                &self.engine,
                mcu_protocol::encode_transport(TransportCommand::Cycle),
            ),
            "transport.toggle_metronome" => send(
                &self.engine,
                mcu_protocol::encode_transport(TransportCommand::Click),
            ),
            "track.select" => {
                let Some(index) = params.get("index").and_then(|s| s.parse().ok()) else {
                    return ChannelResult::err("requires explicit index");
                };
                send(&self.engine, mcu_protocol::encode_select(index))
            }
            "track.set_mute" => {
                let Some(index) = params.get("index").and_then(|s| s.parse().ok()) else {
                    return ChannelResult::err("requires explicit index");
                };
                let enabled = params
                    .get("enabled")
                    .map(|s| s != "false" && s != "0")
                    .unwrap_or(true);
                send(&self.engine, mcu_protocol::encode_mute(index, enabled))
            }
            "track.set_solo" => {
                let Some(index) = params.get("index").and_then(|s| s.parse().ok()) else {
                    return ChannelResult::err("requires explicit index");
                };
                let enabled = params
                    .get("enabled")
                    .map(|s| s != "false" && s != "0")
                    .unwrap_or(true);
                send(&self.engine, mcu_protocol::encode_solo(index, enabled))
            }
            "track.set_arm" => {
                let Some(index) = params.get("index").and_then(|s| s.parse().ok()) else {
                    return ChannelResult::err("requires explicit index");
                };
                let enabled = params
                    .get("enabled")
                    .map(|s| s != "false" && s != "0")
                    .unwrap_or(true);
                send(&self.engine, mcu_protocol::encode_arm(index, enabled))
            }
            "mixer.set_volume" => {
                let Some(track) = params
                    .get("track")
                    .or_else(|| params.get("index"))
                    .and_then(|s| s.parse().ok())
                else {
                    return ChannelResult::err("requires explicit track/index");
                };
                let Some(value) = params
                    .get("value")
                    .or_else(|| params.get("volume"))
                    .and_then(|s| s.parse().ok())
                else {
                    return ChannelResult::err("requires value or volume");
                };
                let bytes = mcu_protocol::encode_fader(track, value);
                if self.engine.send_bytes(&bytes).is_err() {
                    return ChannelResult::err("MCU send failed");
                }
                let cache = mcu_state::McuStateCache::global();
                let observed =
                    mcu_state::poll_fader_echo(&cache, track as usize, value, 500, 2.0 / 16383.0);
                let detail = serde_json::json!({
                    "requested": value,
                    "observed": observed,
                    "track": track,
                });
                if observed.is_some_and(|o| (o - value).abs() <= 2.0 / 16383.0) {
                    return ChannelResult::Success {
                        message: "ok".into(),
                        verified: Some(true),
                        reason: None,
                        detail: Some(detail),
                    };
                }
                ChannelResult::Success {
                    message: "ok".into(),
                    verified: Some(false),
                    reason: Some("echo_timeout_ms".into()),
                    detail: Some(detail),
                }
            }
            "mixer.set_pan" => {
                let Some(track) = params
                    .get("track")
                    .or_else(|| params.get("index"))
                    .and_then(|s| s.parse().ok())
                else {
                    return ChannelResult::err("requires explicit track/index");
                };
                let Some(value) = params
                    .get("value")
                    .or_else(|| params.get("pan"))
                    .and_then(|s| s.parse().ok())
                else {
                    return ChannelResult::err("requires value or pan");
                };
                send(&self.engine, mcu_protocol::encode_pan(track, value))
            }
            "mixer.set_master_volume" => {
                let Some(value) = params
                    .get("value")
                    .or_else(|| params.get("volume"))
                    .and_then(|s| s.parse().ok())
                else {
                    return ChannelResult::err("requires value or volume");
                };
                send(&self.engine, mcu_protocol::encode_fader(8, value))
            }
            "mixer.set_send"
            | "mixer.toggle_eq"
            | "mixer.reset_strip"
            | "mixer.set_output_volume"
            | "mixer.set_output"
            | "mixer.set_input" => ChannelResult::not_implemented(operation),
            "track.set_automation" => {
                let _index = params.get("index").and_then(|s| s.parse::<u32>().ok());
                let mode = params.get("mode").map(String::as_str).unwrap_or("read");
                let bytes = match mcu_protocol::encode_automation(mode) {
                    Ok(b) => b,
                    Err(e) => return ChannelResult::err(e),
                };
                send(&self.engine, bytes)
            }
            "mixer.set_plugin_param" => {
                ChannelResult::err("mixer.set_plugin_param requires Scripter channel")
            }
            "transport.get_state" => transport_state_from_mcu(),
            "mixer.get_state" => mixer_state_from_mcu(),
            _ => ChannelResult::err(format!("Unknown MCU operation: {operation}")),
        }
    }
}

fn transport_state_from_mcu() -> ChannelResult {
    let cache = mcu_state::McuStateCache::global();
    let summary = cache.summary();
    let transport = summary.get("transport").cloned().unwrap_or(json!({}));
    ChannelResult::Success {
        message: "ok".into(),
        verified: Some(cache.is_feedback_fresh(5)),
        reason: if cache.is_feedback_fresh(5) {
            None
        } else {
            Some("mcu_feedback_stale".into())
        },
        detail: Some(json!({
            "isPlaying": transport.get("play").and_then(|v| v.as_bool()).unwrap_or(false),
            "isRecording": transport.get("record").and_then(|v| v.as_bool()).unwrap_or(false),
            "tempo": 120.0,
            "position": "1.1.1.1",
        })),
    }
}

fn mixer_state_from_mcu() -> ChannelResult {
    let cache = mcu_state::McuStateCache::global();
    let summary = cache.summary();
    let strips: Vec<_> = summary
        .get("strips")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| {
                    Some(json!({
                        "trackIndex": v.get("index")?.as_u64()?,
                        "volume": v.get("volume").and_then(|x| x.as_f64()).unwrap_or(0.0),
                        "pan": v.get("pan").and_then(|x| x.as_f64()).unwrap_or(0.0),
                    }))
                })
                .collect()
        })
        .unwrap_or_default();
    ChannelResult::Success {
        message: "ok".into(),
        verified: Some(cache.is_feedback_fresh(5)),
        reason: if cache.is_feedback_fresh(5) {
            None
        } else {
            Some("mcu_feedback_stale".into())
        },
        detail: Some(json!({ "strips": strips })),
    }
}

fn send(engine: &MidiEngine, bytes: [u8; 3]) -> ChannelResult {
    if engine.send_bytes(&bytes).is_err() {
        return ChannelResult::err("MCU send failed");
    }
    ChannelResult::ok(format!("MCU sent {:02X?}", bytes))
}
