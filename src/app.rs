use crate::agent::types::{AppEvent, ChatLine, ChatRole, ToolLogLine};
use crate::agent::{run_user_turn, AgentState};
use crate::config::{append_journal, AppConfig};
use crate::tools::policy::ApprovalMap;
use crate::workspace::Workspace;
use eframe::egui::{self, RichText, TextEdit};
use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct LeetcodeApp {
    config: AppConfig,
    api_key_input: String,
    model_input: String,
    workspace: Option<Workspace>,
    file_rows: Vec<String>,
    selected_file: Option<String>,
    selected_preview: String,
    original_file_content: String,
    selected_file_editable: bool,
    editor_status: String,
    input: String,
    chat: Vec<ChatLine>,
    tool_log: Vec<ToolLogLine>,
    git_summary: String,
    events_rx: Option<Receiver<AppEvent>>,
    is_running: bool,
    cancel: Option<Arc<AtomicBool>>,
    agent_state: Arc<Mutex<AgentState>>,
    approvals: ApprovalMap,
    pending_approval: Option<PendingApproval>,
}

#[derive(Clone, Debug)]
struct PendingApproval {
    id: String,
    summary: String,
    detail: String,
}

impl LeetcodeApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        let workspace = config
            .last_workspace
            .clone()
            .and_then(|path| Workspace::new(path).ok());
        let file_rows = workspace
            .as_ref()
            .map(|workspace| workspace.ui_file_rows(600))
            .unwrap_or_default();

        let api_key_input = config.api_key.clone();
        let model_input = config.model.clone();
        let approvals = Arc::new(Mutex::new(HashMap::new()));

        Self {
            config,
            api_key_input,
            model_input,
            workspace,
            file_rows,
            selected_file: None,
            selected_preview: String::new(),
            original_file_content: String::new(),
            selected_file_editable: false,
            editor_status: String::new(),
            input: String::new(),
            chat: vec![ChatLine::system(
                "Выбери проект, проверь модель/API key и отправь задачу агенту.",
            )],
            tool_log: Vec::new(),
            git_summary: String::new(),
            events_rx: None,
            is_running: false,
            cancel: None,
            agent_state: Arc::new(Mutex::new(AgentState::default())),
            approvals,
            pending_approval: None,
        }
    }

    fn choose_workspace(&mut self) {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };

        match Workspace::new(path.clone()) {
            Ok(workspace) => {
                self.config.last_workspace = Some(path);
                self.workspace = Some(workspace);
                self.refresh_file_rows();
                self.selected_file = None;
                self.selected_preview.clear();
                self.original_file_content.clear();
                self.selected_file_editable = false;
                self.editor_status.clear();
                self.refresh_git_summary();
                self.agent_state
                    .lock()
                    .expect("agent state poisoned")
                    .reset();
                let _ = self.config.save();
            }
            Err(err) => self.chat.push(ChatLine::system(format!(
                "Не удалось открыть workspace: {err}"
            ))),
        }
    }

    fn refresh_file_rows(&mut self) {
        self.file_rows = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.ui_file_rows(600))
            .unwrap_or_default();
    }

    fn refresh_git_summary(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.git_summary.clear();
            return;
        };

        let status = Command::new("git")
            .arg("status")
            .arg("--short")
            .current_dir(workspace.root())
            .output();
        let diff = Command::new("git")
            .arg("diff")
            .arg("--stat")
            .current_dir(workspace.root())
            .output();

        let status_text = match status {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().is_empty() {
                    "status: clean".to_string()
                } else {
                    format!("status:\n{stdout}")
                }
            }
            Ok(output) => format!(
                "status failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(err) => format!("status failed: {err}"),
        };

        let diff_text = match diff {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().is_empty() {
                    "diff: no unstaged diff".to_string()
                } else {
                    format!("diff:\n{stdout}")
                }
            }
            Ok(output) => format!("diff failed:\n{}", String::from_utf8_lossy(&output.stderr)),
            Err(err) => format!("diff failed: {err}"),
        };

        self.git_summary = format!("{status_text}\n\n{diff_text}");
    }

    fn load_file_preview(&mut self, rel: &str) {
        self.selected_file = Some(rel.to_string());
        let Some(workspace) = &self.workspace else {
            self.selected_preview = "Workspace is not selected".to_string();
            self.original_file_content.clear();
            self.selected_file_editable = false;
            self.editor_status = "workspace is not selected".to_string();
            return;
        };

        match workspace.read_text(rel, 2_000_000) {
            Ok(text) => {
                self.selected_preview = text.clone();
                self.original_file_content = text;
                self.selected_file_editable = true;
                self.editor_status = "loaded".to_string();
            }
            Err(err) => {
                self.selected_preview = format!("Could not open editable file: {err}");
                self.original_file_content.clear();
                self.selected_file_editable = false;
                self.editor_status = "not editable".to_string();
            }
        }
    }

    fn editor_dirty(&self) -> bool {
        self.selected_file_editable && self.selected_preview != self.original_file_content
    }

    fn save_selected_file(&mut self) {
        let Some(path) = self.selected_file.clone() else {
            return;
        };
        let Some(workspace) = &self.workspace else {
            self.editor_status = "workspace is not selected".to_string();
            return;
        };

        match workspace.write_text(&path, &self.selected_preview) {
            Ok(()) => {
                self.original_file_content = self.selected_preview.clone();
                self.editor_status = "saved".to_string();
                self.refresh_file_rows();
            }
            Err(err) => {
                self.editor_status = format!("save failed: {err}");
            }
        }
    }

    fn revert_selected_file(&mut self) {
        if self.selected_file_editable {
            self.selected_preview = self.original_file_content.clone();
            self.editor_status = "reverted".to_string();
        }
    }

    fn reload_selected_file(&mut self) {
        let Some(path) = self.selected_file.clone() else {
            return;
        };
        self.load_file_preview(&path);
    }

    fn send_current_input(&mut self) {
        let message = self.input.trim().to_string();
        if message.is_empty() || self.is_running {
            return;
        }
        if self.workspace.is_none() {
            self.chat
                .push(ChatLine::system("Сначала выбери папку проекта."));
            return;
        }

        self.config.api_key = self.api_key_input.trim().to_string();
        self.config.model = self.model_input.trim().to_string();
        let _ = self.config.save();

        self.input.clear();
        self.chat.push(ChatLine::user(message.clone()));
        append_journal(format!("user_input\t{}", compact(&message, 500)));
        self.tool_log.push(ToolLogLine {
            title: "run".to_string(),
            content: "Agent run started".to_string(),
        });

        let (tx, rx) = mpsc::channel();
        let config = self.config.clone();
        let workspace = self.workspace.clone();
        let state = self.agent_state.clone();
        let approvals = self.approvals.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();

        self.events_rx = Some(rx);
        self.cancel = Some(cancel);
        self.is_running = true;

        thread::spawn(move || {
            let result = tokio::runtime::Runtime::new()
                .expect("failed to start tokio runtime")
                .block_on(run_user_turn(
                    message,
                    config,
                    workspace,
                    state,
                    tx.clone(),
                    approvals,
                    worker_cancel,
                ));

            if let Err(err) = result {
                let _ = tx.send(AppEvent::Error(err.to_string()));
            }
            let _ = tx.send(AppEvent::Done);
        });
    }

    fn stop_run(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::SeqCst);
        }
        if self.pending_approval.is_some() {
            self.answer_approval(false);
        }
        self.tool_log.push(ToolLogLine {
            title: "stop".to_string(),
            content: "Stop requested".to_string(),
        });
    }

    fn reset_conversation(&mut self) {
        self.agent_state
            .lock()
            .expect("agent state poisoned")
            .reset();
        self.chat.clear();
        self.chat.push(ChatLine::system(
            "Диалог сброшен. Workspace и настройки сохранены.",
        ));
        self.tool_log.clear();
    }

    fn drain_events(&mut self) {
        let mut events = Vec::new();
        if let Some(rx) = &self.events_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }

        for event in events {
            append_journal(format!("event\t{}", compact(&format!("{event:?}"), 2_000)));
            match event {
                AppEvent::AssistantText(text) => {
                    self.chat.push(ChatLine::assistant(text));
                }
                AppEvent::AssistantDelta(delta) => {
                    if let Some(last) = self.chat.last_mut() {
                        if matches!(last.role, ChatRole::Assistant) {
                            last.content.push_str(&delta);
                        } else {
                            self.chat.push(ChatLine::assistant(delta));
                        }
                    } else {
                        self.chat.push(ChatLine::assistant(delta));
                    }
                }
                AppEvent::ToolStarted { id, name, summary } => {
                    self.tool_log.push(ToolLogLine {
                        title: format!("{name} {id}"),
                        content: summary,
                    });
                }
                AppEvent::ToolOutput { id, chunk } => {
                    self.tool_log.push(ToolLogLine {
                        title: format!("output {id}"),
                        content: chunk,
                    });
                }
                AppEvent::ToolFinished { id, output } => {
                    self.tool_log.push(ToolLogLine {
                        title: format!("done {id}"),
                        content: compact(&output, 2_000),
                    });
                }
                AppEvent::ApprovalRequested {
                    id,
                    summary,
                    detail,
                } => {
                    self.pending_approval = Some(PendingApproval {
                        id,
                        summary,
                        detail,
                    });
                }
                AppEvent::Error(err) => {
                    self.chat.push(ChatLine::system(format!("Ошибка: {err}")));
                }
                AppEvent::Done => {
                    self.is_running = false;
                    self.cancel = None;
                    self.refresh_file_rows();
                    self.refresh_git_summary();
                    if self.selected_file.is_some()
                        && self.selected_file_editable
                        && !self.editor_dirty()
                    {
                        self.reload_selected_file();
                    }
                    self.tool_log.push(ToolLogLine {
                        title: "run".to_string(),
                        content: "Agent run finished".to_string(),
                    });
                }
            }
        }
    }

    fn answer_approval(&mut self, approved: bool) {
        let Some(prompt) = self.pending_approval.take() else {
            return;
        };

        let sender = self
            .approvals
            .lock()
            .expect("approval map poisoned")
            .remove(&prompt.id);
        if let Some(sender) = sender {
            let _ = sender.send(approved);
        }

        self.tool_log.push(ToolLogLine {
            title: "approval".to_string(),
            content: if approved {
                format!("Approved: {}", prompt.summary)
            } else {
                format!("Denied: {}", prompt.summary)
            },
        });
        append_journal(format!(
            "approval\t{}\t{}",
            if approved { "approved" } else { "denied" },
            prompt.summary
        ));
    }

    fn show_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Модель");
                ui.add_sized([150.0, 22.0], TextEdit::singleline(&mut self.model_input));

                ui.separator();
                ui.label("API key");
                ui.add_sized(
                    [230.0, 22.0],
                    TextEdit::singleline(&mut self.api_key_input).password(true),
                );

                ui.checkbox(&mut self.config.require_shell_approval, "Confirm shell");
                ui.checkbox(&mut self.config.require_write_approval, "Confirm write");

                if ui.button("Сохранить").clicked() {
                    self.config.model = self.model_input.trim().to_string();
                    let _ = self.config.save();
                }

                ui.separator();
                if ui
                    .add_enabled(!self.is_running, egui::Button::new("Выбрать проект"))
                    .clicked()
                {
                    self.choose_workspace();
                }

                if let Some(workspace) = &self.workspace {
                    ui.label(RichText::new(workspace.display_name()).strong());
                } else {
                    ui.label(RichText::new("проект не выбран").weak());
                }

                ui.separator();
                if ui
                    .add_enabled(!self.is_running, egui::Button::new("Сброс"))
                    .clicked()
                {
                    self.reset_conversation();
                }
                if self.is_running {
                    if ui.button("Stop").clicked() {
                        self.stop_run();
                    }
                    ui.spinner();
                }
            });
        });
    }

    fn show_file_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("files")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Файлы");
                    if ui.button("Обновить").clicked() {
                        self.refresh_file_rows();
                    }
                });

                egui::ScrollArea::vertical()
                    .id_salt("file_tree_scroll")
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for idx in 0..self.file_rows.len() {
                            let row = self.file_rows[idx].clone();
                            let selected = self.selected_file.as_deref() == Some(row.as_str());
                            if ui.selectable_label(selected, row.as_str()).clicked()
                                && !row.ends_with('/')
                                && row != "..."
                            {
                                self.load_file_preview(&row);
                            }
                        }
                    });

                ui.separator();
                if let Some(file) = &self.selected_file {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(file).strong());
                        if self.editor_dirty() {
                            ui.label(RichText::new("modified").italics());
                        } else if !self.editor_status.is_empty() {
                            ui.label(RichText::new(&self.editor_status).weak());
                        }
                    });
                } else {
                    ui.label(RichText::new("Файл не выбран").weak());
                }

                ui.horizontal(|ui| {
                    let dirty = self.editor_dirty();
                    if ui
                        .add_enabled(
                            self.selected_file_editable && dirty,
                            egui::Button::new("Save"),
                        )
                        .clicked()
                    {
                        self.save_selected_file();
                    }
                    if ui
                        .add_enabled(
                            self.selected_file_editable && dirty,
                            egui::Button::new("Revert"),
                        )
                        .clicked()
                    {
                        self.revert_selected_file();
                    }
                    if ui
                        .add_enabled(self.selected_file.is_some(), egui::Button::new("Reload"))
                        .clicked()
                    {
                        self.reload_selected_file();
                    }
                });

                egui::ScrollArea::vertical()
                    .id_salt("file_preview_scroll")
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(&mut self.selected_preview)
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Monospace)
                                .interactive(self.selected_file_editable),
                        );
                    });
            });
    }

    fn show_tool_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("tools")
            .resizable(true)
            .default_width(340.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Git");
                    if ui.button("Обновить").clicked() {
                        self.refresh_git_summary();
                    }
                });
                ui.add(
                    egui::Label::new(
                        RichText::new(if self.git_summary.trim().is_empty() {
                            "git status пока не загружен"
                        } else {
                            &self.git_summary
                        })
                        .text_style(egui::TextStyle::Monospace),
                    )
                    .wrap(),
                );
                ui.separator();
                ui.heading("Инструменты");
                egui::ScrollArea::vertical()
                    .id_salt("tool_log_scroll")
                    .show(ui, |ui| {
                        for line in &self.tool_log {
                            ui.label(RichText::new(&line.title).strong());
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&line.content)
                                        .text_style(egui::TextStyle::Monospace),
                                )
                                .wrap(),
                            );
                            ui.separator();
                        }
                    });
            });
    }

    fn show_chat_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Leetcode");
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("chat_transcript_scroll")
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.chat {
                        let label = match line.role {
                            ChatRole::User => "Вы",
                            ChatRole::Assistant => "Assistant",
                            ChatRole::System => "System",
                        };
                        let text = match line.role {
                            ChatRole::User => RichText::new(label).strong(),
                            ChatRole::Assistant => RichText::new(label).strong(),
                            ChatRole::System => RichText::new(label).weak(),
                        };
                        ui.label(text);
                        ui.add(egui::Label::new(line.content.as_str()).wrap());
                        ui.add_space(8.0);
                    }
                });
        });
    }

    fn show_input_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("input_bar")
            .exact_height(88.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let send_width = 112.0;
                    let input_width = (ui.available_width() - send_width - 12.0).max(220.0);
                    let response = ui.add_sized(
                        [input_width, 68.0],
                        TextEdit::multiline(&mut self.input)
                            .id_salt("main_prompt_input")
                            .hint_text("Что сделать в выбранном проекте? Ctrl+Enter для отправки")
                            .desired_width(f32::INFINITY),
                    );

                    let send_clicked = ui
                        .add_sized(
                            [send_width, 68.0],
                            egui::Button::new(RichText::new("Отправить").strong()),
                        )
                        .clicked()
                        && !self.is_running;
                    let enter_pressed = response.has_focus()
                        && ui.input(|input| {
                            input.key_pressed(egui::Key::Enter) && input.modifiers.ctrl
                        });

                    if (send_clicked || enter_pressed) && !self.is_running {
                        self.send_current_input();
                    }
                });
            });
    }

    fn show_approval_window(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.pending_approval.clone() else {
            return;
        };

        egui::Window::new("Подтверждение действия")
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(prompt.summary).strong());
                ui.separator();
                let mut detail = prompt.detail.clone();
                ui.add(
                    TextEdit::multiline(&mut detail)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(10)
                        .interactive(false),
                );
                ui.horizontal(|ui| {
                    if ui.button("Разрешить").clicked() {
                        self.answer_approval(true);
                    }
                    if ui.button("Отклонить").clicked() {
                        self.answer_approval(false);
                    }
                });
            });
    }
}

impl eframe::App for LeetcodeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        self.show_top_bar(ctx);
        self.show_input_bar(ctx);
        self.show_file_panel(ctx);
        self.show_tool_panel(ctx);
        self.show_chat_panel(ctx);
        self.show_approval_window(ctx);

        if self.is_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

fn compact(text: &str, max_chars: usize) -> String {
    let mut compacted = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        compacted.push_str("\n... truncated ...");
    }
    compacted
}
