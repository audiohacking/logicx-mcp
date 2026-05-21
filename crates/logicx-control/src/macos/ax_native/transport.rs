//! Control-bar transport: bar position, cycle range, play/stop.

use super::core::*;
use crate::macos::cg_input;
use core_foundation::base::CFTypeRef;
use core_graphics::geometry::CGPoint;
use logicx_core::HonestResult;
use std::thread;
use std::time::Duration;

pub fn goto_bar(bar: u32) -> Option<HonestResult> {
    unsafe {
        let app = logic_app()?;
        let slider = find_bar_slider(app.get())?;
        let _guard = RetainedAx::new(slider)?;

        if set_ax_value_f64(slider, bar as f64) {
            thread::sleep(Duration::from_millis(150));
            if let Some(observed) = ax_value_f64(slider)
                && (observed - bar as f64).abs() <= 0.5
            {
                return Some(HonestResult {
                    success: true,
                    verified: Some(true),
                    reason: None,
                    error: None,
                    detail: Some(serde_json::json!({
                        "requested": bar,
                        "observed": observed,
                        "via": "ax_bar_slider",
                    })),
                });
            }
        }

        if let Some((pos, size)) = ax_position_size(slider) {
            let center = CGPoint::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
            cg_input::double_click(center);
            thread::sleep(Duration::from_millis(120));
            cg_input::type_numeric_string(&bar.to_string());
            thread::sleep(Duration::from_millis(50));
            cg_input::press_return();
            thread::sleep(Duration::from_millis(150));
        }

        if let Some(slider) = find_bar_slider(app.get()) {
            let observed = ax_value_f64(slider);
            release(slider as CFTypeRef);
            if let Some(observed) = observed {
                return Some(HonestResult {
                    success: true,
                    verified: Some((observed - bar as f64).abs() <= 0.5),
                    reason: if (observed - bar as f64).abs() <= 0.5 {
                        None
                    } else {
                        Some("readback_mismatch".into())
                    },
                    error: None,
                    detail: Some(serde_json::json!({
                        "requested": bar,
                        "observed": observed,
                        "via": "ax_bar_slider",
                    })),
                });
            }
        }

        Some(HonestResult {
            success: true,
            verified: Some(false),
            reason: Some("readback_unavailable".into()),
            error: None,
            detail: Some(serde_json::json!({
                "requested": bar,
                "via": "ax_bar_slider",
            })),
        })
    }
}

pub fn set_cycle_range(start: u32, end: u32) -> Option<HonestResult> {
    unsafe {
        let app = logic_app()?;
        let cycle = find_checkbox(app.get(), &["Cycle", "사이클"])?;
        let _guard = RetainedAx::new(cycle)?;

        let enabled = ax_value_f64(cycle).map(|v| v >= 0.5).unwrap_or(false);
        if !enabled {
            click_element(cycle);
            thread::sleep(Duration::from_millis(150));
        }

        thread::sleep(Duration::from_millis(100));
        cg_input::type_numeric_string(&start.to_string());
        cg_input::press_tab();
        cg_input::type_numeric_string(&end.to_string());
        cg_input::press_return();
        thread::sleep(Duration::from_millis(150));

        Some(HonestResult {
            success: true,
            verified: Some(false),
            reason: Some("readback_unavailable".into()),
            error: None,
            detail: Some(serde_json::json!({
                "start": start,
                "end": end,
                "via": "ax_cycle",
            })),
        })
    }
}

pub fn transport_play() -> Option<HonestResult> {
    toggle_transport_checkbox(&["Play", "재생"], true, "ax_play")
}

pub fn transport_stop() -> Option<HonestResult> {
    toggle_transport_checkbox(&["Play", "재생"], false, "ax_stop")
}

/// Read transport LEDs and control-bar sliders for supplementary polling.
pub fn read_transport_state() -> Option<HonestResult> {
    unsafe {
        let app = logic_app()?;
        let is_playing = find_checkbox(app.get(), &["Play", "재생"])
            .map(|cb| {
                let on = ax_value_f64(cb).map(|v| v >= 0.5).unwrap_or(false);
                release(cb as CFTypeRef);
                on
            })
            .unwrap_or(false);
        let is_recording = find_checkbox(app.get(), &["Record", "녹음"])
            .map(|cb| {
                let on = ax_value_f64(cb).map(|v| v >= 0.5).unwrap_or(false);
                release(cb as CFTypeRef);
                on
            })
            .unwrap_or(false);
        let position = find_bar_slider(app.get())
            .and_then(|slider| {
                let bar = ax_value_f64(slider).map(|v| format!("{}.1.1.1", v.max(1.0) as u32));
                release(slider as CFTypeRef);
                bar
            })
            .unwrap_or_else(|| "1.1.1.1".into());
        let tempo = super::tempo::read_tempo_value(app.get()).unwrap_or(120.0);
        Some(HonestResult {
            success: true,
            verified: Some(true),
            reason: None,
            error: None,
            detail: Some(serde_json::json!({
                "isPlaying": is_playing,
                "isRecording": is_recording,
                "tempo": tempo,
                "position": position,
            })),
        })
    }
}

pub fn toggle_checkbox(titles: &[&str], want_on: Option<bool>, via: &str) -> Option<HonestResult> {
    unsafe {
        let app = logic_app()?;
        let checkbox = find_checkbox(app.get(), titles)?;
        let _guard = RetainedAx::new(checkbox)?;
        let on = ax_value_f64(checkbox).map(|v| v >= 0.5).unwrap_or(false);
        if want_on.map(|w| w != on).unwrap_or(true) {
            click_element(checkbox);
            thread::sleep(Duration::from_millis(120));
        }
        Some(HonestResult {
            success: true,
            verified: Some(false),
            reason: Some("readback_unavailable".into()),
            error: None,
            detail: Some(serde_json::json!({ "via": via, "wanted_on": want_on })),
        })
    }
}

fn toggle_transport_checkbox(titles: &[&str], want_on: bool, via: &str) -> Option<HonestResult> {
    toggle_checkbox(titles, Some(want_on), via)
}

unsafe fn find_bar_slider(app: AxRef) -> Option<AxRef> {
    find_in_windows(app, 8, &|el| {
        ax_role(el).as_deref() == Some(AX_SLIDER)
            && ax_description(el)
                .map(|d| bar_slider_description(&d))
                .unwrap_or(false)
    })
}

unsafe fn find_checkbox(app: AxRef, titles: &[&str]) -> Option<AxRef> {
    find_in_windows(app, 10, &|el| {
        ax_role(el).as_deref() == Some(AX_CHECKBOX) && title_matches(el, titles)
    })
}

unsafe fn click_element(element: AxRef) {
    if perform_action(element, K_AX_PRESS) {
        return;
    }
    if let Some((pos, size)) = ax_position_size(element) {
        let center = CGPoint::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        cg_input::single_click(center);
    }
}
