//! Shared native Accessibility helpers (logic-pro-mcp AXLogicProElements parity).

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFGetTypeID, CFRelease, CFRetain, CFType, CFTypeID, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::geometry::{CGPoint, CGSize};
use std::ffi::c_void;
use std::process::Command;

pub type AxRef = *mut c_void;

pub const AX_SUCCESS: i32 = 0;
const AX_VALUE_CGPOINT: u32 = 1;
const AX_VALUE_CGSIZE: u32 = 2;

pub const K_AX_ROLE: &str = "AXRole";
pub const K_AX_TITLE: &str = "AXTitle";
pub const K_AX_DESCRIPTION: &str = "AXDescription";
pub const K_AX_CHILDREN: &str = "AXChildren";
pub const K_AX_POSITION: &str = "AXPosition";
pub const K_AX_SIZE: &str = "AXSize";
pub const K_AX_VALUE: &str = "AXValue";
pub const AX_SLIDER: &str = "AXSlider";
pub const AX_CHECKBOX: &str = "AXCheckBox";
pub const AX_MENU_BAR: &str = "AXMenuBar";
pub const AX_MENU_BAR_ITEM: &str = "AXMenuBarItem";
pub const AX_MENU: &str = "AXMenu";
pub const AX_MENU_ITEM: &str = "AXMenuItem";
pub const K_AX_INCREMENT: &str = "AXIncrement";
pub const K_AX_DECREMENT: &str = "AXDecrement";
pub const K_AX_PRESS: &str = "AXPress";

/// Owns one +1 retain on an AX element ref.
pub struct RetainedAx(AxRef);

impl RetainedAx {
    pub unsafe fn new(ptr: AxRef) -> Option<Self> {
        if ptr.is_null() {
            None
        } else {
            Some(Self(ptr))
        }
    }

    pub fn get(&self) -> AxRef {
        self.0
    }
}

impl Drop for RetainedAx {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                CFRelease(self.0 as CFTypeRef);
            }
        }
    }
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> AxRef;
    fn AXUIElementCopyAttributeValue(
        element: AxRef,
        attribute: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementPerformAction(element: AxRef, action: CFTypeRef) -> i32;
    fn AXUIElementSetAttributeValue(element: AxRef, attribute: CFTypeRef, value: CFTypeRef) -> i32;
    fn AXUIElementSetMessagingTimeout(element: AxRef, timeout: f32) -> i32;
    fn AXValueGetValue(value: CFTypeRef, value_type: u32, value_out: *mut c_void) -> bool;
}

pub fn logic_pid() -> Option<i32> {
    let output = Command::new("/usr/bin/pgrep")
        .args(["-x", "Logic Pro"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .and_then(|l| l.trim().parse().ok())
}

pub unsafe fn logic_app() -> Option<RetainedAx> {
    let pid = logic_pid()?;
    let app_raw = AXUIElementCreateApplication(pid);
    let app = RetainedAx::new(app_raw)?;
    AXUIElementSetMessagingTimeout(app.get(), 2.5);
    Some(app)
}

pub unsafe fn retain_match(root: AxRef, predicate: impl Fn(AxRef) -> bool) -> Option<AxRef> {
    if predicate(root) {
        CFRetain(root as CFTypeRef);
        Some(root)
    } else {
        None
    }
}

pub unsafe fn find_in_subtree(
    root: AxRef,
    max_depth: u32,
    predicate: &dyn Fn(AxRef) -> bool,
) -> Option<AxRef> {
    if max_depth == 0 {
        return None;
    }
    if let Some(found) = retain_match(root, predicate) {
        return Some(found);
    }
    let children = copy_attr_array(root, K_AX_CHILDREN)?;
    for i in 0..children.len() {
        let child = children.get(i)?.as_concrete_TypeRef() as AxRef;
        if let Some(found) = find_in_subtree(child, max_depth - 1, predicate) {
            return Some(found);
        }
    }
    None
}

pub unsafe fn find_in_windows(
    app: AxRef,
    max_depth: u32,
    predicate: &dyn Fn(AxRef) -> bool,
) -> Option<AxRef> {
    if let Some(windows) = copy_attr_array(app, "AXWindows") {
        for i in 0..windows.len() {
            if let Some(window) = windows.get(i) {
                let window_ref = window.as_concrete_TypeRef() as AxRef;
                if let Some(found) = find_in_subtree(window_ref, max_depth, predicate) {
                    return Some(found);
                }
            }
        }
    }
    find_in_subtree(app, max_depth + 2, predicate)
}

pub fn title_matches(element: AxRef, titles: &[&str]) -> bool {
    unsafe {
        ax_title(element)
            .map(|t| titles.iter().any(|name| titles_equal(&t, name)))
            .unwrap_or(false)
    }
}

pub fn titles_equal(actual: &str, expected: &str) -> bool {
    let a = normalize_menu_title(actual);
    let e = normalize_menu_title(expected);
    a == e || a.starts_with(&e) || e.starts_with(&a)
}

fn normalize_menu_title(s: &str) -> String {
    s.replace('…', "...")
        .trim()
        .trim_end_matches('.')
        .to_lowercase()
}

pub unsafe fn ax_role(element: AxRef) -> Option<String> {
    cf_string_attr(element, K_AX_ROLE)
}

pub unsafe fn ax_title(element: AxRef) -> Option<String> {
    cf_string_attr(element, K_AX_TITLE)
}

pub unsafe fn ax_description(element: AxRef) -> Option<String> {
    cf_string_attr(element, K_AX_DESCRIPTION)
}

pub unsafe fn cf_string_attr(element: AxRef, attr: &str) -> Option<String> {
    let value = copy_attr(element, attr)?;
    if cf_type_id(value) != CFString::type_id() {
        release(value);
        return None;
    }
    let cf: CFString = CFType::wrap_under_create_rule(value).downcast_into()?;
    Some(cf.to_string())
}

pub unsafe fn copy_attr(element: AxRef, attr: &str) -> Option<CFTypeRef> {
    let key = CFString::new(attr);
    let mut value: CFTypeRef = std::ptr::null_mut();
    if AXUIElementCopyAttributeValue(
        element,
        key.as_concrete_TypeRef() as CFTypeRef,
        &mut value,
    ) != AX_SUCCESS
    {
        return None;
    }
    if value.is_null() {
        None
    } else {
        Some(value)
    }
}

pub unsafe fn copy_attr_array(element: AxRef, attr: &str) -> Option<CFArray<CFType>> {
    let value = copy_attr(element, attr)?;
    if cf_type_id(value) != CFArray::<CFType>::type_id() {
        release(value);
        return None;
    }
    let array_ref = value as CFArrayRef;
    Some(CFArray::wrap_under_create_rule(array_ref))
}

pub unsafe fn ax_position_size(element: AxRef) -> Option<(CGPoint, CGSize)> {
    let pos_ref = copy_attr(element, K_AX_POSITION)?;
    let size_ref = copy_attr(element, K_AX_SIZE)?;
    let mut point = CGPoint::new(0.0, 0.0);
    let mut size = CGSize::new(0.0, 0.0);
    if !AXValueGetValue(pos_ref, AX_VALUE_CGPOINT, &mut point as *mut _ as *mut c_void) {
        CFRelease(pos_ref);
        CFRelease(size_ref);
        return None;
    }
    if !AXValueGetValue(size_ref, AX_VALUE_CGSIZE, &mut size as *mut _ as *mut c_void) {
        CFRelease(pos_ref);
        CFRelease(size_ref);
        return None;
    }
    CFRelease(pos_ref);
    CFRelease(size_ref);
    Some((point, size))
}

pub unsafe fn ax_value_f64(element: AxRef) -> Option<f64> {
    let value = copy_attr(element, K_AX_VALUE)?;
    if cf_type_id(value) == CFNumber::type_id() {
        let num: CFNumber = CFType::wrap_under_create_rule(value).downcast_into()?;
        num.to_f64()
    } else if cf_type_id(value) == CFString::type_id() {
        let s: CFString = CFType::wrap_under_create_rule(value).downcast_into()?;
        s.to_string().parse().ok()
    } else if cf_type_id(value) == CFBoolean::type_id() {
        let b: CFBoolean = CFType::wrap_under_create_rule(value).downcast_into()?;
        Some(if b == CFBoolean::true_value() { 1.0 } else { 0.0 })
    } else {
        release(value);
        None
    }
}

pub unsafe fn set_ax_value_f64(element: AxRef, value: f64) -> bool {
    let num = CFNumber::from(value as f64);
    let key = CFString::new(K_AX_VALUE);
    let ok = AXUIElementSetAttributeValue(
        element,
        key.as_concrete_TypeRef() as CFTypeRef,
        num.as_concrete_TypeRef() as CFTypeRef,
    ) == AX_SUCCESS;
    std::mem::forget(num);
    ok
}

pub unsafe fn perform_action(element: AxRef, action: &str) -> bool {
    let key = CFString::new(action);
    AXUIElementPerformAction(element, key.as_concrete_TypeRef() as CFTypeRef) == AX_SUCCESS
}

pub unsafe fn cf_type_id(value: CFTypeRef) -> CFTypeID {
    CFGetTypeID(value)
}

pub unsafe fn release(value: CFTypeRef) {
    if !value.is_null() {
        CFRelease(value);
    }
}

pub fn tempo_description(desc: &str) -> bool {
    let d = desc.to_lowercase();
    d == "tempo" || d == "bpm" || d == "템포" || d.contains("tempo")
}

pub fn bar_slider_description(desc: &str) -> bool {
    let d = desc.to_lowercase();
    (d.contains("bar") || d.contains("마디")) && !d.contains("tempo") && !d.contains("bpm")
}

/// Scan for modal AX dialogs occluding the arrange surface.
pub fn dialog_present() -> bool {
    unsafe {
        let Some(app) = logic_app() else {
            return false;
        };
        let Some(windows) = copy_attr_array(app.get(), "AXWindows") else {
            return false;
        };
        for i in 0..windows.len() {
            let Some(window) = windows.get(i) else {
                continue;
            };
            let w = window.as_concrete_TypeRef() as AxRef;
            if let Some(role) = ax_role(w) {
                let r = role.to_lowercase();
                if r.contains("dialog") || r.contains("sheet") {
                    return true;
                }
            }
        }
    }
    false
}
