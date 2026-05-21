use crate::notes::NoteEvent;
use logicx_core::runtime::support_dir;
use std::path::PathBuf;

const TICKS_PER_QUARTER: u16 = 480;

pub fn write_temp_file(
    events: &[NoteEvent],
    tempo_bpm: f64,
    bar_count: u32,
) -> Result<PathBuf, String> {
    let mut path = support_dir().join("sequences");
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push(format!("{}.mid", uuid_simple()));

    let data = build_smf(events, tempo_bpm, bar_count)?;
    std::fs::write(&path, &data).map_err(|e| e.to_string())?;
    Ok(path)
}

/// Remove `.mid` files older than `max_age_secs` in `dir` (logic-pro-mcp SMFWriter.cleanupOrphanFiles).
pub fn cleanup_orphan_files(dir: &std::path::Path, max_age_secs: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(max_age_secs))
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("mid") {
            continue;
        }
        if let Ok(meta) = entry.metadata()
            && let Ok(modified) = meta.modified()
            && modified < cutoff
        {
            let _ = std::fs::remove_file(&path);
        }
    }
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

/// Build SMF bytes for tests and internal use.
pub fn build_smf_bytes(
    events: &[NoteEvent],
    tempo_bpm: f64,
    bar_count: u32,
) -> Result<Vec<u8>, String> {
    build_smf(events, tempo_bpm, bar_count)
}

/// logic-pro-mcp SMFWriter.msToTicks parity (round half up).
pub fn ms_to_ticks_public(offset_ms: u32, duration_ms: u32, tempo_bpm: f64) -> (u32, u32) {
    let ms_per_tick = (60_000.0 / tempo_bpm) / f64::from(TICKS_PER_QUARTER);
    (
        ms_to_ticks_single(offset_ms, ms_per_tick),
        ms_to_ticks_single(duration_ms, ms_per_tick),
    )
}

/// Minimal Type-0 SMF with tempo meta + note events.
fn build_smf(events: &[NoteEvent], tempo_bpm: f64, bar_count: u32) -> Result<Vec<u8>, String> {
    if events.is_empty() {
        return Err("SMF requires at least one note event".into());
    }

    let mut track = Vec::new();

    let us_per_quarter = (60_000_000.0 / tempo_bpm).round() as u32;
    write_var_len(&mut track, 0);
    track.extend([0xFF, 0x51, 0x03]);
    track.push(((us_per_quarter >> 16) & 0xFF) as u8);
    track.push(((us_per_quarter >> 8) & 0xFF) as u8);
    track.push((us_per_quarter & 0xFF) as u8);

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
        if ev.pitch > 127 {
            return Err(format!("invalid pitch {} (must be 0-127)", ev.pitch));
        }
        let start_tick = ms_to_ticks_single(ev.offset_ms, ms_per_tick);
        let dur_ticks = ms_to_ticks_single(ev.duration_ms, ms_per_tick).max(1);
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

fn ms_to_ticks_single(ms: u32, ms_per_tick: f64) -> u32 {
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

    fn find_pattern(pattern: &[u8], bytes: &[u8]) -> bool {
        if pattern.len() > bytes.len() {
            return false;
        }
        bytes.windows(pattern.len()).any(|w| w == pattern)
    }

    fn one_note() -> Vec<NoteEvent> {
        vec![NoteEvent {
            pitch: 60,
            offset_ms: 0,
            duration_ms: 480,
            velocity: 100,
            channel: 1,
        }]
    }

    #[test]
    fn smf_has_header() {
        let data = build_smf(&one_note(), 120.0, 1).unwrap();
        assert!(data.starts_with(b"MThd"));
        assert_eq!(&data[12..14], &[0x01, 0xE0]);
    }

    #[test]
    fn smf_tempo_meta_event_120bpm() {
        let data = build_smf(&one_note(), 120.0, 1).unwrap();
        assert!(find_pattern(&[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20], &data));
    }

    #[test]
    fn smf_time_signature_4_4() {
        let data = build_smf(&one_note(), 120.0, 1).unwrap();
        assert!(find_pattern(
            &[0xFF, 0x58, 0x04, 0x04, 0x02, 0x18, 0x08],
            &data
        ));
    }

    #[test]
    fn smf_end_of_track() {
        let data = build_smf(&one_note(), 120.0, 1).unwrap();
        assert_eq!(&data[data.len() - 3..], &[0xFF, 0x2F, 0x00]);
    }

    #[test]
    fn smf_padding_cc_when_bar_gt_one() {
        let data = build_smf(&one_note(), 120.0, 5).unwrap();
        assert!(find_pattern(&[0xB0, 0x6E, 0x00], &data));
    }

    #[test]
    fn smf_no_padding_when_bar_one() {
        let data = build_smf(&one_note(), 120.0, 1).unwrap();
        assert!(!find_pattern(&[0xB0, 0x6E, 0x00], &data));
    }

    #[test]
    fn smf_channel_one_maps_to_wire_zero() {
        let data = build_smf(&one_note(), 120.0, 1).unwrap();
        assert!(find_pattern(&[0x90, 0x3C, 0x64], &data));
    }

    #[test]
    fn smf_rejects_empty_events() {
        assert!(build_smf(&[], 120.0, 1).is_err());
    }

    #[test]
    fn ms_to_ticks_round_half_up() {
        let (offset, _) = ms_to_ticks_public(83, 100, 137.0);
        assert_eq!(offset, 91);
        let (o2, d2) = ms_to_ticks_public(500, 500, 120.0);
        assert_eq!(o2, 480);
        assert_eq!(d2, 480);
    }

    #[test]
    fn smf_rejects_invalid_pitch() {
        let bad = vec![NoteEvent {
            pitch: 128,
            offset_ms: 0,
            duration_ms: 480,
            velocity: 100,
            channel: 1,
        }];
        assert!(build_smf(&bad, 120.0, 1).is_err());
    }

    #[test]
    fn smf_chord_three_note_ons() {
        let chord = vec![
            NoteEvent {
                pitch: 60,
                offset_ms: 0,
                duration_ms: 480,
                velocity: 100,
                channel: 1,
            },
            NoteEvent {
                pitch: 64,
                offset_ms: 0,
                duration_ms: 480,
                velocity: 100,
                channel: 1,
            },
            NoteEvent {
                pitch: 67,
                offset_ms: 0,
                duration_ms: 480,
                velocity: 100,
                channel: 1,
            },
        ];
        let data = build_smf(&chord, 120.0, 1).unwrap();
        let count = data
            .windows(3)
            .filter(|w| w[0] == 0x90 && w[2] == 0x64)
            .count();
        assert_eq!(count, 3);
    }

    #[test]
    fn smf_accepts_1024_notes() {
        let events: Vec<NoteEvent> = (0..1024)
            .map(|i| NoteEvent {
                pitch: 60 + (i % 12) as u8,
                offset_ms: i * 10,
                duration_ms: 100,
                velocity: 80,
                channel: 1,
            })
            .collect();
        let data = build_smf(&events, 120.0, 1).unwrap();
        assert!(!data.is_empty());
    }

    #[test]
    fn cleanup_orphan_files_removes_old() {
        let dir = std::env::temp_dir().join(format!("smf-cleanup-{}", uuid_simple()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join("old.mid");
        let recent = dir.join("recent.mid");
        std::fs::write(&old, b"old").unwrap();
        std::fs::write(&recent, b"new").unwrap();
        #[cfg(unix)]
        {
            let _ = std::process::Command::new("touch")
                .args(["-t", "202001010000"])
                .arg(&old)
                .status();
        }
        cleanup_orphan_files(&dir, 300);
        assert!(!old.exists());
        assert!(recent.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cleanup_orphan_files_missing_dir_no_panic() {
        cleanup_orphan_files(
            std::path::Path::new("/tmp/does-not-exist-logicx-mcp-smf-cleanup"),
            300,
        );
    }
}
