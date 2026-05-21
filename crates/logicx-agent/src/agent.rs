use crate::connection::check_ollama_connection_with_events;
use crate::ollama::{ChatRequest, OllamaClient, OllamaMessage};
use logicx_control::LogicExecutor;
use logicx_core::{
    AgentSettings, ChatMessage, ChatRole, SYSTEM_PROMPT, ToolInvocation, UiAgentEvent,
    ollama_tool_definitions,
};
use serde_json::Value;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::Sender;

pub fn run_agent(
    user_text: String,
    history: Vec<ChatMessage>,
    settings: AgentSettings,
    events: Sender<UiAgentEvent>,
    build_id: String,
) {
    check_ollama_connection_with_events(settings.clone(), build_id.clone(), events.clone());

    let _ = events.send(UiAgentEvent::Status {
        text: format!("Connecting to Ollama ({})…", settings.model),
    });

    let client = OllamaClient::new(&settings.ollama_base_url, &settings.model);
    let executor = LogicExecutor::new();
    let tools = ollama_tool_definitions();

    let mut messages = vec![OllamaMessage {
        role: "system".into(),
        content: SYSTEM_PROMPT.into(),
        tool_calls: None,
        tool_name: None,
    }];

    for msg in history {
        if msg.role == ChatRole::System {
            continue;
        }
        messages.push(chat_to_ollama(&msg));
    }
    messages.push(OllamaMessage {
        role: "user".into(),
        content: user_text,
        tool_calls: None,
        tool_name: None,
    });

    for round in 0..settings.max_tool_rounds {
        let _ = events.send(UiAgentEvent::Status {
            text: if round == 0 {
                "Thinking…".into()
            } else {
                format!("Tool round {}…", round + 1)
            },
        });

        let response = match client.chat(ChatRequest {
            messages: messages.clone(),
            tools: Some(tools.clone()),
            temperature: Some(0.2),
            max_tokens: Some(4096),
        }) {
            Ok(r) => r,
            Err(e) => {
                let _ = events.send(UiAgentEvent::Debug {
                    line: format!("chat error: {e}"),
                });
                let _ = events.send(UiAgentEvent::Debug {
                    line: format!("ollama_url={}", settings.ollama_base_url),
                });
                let _ = events.send(UiAgentEvent::Error {
                    message: format!("Ollama error: {e}"),
                });
                let _ = events.send(UiAgentEvent::Done);
                return;
            }
        };

        let assistant = response.message.clone();
        messages.push(assistant.clone());

        if let Some(calls) = assistant.tool_calls.as_ref().filter(|c| !c.is_empty()) {
            for call in calls {
                let name = call.function.name.clone();
                let args = normalize_tool_args(&call.function.arguments);

                let _ = events.send(UiAgentEvent::ToolStarted {
                    name: name.clone(),
                    arguments: args.clone(),
                });

                let result = match catch_unwind(AssertUnwindSafe(|| {
                    executor.execute(&ToolInvocation {
                        name: name.clone(),
                        arguments: args.clone(),
                    })
                })) {
                    Ok(Ok(json)) => json,
                    Ok(Err(e)) => format!("{{\"success\":false,\"error\":\"{e}\"}}"),
                    Err(_) => {
                        logicx_core::diagnostic_log::append_plugin_log(format!(
                            "tool PANIC: {name}"
                        ));
                        "{\"success\":false,\"error\":\"tool handler panicked (see plugin.log)\"}"
                            .into()
                    }
                };

                let _ = events.send(UiAgentEvent::ToolFinished {
                    name: name.clone(),
                    result: result.clone(),
                });

                messages.push(OllamaMessage {
                    role: "tool".into(),
                    content: result,
                    tool_calls: None,
                    tool_name: Some(name),
                });
            }
            continue;
        }

        let text = assistant.content.trim();
        if !text.is_empty() {
            let _ = events.send(UiAgentEvent::Assistant {
                content: text.to_string(),
            });
        } else {
            let fallback = synthesize_assistant_reply(&messages);
            let _ = events.send(UiAgentEvent::Assistant { content: fallback });
        }
        let _ = events.send(UiAgentEvent::Done);
        return;
    }

    let _ = events.send(UiAgentEvent::Error {
        message: format!(
            "Stopped after {} tool rounds — try a simpler request.",
            settings.max_tool_rounds
        ),
    });
    let _ = events.send(UiAgentEvent::Done);
}

fn synthesize_assistant_reply(messages: &[OllamaMessage]) -> String {
    for msg in messages.iter().rev() {
        if msg.role != "tool" {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(&msg.content) {
            if v.get("success").and_then(|s| s.as_bool()) == Some(true) {
                if let Some(detail) = v.get("detail") {
                    return format!(
                        "Done.\n{}",
                        serde_json::to_string_pretty(detail).unwrap_or_default()
                    );
                }
                return "Done — tool succeeded (see debug log for details).".into();
            }
            if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
                return format!("Tool failed: {err}");
            }
        }
        let preview: String = msg.content.chars().take(400).collect();
        return format!("Tool result:\n{preview}");
    }
    "Command completed.".into()
}

fn chat_to_ollama(msg: &ChatMessage) -> OllamaMessage {
    let role = match msg.role {
        ChatRole::User => "user",
        ChatRole::Assistant => "assistant",
        ChatRole::Tool => "tool",
        ChatRole::System => "system",
    };
    OllamaMessage {
        role: role.into(),
        content: msg.content.clone(),
        tool_calls: None,
        tool_name: msg.tool_name.clone(),
    }
}

fn normalize_tool_args(args: &Value) -> Value {
    match args {
        Value::String(s) => {
            serde_json::from_str(s).unwrap_or_else(|_| serde_json::json!({ "command": s.trim() }))
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_string_args() {
        let v = normalize_tool_args(&Value::String(r#"{"command":"play"}"#.into()));
        assert_eq!(v["command"], "play");
    }
}
