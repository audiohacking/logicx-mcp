//! Live Logic Pro integration tests.
//!
//! **Always reinstall first:**
//! ```bash
//! ./scripts/reinstall-for-test.sh
//! # or full live suite:
//! ./scripts/test-live.sh
//! ```
//!
//! Manual run (after reinstall):
//! ```bash
//! cargo test -p logicx-control --test live_logic -- --ignored --nocapture
//! ```

use logicx_control::LogicExecutor;
use logicx_core::ToolInvocation;
use serde_json::json;

#[test]
#[ignore = "requires Logic Pro + permissions"]
fn health_and_permissions() {
    let ex = LogicExecutor::new();
    let health = ex
        .execute(&ToolInvocation {
            name: "logic_system".into(),
            arguments: json!({"command": "health", "params": {}}),
        })
        .unwrap();
    println!("health: {health}");
    assert!(health.contains("logic_pro_running"));

    let perms = ex
        .execute(&ToolInvocation {
            name: "logic_system".into(),
            arguments: json!({"command": "permissions", "params": {}}),
        })
        .unwrap();
    println!("permissions: {perms}");
}

#[test]
#[ignore = "requires Logic Pro + permissions"]
fn set_tempo_140() {
    let ex = LogicExecutor::new();
    let out = ex
        .execute(&ToolInvocation {
            name: "logic_transport".into(),
            arguments: json!({"command": "set_tempo", "params": {"tempo": 140.0}}),
        })
        .unwrap();
    println!("set_tempo: {out}");
    assert!(out.contains("\"success\": true") || out.contains("\"success\":true"));
}

#[test]
#[ignore = "requires Logic Pro + permissions"]
fn create_instrument_track() {
    let ex = LogicExecutor::new();
    let out = ex
        .execute(&ToolInvocation {
            name: "logic_tracks".into(),
            arguments: json!({"command": "create_instrument", "params": {}}),
        })
        .unwrap();
    println!("create_instrument: {out}");
    assert!(out.contains("\"success\": true") || out.contains("\"success\":true"));
}
