use arboard::Clipboard;
use eframe::egui::{self, RichText, TextEdit};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

#[path = "../relay.rs"]
mod relay;

use relay::{
    RelayClientApprovalRequest, RelayClientCommandRequest, RelayClientRequest,
    RelayClientSessionReply, RelayClientSessionRequest, RelayClientTaskRequest, RelayPairReply,
    RelayPairRequest, RelayPairStatusRequest, RelayQueuedReply, RelayStateReply, DEFAULT_RELAY_URL,
};

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
    #[serde(default = "default_relay_url")]
    relay_url: String,
    #[serde(default)]
    use_relay: bool,
    #[serde(default)]
    agent_id: String,
    #[serde(default = "default_device_name")]
    device_name: String,
    #[serde(default)]
    device_id: String,
    #[serde(default)]
    token: String,
    #[serde(default = "default_true")]
    remember_token: bool,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            remote_url: default_remote_url(),
            relay_url: default_relay_url(),
            use_relay: false,
            agent_id: String::new(),
            device_name: default_device_name(),
            device_id: String::new(),
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
    #[serde(default)]
    kind: String,
    #[serde(default)]
    risk: String,
    #[serde(default)]
    requires_confirmation: bool,
    #[serde(default)]
    requires_approval: bool,
    #[serde(default)]
    requires_run: bool,
    #[serde(default)]
    requires_desktop: bool,
    #[serde(default)]
    steps: Vec<String>,
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
    remote_status: String,
    #[serde(default)]
    remote_server_running: bool,
    #[serde(default)]
    remote_api_url: String,
    #[serde(default)]
    remote_bind_host: String,
    #[serde(default)]
    remote_port: u16,
    #[serde(default)]
    remote_allowed_origins: String,
    #[serde(default)]
    remote_rate_limit_per_minute: u32,
    #[serde(default)]
    remote_last_action: String,
    #[serde(default)]
    relay_enabled: bool,
    #[serde(default)]
    relay_url: String,
    #[serde(default)]
    relay_status: String,
    #[serde(default)]
    relay_last_success_at: u64,
    #[serde(default)]
    relay_last_action_count: usize,
    #[serde(default)]
    relay_sync_in_flight: bool,
    #[serde(default)]
    relay_last_latency_ms: u64,
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

#[derive(Clone, Debug)]
struct ClientSnapshot {
    snapshot: RemoteControlSnapshot,
    via_relay: bool,
    relay_host_online: bool,
    relay_host_age_secs: u64,
    relay_queued_actions: usize,
    relay_recommended_client_poll_ms: u64,
}

impl ClientSnapshot {
    fn recommended_client_poll_ms(&self) -> u64 {
        if self.relay_recommended_client_poll_ms == 0 {
            2_000
        } else {
            self.relay_recommended_client_poll_ms.clamp(500, 60_000)
        }
    }
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

#[derive(Clone, Debug, Deserialize)]
struct PairReply {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    device_id: String,
    #[serde(default)]
    device_name: String,
    #[serde(default)]
    device_token: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    request_id: String,
    #[serde(default)]
    poll_after_ms: u64,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug)]
enum ClientEvent {
    State(Result<ClientSnapshot, String>),
    Paired(Result<PairReply, String>),
    Action(Result<String, String>),
}

struct ThinClientApp {
    config: ClientConfig,
    remote_url_input: String,
    relay_url_input: String,
    use_relay: bool,
    agent_id_input: String,
    device_name_input: String,
    pairing_code_input: String,
    token_input: String,
    task_input: String,
    status: String,
    action_status: String,
    snapshot: Option<ClientSnapshot>,
    selected_command_filter: String,
    events_rx: Option<Receiver<ClientEvent>>,
    poll_in_flight: bool,
    last_poll: Option<Instant>,
    poll_failures: u32,
    poll_delay: Duration,
    connected: bool,
    pending_pair_request_id: String,
}

impl ThinClientApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_client_theme(&cc.egui_ctx);
        let config = load_config();
        let mut app = Self {
            remote_url_input: config.remote_url.clone(),
            relay_url_input: config.relay_url.clone(),
            use_relay: config.use_relay,
            agent_id_input: config.agent_id.clone(),
            device_name_input: config.device_name.clone(),
            pairing_code_input: String::new(),
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
            poll_failures: 0,
            poll_delay: relay_backoff_delay(0),
            connected: false,
            pending_pair_request_id: String::new(),
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
        self.poll_failures = 0;
        self.poll_delay = relay_backoff_delay(0);
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
        self.config.relay_url = normalize_remote_url(&self.relay_url_input);
        self.relay_url_input = self.config.relay_url.clone();
        self.config.use_relay = self.use_relay;
        self.config.agent_id = self.agent_id_input.trim().to_string();
        self.config.device_name = if self.device_name_input.trim().is_empty() {
            default_device_name()
        } else {
            self.device_name_input.trim().to_string()
        };
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
                    self.poll_failures = 0;
                    self.poll_delay = Duration::from_millis(snapshot.recommended_client_poll_ms());
                    self.connected = true;
                    let transport_label = if snapshot.via_relay {
                        if snapshot.relay_host_online {
                            format!("relay online · snapshot {} с", snapshot.relay_host_age_secs)
                        } else {
                            "relay offline".to_string()
                        }
                    } else {
                        "direct".to_string()
                    };
                    self.status = format!(
                        "Подключено к {} {} · {} · {}",
                        empty_as(&snapshot.snapshot.app, "Leetcode"),
                        empty_as(&snapshot.snapshot.version, "unknown"),
                        empty_as(&snapshot.snapshot.agent_status, "ожидает"),
                        transport_label
                    );
                    self.snapshot = Some(snapshot);
                }
                ClientEvent::State(Err(err)) => {
                    self.poll_in_flight = false;
                    self.last_poll = Some(Instant::now());
                    self.poll_failures = self.poll_failures.saturating_add(1);
                    self.poll_delay = relay_backoff_delay(self.poll_failures);
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
                ClientEvent::Paired(Ok(reply)) => {
                    if reply.device_token.trim().is_empty()
                        && reply.status == "pending"
                        && !reply.request_id.trim().is_empty()
                    {
                        self.pending_pair_request_id = reply.request_id.clone();
                        self.pairing_code_input.clear();
                        self.action_status = format!(
                            "Запрос отправлен. Подтвердите устройство в основном Leetcode, затем нажмите «Проверить подтверждение». ID: {} · повтор через {} мс",
                            reply.request_id,
                            reply.poll_after_ms.max(1_000)
                        );
                        return;
                    }
                    if reply.device_token.trim().is_empty() {
                        self.pending_pair_request_id.clear();
                        self.action_status = format!(
                            "Подключение не завершено: {}",
                            empty_as(&reply.status, "нет device token")
                        );
                        return;
                    }
                    self.config.device_id = reply.device_id.clone();
                    self.config.device_name =
                        empty_as(&reply.device_name, &self.config.device_name);
                    self.device_name_input = self.config.device_name.clone();
                    self.token_input = reply.device_token;
                    self.pending_pair_request_id.clear();
                    self.pairing_code_input.clear();
                    self.config.remember_token = true;
                    self.sync_config_from_inputs();
                    self.save_config();
                    self.action_status = format!(
                        "Устройство подключено: {} ({})",
                        empty_as(&self.config.device_name, "Leetcode Client"),
                        empty_as(&self.config.device_id, "device")
                    );
                    self.connect();
                }
                ClientEvent::Paired(Err(err)) => {
                    self.action_status = format!("Ошибка подключения устройства: {err}");
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
            .map(|time| time.elapsed() >= self.poll_delay)
            .unwrap_or(true);
        if due {
            self.poll_now();
        }
    }

    fn poll_now(&mut self) {
        if self.poll_in_flight {
            return;
        }
        self.sync_config_from_inputs();
        let use_relay = self.use_relay;
        let remote_url = normalize_remote_url(&self.remote_url_input);
        let relay_url = normalize_remote_url(&self.relay_url_input);
        let agent_id = self.agent_id_input.trim().to_string();
        let token = self.token_input.trim().to_string();
        let (tx, rx) = mpsc::channel();
        self.events_rx = Some(rx);
        self.poll_in_flight = true;
        thread::spawn(move || {
            let result = if use_relay {
                get_relay_state(&relay_url, &agent_id, &token)
            } else {
                get_state(&remote_url, &token)
            };
            let _ = tx.send(ClientEvent::State(result));
        });
    }

    fn submit_task(&mut self) {
        let message = self.task_input.trim().to_string();
        if message.is_empty() {
            self.action_status = "Введите задачу для агента.".to_string();
            return;
        }
        self.sync_config_from_inputs();
        let use_relay = self.use_relay;
        let remote_url = normalize_remote_url(&self.remote_url_input);
        let relay_url = normalize_remote_url(&self.relay_url_input);
        let agent_id = self.agent_id_input.trim().to_string();
        let token = self.token_input.trim().to_string();
        let (tx, rx) = mpsc::channel();
        self.events_rx = Some(rx);
        self.action_status = "Отправляю задачу...".to_string();
        self.task_input.clear();
        thread::spawn(move || {
            let result = if use_relay {
                post_relay_task(&relay_url, &agent_id, &token, message)
            } else {
                post_json(
                    &remote_url,
                    &token,
                    "/api/tasks",
                    json!({"message": message, "source": "leetcode-client"}),
                )
            }
            .map(|reply| {
                format!(
                    "Задача поставлена: {}",
                    reply.id.unwrap_or_else(|| "queued".to_string())
                )
            });
            let _ = tx.send(ClientEvent::Action(result));
        });
    }

    fn pair_device(&mut self) {
        let code = self.pairing_code_input.trim().to_string();
        if code.is_empty() {
            self.action_status = "Введите pairing code из основного Leetcode.".to_string();
            return;
        }
        self.sync_config_from_inputs();
        self.save_config();
        self.sync_config_from_inputs();
        let use_relay = self.use_relay;
        let remote_url = normalize_remote_url(&self.remote_url_input);
        let relay_url = normalize_remote_url(&self.relay_url_input);
        let agent_id = self.agent_id_input.trim().to_string();
        let device_name = self.device_name_input.trim().to_string();
        let (tx, rx) = mpsc::channel();
        self.events_rx = Some(rx);
        self.action_status = "Подключаю устройство...".to_string();
        thread::spawn(move || {
            let result = if use_relay {
                post_relay_pair(&relay_url, &agent_id, &code, &device_name)
            } else {
                post_pair(
                    &remote_url,
                    json!({
                        "agent_id": agent_id,
                        "pairing_code": code,
                        "device_name": if device_name.trim().is_empty() { default_device_name() } else { device_name },
                        "role_view": true,
                        "role_chat": true,
                        "role_approve": true,
                        "role_files": false,
                        "role_run": false,
                        "role_desktop": false
                    }),
                )
            };
            let _ = tx.send(ClientEvent::Paired(result));
        });
    }

    fn check_pairing_status(&mut self) {
        let request_id = self.pending_pair_request_id.trim().to_string();
        if request_id.is_empty() {
            self.action_status = "Нет ожидающего запроса подтверждения.".to_string();
            return;
        }
        self.sync_config_from_inputs();
        let relay_url = normalize_remote_url(&self.relay_url_input);
        let agent_id = self.agent_id_input.trim().to_string();
        let (tx, rx) = mpsc::channel();
        self.events_rx = Some(rx);
        self.action_status = "Проверяю подтверждение устройства...".to_string();
        thread::spawn(move || {
            let result = post_relay_pair_status(&relay_url, &agent_id, &request_id);
            let _ = tx.send(ClientEvent::Paired(result));
        });
    }

    fn paste_pairing_passport_from_clipboard(&mut self) {
        let text = match Clipboard::new().and_then(|mut clipboard| clipboard.get_text()) {
            Ok(text) => text,
            Err(err) => {
                self.action_status = format!("Не удалось прочитать буфер обмена: {err}");
                return;
            }
        };
        let passport = parse_pairing_passport(&text);
        if !passport.has_any_value() {
            self.action_status = "В буфере не найден паспорт подключения Leetcode.".to_string();
            return;
        }
        if let Some(remote_url) = passport.remote_url {
            self.remote_url_input = remote_url;
        }
        if let Some(relay_url) = passport.relay_url {
            self.relay_url_input = relay_url;
            self.use_relay = true;
        }
        if let Some(agent_id) = passport.agent_id {
            self.agent_id_input = agent_id;
        }
        if let Some(pairing_code) = passport.pairing_code {
            self.pairing_code_input = pairing_code;
        }
        if let Some(device_name) = passport.device_name {
            if !device_name.trim().is_empty() {
                self.device_name_input = device_name;
            }
        }
        if let Some(token) = passport.token {
            self.token_input = token;
        }
        self.sync_config_from_inputs();
        self.save_config();
        self.action_status =
            "Паспорт подключения вставлен. Нажмите «Подключить по коду».".to_string();
    }

    fn run_command(&mut self, command_id: String, confirmed: bool) {
        self.sync_config_from_inputs();
        let use_relay = self.use_relay;
        let remote_url = normalize_remote_url(&self.remote_url_input);
        let relay_url = normalize_remote_url(&self.relay_url_input);
        let agent_id = self.agent_id_input.trim().to_string();
        let token = self.token_input.trim().to_string();
        let (tx, rx) = mpsc::channel();
        self.events_rx = Some(rx);
        self.action_status = "Отправляю команду...".to_string();
        thread::spawn(move || {
            let result = if use_relay {
                post_relay_command(&relay_url, &agent_id, &token, command_id, confirmed)
            } else {
                post_json(
                    &remote_url,
                    &token,
                    "/api/commands",
                    json!({"id": command_id, "source": "leetcode-client", "confirmed": confirmed}),
                )
            }
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
        self.sync_config_from_inputs();
        let use_relay = self.use_relay;
        let remote_url = normalize_remote_url(&self.remote_url_input);
        let relay_url = normalize_remote_url(&self.relay_url_input);
        let agent_id = self.agent_id_input.trim().to_string();
        let token = self.token_input.trim().to_string();
        let (tx, rx) = mpsc::channel();
        self.events_rx = Some(rx);
        self.action_status = "Отправляю подтверждение...".to_string();
        thread::spawn(move || {
            let approved = action == "approve";
            let result = if use_relay {
                let relay_endpoint = if endpoint.contains("run-gate") {
                    "/api/clients/run-gate"
                } else {
                    "/api/clients/approval"
                };
                post_relay_approval(&relay_url, &agent_id, &token, relay_endpoint, approved)
            } else {
                post_json(
                    &remote_url,
                    &token,
                    endpoint,
                    json!({"action": action, "source": "leetcode-client"}),
                )
            }
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
            ui.horizontal_wrapped(|ui| {
                if ui
                    .checkbox(&mut self.use_relay, "Relay по Agent ID")
                    .changed()
                {
                    self.sync_config_from_inputs();
                    self.save_config();
                }
                ui.label(
                    RichText::new(if self.use_relay {
                        "Клиент ходит в relay, host-агент сам забирает действия."
                    } else {
                        "Клиент подключается напрямую к Remote API host-агента."
                    })
                    .weak()
                    .small(),
                );
            });
            egui::Grid::new("connection_grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Relay URL").weak());
                    ui.add(
                        TextEdit::singleline(&mut self.relay_url_input)
                            .desired_width(420.0)
                            .hint_text(DEFAULT_RELAY_URL),
                    )
                    .on_hover_text("Нужен только для режима Relay по Agent ID.");
                    ui.end_row();

                    ui.label(RichText::new("Remote URL").weak());
                    ui.add(
                        TextEdit::singleline(&mut self.remote_url_input)
                            .desired_width(420.0)
                            .hint_text("http://127.0.0.1:17890"),
                    )
                    .on_hover_text("Нужен только для прямого подключения без relay.");
                    ui.end_row();

                    ui.label(RichText::new("Agent ID").weak());
                    ui.add(
                        TextEdit::singleline(&mut self.agent_id_input)
                            .desired_width(420.0)
                            .hint_text("LC-AB12-CD34-EF56"),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Имя устройства").weak());
                    ui.add(
                        TextEdit::singleline(&mut self.device_name_input)
                            .desired_width(420.0)
                            .hint_text("Домашний ноутбук"),
                    );
                    ui.end_row();

                    ui.label(RichText::new("Pairing code").weak());
                    ui.add(
                        TextEdit::singleline(&mut self.pairing_code_input)
                            .desired_width(220.0)
                            .hint_text("ABC-123"),
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
                if ui.button("Вставить паспорт").clicked() {
                    self.paste_pairing_passport_from_clipboard();
                }
                if ui.button("Подключить по коду").clicked() {
                    self.pair_device();
                }
                if ui
                    .add_enabled(
                        !self.pending_pair_request_id.trim().is_empty(),
                        egui::Button::new("Проверить подтверждение"),
                    )
                    .clicked()
                {
                    self.check_pairing_status();
                }
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
            ui.label(
                RichText::new(
                    "Паспорт подключения копируется в основном Leetcode: Контроль → Удалённый доступ → Подключение устройств.",
                )
                .weak()
                .small(),
            );
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

        let relay_snapshot = snapshot;
        let snapshot = relay_snapshot.snapshot.clone();

        panel(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Агент").strong().size(18.0));
                pill(ui, &empty_as(&snapshot.agent_id, "без Agent ID"));
                if relay_snapshot.via_relay {
                    pill(
                        ui,
                        if relay_snapshot.relay_host_online {
                            "relay online"
                        } else {
                            "relay offline"
                        },
                    );
                    pill(
                        ui,
                        &format!("snapshot {} с", relay_snapshot.relay_host_age_secs),
                    );
                    pill(
                        ui,
                        &format!("relay очередь {}", relay_snapshot.relay_queued_actions),
                    );
                } else {
                    pill(ui, "direct");
                }
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
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                pill(
                    ui,
                    if snapshot.remote_server_running {
                        "remote api running"
                    } else if snapshot.remote_enabled {
                        "remote api stopped"
                    } else {
                        "remote api off"
                    },
                );
                if !snapshot.remote_api_url.trim().is_empty() {
                    pill(ui, &compact_client_text(&snapshot.remote_api_url, 48));
                }
                if snapshot.remote_port > 0 {
                    pill(
                        ui,
                        &format!(
                            "bind {}:{}",
                            snapshot.remote_bind_host, snapshot.remote_port
                        ),
                    );
                }
                if snapshot.remote_rate_limit_per_minute > 0 {
                    pill(
                        ui,
                        &format!("rate {}/min", snapshot.remote_rate_limit_per_minute),
                    );
                }
                if !snapshot.remote_allowed_origins.trim().is_empty() {
                    pill(
                        ui,
                        &format!(
                            "origins {}",
                            compact_client_text(&snapshot.remote_allowed_origins, 28)
                        ),
                    );
                }
                if snapshot.relay_enabled {
                    pill(ui, "relay enabled");
                    if !snapshot.relay_url.trim().is_empty() {
                        pill(ui, &compact_client_text(&snapshot.relay_url, 42));
                    }
                    if snapshot.relay_sync_in_flight {
                        pill(ui, "relay sync...");
                    }
                    if snapshot.relay_last_latency_ms > 0 {
                        pill(
                            ui,
                            &format!("latency {} ms", snapshot.relay_last_latency_ms),
                        );
                    }
                    pill(
                        ui,
                        &format!("last actions {}", snapshot.relay_last_action_count),
                    );
                    if snapshot.relay_last_success_at > 0 {
                        pill(
                            ui,
                            &format!("host sync {}", age_label(snapshot.relay_last_success_at)),
                        );
                    }
                }
            });
            if !snapshot.relay_status.trim().is_empty() || !snapshot.remote_status.trim().is_empty()
            {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "{}{}{}",
                        snapshot.remote_status,
                        if !snapshot.remote_status.is_empty() && !snapshot.relay_status.is_empty() {
                            " · "
                        } else {
                            ""
                        },
                        snapshot.relay_status
                    ))
                    .weak()
                    .small(),
                );
            }
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
        let snapshot = snapshot.snapshot;
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
                        .add_enabled(command.enabled, egui::Button::new("Предпросмотр"))
                        .clicked()
                    {
                        self.action_status = command_preview_text(command);
                    }
                    if ui
                        .add_enabled(command.enabled, egui::Button::new("Запустить"))
                        .clicked()
                    {
                        self.run_command(
                            command.id.clone(),
                            command.requires_confirmation
                                || command.requires_approval
                                || command.requires_run
                                || command.requires_desktop,
                        );
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
        let snapshot = &snapshot.snapshot;
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

fn command_preview_text(command: &RemoteCommandSummary) -> String {
    let risk = if command.risk.trim().is_empty() {
        "low"
    } else {
        command.risk.trim()
    };
    let kind = if command.kind.trim().is_empty() {
        "single"
    } else {
        command.kind.trim()
    };
    let mut lines = vec![
        format!("Команда: {}", empty_as(&command.title, &command.id)),
        format!("ID: {}", command.id),
        format!("Тип: {kind}"),
        format!("Категория: {}", empty_as(&command.category, "команда")),
        format!("Риск: {risk}"),
        format!(
            "Подтверждение: {}",
            if command.requires_confirmation {
                "нужно"
            } else {
                "не нужно"
            }
        ),
        format!(
            "Роль approve: {}",
            if command.requires_approval {
                "нужна"
            } else {
                "не нужна"
            }
        ),
        format!(
            "Роль run: {}",
            if command.requires_run {
                "нужна"
            } else {
                "не нужна"
            }
        ),
        format!(
            "Роль desktop: {}",
            if command.requires_desktop {
                "нужна"
            } else {
                "не нужна"
            }
        ),
    ];
    if !command.description.trim().is_empty() {
        lines.push(format!("Описание: {}", command.description.trim()));
    }
    if !command.steps.is_empty() {
        lines.push("Шаги:".to_string());
        for (index, step) in command.steps.iter().enumerate() {
            lines.push(format!("{}. {}", index + 1, step));
        }
    }
    lines.join("\n")
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

fn get_state(remote_url: &str, token: &str) -> Result<ClientSnapshot, String> {
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
    let snapshot = response
        .json::<RemoteControlSnapshot>()
        .map_err(|err| err.to_string())?;
    Ok(ClientSnapshot {
        snapshot,
        via_relay: false,
        relay_host_online: true,
        relay_host_age_secs: 0,
        relay_queued_actions: 0,
        relay_recommended_client_poll_ms: 2_000,
    })
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

fn post_pair(remote_url: &str, body: serde_json::Value) -> Result<PairReply, String> {
    let client = http_client()?;
    let url = endpoint_url(remote_url, "/api/pair")?;
    let response = client
        .post(url)
        .json(&body)
        .send()
        .map_err(|err| err.to_string())?;
    let status = response.status();
    let reply = response.json::<PairReply>().unwrap_or(PairReply {
        ok: status.is_success(),
        device_id: String::new(),
        device_name: String::new(),
        device_token: String::new(),
        status: String::new(),
        request_id: String::new(),
        poll_after_ms: 0,
        error: None,
    });
    if status.is_success() && reply.ok && !reply.device_token.trim().is_empty() {
        Ok(reply)
    } else {
        Err(reply
            .error
            .unwrap_or_else(|| format!("сервер вернул {status}")))
    }
}

fn get_relay_state(
    relay_url: &str,
    agent_id: &str,
    device_token: &str,
) -> Result<ClientSnapshot, String> {
    let client = http_client()?;
    let url = endpoint_url(relay_url, "/api/clients/state")?;
    let session_token = relay_session_token(relay_url, agent_id, device_token);
    let response = client
        .post(url)
        .json(&RelayClientRequest {
            agent_id: agent_id.trim().to_string(),
            device_token: device_token.trim().to_string(),
            session_token,
        })
        .send()
        .map_err(|err| err.to_string())?;
    let status = response.status();
    let reply = response.json::<RelayStateReply>().map_err(|err| {
        if status.is_success() {
            err.to_string()
        } else {
            format!("relay вернул {status}: {err}")
        }
    })?;
    if status.is_success() && reply.ok {
        let snapshot = serde_json::from_value::<RemoteControlSnapshot>(reply.state)
            .map_err(|err| err.to_string())?;
        Ok(ClientSnapshot {
            snapshot,
            via_relay: true,
            relay_host_online: reply.host_online,
            relay_host_age_secs: reply.host_age_secs,
            relay_queued_actions: reply.queued_actions,
            relay_recommended_client_poll_ms: reply.recommended_client_poll_ms,
        })
    } else {
        Err(reply
            .error
            .unwrap_or_else(|| format!("relay вернул {status}")))
    }
}

fn post_relay_pair(
    relay_url: &str,
    agent_id: &str,
    pairing_code: &str,
    device_name: &str,
) -> Result<PairReply, String> {
    let client = http_client()?;
    let url = endpoint_url(relay_url, "/api/clients/pair")?;
    let response = client
        .post(url)
        .json(&RelayPairRequest {
            agent_id: agent_id.trim().to_string(),
            pairing_code: pairing_code.trim().to_string(),
            device_name: if device_name.trim().is_empty() {
                default_device_name()
            } else {
                device_name.trim().to_string()
            },
            role_view: true,
            role_chat: true,
            role_approve: true,
            role_files: false,
            role_run: false,
            role_desktop: false,
        })
        .send()
        .map_err(|err| err.to_string())?;
    let status = response.status();
    let reply = response.json::<RelayPairReply>().unwrap_or(RelayPairReply {
        ok: status.is_success(),
        device_id: String::new(),
        device_name: String::new(),
        device_token: String::new(),
        status: String::new(),
        request_id: String::new(),
        poll_after_ms: 0,
        error: None,
    });
    if status.is_success() && reply.ok {
        Ok(PairReply {
            ok: true,
            device_id: reply.device_id,
            device_name: reply.device_name,
            device_token: reply.device_token,
            status: reply.status,
            request_id: reply.request_id,
            poll_after_ms: reply.poll_after_ms,
            error: None,
        })
    } else {
        Err(reply
            .error
            .unwrap_or_else(|| format!("relay вернул {status}")))
    }
}

fn post_relay_pair_status(
    relay_url: &str,
    agent_id: &str,
    request_id: &str,
) -> Result<PairReply, String> {
    let client = http_client()?;
    let url = endpoint_url(relay_url, "/api/clients/pair/status")?;
    let response = client
        .post(url)
        .json(&RelayPairStatusRequest {
            agent_id: agent_id.trim().to_string(),
            request_id: request_id.trim().to_string(),
        })
        .send()
        .map_err(|err| err.to_string())?;
    let status = response.status();
    let reply = response.json::<RelayPairReply>().unwrap_or(RelayPairReply {
        ok: status.is_success(),
        device_id: String::new(),
        device_name: String::new(),
        device_token: String::new(),
        status: String::new(),
        request_id: request_id.trim().to_string(),
        poll_after_ms: 0,
        error: None,
    });
    if status.is_success()
        && (reply.ok
            || reply.status == "pending"
            || reply.status == "denied"
            || reply.status == "expired"
            || reply.status == "unknown")
    {
        Ok(PairReply {
            ok: reply.ok,
            device_id: reply.device_id,
            device_name: reply.device_name,
            device_token: reply.device_token,
            status: reply.status,
            request_id: reply.request_id,
            poll_after_ms: reply.poll_after_ms,
            error: reply.error,
        })
    } else {
        Err(reply
            .error
            .unwrap_or_else(|| format!("relay вернул {status}")))
    }
}

fn post_relay_task(
    relay_url: &str,
    agent_id: &str,
    device_token: &str,
    message: String,
) -> Result<ApiReply, String> {
    let session_token = relay_session_token(relay_url, agent_id, device_token);
    post_relay_queued(
        relay_url,
        "/api/clients/tasks",
        &RelayClientTaskRequest {
            agent_id: agent_id.trim().to_string(),
            device_token: device_token.trim().to_string(),
            session_token,
            message,
            source: "leetcode-client-relay".to_string(),
        },
    )
}

fn post_relay_command(
    relay_url: &str,
    agent_id: &str,
    device_token: &str,
    command_id: String,
    confirmed: bool,
) -> Result<ApiReply, String> {
    let session_token = relay_session_token(relay_url, agent_id, device_token);
    post_relay_queued(
        relay_url,
        "/api/clients/commands",
        &RelayClientCommandRequest {
            agent_id: agent_id.trim().to_string(),
            device_token: device_token.trim().to_string(),
            session_token,
            id: command_id,
            source: "leetcode-client-relay".to_string(),
            confirmed,
        },
    )
}

fn post_relay_approval(
    relay_url: &str,
    agent_id: &str,
    device_token: &str,
    endpoint: &str,
    approved: bool,
) -> Result<ApiReply, String> {
    let session_token = relay_session_token(relay_url, agent_id, device_token);
    post_relay_queued(
        relay_url,
        endpoint,
        &RelayClientApprovalRequest {
            agent_id: agent_id.trim().to_string(),
            device_token: device_token.trim().to_string(),
            session_token,
            approved,
        },
    )
}

fn relay_session_token(relay_url: &str, agent_id: &str, device_token: &str) -> String {
    if agent_id.trim().is_empty() || device_token.trim().is_empty() {
        return String::new();
    }
    let Ok(client) = http_client() else {
        return String::new();
    };
    let Ok(url) = endpoint_url(relay_url, "/api/clients/sessions") else {
        return String::new();
    };
    let Ok(response) = client
        .post(url)
        .json(&RelayClientSessionRequest {
            agent_id: agent_id.trim().to_string(),
            device_token: device_token.trim().to_string(),
        })
        .send()
    else {
        return String::new();
    };
    if !response.status().is_success() {
        return String::new();
    }
    response
        .json::<RelayClientSessionReply>()
        .ok()
        .filter(|reply| reply.ok)
        .map(|reply| reply.session_token)
        .unwrap_or_default()
}

fn post_relay_queued<T: Serialize>(
    relay_url: &str,
    endpoint: &str,
    body: &T,
) -> Result<ApiReply, String> {
    let client = http_client()?;
    let url = endpoint_url(relay_url, endpoint)?;
    let response = client
        .post(url)
        .json(body)
        .send()
        .map_err(|err| err.to_string())?;
    let status = response.status();
    let reply = response
        .json::<RelayQueuedReply>()
        .unwrap_or(RelayQueuedReply {
            ok: status.is_success(),
            id: None,
            status: Some(status.to_string()),
            error: None,
        });
    if status.is_success() && reply.ok {
        Ok(ApiReply {
            ok: true,
            id: reply.id,
            status: reply.status,
            error: None,
        })
    } else {
        Err(reply
            .error
            .unwrap_or_else(|| format!("relay вернул {status}")))
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

#[derive(Default)]
struct PairingPassport {
    remote_url: Option<String>,
    relay_url: Option<String>,
    agent_id: Option<String>,
    pairing_code: Option<String>,
    device_name: Option<String>,
    token: Option<String>,
}

impl PairingPassport {
    fn has_any_value(&self) -> bool {
        self.remote_url.is_some()
            || self.relay_url.is_some()
            || self.agent_id.is_some()
            || self.pairing_code.is_some()
            || self.device_name.is_some()
            || self.token.is_some()
    }
}

fn parse_pairing_passport(text: &str) -> PairingPassport {
    let mut passport = PairingPassport::default();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
        passport.remote_url = json_string(&value, &["remote_url", "url"]);
        passport.relay_url = json_string(&value, &["relay_url", "relayUrl"]);
        passport.agent_id = json_string(&value, &["agent_id", "agentId"]);
        passport.pairing_code = json_string(&value, &["pairing_code", "code"]);
        passport.device_name = json_string(&value, &["device_name", "name"]);
        passport.token = json_string(&value, &["device_token", "token"]);
        if passport.has_any_value() {
            return passport;
        }
    }

    for raw_line in text.lines() {
        let line = raw_line
            .trim()
            .trim_start_matches(['-', '*', '•', ' '])
            .trim();
        let Some((key, value)) = line.split_once('=').or_else(|| line.split_once(':')) else {
            continue;
        };
        let key = normalize_passport_key(key);
        let value = value.trim().trim_matches('"').to_string();
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "remoteurl" | "url" => passport.remote_url = Some(value),
            "relayurl" => passport.relay_url = Some(value),
            "agentid" => passport.agent_id = Some(value),
            "pairingcode" | "code" => passport.pairing_code = Some(value.to_ascii_uppercase()),
            "devicename" | "name" => passport.device_name = Some(value),
            "devicetoken" | "token" => passport.token = Some(value),
            _ => {}
        }
    }
    passport
}

fn normalize_passport_key(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn json_string(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
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

fn default_relay_url() -> String {
    DEFAULT_RELAY_URL.to_string()
}

fn default_device_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|name| !name.trim().is_empty())
        .map(|name| format!("Leetcode Client · {}", name.trim()))
        .unwrap_or_else(|| "Leetcode Client".to_string())
}

fn default_true() -> bool {
    true
}

fn relay_backoff_delay(failures: u32) -> Duration {
    let seconds = match failures {
        0 => 2,
        1 => 3,
        2 => 5,
        3 => 8,
        4 => 13,
        5 => 21,
        _ => 30,
    };
    Duration::from_secs(seconds)
}

fn empty_as(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.trim().to_string()
    }
}

fn compact_client_text(value: &str, max_chars: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut result = value.chars().take(keep).collect::<String>();
    result.push('…');
    result
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_value_pairing_passport() {
        let passport = parse_pairing_passport(
            "Leetcode Remote Pairing\nremote_url=http://10.0.0.2:17890\nagent_id=LC-123\npairing_code=abc-123\ndevice_name=Office PC\n",
        );

        assert_eq!(
            passport.remote_url.as_deref(),
            Some("http://10.0.0.2:17890")
        );
        assert_eq!(passport.agent_id.as_deref(), Some("LC-123"));
        assert_eq!(passport.pairing_code.as_deref(), Some("ABC-123"));
        assert_eq!(passport.device_name.as_deref(), Some("Office PC"));
    }

    #[test]
    fn parses_json_pairing_passport() {
        let passport = parse_pairing_passport(
            r#"{"remote_url":"http://127.0.0.1:17890","agent_id":"LC-XYZ","pairing_code":"QWE-777"}"#,
        );

        assert_eq!(
            passport.remote_url.as_deref(),
            Some("http://127.0.0.1:17890")
        );
        assert_eq!(passport.agent_id.as_deref(), Some("LC-XYZ"));
        assert_eq!(passport.pairing_code.as_deref(), Some("QWE-777"));
    }

    #[test]
    fn relay_poll_backoff_is_bounded() {
        assert_eq!(relay_backoff_delay(0), Duration::from_secs(2));
        assert_eq!(relay_backoff_delay(3), Duration::from_secs(8));
        assert_eq!(relay_backoff_delay(99), Duration::from_secs(30));
    }
}
