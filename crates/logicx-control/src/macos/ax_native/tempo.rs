//! Control-bar tempo slider via native AX + HID typing.

use super::core::*;
use crate::macos::cg_input;
use core_foundation::base::CFTypeRef;
use core_graphics::geometry::CGPoint;
use logicx_core::HonestResult;
use std::thread;
use std::time::Duration;

pub fn set_tempo(tempo: f64) -> Option<HonestResult> {
    let tempo_str = if (tempo - tempo.round()).abs() < f64::EPSILON {
        format!("{}", tempo.round() as u32)
    } else {
        format!("{tempo:.2}")
    };

    unsafe {
        let app = logic_app()?;
        let slider = find_tempo_slider(app.get())?;
        let _slider_guard = RetainedAx::new(slider)?;
        type_into_slider(slider, &tempo_str);
        thread::sleep(Duration::from_millis(150));

        if let Some(observed) = read_tempo_value(app.get())
            && (observed - tempo).abs() <= 1.0
        {
            return Some(HonestResult {
                success: true,
                verified: Some(true),
                reason: None,
                error: None,
                detail: Some(serde_json::json!({
                    "requested": tempo,
                    "observed": observed,
                    "via": "ax_slider",
                })),
            });
        }

        cg_input::press_escape();
        thread::sleep(Duration::from_millis(50));

        if let Some(current) = read_tempo_value(app.get()) {
            let delta = tempo - current;
            let steps = (delta.abs() / 10.0).round() as i32;
            if steps > 0 {
                let action = if delta > 0.0 {
                    K_AX_INCREMENT
                } else {
                    K_AX_DECREMENT
                };
                if let Some(slider) = find_tempo_slider(app.get()) {
                    let _g = RetainedAx::new(slider);
                    for _ in 0..steps {
                        let _ = perform_action(slider, action);
                    }
                }
            }
            if let Some(after) = read_tempo_value(app.get()) {
                return Some(HonestResult {
                    success: true,
                    verified: Some((after - tempo).abs() <= 1.0),
                    reason: Some("readback_mismatch".into()),
                    error: None,
                    detail: Some(serde_json::json!({
                        "requested": tempo,
                        "observed": after,
                        "via": "ax_slider_increment",
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
                "requested": tempo,
                "via": "ax_slider",
                "note": "tempo entry sent; could not read back slider value",
            })),
        })
    }
}

unsafe fn find_tempo_slider(app: AxRef) -> Option<AxRef> {
    find_in_windows(app, 8, &|el| {
        ax_role(el).as_deref() == Some(AX_SLIDER)
            && ax_description(el)
                .map(|d| tempo_description(&d))
                .unwrap_or(false)
    })
}

pub(crate) fn read_tempo_value(app: AxRef) -> Option<f64> {
    unsafe {
        find_tempo_slider(app).and_then(|slider| {
            let v = ax_value_f64(slider);
            release(slider as CFTypeRef);
            v
        })
    }
}

pub(super) unsafe fn type_into_slider(slider: AxRef, text: &str) {
    if let Some((pos, size)) = ax_position_size(slider) {
        let center = CGPoint::new(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
        cg_input::double_click(center);
        thread::sleep(Duration::from_millis(120));
        cg_input::type_numeric_string(text);
        thread::sleep(Duration::from_millis(50));
        cg_input::press_return();
    }
}
