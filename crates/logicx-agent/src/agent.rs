use crate::ollama::{ChatRequest, OllamaClient, OllamaMessage};
use logicx_control::LogicExecutor;
use logicx_core::{
    AgentSettings, ChatMessage, ChatRole, SYSTEM_PROMPT, ToolInvocation, UiAgentEvent,
    ollama_tool_definitions,
};
use serde_json::Value;
use std::sync::mpsc::Sender;

pub fn run_agent(
    user_text: String,
    history: Vec<ChatMessage>,
    settings: AgentSettings,
    events: Sender<UiAgentEvent>,
) {
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

                let result = executor
                    .execute(&ToolInvocation {
                        name: name.clone(),
                        arguments: args,
                    })
                    .unwrap_or_else(|e| format!("{{\"success\":false,\"error\":\"{e}\"}}"));

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
            let _ = events.send(UiAgentEvent::Assistant {
                content: "(No response text from model.)".into(),
            });
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
