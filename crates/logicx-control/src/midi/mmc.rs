//! MIDI Machine Control SysEx builders (logic-pro-mcp compatible).

pub const DEVICE_ID: u8 = 0x7F;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmcError {
    HoursOutOfRange,
    MinutesOutOfRange,
    SecondsOutOfRange,
    FramesOutOfRange,
    SubframesOutOfRange,
    DeviceIdOutOfRange,
    TargetOutOfRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameRate {
    Fps24,
    Fps25,
    Fps2997Df,
    Fps30,
}

impl FrameRate {
    pub fn encoding(self) -> u8 {
        match self {
            Self::Fps24 => 0b0000_0000,
            Self::Fps25 => 0b0010_0000,
            Self::Fps2997Df => 0b0100_0000,
            Self::Fps30 => 0b0110_0000,
        }
    }

    pub fn max_frame(self) -> u8 {
        match self {
            Self::Fps24 => 23,
            Self::Fps25 => 24,
            Self::Fps2997Df | Self::Fps30 => 29,
        }
    }

    pub fn fps(self) -> f64 {
        match self {
            Self::Fps24 => 24.0,
            Self::Fps25 => 25.0,
            Self::Fps2997Df => 30_000.0 / 1_001.0,
            Self::Fps30 => 30.0,
        }
    }
}

pub fn play() -> Vec<u8> {
    sys_ex(0x02)
}

pub fn stop() -> Vec<u8> {
    sys_ex(0x01)
}

pub fn record_strobe() -> Vec<u8> {
    sys_ex(0x06)
}

pub fn record_exit() -> Vec<u8> {
    sys_ex(0x07)
}

pub fn pause() -> Vec<u8> {
    sys_ex(0x09)
}

pub fn fast_forward() -> Vec<u8> {
    sys_ex(0x04)
}

pub fn rewind() -> Vec<u8> {
    sys_ex(0x05)
}

pub fn deferred_play() -> Vec<u8> {
    sys_ex(0x03)
}

pub fn reset() -> Vec<u8> {
    sys_ex(0x0D)
}

pub fn write() -> Vec<u8> {
    sys_ex(0x40)
}

pub fn locate(hours: u8, minutes: u8, seconds: u8, frames: u8, subframes: u8) -> Vec<u8> {
    vec![
        0xF0, 0x7F, DEVICE_ID, 0x06, 0x44, 0x06, 0x01, hours, minutes, seconds, frames, subframes,
        0xF7,
    ]
}

pub fn locate_strict(
    hours: u8,
    minutes: u8,
    seconds: u8,
    frames: u8,
    subframes: u8,
    frame_rate: FrameRate,
) -> Result<Vec<u8>, MmcError> {
    validate_device_id(DEVICE_ID)?;
    if hours > 23 {
        return Err(MmcError::HoursOutOfRange);
    }
    if minutes > 59 {
        return Err(MmcError::MinutesOutOfRange);
    }
    if seconds > 59 {
        return Err(MmcError::SecondsOutOfRange);
    }
    if frames > frame_rate.max_frame() {
        return Err(MmcError::FramesOutOfRange);
    }
    if subframes > 99 {
        return Err(MmcError::SubframesOutOfRange);
    }
    let encoded_hours = hours | frame_rate.encoding();
    Ok(vec![
        0xF0,
        0x7F,
        DEVICE_ID,
        0x06,
        0x44,
        0x06,
        0x01,
        encoded_hours,
        minutes,
        seconds,
        frames,
        subframes,
        0xF7,
    ])
}

pub fn goto_target(target: u8) -> Result<Vec<u8>, MmcError> {
    validate_device_id(DEVICE_ID)?;
    if target > 0x7F {
        return Err(MmcError::TargetOutOfRange);
    }
    Ok(vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x44, 0x01, target, 0xF7])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmpteTime {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub frames: u8,
    pub subframes: u8,
}

pub fn bar_beat_to_smpte(
    bar: i32,
    beat: f64,
    tempo: f64,
    beats_per_bar: i32,
    frame_rate: FrameRate,
) -> Option<SmpteTime> {
    if bar < 1 || beat < 1.0 || tempo <= 0.0 || beats_per_bar <= 0 {
        return None;
    }
    let total_beats = f64::from(bar - 1) * f64::from(beats_per_bar) + (beat - 1.0);
    let total_seconds = total_beats * 60.0 / tempo;
    if !total_seconds.is_finite() || !(0.0..86_400.0).contains(&total_seconds) {
        return None;
    }
    let whole_seconds = total_seconds.floor() as i64;
    let hours = (whole_seconds / 3600) as u8;
    let minutes = ((whole_seconds / 60) % 60) as u8;
    let seconds = (whole_seconds % 60) as u8;
    let fractional = total_seconds - whole_seconds as f64;
    let frames_double = fractional * frame_rate.fps();
    let whole_frames = frames_double.floor() as i32;
    let capped_frames = whole_frames.min(i32::from(frame_rate.max_frame())).max(0) as u8;
    let subframes_double = (frames_double - f64::from(whole_frames)) * 100.0;
    let subframes = subframes_double.round().clamp(0.0, 99.0) as u8;
    Some(SmpteTime {
        hours,
        minutes,
        seconds,
        frames: capped_frames,
        subframes,
    })
}

fn validate_device_id(device_id: u8) -> Result<(), MmcError> {
    if device_id > 0x7F {
        Err(MmcError::DeviceIdOutOfRange)
    } else {
        Ok(())
    }
}

fn sys_ex(command: u8) -> Vec<u8> {
    vec![0xF0, 0x7F, DEVICE_ID, 0x06, command, 0xF7]
}

pub fn parse_locate_time(time: &str) -> Option<(u8, u8, u8, u8)> {
    let parts: Vec<&str> = time.split(':').collect();
    if parts.len() != 4 {
        return None;
    }
    let h = parts[0].parse().ok()?;
    let m = parts[1].parse().ok()?;
    let s = parts[2].parse().ok()?;
    let f = parts[3].parse().ok()?;
    Some((h, m, s, f))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_stop_record_strobe_bytes() {
        assert_eq!(play(), vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x02, 0xF7]);
        assert_eq!(stop(), vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x01, 0xF7]);
        assert_eq!(
            record_strobe(),
            vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x06, 0xF7]
        );
        assert_eq!(record_exit(), vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x07, 0xF7]);
        assert_eq!(pause(), vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x09, 0xF7]);
        assert_eq!(rewind(), vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x05, 0xF7]);
        assert_eq!(
            fast_forward(),
            vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x04, 0xF7]
        );
    }

    #[test]
    fn extended_mmc_commands() {
        assert_eq!(
            deferred_play(),
            vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x03, 0xF7]
        );
        assert_eq!(reset(), vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x0D, 0xF7]);
        assert_eq!(write(), vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x40, 0xF7]);
    }

    #[test]
    fn locate_byte_layout_unchanged() {
        assert_eq!(
            locate(1, 2, 3, 4, 5),
            vec![
                0xF0, 0x7F, DEVICE_ID, 0x06, 0x44, 0x06, 0x01, 1, 2, 3, 4, 5, 0xF7
            ]
        );
    }

    #[test]
    fn goto_target_bytes() {
        assert_eq!(
            goto_target(0x05).unwrap(),
            vec![0xF0, 0x7F, DEVICE_ID, 0x06, 0x44, 0x01, 0x05, 0xF7]
        );
        assert!(goto_target(0x80).is_err());
    }

    #[test]
    fn locate_strict_validates_ranges() {
        assert!(locate_strict(24, 0, 0, 0, 0, FrameRate::Fps30).is_err());
        assert!(locate_strict(0, 60, 0, 0, 0, FrameRate::Fps30).is_err());
        assert!(locate_strict(0, 0, 60, 0, 0, FrameRate::Fps30).is_err());
        assert!(locate_strict(0, 0, 0, 30, 0, FrameRate::Fps30).is_err());
        assert!(locate_strict(0, 0, 0, 0, 100, FrameRate::Fps30).is_err());
    }

    #[test]
    fn locate_strict_frame_rate_encoding() {
        let fps24 = locate_strict(1, 2, 3, 4, 0, FrameRate::Fps24).unwrap();
        assert_eq!(fps24[7] & 0b0110_0000, 0);
        assert_eq!(fps24[7] & 0b0001_1111, 1);

        let fps25 = locate_strict(2, 0, 0, 0, 0, FrameRate::Fps25).unwrap();
        assert_eq!(fps25[7] & 0b0110_0000, 0b0010_0000);

        let fps30 = locate_strict(4, 0, 0, 0, 0, FrameRate::Fps30).unwrap();
        assert_eq!(fps30[7] & 0b0110_0000, 0b0110_0000);
    }

    #[test]
    fn bar_beat_to_smpte_conversions() {
        let zero = bar_beat_to_smpte(1, 1.0, 120.0, 4, FrameRate::Fps30).unwrap();
        assert_eq!(zero.hours, 0);
        assert_eq!(zero.minutes, 0);
        assert_eq!(zero.seconds, 0);
        assert_eq!(zero.frames, 0);

        let two_bars = bar_beat_to_smpte(2, 1.0, 120.0, 4, FrameRate::Fps30).unwrap();
        assert_eq!(two_bars.seconds, 2);

        let two_min = bar_beat_to_smpte(61, 1.0, 120.0, 4, FrameRate::Fps30).unwrap();
        assert_eq!(two_min.minutes, 2);
        assert_eq!(two_min.seconds, 0);
    }

    #[test]
    fn bar_beat_to_smpte_rejects_invalid() {
        assert!(bar_beat_to_smpte(0, 1.0, 120.0, 4, FrameRate::Fps30).is_none());
        assert!(bar_beat_to_smpte(1, 0.5, 120.0, 4, FrameRate::Fps30).is_none());
        assert!(bar_beat_to_smpte(1, 1.0, 0.0, 4, FrameRate::Fps30).is_none());
        assert!(bar_beat_to_smpte(1, 1.0, 120.0, 0, FrameRate::Fps30).is_none());
        assert!(bar_beat_to_smpte(86_402, 1.0, 1.0, 1, FrameRate::Fps30).is_none());
    }

    #[test]
    fn parse_locate_time_valid_and_invalid() {
        assert_eq!(parse_locate_time("00:00:10:12"), Some((0, 0, 10, 12)));
        assert!(parse_locate_time("00:00:10").is_none());
    }
}
