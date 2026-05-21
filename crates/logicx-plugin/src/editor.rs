use crate::BUILD_ID;
use crate::LogicxMcpParams;
use crate::plugin_state::PluginState;
use logicx_agent::{check_ollama_connection, run_agent};
use logicx_core::{AgentSettings, ChatMessage, ChatRole, UiAgentEvent, prompt::UI_HINT};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use truce_core::custom_state::StateBinding;
use truce_core::editor::PluginContext;
use truce_egui::theme::{HEADER_BG, HEADER_TEXT};
use truce_egui::{EditorUi, EguiEditor};
use truce_gui::font;

const WINDOW_W: u32 = 420;
const WINDOW_H: u32 = 640;

pub fn build_editor(params: Arc<LogicxMcpParams>) -> Box<dyn truce_core::Editor> {
    Box::new(
        EguiEditor::with_ui(
            params,
            (WINDOW_W, WINDOW_H),
            ChatEditor {
                binding: StateBinding::default(),
                input: String::new(),
                settings_open: false,
                debug_open: false,
                debug_log: String::new(),
                ollama_status: "checking".into(),
                connection_summary: "Checking Ollama…".into(),
                messages: Vec::new(),
                busy: false,
                status_line: String::new(),
                ollama_base_url: String::new(),
                model: String::new(),
                event_rx: None,
                conn_rx: None,
                permissions: None,
                permissions_checking: false,
                perm_rx: None,
            },
        )
        .with_visuals(truce_egui::theme::dark())
        .with_font(font::JETBRAINS_MONO),
    )
}

struct ChatEditor {
    /// Used only for initial hydrate + best-effort persist (standalone).
    /// AU `set_state` expects a wrapped envelope, so `update()` is a no-op there;
    /// all live UI state lives in the fields below — never call `binding.sync()` per frame.
    binding: StateBinding<PluginState>,
    input: String,
    settings_open: bool,
    debug_open: bool,
    debug_log: String,
    ollama_status: String,
    connection_summary: String,
    messages: Vec<ChatMessage>,
    busy: bool,
    status_line: String,
    ollama_base_url: String,
    model: String,
    event_rx: Option<Receiver<UiAgentEvent>>,
    conn_rx: Option<Receiver<logicx_core::OllamaConnectionReport>>,
    permissions: Option<logicx_control::PermissionsSnapshot>,
    permissions_checking: bool,
    perm_rx: Option<Receiver<logicx_control::PermissionsSnapshot>>,
}

impl EditorUi<LogicxMcpParams> for ChatEditor {
    fn opened(&mut self, ctx: &PluginContext<LogicxMcpParams>) {
        logicx_core::diagnostic_log::install_panic_hook("LogicX MCP AU");
        self.binding = StateBinding::new(ctx);
        self.hydrate_from_binding();
        let tail = logicx_core::diagnostic_log::read_plugin_log_tail(48_000);
        self.debug_log = if tail.is_empty() {
            String::new()
        } else {
            format!("── log restored ──\n{tail}")
        };
        self.ollama_status = "checking".into();
        self.connection_summary = "Checking Ollama…".into();
        self.schedule_connection_check();
        #[cfg(target_os = "macos")]
        self.schedule_permissions_check(true);
    }

    fn state_changed(&mut self, ctx: &PluginContext<LogicxMcpParams>) {
        // Preset / session restore from host — do not clobber an in-flight agent run.
        if self.busy {
            return;
        }
        self.binding = StateBinding::new(ctx);
        self.hydrate_from_binding();
    }

    fn ui(&mut self, ctx: &egui::Context, _state: &PluginContext<LogicxMcpParams>) {
        self.poll_connection_check();
        self.poll_agent_events();
        self.poll_permissions_check();

        let connection_summary = self.connection_summary.clone();
        let ollama_status = self.ollama_status.clone();
        let show_permissions = self.show_permissions_onboarding();
        let (dot_color, dot_label) = match ollama_status.as_str() {
            "connected" => (egui::Color32::from_rgb(80, 220, 120), "●"),
            "disconnected" => (egui::Color32::from_rgb(240, 90, 90), "●"),
            _ => (egui::Color32::from_rgb(240, 200, 80), "◌"),
        };

        egui::TopBottomPanel::top("header")
            .exact_height(52.0)
            .frame(egui::Frame::NONE.fill(HEADER_BG))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("LogicX MCP")
                                    .size(15.0)
                                    .color(HEADER_TEXT)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(format!("v{BUILD_ID}"))
                                    .size(10.0)
                                    .color(egui::Color32::from_gray(120)),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(dot_label).color(dot_color));
                            ui.label(
                                egui::RichText::new(connection_summary)
                                    .size(11.0)
                                    .color(egui::Color32::from_gray(180)),
                            );
                        });
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("⚙").clicked() {
                            self.settings_open = !self.settings_open;
                        }
                        if ui.small_button("Debug").clicked() {
                            self.debug_open = !self.debug_open;
                            if self.debug_open {
                                self.append_debug("debug panel opened");
                            }
                        }
                        if ui.small_button("Clear").clicked() {
                            self.messages.clear();
                            self.status_line.clear();
                            self.persist_to_binding();
                        }
                    });
                });
            });

        if show_permissions {
            self.draw_permissions_panel(ctx);
        }

        if self.settings_open {
            egui::TopBottomPanel::top("settings")
                .exact_height(108.0)
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.label(format!(
                        "Build {BUILD_ID} · direct curl to Ollama (same as standalone)"
                    ));
                    ui.label("Ollama settings (local or remote URL)");
                    let mut url = self.ollama_base_url.clone();
                    let mut model = self.model.clone();
                    ui.horizontal(|ui| {
                        ui.label("URL");
                        ui.text_edit_singleline(&mut url);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Model");
                        ui.text_edit_singleline(&mut model);
                    });
                    if url != self.ollama_base_url || model != self.model {
                        self.ollama_base_url = url;
                        self.model = model;
                        self.persist_to_binding();
                        self.schedule_connection_check();
                    }
                });
        }

        if self.debug_open {
            egui::TopBottomPanel::top("debug")
                .min_height(160.0)
                .max_height(280.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Debug log")
                                .strong()
                                .color(egui::Color32::from_rgb(200, 180, 120)),
                        );
                        if ui.small_button("Copy").clicked() {
                            copy_to_clipboard(&self.debug_log);
                        }
                        if ui.small_button("Clear log").clicked() {
                            self.debug_log.clear();
                        }
                    });
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            if self.debug_log.is_empty() {
                                ui.label(
                                    egui::RichText::new("(no debug output yet)")
                                        .color(egui::Color32::from_gray(120))
                                        .monospace(),
                                );
                            } else {
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(&self.debug_log)
                                            .monospace()
                                            .size(11.0)
                                            .color(egui::Color32::from_gray(200)),
                                    )
                                    .selectable(true),
                                );
                            }
                        });
                });
        }

        egui::TopBottomPanel::bottom("composer")
            .min_height(96.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                let connected = self.ollama_status == "connected";
                let can_send = !self.busy && !self.input.trim().is_empty() && connected;

                ui.vertical(|ui| {
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.input)
                            .hint_text(UI_HINT)
                            .desired_width(ui.available_width())
                            .desired_rows(2),
                    );

                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    can_send,
                                    egui::Button::new("Send").min_size(egui::vec2(72.0, 28.0)),
                                )
                                .clicked()
                            {
                                self.submit_prompt();
                            }
                        });
                    });

                    if response.has_focus()
                        && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
                        && can_send
                    {
                        self.submit_prompt();
                    }
                });

                if !self.status_line.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.status_line)
                            .small()
                            .color(egui::Color32::from_gray(160)),
                    );
                }
                if !connected && !self.busy {
                    ui.label(
                        egui::RichText::new("Ollama not connected - check settings or open Debug")
                            .small()
                            .color(egui::Color32::from_rgb(240, 140, 140)),
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
                        if self.messages.is_empty() {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{UI_HINT}\n\nBuild {BUILD_ID}. Requires Ollama (local or remote URL in ⚙)."
                                ))
                                .color(egui::Color32::from_gray(140)),
                            );
                        }
                        for msg in &self.messages {
                            ui.add_space(6.0);
                            let (label, color) = match msg.role {
                                ChatRole::User => ("You", egui::Color32::from_rgb(120, 180, 255)),
                                ChatRole::Assistant => {
                                    ("LogicX", egui::Color32::from_rgb(140, 220, 160))
                                }
                                ChatRole::Tool => {
                                    ("Tool", egui::Color32::from_rgb(200, 180, 120))
                                }
                                ChatRole::System => continue,
                            };
                            ui.label(egui::RichText::new(label).strong().color(color));
                            ui.label(&msg.content);
                        }
                    });
            });

        if self.busy {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        } else if self.ollama_status == "checking" {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        } else if show_permissions {
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
            if self.perm_rx.is_none() {
                self.schedule_permissions_check(false);
            }
        }
    }
}

impl ChatEditor {
    #[cfg(target_os = "macos")]
    fn show_permissions_onboarding(&self) -> bool {
        if self.permissions_checking && self.permissions.is_none() {
            return true;
        }
        self.permissions
            .as_ref()
            .map(|p| p.show_onboarding())
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "macos"))]
    fn show_permissions_onboarding(&self) -> bool {
        false
    }

    #[cfg(target_os = "macos")]
    fn draw_permissions_panel(&mut self, ctx: &egui::Context) {
        let accent = egui::Color32::from_rgb(240, 180, 90);
        egui::TopBottomPanel::top("permissions")
            .min_height(120.0)
            .max_height(220.0)
            .frame(
                egui::Frame::NONE
                    .fill(egui::Color32::from_rgb(38, 32, 24))
                    .inner_margin(10.0),
            )
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Logic control needs macOS permissions")
                        .strong()
                        .color(accent),
                );
                ui.add_space(4.0);

                if self.permissions_checking && self.permissions.is_none() {
                    ui.label("Checking permissions and starting control bridge…");
                    return;
                }

                let Some(perms) = self.permissions.clone() else {
                    ui.label("Permission check failed — use Check again.");
                    if ui.button("Check again").clicked() {
                        self.schedule_permissions_check(true);
                    }
                    return;
                };

                ui.label(format!(
                    "Enable permissions for \"{}\" (not logicx-control-bridge).",
                    perms.permission_subject
                ));

                if !perms.companion_app_installed {
                    ui.label(
                        egui::RichText::new(
                            "LogicX MCP.app is not installed. Run ./scripts/install-au.sh or the .pkg installer.",
                        )
                        .color(egui::Color32::from_rgb(240, 140, 140)),
                    );
                }

                if let Some(err) = &perms.error {
                    ui.label(
                        egui::RichText::new(err)
                            .small()
                            .color(egui::Color32::from_rgb(240, 140, 140)),
                    );
                }

                permission_row(ui, "Control bridge running", perms.bridge_running);
                permission_row(
                    ui,
                    "Accessibility (required for tempo & transport)",
                    perms.accessibility,
                );
                permission_row(
                    ui,
                    "Automation → Logic Pro (optional, tracks & MIDI)",
                    perms.automation_logic_pro,
                );
                permission_row(
                    ui,
                    "Automation → System Events (optional, menu fallbacks)",
                    perms.automation_system_events,
                );

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Open Accessibility").clicked() {
                        logicx_control::open_accessibility_settings();
                    }
                    if ui.button("Open Automation").clicked() {
                        logicx_control::open_automation_settings();
                    }
                    if ui.button("Check again").clicked() {
                        self.schedule_permissions_check(true);
                    }
                });
            });
    }

    #[cfg(not(target_os = "macos"))]
    fn draw_permissions_panel(&mut self, _ctx: &egui::Context) {}

    #[cfg(target_os = "macos")]
    fn schedule_permissions_check(&mut self, reconcile_bridge: bool) {
        if self.perm_rx.is_some() {
            return;
        }
        self.permissions_checking = true;
        let (tx, rx) = mpsc::channel();
        self.perm_rx = Some(rx);
        thread::spawn(move || {
            let snap = if reconcile_bridge {
                logicx_control::permissions_check::snapshot()
            } else {
                logicx_control::permissions_check::refresh()
            };
            let _ = tx.send(snap);
        });
    }

    #[cfg(not(target_os = "macos"))]
    fn schedule_permissions_check(&mut self, _reconcile_bridge: bool) {}

    fn poll_permissions_check(&mut self) {
        let snaps: Vec<_> = if let Some(rx) = &self.perm_rx {
            rx.try_iter().collect()
        } else {
            return;
        };

        for snap in snaps {
            self.permissions = Some(snap);
            self.permissions_checking = false;
            self.perm_rx = None;
        }
    }

    fn hydrate_from_binding(&mut self) {
        let st = self.binding.get();
        let defaults = AgentSettings::default_local();
        self.ollama_base_url = if st.ollama_base_url.is_empty() {
            defaults.ollama_base_url
        } else {
            st.ollama_base_url.clone()
        };
        self.model = if st.model.is_empty() {
            defaults.model
        } else {
            st.model.clone()
        };
        self.messages = st.messages();
        self.busy = st.busy;
        self.status_line = st.status_line.clone();
    }

    fn settings(&self) -> AgentSettings {
        AgentSettings {
            ollama_base_url: self.ollama_base_url.clone(),
            model: self.model.clone(),
            max_tool_rounds: 12,
        }
    }

    fn persist_to_binding(&mut self) {
        let url = self.ollama_base_url.clone();
        let model = self.model.clone();
        let messages_json =
            serde_json::to_string(&self.messages).unwrap_or_else(|_| "[]".to_string());
        let busy = self.busy;
        let status_line = self.status_line.clone();
        self.binding.update(|s| {
            s.ollama_base_url = url;
            s.model = model;
            s.messages_json = messages_json;
            s.busy = busy;
            s.status_line = status_line;
        });
    }

    fn schedule_connection_check(&mut self) {
        let settings = self.settings();
        let build_id = BUILD_ID.to_string();
        let (tx, rx) = mpsc::channel();
        self.conn_rx = Some(rx);

        self.ollama_status = "checking".into();
        self.connection_summary = "Checking Ollama…".into();
        self.append_debug("── scheduled connectivity check ──");

        thread::spawn(move || {
            let report = check_ollama_connection(&settings, &build_id);
            let _ = tx.send(report);
        });
    }

    fn append_debug(&mut self, line: impl AsRef<str>) {
        let line = line.as_ref();
        eprintln!("[LogicX MCP UI] {line}");
        logicx_core::diagnostic_log::append_plugin_log(line);
        if self.debug_log.is_empty() {
            self.debug_log = line.to_string();
        } else {
            self.debug_log.push('\n');
            self.debug_log.push_str(line);
        }
        // Keep in-memory panel bounded; full history is on disk.
        const MAX_LINES: usize = 400;
        if self.debug_log.lines().count() > MAX_LINES {
            let skip = self.debug_log.lines().count() - MAX_LINES;
            self.debug_log = self
                .debug_log
                .lines()
                .skip(skip)
                .collect::<Vec<_>>()
                .join("\n");
        }
    }

    fn apply_connection_report(&mut self, report: &logicx_core::OllamaConnectionReport) {
        self.ollama_status = if report.connected {
            "connected".into()
        } else {
            "disconnected".into()
        };
        self.connection_summary = report.summary();
        for line in &report.debug {
            self.append_debug(line);
        }
    }

    fn poll_connection_check(&mut self) {
        let reports: Vec<_> = if let Some(rx) = &self.conn_rx {
            rx.try_iter().collect()
        } else {
            return;
        };

        for report in reports {
            self.apply_connection_report(&report);
            self.conn_rx = None;
        }
    }

    fn submit_prompt(&mut self) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        if self.ollama_status != "connected" {
            self.append_debug("send blocked: ollama_status != connected");
            self.status_line = "Connect Ollama first (check settings)".into();
            return;
        }

        self.input.clear();

        let settings = self.settings();
        let history = self.messages.clone();
        let build_id = BUILD_ID.to_string();

        self.messages.push(ChatMessage::user(text.clone()));
        self.busy = true;
        self.status_line = "Sending…".into();
        self.persist_to_binding();
        self.append_debug(format!("── agent run: {text} ──"));

        let (tx, rx) = mpsc::channel();
        self.event_rx = Some(rx);

        thread::spawn(move || {
            run_agent(text, history, settings, tx, build_id);
        });
    }

    fn poll_agent_events(&mut self) {
        let events: Vec<UiAgentEvent> = if let Some(rx) = &self.event_rx {
            rx.try_iter().collect()
        } else {
            return;
        };

        let mut dirty = false;

        for ev in events {
            match ev {
                UiAgentEvent::Status { text } => {
                    self.append_debug(format!("status: {text}"));
                    self.status_line = text;
                    dirty = true;
                }
                UiAgentEvent::Assistant { content } => {
                    self.append_debug(format!("assistant: {}", preview(&content, 120)));
                    self.messages.push(ChatMessage::assistant(content));
                    self.status_line = "Ready".into();
                    self.busy = false;
                    dirty = true;
                    self.event_rx = None;
                }
                UiAgentEvent::ToolStarted { name, arguments } => {
                    let summary = format!(
                        "→ {name} {}",
                        summarize_tool_args_for_display(&name, &arguments)
                    );
                    self.append_debug(format!("tool start: {summary}"));
                    self.messages.push(ChatMessage::tool(name.clone(), summary));
                    self.status_line = "Running tools…".into();
                    dirty = true;
                }
                UiAgentEvent::ToolFinished { name, result } => {
                    let preview = preview(&result, 200);
                    self.append_debug(format!("tool done ({name}): {preview}"));
                    self.messages.push(ChatMessage::tool(name.clone(), preview));
                    dirty = true;
                }
                UiAgentEvent::Error { message } => {
                    self.append_debug(format!("ERROR: {message}"));
                    self.ollama_status = "disconnected".into();
                    self.connection_summary = message.clone();
                    self.messages
                        .push(ChatMessage::assistant(format!("⚠ {message}")));
                    self.status_line = "Error".into();
                    self.busy = false;
                    dirty = true;
                    self.event_rx = None;
                }
                UiAgentEvent::Connection { report } => {
                    self.apply_connection_report(&report);
                }
                UiAgentEvent::Debug { line } => {
                    self.append_debug(line);
                }
                UiAgentEvent::Done => {
                    self.append_debug("agent done");
                    if self.busy {
                        self.status_line = "Ready".into();
                        self.busy = false;
                        dirty = true;
                    }
                    self.event_rx = None;
                }
            }
        }

        if dirty {
            self.persist_to_binding();
        }
    }
}

fn preview(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

#[cfg(target_os = "macos")]
fn permission_row(ui: &mut egui::Ui, label: &str, ok: bool) {
    let (mark, color) = if ok {
        ("✓", egui::Color32::from_rgb(80, 220, 120))
    } else {
        ("○", egui::Color32::from_rgb(240, 140, 140))
    };
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(mark).color(color).monospace());
        ui.label(label);
    });
}

fn summarize_tool_args_for_display(tool: &str, args: &serde_json::Value) -> String {
    let mut out = args.clone();
    if tool == "logic_tracks"
        && let Some(obj) = out.as_object_mut()
        && obj.get("command").and_then(|c| c.as_str()) == Some("record_sequence")
        && let Some(params) = obj.get_mut("params").and_then(|p| p.as_object_mut())
        && let Some(notes) = params.get("notes").and_then(|n| n.as_str())
    {
        let events = if notes.contains(';') {
            notes.split(';').filter(|s| !s.trim().is_empty()).count()
        } else {
            notes.split(',').filter(|s| !s.trim().is_empty()).count() / 3
        };
        params.insert(
            "notes".into(),
            serde_json::Value::String(format!("<~{events} events, {} chars>", notes.len())),
        );
    }
    out.to_string()
}

#[cfg(target_os = "macos")]
fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    if let Ok(mut child) = Command::new("/usr/bin/pbcopy")
        .stdin(Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    }
}

#[cfg(not(target_os = "macos"))]
fn copy_to_clipboard(text: &str) {
    let _ = text;
}
