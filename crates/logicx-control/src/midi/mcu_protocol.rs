//! Mackie Control Universal protocol encoder (logic-pro-mcp compatible).

pub const PORT_NAME: &str = "LogicProMCP-MCU-Internal";
/// Logic Pro Control Surface **Output** (Logic → us). Input stays [`PORT_NAME`].
pub const FEEDBACK_PORT_NAME: &str = "LogicProMCP-MCU-Feedback";

#[derive(Clone, Copy)]
pub enum TransportCommand {
    Play,
    Stop,
    Record,
    Rewind,
    FastForward,
    Cycle,
    Click,
}

#[derive(Clone, Copy)]
pub enum ButtonFunction {
    RecArm = 0x00,
    Solo = 0x08,
    Mute = 0x10,
    Select = 0x18,
    AutomationRead = 0x4A,
    AutomationWrite = 0x4B,
    AutomationTrim = 0x4C,
    AutomationTouch = 0x4D,
    AutomationLatch = 0x4E,
    Cycle = 0x56,
    Click = 0x59,
    Rewind = 0x5B,
    FastForward = 0x5C,
    Stop = 0x5D,
    Play = 0x5E,
    Record = 0x5F,
}

impl ButtonFunction {
    fn is_strip_relative(self) -> bool {
        matches!(
            self,
            ButtonFunction::RecArm
                | ButtonFunction::Solo
                | ButtonFunction::Mute
                | ButtonFunction::Select
        )
    }
}

pub fn encode_fader(track: u8, value: f64) -> [u8; 3] {
    let channel = track.min(8);
    let clamped = value.clamp(0.0, 1.0);
    let int_value = (clamped * 16383.0) as u16;
    let lsb = (int_value & 0x7F) as u8;
    let msb = ((int_value >> 7) & 0x7F) as u8;
    [0xE0 | channel, lsb, msb]
}

pub fn encode_pan(track: u8, value: f64) -> [u8; 3] {
    encode_vpot(track, value)
}

fn encode_vpot(strip: u8, value: f64) -> [u8; 3] {
    let cc = 0x10 + strip.min(7);
    let clamped = value.clamp(0.0, 1.0);
    let speed = ((clamped * 15.0).round() as u8).clamp(1, 15);
    [0xB0, cc, speed]
}

fn encode_button(function: ButtonFunction, strip: u8, on: bool) -> [u8; 3] {
    let note = match function {
        ButtonFunction::RecArm => strip.min(7),
        ButtonFunction::Solo => 0x08 + strip.min(7),
        ButtonFunction::Mute => 0x10 + strip.min(7),
        ButtonFunction::Select => 0x18 + strip.min(7),
        ButtonFunction::AutomationRead => 0x4A,
        ButtonFunction::AutomationWrite => 0x4B,
        ButtonFunction::AutomationTrim => 0x4C,
        ButtonFunction::AutomationTouch => 0x4D,
        ButtonFunction::AutomationLatch => 0x4E,
        ButtonFunction::Cycle => 0x56,
        ButtonFunction::Click => 0x59,
        ButtonFunction::Rewind => 0x5B,
        ButtonFunction::FastForward => 0x5C,
        ButtonFunction::Stop => 0x5D,
        ButtonFunction::Play => 0x5E,
        ButtonFunction::Record => 0x5F,
    };
    [0x90, note, if on { 0x7F } else { 0x00 }]
}

pub fn encode_transport(cmd: TransportCommand) -> [u8; 3] {
    let function = match cmd {
        TransportCommand::Play => ButtonFunction::Play,
        TransportCommand::Stop => ButtonFunction::Stop,
        TransportCommand::Record => ButtonFunction::Record,
        TransportCommand::Rewind => ButtonFunction::Rewind,
        TransportCommand::FastForward => ButtonFunction::FastForward,
        TransportCommand::Cycle => ButtonFunction::Cycle,
        TransportCommand::Click => ButtonFunction::Click,
    };
    encode_button(function, 0, true)
}

pub fn encode_mute(strip: u8, on: bool) -> [u8; 3] {
    encode_button(ButtonFunction::Mute, strip, on)
}

pub fn encode_solo(strip: u8, on: bool) -> [u8; 3] {
    encode_button(ButtonFunction::Solo, strip, on)
}

pub fn encode_arm(strip: u8, on: bool) -> [u8; 3] {
    encode_button(ButtonFunction::RecArm, strip, on)
}

pub fn encode_select(strip: u8) -> [u8; 3] {
    encode_button(ButtonFunction::Select, strip, true)
}

pub fn encode_automation(mode: &str) -> Result<[u8; 3], &'static str> {
    let function = match mode {
        "read" => ButtonFunction::AutomationRead,
        "write" => ButtonFunction::AutomationWrite,
        "touch" => ButtonFunction::AutomationTouch,
        "latch" => ButtonFunction::AutomationLatch,
        "trim" => ButtonFunction::AutomationTrim,
        "off" => return Ok(encode_button(ButtonFunction::AutomationRead, 0, false)),
        _ => return Err("unknown automation mode"),
    };
    Ok(encode_button(function, 0, true))
}

pub fn encode_device_query() -> Vec<u8> {
    vec![0xF0, 0x00, 0x00, 0x66, 0x14, 0x00, 0xF7]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VPotDirection {
    Clockwise,
    CounterClockwise,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum JogDirection {
    Clockwise,
    CounterClockwise,
}

pub fn encode_vpot_direction(strip: u8, direction: VPotDirection, speed: u8) -> [u8; 3] {
    let cc = 0x10 + strip.min(7);
    let speed = speed.clamp(1, 15);
    let value = match direction {
        VPotDirection::Clockwise => speed,
        VPotDirection::CounterClockwise => speed | 0x40,
    };
    [0xB0, cc, value]
}

pub fn encode_bank_right() -> [u8; 3] {
    [0x90, 0x2F, 0x7F]
}

pub fn encode_jog(direction: JogDirection, clicks: u8) -> [u8; 3] {
    let clicks = clicks.min(127);
    let value = match direction {
        JogDirection::Clockwise => clicks,
        JogDirection::CounterClockwise => clicks | 0x40,
    };
    [0xB0, 0x3C, value]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LcdRow {
    Upper,
    Lower,
}

pub struct LcdDecode {
    pub offset: u8,
    pub text: String,
    pub row: LcdRow,
}

pub fn decode_lcd_sysex(sysex: &[u8]) -> Option<LcdDecode> {
    if sysex.len() < 8 || sysex.first() != Some(&0xF0) || sysex.last() != Some(&0xF7) {
        return None;
    }
    if sysex.get(1..6) != Some(&[0x00, 0x00, 0x66, 0x14, 0x12]) {
        return None;
    }
    let offset = sysex[6];
    let row = if offset >= 0x38 {
        LcdRow::Lower
    } else {
        LcdRow::Upper
    };
    let text = String::from_utf8_lossy(&sysex[7..sysex.len() - 1]).into_owned();
    Some(LcdDecode { offset, text, row })
}

#[derive(Debug, PartialEq, Eq)]
pub enum DeviceResponse {
    Success(Vec<u8>),
    NoResponse,
    Failure(String),
}

pub fn parse_device_response(response: &[u8]) -> DeviceResponse {
    if response.is_empty() {
        return DeviceResponse::NoResponse;
    }
    if response.len() < 7
        || response.first() != Some(&0xF0)
        || response.get(1..5) != Some(&[0x00, 0x00, 0x66, 0x14])
    {
        return DeviceResponse::Failure("malformed sysex".into());
    }
    if response[5] != 0x01 {
        return DeviceResponse::Failure("unexpected sub-ID".into());
    }
    if response.last() != Some(&0xF7) {
        return DeviceResponse::Failure("missing EOX".into());
    }
    DeviceResponse::Success(response[6..response.len() - 1].to_vec())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VPotLedRing {
    pub strip: u8,
    pub position: u8,
    pub center: bool,
}

pub fn decode_vpot_led_ring(cc: u8, value: u8) -> Option<VPotLedRing> {
    if !(0x30..=0x37).contains(&cc) {
        return None;
    }
    Some(VPotLedRing {
        strip: cc - 0x30,
        position: value & 0x0F,
        center: value & 0x40 != 0,
    })
}

pub fn vpot_position_to_pan(position: u8) -> f64 {
    ((i16::from(position) - 6) as f64 / 5.0).clamp(-1.0, 1.0)
}

pub fn pan_to_vpot_position(pan: f64) -> u8 {
    if pan <= -1.0 {
        return 0;
    }
    if pan >= 1.0 {
        return 11;
    }
    ((pan * 5.0 + 6.0).round() as i32).clamp(0, 11) as u8
}

pub fn decode_fader(bytes: [u8; 3]) -> Option<(u8, f64)> {
    if bytes[0] & 0xF0 != 0xE0 {
        return None;
    }
    let track = bytes[0] & 0x0F;
    let value = (u16::from(bytes[2]) << 7) | u16::from(bytes[1]);
    Some((track, f64::from(value) / 16383.0))
}

pub fn decode_button(bytes: [u8; 3]) -> Option<(ButtonFunction, u8, bool)> {
    if bytes[0] != 0x90 {
        return None;
    }
    let (function, strip) = decode_button_note(bytes[1])?;
    Some((function, strip, bytes[2] >= 0x40))
}

pub fn is_valid_sysex(data: &[u8]) -> bool {
    if data.len() < 2 || data.first() != Some(&0xF0) || data.last() != Some(&0xF7) {
        return false;
    }
    data[1..data.len() - 1].iter().all(|b| *b <= 0x7F)
}

pub fn decode_button_note(note: u8) -> Option<(ButtonFunction, u8)> {
    if note <= 0x07 {
        return Some((ButtonFunction::RecArm, note));
    }
    if (0x08..=0x0F).contains(&note) {
        return Some((ButtonFunction::Solo, note - 0x08));
    }
    if (0x10..=0x17).contains(&note) {
        return Some((ButtonFunction::Mute, note - 0x10));
    }
    if (0x18..=0x1F).contains(&note) {
        return Some((ButtonFunction::Select, note - 0x18));
    }
    match note {
        0x4A => Some((ButtonFunction::AutomationRead, 0)),
        0x4B => Some((ButtonFunction::AutomationWrite, 0)),
        0x4C => Some((ButtonFunction::AutomationTrim, 0)),
        0x4D => Some((ButtonFunction::AutomationTouch, 0)),
        0x4E => Some((ButtonFunction::AutomationLatch, 0)),
        0x56 => Some((ButtonFunction::Cycle, 0)),
        0x59 => Some((ButtonFunction::Click, 0)),
        0x5B => Some((ButtonFunction::Rewind, 0)),
        0x5C => Some((ButtonFunction::FastForward, 0)),
        0x5D => Some((ButtonFunction::Stop, 0)),
        0x5E => Some((ButtonFunction::Play, 0)),
        0x5F => Some((ButtonFunction::Record, 0)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_fader_position() {
        let bytes = encode_fader(0, 0.5);
        assert_eq!(bytes[0], 0xE0);
        let value = (u16::from(bytes[2]) << 7) | u16::from(bytes[1]);
        assert!((i32::from(value) - 8192).abs() <= 1);
    }

    #[test]
    fn encode_fader_max() {
        let bytes = encode_fader(7, 1.0);
        assert_eq!(bytes[0], 0xE7);
        let value = (u16::from(bytes[2]) << 7) | u16::from(bytes[1]);
        assert_eq!(value, 0x3FFF);
    }

    #[test]
    fn decode_fader_feedback() {
        let bytes = [0xE3, 0x00, 0x20];
        let (track, value) = decode_fader(bytes).unwrap();
        assert_eq!(track, 3);
        assert!((value - 0.25).abs() < 0.02);
    }

    #[test]
    fn encode_mute_button() {
        assert_eq!(encode_mute(2, true), [0x90, 0x12, 0x7F]);
    }

    #[test]
    fn decode_solo_led() {
        let (func, strip, on) = decode_button([0x90, 0x0A, 0x7F]).unwrap();
        assert_eq!(func as u8, ButtonFunction::Solo as u8);
        assert_eq!(strip, 2);
        assert!(on);
    }

    #[test]
    fn encode_transport_play_stop() {
        assert_eq!(encode_transport(TransportCommand::Play), [0x90, 0x5E, 0x7F]);
        assert_eq!(encode_transport(TransportCommand::Stop), [0x90, 0x5D, 0x7F]);
    }

    #[test]
    fn encode_device_query() {
        assert_eq!(
            super::encode_device_query(),
            vec![0xF0, 0x00, 0x00, 0x66, 0x14, 0x00, 0xF7]
        );
    }

    #[test]
    fn sysex_validation() {
        assert!(is_valid_sysex(&[0xF0, 0x00, 0x01, 0x7F, 0xF7]));
        assert!(!is_valid_sysex(&[0xF0, 0x00, 0x80, 0x01, 0xF7]));
        assert!(!is_valid_sysex(&[0x00, 0x01, 0xF7]));
        assert!(!is_valid_sysex(&[0xF0, 0x00, 0x01]));
    }

    #[test]
    fn encode_vpot_cw_ccw() {
        assert_eq!(
            encode_vpot_direction(0, VPotDirection::Clockwise, 3),
            [0xB0, 0x10, 0x03]
        );
        assert_eq!(
            encode_vpot_direction(0, VPotDirection::CounterClockwise, 3),
            [0xB0, 0x10, 0x43]
        );
    }

    #[test]
    fn encode_bank_right() {
        assert_eq!(super::encode_bank_right(), [0x90, 0x2F, 0x7F]);
    }

    #[test]
    fn encode_jog_cw() {
        assert_eq!(encode_jog(JogDirection::Clockwise, 1), [0xB0, 0x3C, 0x01]);
    }

    #[test]
    fn decode_lcd_sysex_upper_and_lower() {
        let upper = decode_lcd_sysex(&[
            0xF0, 0x00, 0x00, 0x66, 0x14, 0x12, 0x00, 0x48, 0x65, 0x6C, 0x6C, 0x6F, 0xF7,
        ])
        .unwrap();
        assert_eq!(upper.offset, 0);
        assert_eq!(upper.text, "Hello");
        assert_eq!(upper.row, LcdRow::Upper);

        let lower = decode_lcd_sysex(&[
            0xF0, 0x00, 0x00, 0x66, 0x14, 0x12, 0x38, 0x54, 0x65, 0x73, 0x74, 0xF7,
        ])
        .unwrap();
        assert_eq!(lower.row, LcdRow::Lower);
        assert_eq!(lower.text, "Test");
    }

    #[test]
    fn parse_device_response_variants() {
        let ok =
            parse_device_response(&[0xF0, 0x00, 0x00, 0x66, 0x14, 0x01, 0x42, 0x00, 0x01, 0xF7]);
        assert_eq!(ok, DeviceResponse::Success(vec![0x42, 0x00, 0x01]));

        assert_eq!(parse_device_response(&[]), DeviceResponse::NoResponse);

        let bad = parse_device_response(&[0xF0, 0x00, 0x00, 0xF7]);
        assert!(matches!(bad, DeviceResponse::Failure(_)));

        let wrong_sub = parse_device_response(&[0xF0, 0x00, 0x00, 0x66, 0x14, 0x02, 0xF7]);
        assert!(matches!(wrong_sub, DeviceResponse::Failure(_)));
    }

    #[test]
    fn decode_vpot_led_ring_and_pan_mapping() {
        let center = decode_vpot_led_ring(0x30, 0x06).unwrap();
        assert_eq!(center.strip, 0);
        assert_eq!(center.position, 6);
        assert!((vpot_position_to_pan(6) - 0.0).abs() < f64::EPSILON);
        assert!((vpot_position_to_pan(0) - (-1.0)).abs() < f64::EPSILON);
        assert!((vpot_position_to_pan(11) - 1.0).abs() < f64::EPSILON);
        assert_eq!(pan_to_vpot_position(0.0), 6);
        assert_eq!(pan_to_vpot_position(-1.0), 0);
        assert_eq!(pan_to_vpot_position(1.0), 11);
        for pos in 2..=9 {
            assert_eq!(pan_to_vpot_position(vpot_position_to_pan(pos)), pos);
        }
        assert!(decode_vpot_led_ring(0x2F, 0x06).is_none());
        assert!(decode_vpot_led_ring(0x38, 0x06).is_none());
    }
}
