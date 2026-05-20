//! MCU MIDI feedback listener — parses Logic Pro echo into [`McuStateCache`].

use midir::os::unix::VirtualInput;
use super::mcu_state::McuStateCache;
use super::mcu_protocol::{self, FEEDBACK_PORT_NAME};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::sync::Arc;

static LISTENER: Lazy<Mutex<Option<McuFeedbackHandle>>> = Lazy::new(|| Mutex::new(None));

struct McuFeedbackHandle {
    _port: midir::MidiInputConnection<()>,
}

pub fn ensure_started(cache: Arc<McuStateCache>) {
    let mut guard = LISTENER.lock();
    if guard.is_some() {
        return;
    }
    match start_listener(cache) {
        Ok(handle) => {
            *guard = Some(handle);
            eprintln!("[LogicX MCP] MCU feedback listener started on '{FEEDBACK_PORT_NAME}'");
        }
        Err(e) => {
            eprintln!("[LogicX MCP] MCU feedback listener failed: {e}");
        }
    }
}

fn start_listener(cache: Arc<McuStateCache>) -> Result<McuFeedbackHandle, String> {
    let input = midir::MidiInput::new("LogicX MCP MCU Feedback")
        .map_err(|e| format!("MidiInput init: {e}"))?;

    let cache_cb = cache.clone();
    let port = input
        .create_virtual(
            FEEDBACK_PORT_NAME,
            move |_stamp, message, _| {
                parse_message(&cache_cb, message);
            },
            (),
        )
        .map_err(|e| format!("create_virtual input: {e}"))?;

    Ok(McuFeedbackHandle { _port: port })
}

fn parse_message(cache: &McuStateCache, message: &[u8]) {
    if message.is_empty() {
        return;
    }
    let status = message[0];
    match status & 0xF0 {
        0xE0 if message.len() >= 3 => {
            let ch = (status & 0x0F) as usize;
            let value = u16::from(message[2] & 0x7F) << 7 | u16::from(message[1] & 0x7F);
            let normalized = f64::from(value) / 16383.0;
            cache.update_fader(ch, normalized);
        }
        0x90 if message.len() >= 3 => {
            let note = message[1];
            let vel = message[2];
            if let Some((func, _strip)) = mcu_protocol::decode_button_note(note) {
                use mcu_protocol::ButtonFunction;
                match func {
                    ButtonFunction::Play => {
                        cache.update_transport_leds(vel > 0, false);
                    }
                    ButtonFunction::Record => {
                        cache.update_transport_leds(false, vel > 0);
                    }
                    _ => {}
                }
            }
            cache.touch_feedback();
        }
        0xB0 if message.len() >= 3 => {
            let cc = message[1];
            let val = message[2];
            if let Some(ring) = mcu_protocol::decode_vpot_led_ring(cc, val) {
                let pan = mcu_protocol::vpot_position_to_pan(ring.position);
                cache.update_pan(ring.strip as usize, pan);
            }
            cache.touch_feedback();
        }
        _ => {
            cache.touch_feedback();
        }
    }
}

pub fn send_device_query(engine: &super::engine::MidiEngine) {
    let query = mcu_protocol::encode_device_query();
    let _ = engine.send_sysex(&query);
}
