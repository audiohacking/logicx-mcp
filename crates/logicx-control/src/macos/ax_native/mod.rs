//! Native Accessibility API control (logic-pro-mcp AXLogicProElements parity).

mod core;
mod menu;
mod tempo;
mod transport;

use logicx_core::HonestResult;
use std::panic::{AssertUnwindSafe, catch_unwind};

pub use menu::{click_dialog_button, create_software_instrument_track, open_import_midi_file_dialog};

fn guard<F>(f: F) -> Option<HonestResult>
where
    F: FnOnce() -> Option<HonestResult> + std::panic::UnwindSafe,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(_) => {
            logicx_core::diagnostic_log::append_bridge_log("native AX operation panicked");
            None
        }
    }
}

pub fn set_tempo(tempo: f64) -> Option<HonestResult> {
    guard(|| tempo::set_tempo(tempo))
}

pub fn goto_bar(bar: u32) -> Option<HonestResult> {
    guard(|| transport::goto_bar(bar))
}

pub fn set_cycle_range(start: u32, end: u32) -> Option<HonestResult> {
    guard(|| transport::set_cycle_range(start, end))
}

pub fn transport_play() -> Option<HonestResult> {
    guard(|| transport::transport_play())
}

pub fn transport_stop() -> Option<HonestResult> {
    guard(|| transport::transport_stop())
}

pub fn read_transport_state() -> Option<HonestResult> {
    guard(|| transport::read_transport_state())
}

pub fn toggle_checkbox(
    titles: &[&str],
    want_on: Option<bool>,
    via: &str,
) -> Option<HonestResult> {
    guard(|| transport::toggle_checkbox(titles, want_on, via))
}

/// True when a modal dialog/sheet occludes the arrange window (StatePoller parity).
pub fn dialog_present() -> bool {
    core::dialog_present()
}
