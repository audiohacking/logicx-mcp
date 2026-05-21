//! Injectable AX script runtime for offline tests (FakeAXRuntime parity).

use logicx_core::HonestResult;
use parking_lot::RwLock;
use std::sync::Arc;

type ScriptHook = Arc<dyn Fn(&str) -> Option<HonestResult> + Send + Sync>;

static SCRIPT_HOOK: RwLock<Option<ScriptHook>> = RwLock::new(None);

/// Replace native AX script dispatch (test-only hook; cleared after test).
pub fn set_script_hook(hook: ScriptHook) {
    *SCRIPT_HOOK.write() = Some(hook);
}

pub fn clear_script_hook() {
    *SCRIPT_HOOK.write() = None;
}

pub fn run_script(kind: &str) -> Option<HonestResult> {
    SCRIPT_HOOK.read().as_ref().and_then(|h| h(kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn hook_intercepts_ax_script_kind() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_in = Arc::clone(&calls);
        set_script_hook(Arc::new(move |kind| {
            calls_in.fetch_add(1, Ordering::SeqCst);
            if kind == "toggle_cycle" {
                Some(HonestResult {
                    success: true,
                    verified: Some(true),
                    reason: None,
                    error: None,
                    detail: Some(serde_json::json!({ "via": "mock" })),
                })
            } else {
                None
            }
        }));
        let hit = run_script("toggle_cycle").expect("mock response");
        assert!(hit.success);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(run_script("toggle_metronome").is_none());
        clear_script_hook();
        assert!(run_script("toggle_cycle").is_none());
    }
}
