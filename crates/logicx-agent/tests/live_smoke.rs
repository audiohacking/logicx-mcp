//! Live integration tests (Ollama + Logic Pro required).
//!
//! **Always reinstall first:** `./scripts/reinstall-for-test.sh` or `./scripts/test-live.sh`
//!
//! ```bash
//! cargo test -p logicx-agent --test live_smoke -- --ignored --nocapture
//! ```

use logicx_agent::run_agent;
use logicx_core::{AgentSettings, UiAgentEvent};
use std::sync::mpsc;
use std::time::Duration;

fn collect_agent(prompt: &str) -> Vec<UiAgentEvent> {
    let (tx, rx) = mpsc::channel();
    run_agent(
        prompt.into(),
        vec![],
        AgentSettings::default_local(),
        tx,
        "test".into(),
    );
    let mut events = vec![];
    while let Ok(ev) = rx.recv_timeout(Duration::from_secs(180)) {
        println!("{ev:?}");
        let done = matches!(ev, UiAgentEvent::Done);
        events.push(ev);
        if done {
            break;
        }
    }
    events
}

#[test]
#[ignore = "requires Ollama and Logic Pro"]
fn health_check_prompt() {
    let events = collect_agent("Check Logic Pro MCP health");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, UiAgentEvent::ToolStarted { name, .. } if name == "logic_system")),
        "expected logic_system tool call"
    );
    assert!(
        events.iter().any(|e| matches!(e, UiAgentEvent::Assistant { .. })),
        "expected assistant reply"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, UiAgentEvent::Error { .. })),
        "unexpected agent error"
    );
}
