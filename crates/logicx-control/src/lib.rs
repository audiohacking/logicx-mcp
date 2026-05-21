mod approvals;
pub mod bridge;
mod cache;
pub mod channels;
mod executor;
mod macos;
pub mod midi;
pub mod notes;
pub mod smf;
pub mod state_poller;

/// Injectable AX script hooks for offline channel tests.
pub mod ax_test {
    pub use crate::macos::{clear_script_hook, run_ax_script, set_script_hook};
}

pub use channels::router::{
    bypass_readiness_ops, is_terminal_error, operation_for_tool, route_chain, routing_table,
};

#[cfg(not(target_os = "macos"))]
mod macos {
    use logicx_core::HonestResult;

    pub fn is_logic_running() -> bool {
        false
    }
    pub fn is_ax_trusted() -> bool {
        false
    }
    pub fn automation_ok() -> bool {
        false
    }
    pub fn automation_system_events_ok() -> bool {
        false
    }
    pub fn get_tracks() -> HonestResult {
        HonestResult::failed("macOS only")
    }
    pub fn project_info() -> HonestResult {
        HonestResult::failed("macOS only")
    }
    pub fn get_markers() -> HonestResult {
        HonestResult::failed("macOS only")
    }
}

pub use cache::{
    CacheSnapshot, ChannelStripState, MarkerState, ProjectInfo, RegionState, StateCache,
    TrackState, TransportState,
};
pub use executor::LogicExecutor;

#[cfg(target_os = "macos")]
pub use bridge::{ensure_running, should_delegate};
