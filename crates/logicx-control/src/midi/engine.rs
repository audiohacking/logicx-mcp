use midir::os::unix::VirtualOutput;
use std::sync::Mutex;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MidiEngineError {
    #[error("MIDI init failed: {0}")]
    Init(String),
    #[error("MIDI send failed: {0}")]
    Send(String),
}

pub struct MidiEngine {
    connection: Mutex<Option<midir::MidiOutputConnection>>,
    port_name: String,
}

impl MidiEngine {
    pub fn new(port_name: impl Into<String>) -> Self {
        Self {
            connection: Mutex::new(None),
            port_name: port_name.into(),
        }
    }

    pub fn start(&self) -> Result<(), MidiEngineError> {
        let mut guard = self
            .connection
            .lock()
            .map_err(|e| MidiEngineError::Init(e.to_string()))?;
        if guard.is_some() {
            return Ok(());
        }
        let midi_out =
            midir::MidiOutput::new("LogicX MCP").map_err(|e| MidiEngineError::Init(e.to_string()))?;
        let conn = midi_out
            .create_virtual(&self.port_name)
            .map_err(|e| MidiEngineError::Init(e.to_string()))?;
        *guard = Some(conn);
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.connection
            .lock()
            .ok()
            .is_some_and(|g| g.is_some())
    }

    pub fn send_bytes(&self, bytes: &[u8]) -> Result<(), MidiEngineError> {
        let mut guard = self
            .connection
            .lock()
            .map_err(|e| MidiEngineError::Send(e.to_string()))?;
        let Some(conn) = guard.as_mut() else {
            return Err(MidiEngineError::Send("MIDI port not active".into()));
        };
        conn.send(bytes)
            .map_err(|e| MidiEngineError::Send(e.to_string()))
    }

    pub fn send_sysex(&self, bytes: &[u8]) -> Result<(), MidiEngineError> {
        self.send_bytes(bytes)
    }

    pub fn send_note_on(&self, channel: u8, note: u8, velocity: u8) -> Result<(), MidiEngineError> {
        self.send_bytes(&[0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F])
    }

    pub fn send_note_off(&self, channel: u8, note: u8) -> Result<(), MidiEngineError> {
        self.send_bytes(&[0x80 | (channel & 0x0F), note & 0x7F, 0])
    }

    pub fn send_cc(&self, channel: u8, controller: u8, value: u8) -> Result<(), MidiEngineError> {
        self.send_bytes(&[0xB0 | (channel & 0x0F), controller & 0x7F, value & 0x7F])
    }

    pub fn send_program_change(&self, channel: u8, program: u8) -> Result<(), MidiEngineError> {
        self.send_bytes(&[0xC0 | (channel & 0x0F), program & 0x7F])
    }

    pub fn send_pitch_bend(&self, channel: u8, value: u16) -> Result<(), MidiEngineError> {
        let v = value.min(16383);
        let lsb = (v & 0x7F) as u8;
        let msb = ((v >> 7) & 0x7F) as u8;
        self.send_bytes(&[0xE0 | (channel & 0x0F), lsb, msb])
    }

    pub fn send_aftertouch(&self, channel: u8, pressure: u8) -> Result<(), MidiEngineError> {
        self.send_bytes(&[0xD0 | (channel & 0x0F), pressure & 0x7F])
    }
}

pub fn list_midi_ports() -> serde_json::Value {
    let mut sources = Vec::new();
    let mut destinations = Vec::new();
    if let Ok(midi_in) = midir::MidiInput::new("LogicX MCP list") {
        for port in midi_in.ports() {
            if let Ok(name) = midi_in.port_name(&port) {
                sources.push(name);
            }
        }
    }
    if let Ok(midi_out) = midir::MidiOutput::new("LogicX MCP list") {
        for port in midi_out.ports() {
            if let Ok(name) = midi_out.port_name(&port) {
                destinations.push(name);
            }
        }
    }
    serde_json::json!({ "sources": sources, "destinations": destinations })
}

pub fn midi_channel_param(s: Option<&str>) -> u8 {
    let Some(s) = s else {
        return 0;
    };
    let Ok(v) = s.parse::<i32>() else {
        return 0;
    };
    if (1..=16).contains(&v) {
        (v - 1) as u8
    } else if (0..=15).contains(&v) {
        v as u8
    } else {
        0
    }
}

pub fn midi_data7(s: Option<&str>) -> Option<u8> {
    let v = s?.parse::<i32>().ok()?;
    if (0..=127).contains(&v) {
        Some(v as u8)
    } else {
        None
    }
}
