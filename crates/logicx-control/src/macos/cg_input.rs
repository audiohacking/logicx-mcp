//! CGEvent mouse/keyboard at screen coordinates (logic-pro-mcp AXMouseHelper parity).
//! Uses HID event tap — required for Logic tempo slider double-click + type entry.

use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGKeyCode, CGMouseButton,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use std::ffi::c_void;
use std::thread;
use std::time::Duration;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn CGEventKeyboardSetUnicodeString(
        event: *mut c_void,
        string_length: u64,
        unicode_string: *const u16,
    );
    fn CGEventCreateKeyboardEvent(
        source: *mut c_void,
        virtual_key: u16,
        key_down: bool,
    ) -> *mut c_void;
    fn CGEventPost(tap: u32, event: *mut c_void);
    fn CGEventSourceCreate(state_id: i32) -> *mut c_void;
}

pub fn single_click(at: CGPoint) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    post_mouse(CGEventType::LeftMouseDown, at, 1, &source);
    post_mouse(CGEventType::LeftMouseUp, at, 1, &source);
}

pub fn double_click(at: CGPoint) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    post_mouse(CGEventType::LeftMouseDown, at, 1, &source);
    post_mouse(CGEventType::LeftMouseUp, at, 1, &source);
    thread::sleep(Duration::from_millis(40));
    post_mouse(CGEventType::LeftMouseDown, at, 2, &source);
    post_mouse(CGEventType::LeftMouseUp, at, 2, &source);
}

pub fn type_numeric_string(s: &str) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    for ch in s.chars() {
        if let Some(code) = numeric_key_code(ch) {
            post_key(code, &source);
            thread::sleep(Duration::from_millis(15));
        }
    }
}

pub fn type_string(s: &str) {
    let utf16: Vec<u16> = s.encode_utf16().collect();
    if utf16.is_empty() {
        return;
    }
    unsafe {
        // kCGEventSourceStateCombinedSessionState
        let source = CGEventSourceCreate(0);
        if source.is_null() {
            return;
        }
        let down = CGEventCreateKeyboardEvent(source, 0, true);
        if down.is_null() {
            core_foundation::base::CFRelease(source as _);
            return;
        }
        CGEventKeyboardSetUnicodeString(down, utf16.len() as u64, utf16.as_ptr());
        CGEventPost(CGEventTapLocation::HID as u32, down);
        let up = CGEventCreateKeyboardEvent(source, 0, false);
        if !up.is_null() {
            CGEventPost(CGEventTapLocation::HID as u32, up);
        }
        core_foundation::base::CFRelease(source as _);
    }
    thread::sleep(Duration::from_millis(20));
}

pub fn press_return() {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    post_key(0x24, &source);
}

pub fn press_tab() {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    post_key(0x30, &source);
}

pub fn press_cmd_option_n() {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    let flags = CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagAlternate;
    if let (Ok(down), Ok(up)) = (
        CGEvent::new_keyboard_event(source.clone(), 0x2D, true),
        CGEvent::new_keyboard_event(source.clone(), 0x2D, false),
    ) {
        down.set_flags(flags);
        up.set_flags(flags);
        down.post(CGEventTapLocation::HID);
        up.post(CGEventTapLocation::HID);
    }
}

pub fn press_cmd_shift_g() {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    let flags = CGEventFlags::CGEventFlagCommand | CGEventFlags::CGEventFlagShift;
    if let (Ok(down), Ok(up)) = (
        CGEvent::new_keyboard_event(source.clone(), 0x05, true),
        CGEvent::new_keyboard_event(source.clone(), 0x05, false),
    ) {
        down.set_flags(flags);
        up.set_flags(flags);
        down.post(CGEventTapLocation::HID);
        up.post(CGEventTapLocation::HID);
    }
}

pub fn press_cmd_a() {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    let cmd = CGEventFlags::CGEventFlagCommand;
    if let (Ok(down), Ok(up)) = (
        CGEvent::new_keyboard_event(source.clone(), 0x00, true),
        CGEvent::new_keyboard_event(source.clone(), 0x00, false),
    ) {
        down.set_flags(cmd);
        up.set_flags(cmd);
        down.post(CGEventTapLocation::HID);
        up.post(CGEventTapLocation::HID);
    }
}

pub fn press_escape() {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else {
        return;
    };
    post_key(0x35, &source);
}

fn post_mouse(event_type: CGEventType, at: CGPoint, click_count: i64, source: &CGEventSource) {
    if let Ok(event) = CGEvent::new_mouse_event(source.clone(), event_type, at, CGMouseButton::Left)
    {
        event.set_integer_value_field(
            core_graphics::event::EventField::MOUSE_EVENT_CLICK_STATE,
            click_count,
        );
        event.post(CGEventTapLocation::HID);
    }
}

fn post_key(key_code: CGKeyCode, source: &CGEventSource) {
    if let (Ok(down), Ok(up)) = (
        CGEvent::new_keyboard_event(source.clone(), key_code, true),
        CGEvent::new_keyboard_event(source.clone(), key_code, false),
    ) {
        down.post(CGEventTapLocation::HID);
        up.post(CGEventTapLocation::HID);
    }
}

fn numeric_key_code(ch: char) -> Option<CGKeyCode> {
    Some(match ch {
        '0' => 0x1D,
        '1' => 0x12,
        '2' => 0x13,
        '3' => 0x14,
        '4' => 0x15,
        '5' => 0x17,
        '6' => 0x16,
        '7' => 0x1A,
        '8' => 0x1C,
        '9' => 0x19,
        '.' => 0x2F,
        '-' => 0x1B,
        _ => return None,
    })
}
