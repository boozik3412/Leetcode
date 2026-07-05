use eframe::egui::{self, RichText, TextEdit};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const APP_ICON_PNG: &[u8] = include_bytes!("../../assets/app-icon.png");

fn main() -> eframe::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1040.0, 760.0])
        .with_min_inner_size([720.0, 520.0]);
    let viewport = if let Some(icon) = load_app_icon() {
        viewport.with_icon(icon)
    } else {
        viewport
    };

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Leetcode Client",
        options,
        Box::new(|cc| Ok(Box::new(ThinClientApp::new(cc)))),
    )
}

fn load_app_icon() -> Option<std::sync::Arc<egui::IconData>> {
    let image = image::load_from_memory(APP_ICON_PNG).ok()?.into_rgba8();
    let (width, height) = image.dimensions();

    Some(std::sync::Arc::new(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ClientConfig {
    #[serde(default = "default_remote_url")]
    remote_url: String,
    #[serde(default)]
    token: String,
    #[serde(default = "default_true")]
    remember_token: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            remote_url: default_remote_url(),
            token: String::new(),
            remember_token: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Default)]
struct RemoteCommandSummary {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    enabled: bool,
}

#[derive(Clone, Debug, Deserialize, Default)]
struct RemoteToolLogEntry {
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
}

#[derive(Clone, Debug, Deserialize, Default)]
struct RemoteRunSummary {
    #[serde(default)]
    id: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    user_request: String,
    #[serde(default)]
    final_response: Option<String>,
    #[serde(default)]
    tool_count: usize,
}

#[derive(Clone, Debug, Deserialize, Default)]
struct RemoteControlSnapshot {
    #[serde(default)]
    app: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    agent_id: String,
    #[serde(default)]
    remote_enabled: bool,
    #[serde(default)]
    project_name: String,
    #[serde(default)]
    workspace_path: Option<String>,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    is_running: bool,
    #[serde(default)]
    project_is_running: bool,
    #[serde(default)]
    asset_is_running: bool,
    #[serde(default)]
    terminal_running: bool,
    #[serde(default)]
    pending_approval: bool,
    #[serde(default)]
    pending_run_gate: bool,
    #[serde(default)]
    remote_queue_len: usize,
    #[serde(default)]
    remote_last_action: String,
    #[serde(default)]
    pending_run_gate_summary: Option<String>,
    #[serde(default)]
    pending_approval_summary: Option<String>,
    #[serde(default)]
    remote_commands: Vec<RemoteCommandSummary>,
    #[serde(default)]
    tool_log_tail: Vec<RemoteToolLogEntry>,
    #[serde(default)]
    agent_history_tail: Vec<RemoteRunSummary>,
    #[serde(default)]
    agent_status: String,
    #[serde(default)]
    project_status: String,
    #[serde(default)]
    asset_status: String,
    #[serde(default)]
    updated_at: u64,
}

#[derive(Clone, Debug, Deserialize)]
struct ApiReply {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug)]
enum ClientEvent {
    State(Result<RemoteControlSnapshot, String>),
    Action(Result<String, String>),
}

struct ThinClientApp {
    config: ClientConfig,
    remote_url_input: String,
    token_input: String,
    task_input: String,
    status: String,
    action_status: String,
    snapshot: Option<RemoteControlSnapshot>,
    selected_command_filter: String,
    events_rx: Option<Receiver<ClientEvent>>,
    poll_in_flight: bool,
    last_poll: Option<Instant>,
    connected: bool,
}

impl ThinClientApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_client_theme(&cc.egui_ctx);
        let config = load_config();
        let mut app = Self {
            remote_url_input: config.remote_url.clone(),
            token_input: config.token.clone(),
            config,
            task_input: String::new(),
            status: "Введите адрес Remote API и token агента.".to_string(),
            action_status: String::new(),
            snapshot: None,
            selected_command_filter: String::new(),
            events_rx: None,
            poll_in_flight: false,
            last_poll: None,
            connected: false,
        };
        if !app.token_input.trim().is_empty() {
            app.connect();
        }
        app
    }

    fn connect(&mut self) {
        self.sync_config_from_inputs();
        self.save_config();
        self.connected = true;
        self.status = "Подключаюсь...".to_string();
        self.poll_now();
    }

    fn disconnect(&mut self) {
        self.connected = false;
        self.poll_in_flight = false;
        self.events_rx = None;
        self.status = "Отключено.".to_string();
    }

    fn sync_config_from_inputs(&mut self) {
        self.config.remote_url = normalize_remote_url(&self.remote_url_input);
        self.remote_url_input = self.config.remote_url.clone();
        self.config.token = if self.config.remember_token {
            self.token_input.trim().to_string()
        } else {
            String::new()
        };
    }

    fn save_config(&mut self) {
        if let Err(err) = save_config(&self.config) {
            self.action_status = format!("Не удалось сохранить настройки клиента: {err}");
        }
    }

    fn drain_events(&mut self) {
        let mut events = Vec::new();
        if let Some(rx) = &self.events_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        for event in events {
            match event {
                ClientEvent::State(Ok(snapshot)) => {
                    self.poll_in_flight = false;
                    self.last_poll = Some(Instant::now());
                    self.connected = true;
                    self.status = format!(
                        "Подключено к {} {} · {}",
                        empty_as(&snapshot.app, "Leetcode"),
                        empty_as(&snapshot.version, "unknown"),
                        empty_as(&snapshot.agent_status, "ожидает")
                    );
                    self.snapshot = Some(snapshot);
                }
                ClientEvent::State(Err(err)) => {
                    self.poll_in_flight = false;
                    self.status = format!("Нет подключения: {err}");
                }
                ClientEvent::Action(Ok(message)) => {
                    self.action_status = message;
                    self.poll_now();
                }
                ClientEvent::Action(Err(err)) => {
                    self.action_status = format!("Ошибка: {err}");
                    self.poll_now();
                }
            }
        }
    }

    fn maybe_poll(&mut self) {
        if !self.connected || self.poll_in_flight {
            return;
        }
        let due = self
            .last_poll
            .map(|time| time.elapsed() >= Duration::from_secs(2))
            .unwrap_or(true);
        if due {
            self.poll_now();
        }
    }

    fn poll_now(&mut self) {
        if self.poll_in_flight {
            return;
        }
        let remote_url = normalize_remote_url(&self.remote_url_input);
        let token = self.token_input.trim().to_string();
        let (tx, rx) = mpsc::channel();
        self.events_rx = Some(rx);
        self.poll_in_flight = true;
        thread::spawn(move || {
            let result = get_state(&remote_url, &token);
            let _ = tx.send(ClientEvent::State(result));
        });
    }

    fn submit_task(&mut self) {
        let message = self.task_input.trim().to_string();
        if message.is_empty() {
            self.action_status = "Введите задачу для агента.".to_string();
            return;
        }
        let remote_url = normalize_remote_url(&self.remote_url_input);
        let token = self.token_input.trim().to_string();
        let (tx, rx) = mpsc::channel();
        self.events_rx = Some(rx);
        self.action_status = "Отправляю задачу...".to_string();
        self.task_input.clear();
        thread::spawn(move || {
            let result = post_json(
                &remote_url,
                &token,
                "/api/tasks",
                json!({"message": message, "source": "leetcode-client"}),
            )
            .map(|reply| {
                format!(
                    "Задача поставлена: {}",
                    reply.id.unwrap_or_else(|| "queued".to_string())
                )
            });
            let _ = tx.send(ClientEvent::Action(result));
        });
    }

    fn run_command(&mut self, command_id: String) {
        let remote_url = normalize_remote_url(&self.remote_url_input);
        let token = self.token_input.trim().to_string();
        let (tx, rx) = mpsc::channel();
        self.events_rx = Some(rx);
        self.action_status = "Отправляю команду...".to_string();
        thread::spawn(move || {
            let result = post_json(
                &remote_url,
                &token,
                "/api/commands",
                json!({"id": command_id, "source": "leetcode-client"}),
            )
            .map(|reply| {
                format!(
                    "Команда поставлена: {}",
                    reply.id.unwrap_or_else(|| "queued".to_string())
                )
            });
            let _ = tx.send(ClientEvent::Action(result));
        });
    }

    fn answer(&mut self, endpoint: &'static str, action: &'static str) {
        let remote_url = normalize_remote_url(&self.remote_url_input);
        let token = self.token_input.trim().to_string();
        let (tx, rx) = mpsc::channel();
        self.events_rx = Some(rx);
        self.action_status = "Отправляю подтверждение...".to_string();
        thread::spawn(move || {
            let result = post_json(
                &remote_url,
                &token,
                endpoint,
                json!({"action": action, "source": "leetcode-client"}),
            )
            .map(|reply| {
                format!(
                    "Ответ отправлен: {}",
                    reply.status.unwrap_or_else(|| "queued".to_string())
                )
            });
            let _ = tx.send(ClientEvent::Action(result));
        });
    }

    fn show_connection_panel(&mut self, ui: &mut egui::Ui) {
        panel(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Подключение").strong().size(18.0));
                if self.connected {
                    pill(ui, "online");
                } else {
                    pill(ui, "offline");
                }
            });
            ui.add_space(8.0);
            egui::Grid::new("connection_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Remote URL").weak());
                    ui.add(
                        TextEdit::singleline(&mut self.remote_url_input)
                            .desired_width(420.0)
                            .hint_text("http://127.0.0.1:17890"),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Token").weak());
                    ui.add(
                        TextEdit::singleline(&mut self.token_input)
                            .desired_width(420.0)
                            .password(true)
                            .hint_text("lrt-..."),
                    );
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("Подключиться").clicked() {
                    self.connect();
                }
                if ui.button("Обновить").clicked() {
                    self.poll_now();
                }
                if ui.button("Отключиться").clicked() {
                    self.disconnect();
                }
                if ui
                    .checkbox(&mut self.config.remember_token, "Запомнить token")
                    .changed()
                {
                    self.sync_config_from_inputs();
                    self.save_config();
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new(&self.status).weak());
            if !self.action_status.trim().is_empty() {
                ui.label(RichText::new(&self.action_status).color(accent_color()));
            }
        });
    }

    fn show_snapshot_panel(&mut self, ui: &mut egui::Ui) {
        let Some(snapshot) = self.snapshot.clone() else {
            empty_state(
                ui,
                "Нет состояния агента",
                "Включите Remote API в основном Leetcode и подключитесь по URL/token.",
            );
            return;
        };

        panel(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Агент").strong().size(18.0));
                pill(ui, &empty_as(&snapshot.agent_id, "без Agent ID"));
            });
            ui.add_space(8.0);
            ui.columns(3, |columns| {
                metric(
                    &mut columns[0],
                    "Статус",
                    &empty_as(&snapshot.agent_status, "ожидает"),
                );
                metric(
                    &mut columns[1],
                    "Проект",
                    &empty_as(&snapshot.project_name, "не выбран"),
                );
                metric(
                    &mut columns[2],
                    "Модель",
                    &format!("{}/{}", snapshot.provider, snapshot.model),
                );
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                status_dot(ui, snapshot.is_running, "агент");
                status_dot(ui, snapshot.project_is_running, "команда");
                status_dot(ui, snapshot.asset_is_running, "ассеты");
                status_dot(ui, snapshot.terminal_running, "терминал");
                status_dot(ui, snapshot.remote_enabled, "remote api");
                status_dot(ui, snapshot.pending_run_gate, "план ждёт");
                status_dot(ui, snapshot.pending_approval, "действие ждёт");
                pill(ui, &format!("очередь {}", snapshot.remote_queue_len));
                if snapshot.updated_at > 0 {
                    pill(ui, &format!("обновлено {}", age_label(snapshot.updated_at)));
                }
            });
            if let Some(path) = &snapshot.workspace_path {
                ui.add_space(6.0);
                ui.label(RichText::new(path).weak().small());
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "Проект: {} · Ассеты: {}",
                    empty_as(&snapshot.project_status, "ожидает"),
                    empty_as(&snapshot.asset_status, "ожидают")
                ))
                .weak()
                .small(),
            );
            if !snapshot.remote_last_action.trim().is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(&snapshot.remote_last_action).weak());
            }
        });

        if snapshot.pending_run_gate || snapshot.pending_approval {
            panel(ui, |ui| {
                ui.label(RichText::new("Ожидает решения").strong().size(18.0));
                if let Some(summary) = &snapshot.pending_run_gate_summary {
                    ui.label("План запуска:");
                    ui.add(
                        egui::Label::new(RichText::new(summary).monospace())
                            .wrap()
                            .selectable(true),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Подтвердить план").clicked() {
                            self.answer("/api/run-gate", "approve");
                        }
                        if ui.button("Отклонить план").clicked() {
                            self.answer("/api/run-gate", "deny");
                        }
                    });
                }
                if let Some(summary) = &snapshot.pending_approval_summary {
                    ui.add_space(8.0);
                    ui.label("Действие инструмента:");
                    ui.add(
                        egui::Label::new(RichText::new(summary).monospace())
                            .wrap()
                            .selectable(true),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Разрешить действие").clicked() {
                            self.answer("/api/approval", "approve");
                        }
                        if ui.button("Запретить действие").clicked() {
                            self.answer("/api/approval", "deny");
                        }
                    });
                }
            });
        }

        panel(ui, |ui| {
            ui.label(RichText::new("Новая задача").strong().size(18.0));
            let response = ui.add(
                TextEdit::multiline(&mut self.task_input)
                    .desired_width(f32::INFINITY)
                    .desired_rows(4)
                    .hint_text("Что сделать на удалённом агенте?"),
            );
            let send_shortcut = response.has_focus()
                && ui.input(|input| input.modifiers.ctrl && input.key_pressed(egui::Key::Enter));
            ui.horizontal_wrapped(|ui| {
                if ui.button("Отправить агенту").clicked() || send_shortcut {
                    self.submit_task();
                }
                ui.label(RichText::new("Ctrl+Enter — отправить").weak().small());
            });
        });
    }

    fn show_commands_panel(&mut self, ui: &mut egui::Ui) {
        let Some(snapshot) = self.snapshot.clone() else {
            return;
        };
        panel(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Команды").strong().size(18.0));
                ui.add(
                    TextEdit::singleline(&mut self.selected_command_filter)
                        .desired_width(220.0)
                        .hint_text("фильтр"),
                );
            });
            ui.add_space(6.0);
            let filter = self.selected_command_filter.to_lowercase();
            let mut shown = 0usize;
            for command in snapshot.remote_commands.iter().filter(|command| {
                filter.is_empty()
                    || command.title.to_lowercase().contains(&filter)
                    || command.category.to_lowercase().contains(&filter)
                    || command.description.to_lowercase().contains(&filter)
            }) {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(command.enabled, egui::Button::new("Запустить"))
                        .clicked()
                    {
                        self.run_command(command.id.clone());
                    }
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&command.title).strong());
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                command.category, command.description
                            ))
                            .weak()
                            .small(),
                        );
                    });
                });
                ui.separator();
                shown += 1;
                if shown >= 12 {
                    ui.label(
                        RichText::new("Показаны первые 12 команд. Используйте фильтр.").weak(),
                    );
                    break;
                }
            }
            if shown == 0 {
                ui.label(RichText::new("Команды не найдены.").weak());
            }
        });
    }

    fn show_activity_panel(&self, ui: &mut egui::Ui) {
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        panel(ui, |ui| {
            ui.label(RichText::new("Последние инструменты").strong().size(18.0));
            if snapshot.tool_log_tail.is_empty() {
                ui.label(RichText::new("Журнал пока пуст.").weak());
            }
            for entry in snapshot.tool_log_tail.iter().rev().take(5) {
                ui.collapsing(&entry.title, |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&entry.content).monospace())
                            .wrap()
                            .selectable(true),
                    );
                });
            }
        });

        panel(ui, |ui| {
            ui.label(RichText::new("Последние запуски").strong().size(18.0));
            if snapshot.agent_history_tail.is_empty() {
                ui.label(RichText::new("История запусков пока пуста.").weak());
            }
            for run in snapshot.agent_history_tail.iter().rev().take(5) {
                ui.collapsing(
                    format!(
                        "{} · {} · {}/{} · tools {}",
                        empty_as(&run.status, "run"),
                        empty_as(&run.id, "без id"),
                        empty_as(&run.provider, "provider"),
                        empty_as(&run.model, "model"),
                        run.tool_count
                    ),
                    |ui| {
                        ui.label(RichText::new(&run.user_request).strong());
                        if let Some(response) = &run.final_response {
                            ui.add(
                                egui::Label::new(RichText::new(response).weak())
                                    .wrap()
                                    .selectable(true),
                            );
                        }
                    },
                );
            }
        });
    }
}

impl eframe::App for ThinClientApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        self.maybe_poll();

        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Leetcode Client")
                        .strong()
                        .size(21.0)
                        .color(accent_color()),
                );
                ui.label(RichText::new("тонкий клиент удалённого агента").weak());
            });
            ui.add_space(4.0);
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(bg_color()))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.show_connection_panel(ui);
                        self.show_snapshot_panel(ui);
                        self.show_commands_panel(ui);
                        self.show_activity_panel(ui);
                    });
            });

        ctx.request_repaint_after(Duration::from_millis(200));
    }
}

fn get_state(remote_url: &str, token: &str) -> Result<RemoteControlSnapshot, String> {
    let client = http_client()?;
    let url = endpoint_url(remote_url, "/api/state")?;
    let response = client
        .get(url)
        .bearer_auth(token)
        .send()
        .map_err(|err| err.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("сервер вернул {status}"));
    }
    response
        .json::<RemoteControlSnapshot>()
        .map_err(|err| err.to_string())
}

fn post_json(
    remote_url: &str,
    token: &str,
    endpoint: &str,
    body: serde_json::Value,
) -> Result<ApiReply, String> {
    let client = http_client()?;
    let url = endpoint_url(remote_url, endpoint)?;
    let response = client
        .post(url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .map_err(|err| err.to_string())?;
    let status = response.status();
    let reply = response.json::<ApiReply>().unwrap_or(ApiReply {
        ok: status.is_success(),
        id: None,
        status: Some(status.to_string()),
        error: None,
    });
    if status.is_success() && reply.ok {
        Ok(reply)
    } else {
        Err(reply
            .error
            .unwrap_or_else(|| format!("сервер вернул {status}")))
    }
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|err| err.to_string())
}

fn endpoint_url(remote_url: &str, endpoint: &str) -> Result<String, String> {
    let base = normalize_remote_url(remote_url);
    let endpoint = endpoint.trim_start_matches('/');
    Ok(format!("{base}/{endpoint}"))
}

fn normalize_remote_url(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return default_remote_url();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

fn load_config() -> ClientConfig {
    config_path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<ClientConfig>(&text).ok())
        .unwrap_or_default()
}

fn save_config(config: &ClientConfig) -> anyhow::Result<()> {
    let Some(path) = config_path() else {
        anyhow::bail!("не удалось определить папку настроек");
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}

fn config_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|dir| dir.join("leetcode-client").join("config.json"))
}

fn apply_client_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(9.0, 7.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.visuals = egui::Visuals::dark();
    style.visuals.window_fill = bg_color();
    style.visuals.panel_fill = bg_color();
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(7, 10, 14);
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(29, 34, 43);
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(39, 47, 59);
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(35, 130, 157);
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(39, 145, 175);
    ctx.set_style(style);
}

fn panel(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(panel_color())
        .stroke(egui::Stroke::new(1.0, border_color()))
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
    ui.add_space(10.0);
}

fn empty_state(ui: &mut egui::Ui, title: &str, body: &str) {
    panel(ui, |ui| {
        ui.label(RichText::new(title).strong().size(20.0));
        ui.label(RichText::new(body).weak());
    });
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).weak().small());
        ui.label(RichText::new(value).strong());
    });
}

fn pill(ui: &mut egui::Ui, text: &str) {
    egui::Frame::default()
        .fill(egui::Color32::from_rgb(30, 42, 53))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(54, 68, 83)))
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small());
        });
}

fn status_dot(ui: &mut egui::Ui, active: bool, label: &str) {
    let color = if active {
        egui::Color32::from_rgb(85, 203, 135)
    } else {
        egui::Color32::from_rgb(92, 104, 119)
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("●").color(color));
        ui.label(RichText::new(label).weak().small());
    });
}

fn bg_color() -> egui::Color32 {
    egui::Color32::from_rgb(11, 14, 19)
}

fn panel_color() -> egui::Color32 {
    egui::Color32::from_rgb(17, 22, 29)
}

fn border_color() -> egui::Color32 {
    egui::Color32::from_rgb(39, 49, 62)
}

fn accent_color() -> egui::Color32 {
    egui::Color32::from_rgb(69, 189, 224)
}

fn default_remote_url() -> String {
    "http://127.0.0.1:17890".to_string()
}

fn default_true() -> bool {
    true
}

fn empty_as(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.trim().to_string()
    }
}

fn age_label(timestamp: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(timestamp);
    let seconds = now.saturating_sub(timestamp);
    match seconds {
        0..=4 => "только что".to_string(),
        5..=59 => format!("{seconds} с назад"),
        60..=3599 => format!("{} мин назад", seconds / 60),
        3600..=86399 => format!("{} ч назад", seconds / 3600),
        _ => format!("{} д назад", seconds / 86400),
    }
}
