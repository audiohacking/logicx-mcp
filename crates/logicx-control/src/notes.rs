use std::fmt;

#[derive(Debug, Clone)]
pub struct NoteEvent {
    pub pitch: u8,
    pub offset_ms: u32,
    pub duration_ms: u32,
    pub velocity: u8,
    pub channel: u8,
}

pub fn parse_notes(spec: &str) -> Result<Vec<NoteEvent>, String> {
    let mut events = Vec::new();
    for part in spec.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        let fields: Vec<&str> = part.split(',').map(str::trim).collect();
        if fields.len() < 3 {
            return Err(format!("invalid note event: {part}"));
        }
        let pitch = parse_u8(fields[0], "pitch", 0, 127)?;
        let offset_ms = parse_u32(fields[1], "offsetMs")?;
        let duration_ms = parse_u32(fields[2], "durationMs")?;
        if duration_ms == 0 || duration_ms > 30_000 {
            return Err(format!("duration_ms out of range in: {part}"));
        }
        let velocity = if fields.len() > 3 {
            parse_u8(fields[3], "velocity", 0, 127)?
        } else {
            95
        };
        let channel = if fields.len() > 4 {
            parse_u8(fields[4], "channel", 1, 16)?
        } else {
            1
        };
        events.push(NoteEvent {
            pitch,
            offset_ms,
            duration_ms,
            velocity,
            channel,
        });
    }
    Ok(events)
}

fn parse_u8(s: &str, field: &str, min: u8, max: u8) -> Result<u8, String> {
    let v: u16 = s.parse().map_err(|_| format!("invalid {field}: {s}"))?;
    if v < min as u16 || v > max as u16 {
        return Err(format!("{field} {v} out of range {min}–{max}"));
    }
    Ok(v as u8)
}

fn parse_u32(s: &str, field: &str) -> Result<u32, String> {
    s.parse().map_err(|_| format!("invalid {field}: {s}"))
}

impl fmt::Display for NoteEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "pitch={} @{}ms dur={}ms vel={} ch={}",
            self.pitch, self.offset_ms, self.duration_ms, self.velocity, self.channel
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mcp_format() {
        let ev = parse_notes("45,0,95;57,107,95,100,1").unwrap();
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].pitch, 45);
        assert_eq!(ev[1].velocity, 100);
        assert_eq!(ev[1].channel, 1);
    }
}
