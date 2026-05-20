use crate::notes::NoteEvent;
use std::path::PathBuf;

const TICKS_PER_QUARTER: u16 = 480;

pub fn write_temp_file(
    events: &[NoteEvent],
    tempo_bpm: f64,
    bar_count: u32,
) -> Result<PathBuf, String> {
    let mut path = std::env::temp_dir();
    path.push("LogicXMCP");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push(format!("{}.mid", uuid_simple()));

    let data = build_smf(events, tempo_bpm, bar_count)?;
    std::fs::write(&path, &data).map_err(|e| e.to_string())?;
    Ok(path)
}

fn uuid_simple() -> String {
    #[cfg(target_os = "macos")]
    {
        uuid::Uuid::new_v4().to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "seq".to_string()
    }
}

/// Minimal Type-0 SMF with tempo meta + note events.
fn build_smf(events: &[NoteEvent], tempo_bpm: f64, bar_count: u32) -> Result<Vec<u8>, String> {
    let mut track = Vec::new();

    let us_per_quarter = (60_000_000.0 / tempo_bpm).round() as u32;
    write_var_len(&mut track, 0);
    track.extend([0xFF, 0x51, 0x03]);
    track.extend(us_per_quarter.to_be_bytes());

    write_var_len(&mut track, 0);
    track.extend([0xFF, 0x58, 0x04, 0x04, 0x02, 0x18, 0x08]);

    if bar_count > 1 {
        write_var_len(&mut track, 0);
        track.extend([0xB0, 110, 0]);
    }

    let ms_per_tick = (60_000.0 / tempo_bpm) / f64::from(TICKS_PER_QUARTER);
    let mut last_tick: u32 = 0;

    let mut sorted: Vec<&NoteEvent> = events.iter().collect();
    sorted.sort_by_key(|e| e.offset_ms);

    for ev in sorted {
        let start_tick = ms_to_ticks(ev.offset_ms, ms_per_tick);
        let dur_ticks = ms_to_ticks(ev.duration_ms, ms_per_tick).max(1);
        let ch = ev.channel.saturating_sub(1) & 0x0F;

        write_var_len(&mut track, start_tick.saturating_sub(last_tick));
        track.push(0x90 | ch);
        track.push(ev.pitch);
        track.push(ev.velocity);

        write_var_len(&mut track, dur_ticks);
        track.push(0x80 | ch);
        track.push(ev.pitch);
        track.push(0);
        last_tick = start_tick + dur_ticks;
    }

    write_var_len(&mut track, 0);
    track.extend([0xFF, 0x2F, 0x00]);

    let mut out = Vec::new();
    out.extend(b"MThd");
    out.extend(6u32.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend(1u16.to_be_bytes());
    out.extend(TICKS_PER_QUARTER.to_be_bytes());

    out.extend(b"MTrk");
    out.extend((track.len() as u32).to_be_bytes());
    out.extend(&track);

    Ok(out)
}

fn ms_to_ticks(ms: u32, ms_per_tick: f64) -> u32 {
    (f64::from(ms) / ms_per_tick).round() as u32
}

fn write_var_len(out: &mut Vec<u8>, mut value: u32) {
    if value == 0 {
        out.push(0);
        return;
    }
    let mut buffer = [0u8; 4];
    let mut i = 0;
    while value > 0 {
        buffer[i] = (value & 0x7F) as u8;
        value >>= 7;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        let mut byte = buffer[i];
        if i > 0 {
            byte |= 0x80;
        }
        out.push(byte);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::NoteEvent;

    #[test]
    fn smf_has_header() {
        let ev = vec![NoteEvent {
            pitch: 60,
            offset_ms: 0,
            duration_ms: 100,
            velocity: 100,
            channel: 1,
        }];
        let data = build_smf(&ev, 120.0, 4).unwrap();
        assert!(data.starts_with(b"MThd"));
    }
}
