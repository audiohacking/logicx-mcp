//! Native AX menu navigation — scoped to menu bar only (never walk the full app tree).

use super::core::*;
use crate::macos::cg_input;
use logicx_core::HonestResult;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread;
use std::time::Duration;

/// ⌥⌘N → New Tracks dialog → Return (Logic Pro). Avoids deep AX menu walks that crash Logic.
pub fn create_software_instrument_track() -> Option<HonestResult> {
    guard_ax(|| {
        let _ = crate::macos::run_osascript_output(r#"tell application "Logic Pro" to activate"#);
        thread::sleep(Duration::from_millis(250));
        cg_input::press_cmd_option_n();
        thread::sleep(Duration::from_millis(700));
        cg_input::press_return();
        thread::sleep(Duration::from_millis(500));
        Some(HonestResult {
            success: true,
            verified: Some(false),
            reason: Some("readback_unavailable".into()),
            error: None,
            detail: Some(serde_json::json!({
                "via": "keycmd_new_track",
                "note": "Option+Command+N then Return — new software instrument track",
            })),
        })
    })
}

/// File → Import → MIDI File — opens the file picker on the **current** project.
pub fn open_import_midi_file_dialog() -> Option<HonestResult> {
    guard_ax(|| {
        open_menu_items(&[
            &["File", "파일"],
            &["Import", "가져오기"],
            &["MIDI File…", "MIDI File...", "MIDI 파일…", "MIDI 파일..."],
        ])?;
        thread::sleep(Duration::from_millis(700));
        Some(HonestResult {
            success: true,
            verified: Some(false),
            reason: Some("readback_unavailable".into()),
            error: None,
            detail: Some(serde_json::json!({
                "via": "ax_menu",
                "note": "MIDI import file dialog opened on current project",
            })),
        })
    })
}

pub fn click_dialog_button(window_titles: &[&str], button_titles: &[&str]) -> Option<HonestResult> {
    guard_ax(|| unsafe {
        let app = logic_app()?;
        let window = find_in_windows(app.get(), 5, &|el| {
            ax_role(el).as_deref() == Some("AXWindow")
                && ax_title(el)
                    .map(|t| window_titles.iter().any(|w| t.contains(w)))
                    .unwrap_or(false)
        })?;
        let _wg = RetainedAx::new(window)?;
        let button = find_in_subtree(window, 6, &|el| {
            ax_role(el).as_deref() == Some("AXButton") && title_matches(el, button_titles)
        })?;
        let _bg = RetainedAx::new(button)?;
        perform_action(button, K_AX_PRESS);
        thread::sleep(Duration::from_millis(200));
        Some(HonestResult {
            success: true,
            verified: Some(false),
            reason: Some("readback_unavailable".into()),
            error: None,
            detail: Some(serde_json::json!({ "via": "ax_dialog_button" })),
        })
    })
}

fn open_menu_items(steps: &[&[&str]]) -> Option<()> {
    unsafe {
        let app = logic_app()?;
        for titles in steps {
            if !press_menu_item(app.get(), titles) {
                return None;
            }
            thread::sleep(Duration::from_millis(220));
        }
        Some(())
    }
}

/// Menu bar only — never search the full Logic UI tree (causes AX instability / crashes).
unsafe fn press_menu_item(app: AxRef, titles: &[&str]) -> bool {
    let Some(menubar) = find_in_subtree(app, 3, &|el| ax_role(el).as_deref() == Some(AX_MENU_BAR))
    else {
        return false;
    };
    let _menubar_guard = match RetainedAx::new(menubar) {
        Some(g) => g,
        None => return false,
    };

    if let Some(item) = find_in_subtree(menubar, 6, &|el| {
        ax_role(el).as_deref() == Some(AX_MENU_ITEM) && title_matches(el, titles)
    }) {
        let _item_guard = match RetainedAx::new(item) {
            Some(g) => g,
            None => return false,
        };
        return perform_action(item, K_AX_PRESS);
    }

    let Some(bar_item) = find_in_subtree(menubar, 2, &|el| {
        ax_role(el).as_deref() == Some(AX_MENU_BAR_ITEM) && title_matches(el, titles)
    }) else {
        return false;
    };
    let _bar_guard = match RetainedAx::new(bar_item) {
        Some(g) => g,
        None => return false,
    };
    perform_action(bar_item, K_AX_PRESS)
}

fn guard_ax<T, F>(f: F) -> Option<T>
where
    F: FnOnce() -> Option<T> + std::panic::UnwindSafe,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => {
            logicx_core::diagnostic_log::append_bridge_log("AX menu operation panicked");
            None
        }
    }
}
