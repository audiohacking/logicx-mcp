use crate::channels::{ChannelHealth, ChannelId, ChannelResult, Params};
use crate::midi::{engine::MidiEngine, engine::list_midi_ports, engine::midi_channel_param, engine::midi_data7, mmc};
use crate::notes;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub const PORT_NAME: &str = "LogicX-MCP-Virtual";

pub struct CoreMidiChannel {
    engine: Arc<MidiEngine>,
}

impl CoreMidiChannel {
    pub fn new(engine: Arc<MidiEngine>) -> Self {
        Self { engine }
    }

    pub fn health(&self) -> ChannelHealth {
        if self.engine.is_active() {
            ChannelHealth::healthy("CoreMIDI virtual port active")
        } else {
            ChannelHealth::unavailable("CoreMIDI not started")
        }
    }

    pub fn execute(&self, operation: &str, params: &Params) -> ChannelResult {
        if !self.engine.is_active() {
            return ChannelResult::err("CoreMIDI engine not active");
        }

        match operation {
            "transport.play" | "mmc.play" => {
                send_sysex(&self.engine, &mmc::play());
                ChannelResult::ok("MMC play sent")
            }
            "transport.stop" | "mmc.stop" => {
                send_sysex(&self.engine, &mmc::stop());
                ChannelResult::ok("MMC stop sent")
            }
            "transport.pause" | "mmc.pause" => {
                send_sysex(&self.engine, &mmc::pause());
                ChannelResult::ok("MMC pause sent")
            }
            "transport.record" | "mmc.record_strobe" => {
                send_sysex(&self.engine, &mmc::record_strobe());
                ChannelResult::ok("MMC record strobe sent")
            }
            "mmc.record_exit" => {
                send_sysex(&self.engine, &mmc::record_exit());
                ChannelResult::ok("MMC record exit sent")
            }
            "transport.rewind" | "mmc.rewind" => {
                send_sysex(&self.engine, &mmc::rewind());
                ChannelResult::ok("MMC rewind sent")
            }
            "transport.fast_forward" | "mmc.fast_forward" => {
                send_sysex(&self.engine, &mmc::fast_forward());
                ChannelResult::ok("MMC fast forward sent")
            }
            "mmc.locate" | "transport.locate" => {
                if let Some(bar) = params.get("bar").and_then(|s| s.parse::<i32>().ok()) {
                    let tempo = params
                        .get("tempo")
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(120.0);
                    let beats = params
                        .get("beats_per_bar")
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(4);
                    let beat = params
                        .get("beat")
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(1.0);
                    let Some(smpte) =
                        mmc::bar_beat_to_smpte(bar, beat, tempo, beats, mmc::FrameRate::Fps30)
                    else {
                        return ChannelResult::err(format!("invalid bar locate: bar={bar}"));
                    };
                    let bytes = match mmc::locate_strict(
                        smpte.hours,
                        smpte.minutes,
                        smpte.seconds,
                        smpte.frames,
                        smpte.subframes,
                        mmc::FrameRate::Fps30,
                    ) {
                        Ok(b) => b,
                        Err(e) => return ChannelResult::err(format!("mmc locate validation: {e:?}")),
                    };
                    send_sysex(&self.engine, &bytes);
                    return ChannelResult::ok(format!(
                        "MMC locate sent to bar {bar} ({:02}:{:02}:{:02}:{:02})",
                        smpte.hours, smpte.minutes, smpte.seconds, smpte.frames
                    ));
                }
                let time = params.get("time").map(String::as_str).or_else(|| {
                    params.get("position").map(String::as_str)
                });
                let Some(time) = time else {
                    return ChannelResult::err("mmc.locate requires time HH:MM:SS:FF or bar");
                };
                let Some((h, m, s, f)) = mmc::parse_locate_time(time) else {
                    return ChannelResult::err(format!("invalid locate time: {time}"));
                };
                let bytes = match mmc::locate_strict(h, m, s, f, 0, mmc::FrameRate::Fps30) {
                    Ok(b) => b,
                    Err(e) => return ChannelResult::err(format!("mmc locate validation: {e:?}")),
                };
                send_sysex(&self.engine, &bytes);
                ChannelResult::ok(format!("MMC locate sent to {time}"))
            }
            "transport.goto_position" => {
                ChannelResult::err("CoreMIDI cannot position playhead; use AX fallback")
            }
            "midi.send_note" => self.send_note(params),
            "midi.send_chord" => self.send_chord(params),
            "midi.send_cc" => self.send_cc(params),
            "midi.send_program_change" | "midi.program_change" => self.program_change(params),
            "midi.send_pitch_bend" | "midi.pitch_bend" => self.pitch_bend(params),
            "midi.send_aftertouch" | "midi.aftertouch" => self.aftertouch(params),
            "midi.send_sysex" => self.send_sysex_param(params),
            "midi.play_sequence" => self.play_sequence(params),
            "midi.step_input" => self.step_input(params),
            "midi.list_ports" => {
                ChannelResult::ok(serde_json::to_string(&list_midi_ports()).unwrap_or_else(|_| "{}".into()))
            }
            "midi.create_virtual_port" => {
                let name = params.get("name").cloned().unwrap_or_else(|| PORT_NAME.into());
                ChannelResult::ok(format!("Virtual port '{name}' managed by LogicX MCP engine"))
            }
            "midi.get_input_state" => ChannelResult::ok("{\"active\":true}"),
            _ => ChannelResult::err(format!("Unknown CoreMIDI operation: {operation}")),
        }
    }

    fn send_note(&self, params: &Params) -> ChannelResult {
        let note = match midi_data7(params.get("note").map(String::as_str)) {
            Some(v) => v,
            None => return ChannelResult::err("send_note requires note 0-127"),
        };
        let velocity = midi_data7(params.get("velocity").map(String::as_str)).unwrap_or(100);
        let channel = midi_channel_param(params.get("channel").map(String::as_str));
        let duration_ms = params
            .get("duration_ms")
            .and_then(|s| s.parse().ok())
            .unwrap_or(250)
            .min(30_000);
        let engine = Arc::clone(&self.engine);
        if engine.send_note_on(channel, note, velocity).is_err() {
            return ChannelResult::err("failed to send note on");
        }
        thread::sleep(Duration::from_millis(duration_ms));
        let _ = engine.send_note_off(channel, note);
        ChannelResult::ok(format!("Note {note} ch {channel} vel {velocity} dur {duration_ms}ms"))
    }

    fn send_chord(&self, params: &Params) -> ChannelResult {
        let notes_str = params.get("notes").map(String::as_str).unwrap_or("");
        let parsed: Result<Vec<u8>, ChannelResult> = notes_str
            .split(',')
            .map(|s| {
                let v: i32 = s.trim().parse().map_err(|_| {
                    ChannelResult::err("send_chord notes must be comma-separated integers")
                })?;
                if (0..=127).contains(&v) {
                    Ok(v as u8)
                } else {
                    Err(ChannelResult::err("send_chord notes must be 0-127"))
                }
            })
            .collect();
        let notes = match parsed {
            Ok(n) if !n.is_empty() && n.len() <= 24 => n,
            Ok(n) if n.is_empty() => return ChannelResult::err("send_chord requires notes"),
            Ok(_) => return ChannelResult::err("send_chord max 24 notes"),
            Err(e) => return e,
        };
        let velocity = midi_data7(params.get("velocity").map(String::as_str)).unwrap_or(80);
        let channel = midi_channel_param(params.get("channel").map(String::as_str));
        let duration_ms = params
            .get("duration_ms")
            .and_then(|s| s.parse().ok())
            .unwrap_or(500)
            .min(30_000);
        for &n in &notes {
            let _ = self.engine.send_note_on(channel, n, velocity);
        }
        thread::sleep(Duration::from_millis(duration_ms));
        for &n in &notes {
            let _ = self.engine.send_note_off(channel, n);
        }
        ChannelResult::ok(format!("Chord sent: {} notes", notes.len()))
    }

    fn send_cc(&self, params: &Params) -> ChannelResult {
        let controller = match midi_data7(params.get("controller").map(String::as_str)) {
            Some(v) => v,
            None => return ChannelResult::err("send_cc requires controller 0-127"),
        };
        let value = match midi_data7(params.get("value").map(String::as_str)) {
            Some(v) => v,
            None => return ChannelResult::err("send_cc requires value 0-127"),
        };
        let channel = midi_channel_param(params.get("channel").map(String::as_str));
        let _ = self.engine.send_cc(channel, controller, value);
        ChannelResult::ok(format!("CC {controller}={value} ch {channel}"))
    }

    fn program_change(&self, params: &Params) -> ChannelResult {
        let program = match midi_data7(params.get("program").map(String::as_str)) {
            Some(v) => v,
            None => return ChannelResult::err("program_change requires program 0-127"),
        };
        let channel = midi_channel_param(params.get("channel").map(String::as_str));
        let _ = self.engine.send_program_change(channel, program);
        ChannelResult::ok(format!("Program {program} ch {channel}"))
    }

    fn pitch_bend(&self, params: &Params) -> ChannelResult {
        let value: u16 = if let Some(s) = params.get("value") {
            if let Ok(signed) = s.parse::<i32>() {
                ((signed.clamp(-8192, 8191) + 8192) as u16).min(16383)
            } else {
                s.parse::<u16>().unwrap_or(8192).min(16383)
            }
        } else {
            return ChannelResult::err("pitch_bend requires value");
        };
        let channel = midi_channel_param(params.get("channel").map(String::as_str));
        let _ = self.engine.send_pitch_bend(channel, value);
        ChannelResult::ok(format!("Pitch bend {value} ch {channel}"))
    }

    fn aftertouch(&self, params: &Params) -> ChannelResult {
        let pressure = match midi_data7(
            params
                .get("pressure")
                .or_else(|| params.get("value"))
                .map(String::as_str),
        ) {
            Some(v) => v,
            None => return ChannelResult::err("aftertouch requires pressure 0-127"),
        };
        let channel = midi_channel_param(params.get("channel").map(String::as_str));
        let _ = self.engine.send_aftertouch(channel, pressure);
        ChannelResult::ok(format!("Aftertouch {pressure} ch {channel}"))
    }

    fn send_sysex_param(&self, params: &Params) -> ChannelResult {
        let hex = params
            .get("bytes")
            .or_else(|| params.get("data"))
            .map(String::as_str)
            .unwrap_or("");
        let bytes: Vec<u8> = hex
            .replace("0x", "")
            .replace(',', " ")
            .split_whitespace()
            .filter_map(|t| u8::from_str_radix(t, 16).ok())
            .collect();
        if bytes.len() < 3 || bytes.first() != Some(&0xF0) || bytes.last() != Some(&0xF7) {
            return ChannelResult::err("SysEx must start with F0 and end with F7");
        }
        let len = bytes.len();
        send_sysex(&self.engine, &bytes);
        ChannelResult::ok(format!("SysEx sent ({len} bytes)"))
    }

    fn play_sequence(&self, params: &Params) -> ChannelResult {
        let notes_str = params.get("notes").map(String::as_str).unwrap_or("");
        let events = match notes::parse_notes(notes_str) {
            Ok(e) if !e.is_empty() => e,
            Ok(_) => return ChannelResult::err("play_sequence requires notes"),
            Err(e) => return ChannelResult::err(e),
        };
        if events.len() > 256 {
            return ChannelResult::err("play_sequence max 256 events");
        }
        let start = std::time::Instant::now();
        for ev in &events {
            let target = Duration::from_millis(u64::from(ev.offset_ms));
            if start.elapsed() < target {
                thread::sleep(target - start.elapsed());
            }
            let ch = ev.channel.saturating_sub(1) & 0x0F;
            let _ = self
                .engine
                .send_note_on(ch, ev.pitch, ev.velocity);
            let engine = Arc::clone(&self.engine);
            let pitch = ev.pitch;
            let dur = ev.duration_ms;
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(u64::from(dur)));
                let _ = engine.send_note_off(ch, pitch);
            });
        }
        ChannelResult::ok(format!("Sequence sent: {} events", events.len()))
    }

    fn step_input(&self, params: &Params) -> ChannelResult {
        let note = params
            .get("note")
            .and_then(|s| s.parse().ok())
            .unwrap_or(60) as u8;
        let duration_ms = params
            .get("duration")
            .or_else(|| params.get("duration_ms"))
            .and_then(|s| s.parse().ok())
            .unwrap_or(250)
            .min(30_000);
        let _ = self.engine.send_note_on(0, note, 80);
        thread::sleep(Duration::from_millis(duration_ms));
        let _ = self.engine.send_note_off(0, note);
        ChannelResult::ok(format!("Step input note {note} dur {duration_ms}ms"))
    }
}

fn send_sysex(engine: &MidiEngine, bytes: &[u8]) {
    let _ = engine.send_sysex(bytes);
}

pub fn channel_id() -> ChannelId {
    ChannelId::CoreMidi
}
