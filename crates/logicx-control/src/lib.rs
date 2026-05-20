mod executor;
mod notes;
mod smf;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(not(target_os = "macos"))]
mod macos {
    use logicx_core::HonestResult;

    pub fn is_logic_running() -> bool {
        false
    }

    pub fn transport_play() -> HonestResult {
        HonestResult::failed("macOS only")
    }

    pub fn transport_stop() -> HonestResult {
        HonestResult::failed("macOS only")
    }

    pub fn transport_set_tempo(_tempo: f64) -> HonestResult {
        HonestResult::failed("macOS only")
    }

    pub fn transport_goto_bar(_bar: u32) -> HonestResult {
        HonestResult::failed("macOS only")
    }

    pub fn import_midi_file(_path: &str) -> HonestResult {
        HonestResult::failed("macOS only")
    }
}

pub use executor::LogicExecutor;
