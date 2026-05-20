use crate::LogicxMcpParams;
use crate::plugin_state::PluginState;
use logicx_agent::run_agent;
use logicx_core::{UiAgentEvent, prompt::UI_HINT};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use truce_core::custom_state::StateBinding;
use truce_core::editor::PluginContext;
use truce_egui::theme::{HEADER_BG, HEADER_TEXT};
use truce_egui::{EditorUi, EguiEditor};
use truce_gui::font;

const WINDOW_W: u32 = 420;
const WINDOW_H: u32 = 560;

pub fn build_editor(params: Arc<LogicxMcpParams>) -> Box<dyn truce_core::Editor> {
    Box::new(
        EguiEditor::with_ui(
            params,
            (WINDOW_W, WINDOW_H),
            ChatEditor {
                binding: StateBinding::default(),
                input: String::new(),
                settings_open: false,
                event_rx: None,
            },
        )
        .with_visuals(truce_egui::theme::dark())
        .with_font(font::JETBRAINS_MONO),
    )
}

struct ChatEditor {
    binding: StateBinding<PluginState>,
    input: String,
    settings_open: bool,
    event_rx: Option<Receiver<UiAgentEvent>>,
}

impl EditorUi<LogicxMcpParams> for ChatEditor {
    fn opened(&mut self, ctx: &PluginContext<LogicxMcpParams>) {
        self.binding = StateBinding::new(ctx);
    }

    fn state_changed(&mut self, _ctx: &PluginContext<LogicxMcpParams>) {
        self.binding.sync();
    }

    fn ui(&mut self, ctx: &egui::Context, _state: &PluginContext<LogicxMcpParams>) {
        self.poll_agent_events();
        self.binding.sync();

        egui::TopBottomPanel::top("header")
            .exact_height(36.0)
            .frame(egui::Frame::NONE.fill(HEADER_BG))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("LogicX MCP")
                            .size(15.0)
                            .color(HEADER_TEXT)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("⚙").clicked() {
                            self.settings_open = !self.settings_open;
                        }
                        if ui.small_button("Clear").clicked() {
                            self.binding.update(|s| s.clear_chat());
                        }
                    });
                });
            });

        if self.settings_open {
            egui::TopBottomPanel::top("settings")
                .exact_height(88.0)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.label("Ollama settings (local-first)");
                    let st = self.binding.get();
                    let mut url = st.ollama_base_url.clone();
                    let mut model = st.model.clone();
                    ui.horizontal(|ui| {
                        ui.label("URL");
                        ui.text_edit_singleline(&mut url);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Model");
                        ui.text_edit_singleline(&mut model);
                    });
                    if url != st.ollama_base_url || model != st.model {
                        self.binding.update(|s| {
                            s.ollama_base_url = url;
                            s.model = model;
                        });
                    }
                });
        }

        egui::TopBottomPanel::bottom("composer")
            .min_height(72.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                let busy = self.binding.get().busy;
                ui.horizontal(|ui| {
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.input)
                            .hint_text(UI_HINT)
                            .desired_width(f32::INFINITY)
                            .desired_rows(2),
                    );
                    let send = ui
                        .add_enabled(
                            !busy && !self.input.trim().is_empty(),
                            egui::Button::new("Send"),
                        )
                        .clicked();
                    if (response.lost_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift))
                        || send
                    {
                        self.submit_prompt();
                    }
                });
                let status = self.binding.get().status_line.clone();
                if !status.is_empty() {
                    ui.label(
                        egui::RichText::new(status)
                            .small()
                            .color(egui::Color32::from_gray(160)),
                    );
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).inner_margin(8.0))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        let messages = self.binding.get().messages();
                        if messages.is_empty() {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{UI_HINT}\n\nRequires Ollama running locally (e.g. ollama pull qwen3.5)."
                                ))
                                .color(egui::Color32::from_gray(140)),
                            );
                        }
                        for msg in messages {
                            ui.add_space(6.0);
                            let (label, color) = match msg.role {
                                logicx_core::ChatRole::User => {
                                    ("You", egui::Color32::from_rgb(120, 180, 255))
                                }
                                logicx_core::ChatRole::Assistant => {
                                    ("LogicX", egui::Color32::from_rgb(140, 220, 160))
                                }
                                logicx_core::ChatRole::Tool => {
                                    ("Tool", egui::Color32::from_rgb(200, 180, 120))
                                }
                                logicx_core::ChatRole::System => continue,
                            };
                            ui.label(egui::RichText::new(label).strong().color(color));
                            ui.label(&msg.content);
                        }
                    });
            });

        if self.binding.get().busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

impl ChatEditor {
    fn submit_prompt(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.input.clear();

        let settings = self.binding.get().settings();
        let history = self.binding.get().messages();

        self.binding.update(|s| {
            s.push_user(text.clone());
            s.busy = true;
            s.status_line = "Sending…".into();
        });

        let (tx, rx) = mpsc::channel();
        self.event_rx = Some(rx);

        thread::spawn(move || {
            run_agent(text, history, settings, tx);
        });
    }

    fn poll_agent_events(&mut self) {
        let events: Vec<UiAgentEvent> = if let Some(rx) = &self.event_rx {
            rx.try_iter().collect()
        } else {
            return;
        };

        for ev in events {
            match ev {
                UiAgentEvent::Status { text } => {
                    self.binding.update(|s| s.status_line = text);
                }
                UiAgentEvent::Assistant { content } => {
                    self.binding.update(|s| {
                        s.push_assistant(content);
                        s.status_line = "Ready".into();
                        s.busy = false;
                    });
                    self.event_rx = None;
                }
                UiAgentEvent::ToolStarted { name, arguments } => {
                    let summary = format!("→ {name} {}", arguments);
                    self.binding.update(|s| {
                        s.push_tool_note(name, summary);
                        s.status_line = "Running tools…".into();
                    });
                }
                UiAgentEvent::ToolFinished { name, result } => {
                    let preview: String = result.chars().take(200).collect();
                    self.binding.update(|s| {
                        s.push_tool_note(name, preview);
                    });
                }
                UiAgentEvent::Error { message } => {
                    self.binding.update(|s| {
                        s.push_assistant(format!("⚠ {message}"));
                        s.status_line = "Error".into();
                        s.busy = false;
                    });
                    self.event_rx = None;
                }
                UiAgentEvent::Done => {
                    self.binding.update(|s| {
                        if s.busy {
                            s.status_line = "Ready".into();
                            s.busy = false;
                        }
                    });
                    self.event_rx = None;
                }
            }
        }
    }
}
