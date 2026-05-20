use std::fmt;

#[derive(Debug, Clone)]
pub struct NoteEvent {
    pub pitch: u8,
    pub offset_ms: u32,
    pub duration_ms: u32,
    pub velocity: u8,
    pub channel: u8,
}

/// Parse MCP note spec: semicolon-separated events
/// `pitch,offsetMs,durationMs[,velocity[,channel]]`.
///
/// Also accepts comma-only streams (no semicolons) when the model omits separators.
pub fn parse_notes(spec: &str) -> Result<Vec<NoteEvent>, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Ok(Vec::new());
    }

    if spec.contains(';') {
        parse_semicolon_format(spec)
    } else {
        parse_comma_stream(spec)
    }
}

fn parse_semicolon_format(spec: &str) -> Result<Vec<NoteEvent>, String> {
    let mut events = Vec::new();
    for part in spec.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        events.push(parse_event_fields(part)?);
    }
    Ok(events)
}

/// `36,0,500,107,250,250` → two events when grouped as triplets.
fn parse_comma_stream(spec: &str) -> Result<Vec<NoteEvent>, String> {
    let fields: Vec<&str> = spec.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if fields.len() < 3 {
        return Err(format!(
            "notes must use semicolons between events (e.g. \"36,0,500;36,500,500\"); got: {spec}"
        ));
    }

    let stride = if fields.len().is_multiple_of(5) {
        5
    } else if fields.len().is_multiple_of(4) {
        4
    } else if fields.len().is_multiple_of(3) {
        3
    } else {
        return Err(format!(
            "comma-only notes must be groups of 3–5 fields per event (pitch,offsetMs,durationMs[,velocity[,channel]]); \
             got {} fields — use semicolons between events",
            fields.len()
        ));
    };

    let mut events = Vec::with_capacity(fields.len() / stride);
    for chunk in fields.chunks(stride) {
        events.push(parse_event_fields(&chunk.join(","))?);
    }
    Ok(events)
}

fn parse_event_fields(part: &str) -> Result<NoteEvent, String> {
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
    Ok(NoteEvent {
        pitch,
        offset_ms,
        duration_ms,
        velocity,
        channel,
    })
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

    #[test]
    fn parses_comma_only_triplets() {
        let ev = parse_notes("36,0,500,107,250,250").unwrap();
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].pitch, 36);
        assert_eq!(ev[1].pitch, 107);
    }

    #[test]
    fn rejects_ambiguous_comma_stream() {
        assert!(parse_notes("36,0,500,107,250").is_err()); // 5 fields but channel 250 invalid
        assert!(parse_notes("36,0").is_err());
    }
}
