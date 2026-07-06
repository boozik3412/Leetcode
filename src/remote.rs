use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;
const REMOTE_SESSION_TOKEN_VERSION: &str = "lrs1";
const DEFAULT_REMOTE_SESSION_TTL_SECS: u64 = 15 * 60;

#[derive(Clone, Debug)]
pub struct RemoteControlServerConfig {
    pub host: String,
    pub port: u16,
    pub token: String,
    pub policy: RemoteAccessPolicy,
    pub actions: Option<Sender<RemoteControlAction>>,
}

pub type RemoteControlSharedState = Arc<Mutex<RemoteControlSnapshot>>;

#[derive(Clone, Debug)]
pub enum RemoteControlAction {
    SubmitTask(RemoteSubmittedTask),
    RunCommand(RemoteCommandRequest),
    AnswerRunGate { approved: bool },
    AnswerApproval { approved: bool },
    PairDevice(RemotePairedDevice),
    DeviceSeen { device_id: String, seen_at: u64 },
    Audit(RemoteAuditEvent),
}

#[derive(Clone, Debug)]
pub struct RemoteAccessPolicy {
    pub view: bool,
    pub chat: bool,
    pub approve: bool,
    pub files: bool,
    pub run: bool,
    pub desktop: bool,
    pub agent_id: String,
    pub pairing_code: String,
    pub pairing_expires_at: u64,
    pub default_role_view: bool,
    pub default_role_chat: bool,
    pub default_role_approve: bool,
    pub default_role_files: bool,
    pub default_role_run: bool,
    pub default_role_desktop: bool,
    pub default_device_ttl_days: u32,
    pub devices: Vec<RemoteTrustedDevice>,
    pub allowed_origins: Vec<String>,
    pub rate_limit_per_minute: u32,
    pub device_rate_limit_per_minute: u32,
    pub ip_rate_limit_per_minute: u32,
    pub session_ttl_secs: u64,
    pub audit: bool,
}

impl Default for RemoteAccessPolicy {
    fn default() -> Self {
        Self {
            view: true,
            chat: true,
            approve: true,
            files: true,
            run: true,
            desktop: true,
            agent_id: String::new(),
            pairing_code: String::new(),
            pairing_expires_at: 0,
            default_role_view: true,
            default_role_chat: true,
            default_role_approve: true,
            default_role_files: false,
            default_role_run: false,
            default_role_desktop: false,
            default_device_ttl_days: 30,
            devices: Vec::new(),
            allowed_origins: Vec::new(),
            rate_limit_per_minute: 120,
            device_rate_limit_per_minute: 60,
            ip_rate_limit_per_minute: 180,
            session_ttl_secs: DEFAULT_REMOTE_SESSION_TTL_SECS,
            audit: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteTrustedDevice {
    pub id: String,
    pub name: String,
    pub token: String,
    pub role_view: bool,
    pub role_chat: bool,
    pub role_approve: bool,
    pub role_files: bool,
    #[serde(default)]
    pub role_run: bool,
    #[serde(default)]
    pub role_desktop: bool,
    pub created_at: u64,
    pub last_seen_at: u64,
    #[serde(default)]
    pub expires_at: u64,
    #[serde(default)]
    pub token_rotated_at: u64,
    #[serde(default)]
    pub revoked_at: u64,
    pub revoked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteDeviceSummary {
    pub id: String,
    pub name: String,
    pub role_view: bool,
    pub role_chat: bool,
    pub role_approve: bool,
    pub role_files: bool,
    #[serde(default)]
    pub role_run: bool,
    #[serde(default)]
    pub role_desktop: bool,
    pub created_at: u64,
    pub last_seen_at: u64,
    #[serde(default)]
    pub expires_at: u64,
    #[serde(default)]
    pub token_rotated_at: u64,
    #[serde(default)]
    pub revoked_at: u64,
    pub revoked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemotePairedDevice {
    pub id: String,
    pub name: String,
    pub token: String,
    pub role_view: bool,
    pub role_chat: bool,
    pub role_approve: bool,
    pub role_files: bool,
    #[serde(default)]
    pub role_run: bool,
    #[serde(default)]
    pub role_desktop: bool,
    pub created_at: u64,
    #[serde(default)]
    pub expires_at: u64,
}

#[derive(Clone, Copy, Debug)]
enum RemoteAccessRole {
    View,
    Chat,
    Approve,
    Files,
    Run,
    Desktop,
}

impl RemoteAccessRole {
    fn label(self) -> &'static str {
        match self {
            RemoteAccessRole::View => "view",
            RemoteAccessRole::Chat => "chat",
            RemoteAccessRole::Approve => "approve",
            RemoteAccessRole::Files => "files",
            RemoteAccessRole::Run => "run",
            RemoteAccessRole::Desktop => "desktop",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteAuditEvent {
    pub event: String,
    pub detail: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteSubmittedTask {
    pub id: String,
    pub message: String,
    pub source: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteCommandRequest {
    pub id: String,
    pub source: String,
    pub created_at: u64,
    #[serde(default)]
    pub confirmed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteCommandSummary {
    pub id: String,
    pub title: String,
    pub category: String,
    pub description: String,
    pub enabled: bool,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub risk: String,
    #[serde(default)]
    pub requires_confirmation: bool,
    #[serde(default)]
    pub requires_approval: bool,
    #[serde(default)]
    pub requires_run: bool,
    #[serde(default)]
    pub requires_desktop: bool,
    #[serde(default)]
    pub steps: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteToolLogEntry {
    pub title: String,
    pub content: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteRunSummary {
    pub id: String,
    pub status: String,
    pub started_at: u64,
    pub duration_ms: u64,
    pub provider: String,
    pub model: String,
    pub user_request: String,
    pub final_response: Option<String>,
    pub changed_files: Vec<String>,
    pub errors: Vec<String>,
    pub tool_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteControlSnapshot {
    pub app: String,
    pub version: String,
    pub agent_id: String,
    pub remote_enabled: bool,
    pub project_name: String,
    pub workspace_path: Option<String>,
    pub provider: String,
    pub model: String,
    pub workspace_mode: String,
    pub right_panel: String,
    pub is_running: bool,
    pub project_is_running: bool,
    pub asset_is_running: bool,
    pub terminal_running: bool,
    pub pending_approval: bool,
    pub pending_run_gate: bool,
    pub chat_messages: usize,
    pub tool_log_entries: usize,
    pub git_changed_files: usize,
    pub remote_queue_len: usize,
    #[serde(default)]
    pub remote_status: String,
    #[serde(default)]
    pub remote_server_running: bool,
    #[serde(default)]
    pub remote_api_url: String,
    #[serde(default)]
    pub remote_bind_host: String,
    #[serde(default)]
    pub remote_port: u16,
    #[serde(default)]
    pub remote_allowed_origins: String,
    #[serde(default)]
    pub remote_rate_limit_per_minute: u32,
    #[serde(default)]
    pub remote_device_rate_limit_per_minute: u32,
    #[serde(default)]
    pub remote_ip_rate_limit_per_minute: u32,
    pub remote_last_action: String,
    #[serde(default)]
    pub relay_enabled: bool,
    #[serde(default)]
    pub relay_url: String,
    #[serde(default)]
    pub relay_status: String,
    #[serde(default)]
    pub relay_last_success_at: u64,
    #[serde(default)]
    pub relay_last_action_count: usize,
    #[serde(default)]
    pub relay_sync_in_flight: bool,
    #[serde(default)]
    pub relay_last_latency_ms: u64,
    pub remote_devices: Vec<RemoteDeviceSummary>,
    pub remote_pairing_expires_at: u64,
    pub pending_run_gate_summary: Option<String>,
    pub pending_approval_summary: Option<String>,
    pub remote_commands: Vec<RemoteCommandSummary>,
    pub tool_log_tail: Vec<RemoteToolLogEntry>,
    pub agent_history_tail: Vec<RemoteRunSummary>,
    #[serde(skip_serializing, default)]
    pub agent_history_details: Vec<Value>,
    pub file_rows: Vec<String>,
    pub agent_status: String,
    pub project_status: String,
    pub asset_status: String,
    pub updated_at: u64,
}

impl Default for RemoteControlSnapshot {
    fn default() -> Self {
        Self {
            app: "Leetcode".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            agent_id: String::new(),
            remote_enabled: false,
            project_name: "нет проекта".to_string(),
            workspace_path: None,
            provider: String::new(),
            model: String::new(),
            workspace_mode: String::new(),
            right_panel: String::new(),
            is_running: false,
            project_is_running: false,
            asset_is_running: false,
            terminal_running: false,
            pending_approval: false,
            pending_run_gate: false,
            chat_messages: 0,
            tool_log_entries: 0,
            git_changed_files: 0,
            remote_queue_len: 0,
            remote_status: String::new(),
            remote_server_running: false,
            remote_api_url: String::new(),
            remote_bind_host: String::new(),
            remote_port: 0,
            remote_allowed_origins: String::new(),
            remote_rate_limit_per_minute: 0,
            remote_device_rate_limit_per_minute: 0,
            remote_ip_rate_limit_per_minute: 0,
            remote_last_action: String::new(),
            relay_enabled: false,
            relay_url: String::new(),
            relay_status: String::new(),
            relay_last_success_at: 0,
            relay_last_action_count: 0,
            relay_sync_in_flight: false,
            relay_last_latency_ms: 0,
            remote_devices: Vec::new(),
            remote_pairing_expires_at: 0,
            pending_run_gate_summary: None,
            pending_approval_summary: None,
            remote_commands: Vec::new(),
            tool_log_tail: Vec::new(),
            agent_history_tail: Vec::new(),
            agent_history_details: Vec::new(),
            file_rows: Vec::new(),
            agent_status: "ожидает".to_string(),
            project_status: "ожидает".to_string(),
            asset_status: "ожидает".to_string(),
            updated_at: unix_timestamp(),
        }
    }
}

pub fn generate_remote_access_token() -> String {
    format!("lrt-{}", uuid::Uuid::new_v4().simple())
}

pub fn generate_remote_device_token() -> String {
    format!("ldt-{}", uuid::Uuid::new_v4().simple())
}

pub fn new_remote_shared_state() -> RemoteControlSharedState {
    Arc::new(Mutex::new(RemoteControlSnapshot::default()))
}

pub fn update_remote_shared_state(
    shared_state: &RemoteControlSharedState,
    snapshot: RemoteControlSnapshot,
) {
    if let Ok(mut state) = shared_state.lock() {
        *state = snapshot;
    }
}

pub struct RemoteControlServer {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    bind_addr: String,
}

impl RemoteControlServer {
    pub fn bind_addr(&self) -> &str {
        &self.bind_addr
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for RemoteControlServer {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn start_remote_control_server(
    config: RemoteControlServerConfig,
    shared_state: RemoteControlSharedState,
) -> anyhow::Result<RemoteControlServer> {
    anyhow::ensure!(
        !config.token.trim().is_empty(),
        "remote access token is empty"
    );

    let bind_target = format!("{}:{}", config.host.trim(), config.port);
    let listener = TcpListener::bind(&bind_target)?;
    listener.set_nonblocking(true)?;
    let bind_addr = listener.local_addr()?.to_string();
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = Arc::clone(&stop);
    let token = config.token.trim().to_string();
    let policy = config.policy.clone();
    let actions = config.actions.clone();
    let rate_limit = Arc::new(Mutex::new(RemoteRateLimitState::default()));

    let handle = thread::spawn(move || {
        while !server_stop.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, addr)) => {
                    let state = Arc::clone(&shared_state);
                    let token = token.clone();
                    let policy = policy.clone();
                    let actions = actions.clone();
                    let rate_limit = Arc::clone(&rate_limit);
                    let stop = Arc::clone(&server_stop);
                    let client_ip = addr.ip().to_string();
                    let _ = thread::Builder::new()
                        .name("leetcode-remote-client".to_string())
                        .spawn(move || {
                            handle_client(
                                stream, state, token, policy, actions, rate_limit, stop, client_ip,
                            )
                        });
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(60));
                }
                Err(_) => {
                    thread::sleep(Duration::from_millis(250));
                }
            }
        }
    });

    Ok(RemoteControlServer {
        stop,
        handle: Some(handle),
        bind_addr,
    })
}

fn handle_client(
    mut stream: TcpStream,
    shared_state: RemoteControlSharedState,
    token: String,
    policy: RemoteAccessPolicy,
    actions: Option<Sender<RemoteControlAction>>,
    rate_limit: Arc<Mutex<RemoteRateLimitState>>,
    stop: Arc<AtomicBool>,
    client_ip: String,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let Ok(cloned_stream) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(cloned_stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() || request_line.trim().is_empty() {
        return;
    }

    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let body = read_request_body(&mut reader, &headers);

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or("/");
    let (path, query) = split_target(target);

    if method == "OPTIONS" {
        write_empty_response(&mut stream, 204, "No Content");
        return;
    }

    match (method, path) {
        ("GET", "/") => write_html_response(&mut stream, remote_dashboard_html()),
        ("GET", "/manifest.webmanifest") => write_json_response(
            &mut stream,
            200,
            &json!({
                "name": "Leetcode Remote",
                "short_name": "Leetcode",
                "start_url": "/",
                "display": "standalone",
                "background_color": "#0c1118",
                "theme_color": "#1f9fc4"
            }),
        ),
        ("GET", "/health") => write_json_response(
            &mut stream,
            200,
            &json!({
                "ok": true,
                "app": "Leetcode",
                "version": env!("CARGO_PKG_VERSION"),
                "service": "remote-control"
            }),
        ),
        ("POST", "/api/sessions") => {
            handle_create_session(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                actions.as_ref(),
                &client_ip,
                path,
            );
        }
        ("POST", "/api/pair") => {
            if !origin_is_allowed(&headers, &policy) {
                send_remote_audit(actions.as_ref(), policy.audit, "origin_denied", path);
                write_json_response(
                    &mut stream,
                    403,
                    &json!({"ok": false, "error": "origin is not allowed"}),
                );
                return;
            }
            if let Some(error) = check_remote_rate_limits(&rate_limit, &policy, None, &client_ip) {
                send_remote_audit(
                    actions.as_ref(),
                    policy.audit,
                    "rate_limited",
                    &format!("{path}: {error}"),
                );
                write_json_response(&mut stream, 429, &json!({"ok": false, "error": error}));
                return;
            }
            handle_pair_device(&mut stream, &body, &policy, actions.as_ref());
        }
        ("GET", "/api/state") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::View,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            let snapshot = snapshot_or_default(&shared_state);
            write_json_response(&mut stream, 200, &snapshot);
        }
        ("GET", "/api/events") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::View,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            write_sse_stream(&mut stream, shared_state, stop);
        }
        ("GET", "/api/tool-log") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::View,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            let snapshot = snapshot_or_default(&shared_state);
            write_json_response(
                &mut stream,
                200,
                &json!({"ok": true, "entries": snapshot.tool_log_tail}),
            );
        }
        ("GET", "/api/history") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::View,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            let snapshot = snapshot_or_default(&shared_state);
            write_json_response(
                &mut stream,
                200,
                &json!({"ok": true, "runs": snapshot.agent_history_tail}),
            );
        }
        ("GET", "/api/history/run") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::View,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            let Some(id) = query_param(query, "id") else {
                write_json_response(
                    &mut stream,
                    400,
                    &json!({"ok": false, "error": "id is required"}),
                );
                return;
            };
            let snapshot = snapshot_or_default(&shared_state);
            let run = snapshot
                .agent_history_details
                .iter()
                .find(|run| run.get("id").and_then(Value::as_str) == Some(id.as_str()));
            if let Some(run) = run {
                write_json_response(&mut stream, 200, &json!({"ok": true, "run": run}));
            } else {
                write_json_response(
                    &mut stream,
                    404,
                    &json!({"ok": false, "error": "run not found"}),
                );
            }
        }
        ("GET", "/api/files") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::Files,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            let snapshot = snapshot_or_default(&shared_state);
            write_json_response(
                &mut stream,
                200,
                &json!({"ok": true, "workspace": snapshot.workspace_path, "files": snapshot.file_rows}),
            );
        }
        ("GET", "/api/files/content") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::Files,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            if let Some(file_path) = query_param(query, "path") {
                send_remote_audit(
                    actions.as_ref(),
                    policy.audit,
                    "file_read",
                    &format!("GET /api/files/content path={file_path}"),
                );
            }
            let snapshot = snapshot_or_default(&shared_state);
            write_file_content_response(&mut stream, &snapshot, query);
        }
        ("GET", "/api/commands") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::View,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            let snapshot = snapshot_or_default(&shared_state);
            write_json_response(
                &mut stream,
                200,
                &json!({"ok": true, "commands": snapshot.remote_commands}),
            );
        }
        ("POST", "/api/tasks") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::Chat,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            handle_submit_task(&mut stream, &body, actions.as_ref());
        }
        ("POST", "/api/commands") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::Chat,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            handle_run_command(
                &mut stream,
                &body,
                actions.as_ref(),
                policy.audit,
                &shared_state,
                &headers,
                query,
                &token,
                &policy,
            );
        }
        ("POST", "/api/run-gate") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::Approve,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            handle_binary_action(&mut stream, &body, actions.as_ref(), true);
        }
        ("POST", "/api/approval") => {
            if !authorize_or_write(
                &mut stream,
                &headers,
                query,
                &token,
                &policy,
                &rate_limit,
                RemoteAccessRole::Approve,
                actions.as_ref(),
                &client_ip,
                path,
            ) {
                return;
            }
            handle_binary_action(&mut stream, &body, actions.as_ref(), false);
        }
        _ => write_json_response(
            &mut stream,
            404,
            &json!({
                "ok": false,
                "error": "not found"
            }),
        ),
    }
}

fn read_request_body<R: Read>(reader: &mut R, headers: &HashMap<String, String>) -> Vec<u8> {
    let length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
        .min(64 * 1024);
    if length == 0 {
        return Vec::new();
    }

    let mut body = vec![0_u8; length];
    if reader.read_exact(&mut body).is_ok() {
        body
    } else {
        Vec::new()
    }
}

fn write_file_content_response(
    stream: &mut TcpStream,
    snapshot: &RemoteControlSnapshot,
    query: &str,
) {
    let Some(path) = query_param(query, "path") else {
        write_json_response(
            stream,
            400,
            &json!({"ok": false, "error": "path is required"}),
        );
        return;
    };
    let Some(workspace_path) = snapshot.workspace_path.as_deref() else {
        write_json_response(
            stream,
            400,
            &json!({"ok": false, "error": "workspace is not selected"}),
        );
        return;
    };
    match read_workspace_text_file(workspace_path, &path, 200_000) {
        Ok(file) => write_json_response(stream, 200, &file),
        Err(err) => {
            write_json_response(stream, 400, &json!({"ok": false, "error": err.to_string()}))
        }
    }
}

fn read_workspace_text_file(
    workspace_path: &str,
    requested_path: &str,
    max_bytes: u64,
) -> anyhow::Result<serde_json::Value> {
    let root = PathBuf::from(workspace_path).canonicalize()?;
    let rel = clean_remote_relative_path(requested_path)?;
    let target = root.join(&rel).canonicalize()?;
    anyhow::ensure!(target.starts_with(&root), "path is outside workspace");
    anyhow::ensure!(!target.is_dir(), "path points to a directory");
    let metadata = std::fs::metadata(&target)?;
    anyhow::ensure!(
        metadata.len() <= max_bytes,
        "file is too large for remote preview: {} bytes",
        metadata.len()
    );
    let bytes = std::fs::read(&target)?;
    let content = String::from_utf8(bytes)?;
    let rel_display = target
        .strip_prefix(&root)
        .unwrap_or(&target)
        .to_string_lossy()
        .replace('\\', "/");
    Ok(json!({
        "ok": true,
        "path": rel_display,
        "bytes": metadata.len(),
        "content": content
    }))
}

fn clean_remote_relative_path(value: &str) -> anyhow::Result<PathBuf> {
    let path = Path::new(value.trim());
    anyhow::ensure!(!value.trim().is_empty(), "path is empty");
    anyhow::ensure!(!path.is_absolute(), "absolute paths are not allowed");
    let mut cleaned = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => cleaned.push(part),
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("path traversal is not allowed");
            }
        }
    }
    anyhow::ensure!(!cleaned.as_os_str().is_empty(), "path is empty");
    Ok(cleaned)
}

fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        if name == key {
            Some(percent_decode(value))
        } else {
            None
        }
    })
}

fn handle_pair_device(
    stream: &mut TcpStream,
    body: &[u8],
    policy: &RemoteAccessPolicy,
    actions: Option<&Sender<RemoteControlAction>>,
) {
    let Some(actions) = actions else {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "remote action queue is unavailable"}),
        );
        return;
    };
    if policy.pairing_code.trim().is_empty() || policy.pairing_expires_at <= unix_timestamp() {
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "pairing code is not active"}),
        );
        return;
    }

    let payload = match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(payload) => payload,
        Err(_) => {
            write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
            return;
        }
    };
    let code = payload
        .get("pairing_code")
        .or_else(|| payload.get("code"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    if code != policy.pairing_code.trim().to_ascii_uppercase() {
        send_remote_audit(
            Some(actions),
            policy.audit,
            "pairing_denied",
            "bad pairing code",
        );
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "pairing code is invalid"}),
        );
        return;
    }

    let requested_agent_id = payload
        .get("agent_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    if !requested_agent_id.is_empty()
        && !policy.agent_id.trim().is_empty()
        && requested_agent_id != policy.agent_id.trim().to_ascii_uppercase()
    {
        send_remote_audit(
            Some(actions),
            policy.audit,
            "pairing_denied",
            "agent id mismatch",
        );
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "agent id does not match this host"}),
        );
        return;
    }

    let name = payload
        .get("device_name")
        .or_else(|| payload.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("Leetcode Client")
        .trim();
    let name = if name.is_empty() {
        "Leetcode Client".to_string()
    } else {
        name.chars().take(80).collect::<String>()
    };
    let created_at = unix_timestamp();
    let device = RemotePairedDevice {
        id: format!("device-{}", uuid::Uuid::new_v4().simple()),
        name,
        token: generate_remote_device_token(),
        role_view: payload_bool(&payload, "role_view", policy.default_role_view)
            && policy.default_role_view
            && policy.view,
        role_chat: payload_bool(&payload, "role_chat", policy.default_role_chat)
            && policy.default_role_chat
            && policy.chat,
        role_approve: payload_bool(&payload, "role_approve", policy.default_role_approve)
            && policy.default_role_approve
            && policy.approve,
        role_files: payload_bool(&payload, "role_files", policy.default_role_files)
            && policy.default_role_files
            && policy.files,
        role_run: payload_bool(&payload, "role_run", policy.default_role_run)
            && policy.default_role_run
            && policy.run,
        role_desktop: payload_bool(&payload, "role_desktop", policy.default_role_desktop)
            && policy.default_role_desktop
            && policy.desktop,
        created_at,
        expires_at: remote_device_expires_at(created_at, policy.default_device_ttl_days),
    };
    let response = json!({
        "ok": true,
        "device_id": device.id,
        "device_name": device.name,
        "device_token": device.token,
        "expires_at": device.expires_at,
        "roles": {
            "view": device.role_view,
            "chat": device.role_chat,
            "approve": device.role_approve,
            "files": device.role_files,
            "run": device.role_run,
            "desktop": device.role_desktop
        },
        "status": "paired"
    });
    if actions
        .send(RemoteControlAction::PairDevice(device))
        .is_err()
    {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "remote action queue is closed"}),
        );
        return;
    }
    write_json_response(stream, 201, &response);
}

fn payload_bool(payload: &Value, key: &str, fallback: bool) -> bool {
    payload
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(fallback)
}

fn remote_device_expires_at(created_at: u64, ttl_days: u32) -> u64 {
    if ttl_days == 0 {
        0
    } else {
        created_at.saturating_add(ttl_days as u64 * 86_400)
    }
}

fn handle_create_session(
    stream: &mut TcpStream,
    headers: &HashMap<String, String>,
    query: &str,
    token: &str,
    policy: &RemoteAccessPolicy,
    rate_limit: &Arc<Mutex<RemoteRateLimitState>>,
    actions: Option<&Sender<RemoteControlAction>>,
    client_ip: &str,
    path: &str,
) {
    let Some(subject) = authorized_subject(headers, query, token, policy) else {
        write_unauthorized(stream);
        return;
    };
    if !origin_is_allowed(headers, policy) {
        send_remote_audit(actions, policy.audit, "origin_denied", path);
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "origin is not allowed"}),
        );
        return;
    }
    if !subject.allows(RemoteAccessRole::View) {
        send_remote_audit(actions, policy.audit, "role_denied", path);
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "forbidden: role required: view"}),
        );
        return;
    }
    if let Some(error) = check_remote_rate_limits(rate_limit, policy, Some(&subject), client_ip) {
        send_remote_audit(
            actions,
            policy.audit,
            "rate_limited",
            &format!("{path}: {error}"),
        );
        write_json_response(stream, 429, &json!({"ok": false, "error": error}));
        return;
    }

    let Some(session) = issue_remote_session_token(&subject, token, policy) else {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "session token could not be signed"}),
        );
        return;
    };
    if let Some(device_id) = subject.device_id.as_ref() {
        if let Some(actions) = actions {
            let _ = actions.send(RemoteControlAction::DeviceSeen {
                device_id: device_id.clone(),
                seen_at: unix_timestamp(),
            });
        }
    }
    send_remote_audit(
        actions,
        policy.audit,
        "session_created",
        &format!(
            "{} expires_at={}",
            subject.session_subject_label(),
            session.expires_at
        ),
    );
    write_json_response(
        stream,
        201,
        &json!({
            "ok": true,
            "session_token": session.token,
            "expires_at": session.expires_at,
            "ttl_secs": session.ttl_secs,
            "subject": subject.session_subject_label(),
            "roles": {
                "view": subject.view,
                "chat": subject.chat,
                "approve": subject.approve,
                "files": subject.files,
                "run": subject.run,
                "desktop": subject.desktop
            }
        }),
    );
}

fn handle_submit_task(
    stream: &mut TcpStream,
    body: &[u8],
    actions: Option<&Sender<RemoteControlAction>>,
) {
    let Some(actions) = actions else {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "remote action queue is unavailable"}),
        );
        return;
    };
    let payload = match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(payload) => payload,
        Err(_) => {
            write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
            return;
        }
    };
    let Some(message) = payload.get("message").and_then(|value| value.as_str()) else {
        write_json_response(
            stream,
            400,
            &json!({"ok": false, "error": "message is required"}),
        );
        return;
    };
    let message = message.trim();
    if message.is_empty() {
        write_json_response(
            stream,
            400,
            &json!({"ok": false, "error": "message is empty"}),
        );
        return;
    }
    let message = compact_remote_message(message);
    let source = payload
        .get("source")
        .and_then(|value| value.as_str())
        .unwrap_or("remote-api")
        .trim();
    let task = RemoteSubmittedTask {
        id: format!("remote-{}", uuid::Uuid::new_v4().simple()),
        message,
        source: if source.is_empty() {
            "remote-api".to_string()
        } else {
            source.chars().take(80).collect()
        },
        created_at: unix_timestamp(),
    };
    let id = task.id.clone();
    if actions.send(RemoteControlAction::SubmitTask(task)).is_err() {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "remote action queue is closed"}),
        );
        return;
    }
    write_json_response(
        stream,
        202,
        &json!({"ok": true, "id": id, "status": "queued"}),
    );
}

fn handle_run_command(
    stream: &mut TcpStream,
    body: &[u8],
    actions: Option<&Sender<RemoteControlAction>>,
    audit_enabled: bool,
    shared_state: &RemoteControlSharedState,
    headers: &HashMap<String, String>,
    query: &str,
    token: &str,
    policy: &RemoteAccessPolicy,
) {
    let Some(actions) = actions else {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "remote action queue is unavailable"}),
        );
        return;
    };
    let payload = match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(payload) => payload,
        Err(_) => {
            write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
            return;
        }
    };
    let Some(command_id) = payload
        .get("id")
        .or_else(|| payload.get("command_id"))
        .and_then(|value| value.as_str())
    else {
        write_json_response(
            stream,
            400,
            &json!({"ok": false, "error": "command id is required"}),
        );
        return;
    };
    let command_id = command_id.trim();
    if command_id.is_empty() {
        write_json_response(
            stream,
            400,
            &json!({"ok": false, "error": "command id is empty"}),
        );
        return;
    }
    let confirmed = payload_bool(&payload, "confirmed", false);
    let snapshot = snapshot_or_default(shared_state);
    let Some(summary) = snapshot
        .remote_commands
        .iter()
        .find(|command| command.id == command_id)
        .cloned()
    else {
        send_remote_audit(
            Some(actions),
            audit_enabled,
            "remote_command_denied",
            &format!("unknown command {command_id}"),
        );
        write_json_response(
            stream,
            404,
            &json!({"ok": false, "error": "remote command is not available"}),
        );
        return;
    };
    if !summary.enabled {
        send_remote_audit(
            Some(actions),
            audit_enabled,
            "remote_command_denied",
            &format!("disabled command {command_id}"),
        );
        write_json_response(
            stream,
            409,
            &json!({"ok": false, "error": "remote command is disabled", "command": summary}),
        );
        return;
    }
    if summary.requires_confirmation && !confirmed {
        write_json_response(
            stream,
            409,
            &json!({
                "ok": false,
                "error": "command confirmation is required",
                "status": "preview_required",
                "command": summary
            }),
        );
        return;
    }
    let subject = if summary.requires_run || summary.requires_desktop || summary.requires_approval {
        let Some(subject) = authorized_subject(headers, query, token, policy) else {
            write_unauthorized(stream);
            return;
        };
        Some(subject)
    } else {
        None
    };
    if summary.requires_run
        && !subject
            .as_ref()
            .map(|subject| subject.allows(RemoteAccessRole::Run))
            .unwrap_or(false)
    {
        send_remote_audit(
            Some(actions),
            audit_enabled,
            "remote_command_denied",
            &format!("{command_id} requires run role"),
        );
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "forbidden: command requires run role"}),
        );
        return;
    }
    if summary.requires_desktop
        && !subject
            .as_ref()
            .map(|subject| subject.allows(RemoteAccessRole::Desktop))
            .unwrap_or(false)
    {
        send_remote_audit(
            Some(actions),
            audit_enabled,
            "remote_command_denied",
            &format!("{command_id} requires desktop role"),
        );
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "forbidden: command requires desktop role"}),
        );
        return;
    }
    if summary.requires_approval {
        let Some(subject) = subject.as_ref() else {
            write_unauthorized(stream);
            return;
        };
        if !subject.allows(RemoteAccessRole::Approve) {
            send_remote_audit(
                Some(actions),
                audit_enabled,
                "remote_command_denied",
                &format!("{command_id} requires approve role"),
            );
            write_json_response(
                stream,
                403,
                &json!({"ok": false, "error": "forbidden: command requires approve role"}),
            );
            return;
        }
    }
    let source = payload
        .get("source")
        .and_then(|value| value.as_str())
        .unwrap_or("remote-api")
        .trim();
    let request = RemoteCommandRequest {
        id: compact_remote_text(command_id, 200),
        source: if source.is_empty() {
            "remote-api".to_string()
        } else {
            source.chars().take(80).collect()
        },
        created_at: unix_timestamp(),
        confirmed,
    };
    let id = request.id.clone();
    if actions
        .send(RemoteControlAction::RunCommand(request))
        .is_err()
    {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "remote action queue is closed"}),
        );
        return;
    }
    write_json_response(
        stream,
        202,
        &json!({"ok": true, "id": id, "status": "queued"}),
    );
}

fn handle_binary_action(
    stream: &mut TcpStream,
    body: &[u8],
    actions: Option<&Sender<RemoteControlAction>>,
    run_gate: bool,
) {
    let Some(actions) = actions else {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "remote action queue is unavailable"}),
        );
        return;
    };
    let payload = match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(payload) => payload,
        Err(_) => {
            write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
            return;
        }
    };
    let action = payload
        .get("action")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let approved = match action.as_str() {
        "approve" | "approved" | "yes" | "confirm" => true,
        "deny" | "denied" | "no" | "reject" => false,
        _ => {
            write_json_response(
                stream,
                400,
                &json!({"ok": false, "error": "action must be approve or deny"}),
            );
            return;
        }
    };
    let event = if run_gate {
        RemoteControlAction::AnswerRunGate { approved }
    } else {
        RemoteControlAction::AnswerApproval { approved }
    };
    if actions.send(event).is_err() {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "remote action queue is closed"}),
        );
        return;
    }
    write_json_response(stream, 202, &json!({"ok": true, "status": "queued"}));
}

fn compact_remote_message(message: &str) -> String {
    const LIMIT: usize = 20_000;
    compact_remote_text(message, LIMIT)
}

fn compact_remote_text(message: &str, limit: usize) -> String {
    if message.chars().count() <= limit {
        return message.to_string();
    }
    let mut output = message.chars().take(limit).collect::<String>();
    output.push_str("\n\n[remote message truncated]");
    output
}

#[derive(Debug)]
struct RemoteRateLimitBucket {
    window_started_at: u64,
    count: u32,
}

impl Default for RemoteRateLimitBucket {
    fn default() -> Self {
        Self {
            window_started_at: unix_timestamp(),
            count: 0,
        }
    }
}

#[derive(Debug, Default)]
struct RemoteRateLimitState {
    buckets: HashMap<String, RemoteRateLimitBucket>,
}

#[derive(Clone, Debug)]
struct AuthorizedRemoteSubject {
    device_id: Option<String>,
    view: bool,
    chat: bool,
    approve: bool,
    files: bool,
    run: bool,
    desktop: bool,
}

impl AuthorizedRemoteSubject {
    fn admin(policy: &RemoteAccessPolicy) -> Self {
        Self {
            device_id: None,
            view: policy.view,
            chat: policy.chat,
            approve: policy.approve,
            files: policy.files,
            run: policy.run,
            desktop: policy.desktop,
        }
    }

    fn from_device(device: &RemoteTrustedDevice, policy: &RemoteAccessPolicy) -> Self {
        Self {
            device_id: Some(device.id.clone()),
            view: policy.view && device.role_view,
            chat: policy.chat && device.role_chat,
            approve: policy.approve && device.role_approve,
            files: policy.files && device.role_files,
            run: policy.run && device.role_run,
            desktop: policy.desktop && device.role_desktop,
        }
    }

    fn allows(&self, role: RemoteAccessRole) -> bool {
        match role {
            RemoteAccessRole::View => self.view,
            RemoteAccessRole::Chat => self.chat,
            RemoteAccessRole::Approve => self.approve,
            RemoteAccessRole::Files => self.files,
            RemoteAccessRole::Run => self.run,
            RemoteAccessRole::Desktop => self.desktop,
        }
    }

    fn session_subject_label(&self) -> String {
        self.device_id
            .as_ref()
            .map(|device_id| format!("device:{device_id}"))
            .unwrap_or_else(|| "admin".to_string())
    }
}

#[derive(Debug)]
struct RemoteSessionIssue {
    token: String,
    expires_at: u64,
    ttl_secs: u64,
}

fn authorize_or_write(
    stream: &mut TcpStream,
    headers: &HashMap<String, String>,
    query: &str,
    token: &str,
    policy: &RemoteAccessPolicy,
    rate_limit: &Arc<Mutex<RemoteRateLimitState>>,
    role: RemoteAccessRole,
    actions: Option<&Sender<RemoteControlAction>>,
    client_ip: &str,
    path: &str,
) -> bool {
    let Some(subject) = authorized_subject(headers, query, token, policy) else {
        write_unauthorized(stream);
        return false;
    };

    if !origin_is_allowed(headers, policy) {
        send_remote_audit(actions, policy.audit, "origin_denied", path);
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "origin is not allowed"}),
        );
        return false;
    }

    if !subject.allows(role) {
        send_remote_audit(
            actions,
            policy.audit,
            "role_denied",
            &format!("{path} requires {}", role.label()),
        );
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": format!("forbidden: role required: {}", role.label())}),
        );
        return false;
    }

    if let Some(error) = check_remote_rate_limits(rate_limit, policy, Some(&subject), client_ip) {
        send_remote_audit(
            actions,
            policy.audit,
            "rate_limited",
            &format!("{path}: {error}"),
        );
        write_json_response(stream, 429, &json!({"ok": false, "error": error}));
        return false;
    }

    if let Some(device_id) = subject.device_id {
        if let Some(actions) = actions {
            let _ = actions.send(RemoteControlAction::DeviceSeen {
                device_id,
                seen_at: unix_timestamp(),
            });
        }
    }

    true
}

fn origin_is_allowed(headers: &HashMap<String, String>, policy: &RemoteAccessPolicy) -> bool {
    let Some(origin) = headers.get("origin").map(|value| value.trim()) else {
        return true;
    };
    if origin.is_empty() {
        return true;
    }

    let origin = origin.trim_end_matches('/');
    if policy.allowed_origins.iter().any(|allowed| {
        let allowed = allowed.trim().trim_end_matches('/');
        allowed == "*" || allowed.eq_ignore_ascii_case(origin)
    }) {
        return true;
    }

    let Some(host) = headers.get("host").map(|value| value.trim()) else {
        return false;
    };
    let same_http = format!("http://{host}");
    let same_https = format!("https://{host}");
    origin.eq_ignore_ascii_case(&same_http) || origin.eq_ignore_ascii_case(&same_https)
}

fn check_remote_rate_limits(
    rate_limit: &Arc<Mutex<RemoteRateLimitState>>,
    policy: &RemoteAccessPolicy,
    subject: Option<&AuthorizedRemoteSubject>,
    client_ip: &str,
) -> Option<&'static str> {
    let mut limits = Vec::new();
    if policy.rate_limit_per_minute > 0 {
        limits.push((
            "global".to_string(),
            policy.rate_limit_per_minute,
            "remote API global rate limit exceeded",
        ));
    }
    if let Some(device_id) = subject.and_then(|subject| subject.device_id.as_deref()) {
        if policy.device_rate_limit_per_minute > 0 {
            limits.push((
                format!("device:{device_id}"),
                policy.device_rate_limit_per_minute,
                "remote API device rate limit exceeded",
            ));
        }
    }
    if !client_ip.trim().is_empty() && policy.ip_rate_limit_per_minute > 0 {
        limits.push((
            format!("ip:{}", client_ip.trim()),
            policy.ip_rate_limit_per_minute,
            "remote API IP rate limit exceeded",
        ));
    }
    if limits.is_empty() {
        return None;
    }

    let Ok(mut state) = rate_limit.lock() else {
        return None;
    };
    let now = unix_timestamp();

    for (key, _, _) in &limits {
        let bucket = state.buckets.entry(key.clone()).or_default();
        reset_remote_rate_limit_bucket(bucket, now);
    }

    if let Some((_, _, error)) = limits.iter().find(|(key, limit, _)| {
        state
            .buckets
            .get(key)
            .map(|bucket| bucket.count >= *limit)
            .unwrap_or(false)
    }) {
        return Some(*error);
    }

    for (key, _, _) in &limits {
        if let Some(bucket) = state.buckets.get_mut(key) {
            bucket.count = bucket.count.saturating_add(1);
        }
    }
    prune_remote_rate_limit_buckets(&mut state, now);
    None
}

fn reset_remote_rate_limit_bucket(bucket: &mut RemoteRateLimitBucket, now: u64) {
    if now.saturating_sub(bucket.window_started_at) >= 60 {
        bucket.window_started_at = now;
        bucket.count = 0;
    }
}

fn prune_remote_rate_limit_buckets(state: &mut RemoteRateLimitState, now: u64) {
    if state.buckets.len() <= 256 {
        return;
    }
    state
        .buckets
        .retain(|_, bucket| now.saturating_sub(bucket.window_started_at) < 180);
}

fn send_remote_audit(
    actions: Option<&Sender<RemoteControlAction>>,
    enabled: bool,
    event: &str,
    detail: &str,
) {
    if !enabled {
        return;
    }
    if let Some(actions) = actions {
        let _ = actions.send(RemoteControlAction::Audit(RemoteAuditEvent {
            event: event.to_string(),
            detail: compact_remote_text(detail, 1_000),
            created_at: unix_timestamp(),
        }));
    }
}

fn issue_remote_session_token(
    subject: &AuthorizedRemoteSubject,
    access_token: &str,
    policy: &RemoteAccessPolicy,
) -> Option<RemoteSessionIssue> {
    let now = unix_timestamp();
    let ttl_secs = policy.session_ttl_secs.clamp(60, 86_400);
    let expires_at = now.saturating_add(ttl_secs);
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let (kind, subject_id, secret) = if let Some(device_id) = subject.device_id.as_deref() {
        let device = policy
            .devices
            .iter()
            .find(|device| remote_trusted_device_is_active(device) && device.id == device_id)?;
        (
            "device",
            device.id.as_str(),
            remote_session_device_secret(access_token, &device.token),
        )
    } else {
        ("admin", "_", access_token.trim().to_string())
    };
    if secret.trim().is_empty() {
        return None;
    }
    let payload =
        format!("{REMOTE_SESSION_TOKEN_VERSION}.{kind}.{subject_id}.{now}.{expires_at}.{nonce}");
    let signature = sign_remote_session_payload(&secret, &payload)?;
    Some(RemoteSessionIssue {
        token: format!("{payload}.{signature}"),
        expires_at,
        ttl_secs,
    })
}

fn authorized_remote_session(
    session_token: &str,
    access_token: &str,
    policy: &RemoteAccessPolicy,
) -> Option<AuthorizedRemoteSubject> {
    let parts = session_token.trim().split('.').collect::<Vec<_>>();
    if parts.len() != 7 || parts.first().copied() != Some(REMOTE_SESSION_TOKEN_VERSION) {
        return None;
    }
    let kind = parts[1];
    let subject_id = parts[2];
    let issued_at = parts[3].parse::<u64>().ok()?;
    let expires_at = parts[4].parse::<u64>().ok()?;
    let signature = parts[6];
    let now = unix_timestamp();
    if issued_at > now.saturating_add(60) || expires_at <= now || expires_at <= issued_at {
        return None;
    }
    let payload = parts[..6].join(".");
    match kind {
        "admin" if subject_id == "_" => {
            verify_remote_session_signature(access_token.trim(), &payload, signature)
                .then(|| AuthorizedRemoteSubject::admin(policy))
        }
        "device" => {
            let device = policy.devices.iter().find(|device| {
                remote_trusted_device_is_active(device) && device.id == subject_id
            })?;
            if device.token_rotated_at > 0 && issued_at < device.token_rotated_at {
                return None;
            }
            let secret = remote_session_device_secret(access_token, &device.token);
            verify_remote_session_signature(&secret, &payload, signature)
                .then(|| AuthorizedRemoteSubject::from_device(device, policy))
        }
        _ => None,
    }
}

fn sign_remote_session_payload(secret: &str, payload: &str) -> Option<String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(payload.as_bytes());
    Some(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

fn verify_remote_session_signature(secret: &str, payload: &str, signature: &str) -> bool {
    let Ok(signature) = URL_SAFE_NO_PAD.decode(signature.as_bytes()) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload.as_bytes());
    mac.verify_slice(&signature).is_ok()
}

fn remote_session_device_secret(access_token: &str, device_token: &str) -> String {
    format!("{}:{}", access_token.trim(), device_token.trim())
}

fn split_target(target: &str) -> (&str, &str) {
    if let Some((path, query)) = target.split_once('?') {
        (path, query)
    } else {
        (target, "")
    }
}

fn authorized_subject(
    headers: &HashMap<String, String>,
    query: &str,
    token: &str,
    policy: &RemoteAccessPolicy,
) -> Option<AuthorizedRemoteSubject> {
    if let Some(session_token) = presented_remote_session_token(headers, query) {
        return authorized_remote_session(&session_token, token, policy);
    }
    if is_authorized(headers, query, token) {
        return Some(AuthorizedRemoteSubject::admin(policy));
    }
    let presented_token = presented_remote_token(headers, query)?;
    policy
        .devices
        .iter()
        .find(|device| remote_trusted_device_is_active(device) && device.token == presented_token)
        .map(|device| AuthorizedRemoteSubject::from_device(device, policy))
}

fn remote_trusted_device_is_active(device: &RemoteTrustedDevice) -> bool {
    !device.revoked && (device.expires_at == 0 || device.expires_at > unix_timestamp())
}

fn is_authorized(headers: &HashMap<String, String>, query: &str, token: &str) -> bool {
    presented_remote_token(headers, query).as_deref() == Some(token)
}

fn presented_remote_token(headers: &HashMap<String, String>, query: &str) -> Option<String> {
    if let Some(header) = headers.get("authorization") {
        let header = header.trim();
        if let Some(token) = header.strip_prefix("Bearer ") {
            return Some(token.trim().to_string());
        }
    }
    if let Some(header) = headers.get("x-leetcode-remote-token") {
        return Some(header.trim().to_string());
    }
    if let Some(header) = headers.get("x-leetcode-device-token") {
        return Some(header.trim().to_string());
    }
    query.split('&').find_map(|part| {
        let Some((name, value)) = part.split_once('=') else {
            return None;
        };
        (name == "token").then(|| percent_decode(value))
    })
}

fn presented_remote_session_token(
    headers: &HashMap<String, String>,
    query: &str,
) -> Option<String> {
    if let Some(header) = headers.get("authorization") {
        let header = header.trim();
        if let Some(token) = header.strip_prefix("Bearer ") {
            let token = token.trim();
            if token.starts_with(REMOTE_SESSION_TOKEN_VERSION) {
                return Some(token.to_string());
            }
        }
    }
    if let Some(header) = headers.get("x-leetcode-session-token") {
        let token = header.trim();
        if !token.is_empty() {
            return Some(token.to_string());
        }
    }
    query.split('&').find_map(|part| {
        let Some((name, value)) = part.split_once('=') else {
            return None;
        };
        matches!(name, "session" | "session_token").then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let input = value.as_bytes();
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        match input[index] {
            b'%' if index + 2 < input.len() => {
                let hex = [input[index + 1], input[index + 2]];
                if let Ok(text) = std::str::from_utf8(&hex) {
                    if let Ok(decoded) = u8::from_str_radix(text, 16) {
                        output.push(decoded);
                        index += 3;
                        continue;
                    }
                }
                output.push(input[index]);
                index += 1;
            }
            b'+' => {
                output.push(b' ');
                index += 1;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn snapshot_or_default(shared_state: &RemoteControlSharedState) -> RemoteControlSnapshot {
    shared_state
        .lock()
        .map(|state| state.clone())
        .unwrap_or_default()
}

fn write_json_response<T: Serialize>(stream: &mut TcpStream, status: u16, value: &T) {
    let body = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    let status_text = match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        429 => "Too Many Requests",
        503 => "Service Unavailable",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json; charset=utf-8\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, X-Leetcode-Remote-Token, X-Leetcode-Device-Token, Content-Type\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn write_html_response(stream: &mut TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn write_empty_response(stream: &mut TcpStream, status: u16, status_text: &str) {
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, X-Leetcode-Remote-Token, X-Leetcode-Device-Token, Content-Type\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let _ = stream.write_all(response.as_bytes());
}

fn write_unauthorized(stream: &mut TcpStream) {
    write_json_response(
        stream,
        401,
        &json!({
            "ok": false,
            "error": "unauthorized"
        }),
    );
}

fn write_sse_stream(
    stream: &mut TcpStream,
    shared_state: RemoteControlSharedState,
    stop: Arc<AtomicBool>,
) {
    let header = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n";
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }

    for _ in 0..300 {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let snapshot = snapshot_or_default(&shared_state);
        let Ok(data) = serde_json::to_string(&snapshot) else {
            break;
        };
        if stream
            .write_all(format!("event: state\ndata: {data}\n\n").as_bytes())
            .is_err()
        {
            break;
        }
        let _ = stream.flush();
        thread::sleep(Duration::from_secs(1));
    }
}

fn remote_dashboard_html() -> &'static str {
    r#"<!doctype html>
<html lang="ru">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<link rel="manifest" href="/manifest.webmanifest" />
<title>Leetcode Remote</title>
<style>
:root { color-scheme: dark; font-family: Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; background:#0b0f14; color:#e7eef6; }
body { margin:0; padding:20px; background:linear-gradient(180deg,#101722 0%,#090d12 100%); }
main { max-width:760px; margin:0 auto; display:grid; gap:16px; }
h1 { margin:0; font-size:28px; font-weight:650; }
p { margin:0; color:#8d99a8; }
.panel { border:1px solid #26303d; background:#121923; border-radius:12px; padding:16px; }
.row { display:flex; gap:10px; align-items:center; flex-wrap:wrap; }
input, textarea { flex:1; min-width:220px; background:#070b10; color:#e7eef6; border:1px solid #2d3a49; border-radius:8px; padding:12px; font-size:16px; }
textarea { width:100%; box-sizing:border-box; min-height:110px; resize:vertical; }
button { background:#2289a7; color:white; border:0; border-radius:8px; padding:12px 14px; font-size:15px; }
button.secondary { background:#26303d; color:#d7e1ec; }
.grid { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:10px; }
.metric { border-top:1px solid #26303d; padding-top:12px; }
.metric b { display:block; font-size:24px; margin-bottom:4px; }
.pending { display:none; border-left:3px solid #59c38d; padding-left:12px; margin-top:12px; }
.list { display:grid; gap:8px; margin-top:12px; }
.list button { text-align:left; background:#0b1118; color:#d7e1ec; border:1px solid #26303d; }
.viewer { max-height:420px; }
pre { white-space:pre-wrap; overflow:auto; background:#070b10; border:1px solid #26303d; border-radius:10px; padding:12px; }
@media (max-width: 620px) { body{padding:14px}.grid{grid-template-columns:1fr} }
</style>
</head>
<body>
<main>
  <section>
    <h1>Leetcode Remote</h1>
    <p>Лёгкая панель состояния локального агента: задачи, approvals, наблюдение и безопасные команды.</p>
  </section>
  <section class="panel">
    <h2>Задача агенту</h2>
    <p>Задача попадёт в Leetcode и будет ждать подтверждения перед запуском.</p>
    <textarea id="task" placeholder="Что сделать в выбранном проекте?"></textarea>
    <div class="row">
      <button onclick="submitTask()">Отправить задачу</button>
      <button class="secondary" onclick="document.getElementById('task').value=''">Очистить</button>
    </div>
    <p id="taskStatus"></p>
    <div id="runGate" class="pending">
      <p><b>План ждёт подтверждения</b></p>
      <p id="runGateSummary"></p>
      <div class="row">
        <button onclick="answer('/api/run-gate','approve')">Подтверждаю</button>
        <button class="secondary" onclick="answer('/api/run-gate','deny')">Отклонить</button>
      </div>
    </div>
    <div id="approval" class="pending">
      <p><b>Инструмент ждёт разрешения</b></p>
      <p id="approvalSummary"></p>
      <div class="row">
        <button onclick="answer('/api/approval','approve')">Разрешить</button>
        <button class="secondary" onclick="answer('/api/approval','deny')">Запретить</button>
      </div>
    </div>
  </section>
  <section class="panel">
    <div class="row">
      <input id="token" type="password" placeholder="Remote token" />
      <button onclick="connect()">Подключиться</button>
    </div>
    <p id="status">Введите token из Leetcode: Контроль -> Удалённое управление.</p>
  </section>
  <section class="panel">
    <div class="grid">
      <div class="metric"><b id="agent">ожидает</b><span>агент</span></div>
      <div class="metric"><b id="project">нет проекта</b><span>проект</span></div>
      <div class="metric"><b id="mode">-</b><span>режим</span></div>
      <div class="metric"><b id="updated">-</b><span>обновлено</span></div>
    </div>
  </section>
  <section class="panel">
    <h2>Наблюдение</h2>
    <p>Read-only обзор и безопасные remote-команды для текущего проекта.</p>
    <div class="row">
      <button onclick="loadToolLog()">Логи</button>
      <button onclick="loadHistory()">История</button>
      <button onclick="loadFiles()">Файлы</button>
      <button onclick="loadCommands()">Команды</button>
    </div>
    <div id="observerList" class="list"></div>
    <pre id="observerView" class="viewer">Выберите раздел.</pre>
  </section>
  <section class="panel">
    <pre id="json">{}</pre>
  </section>
</main>
<script>
const tokenInput = document.getElementById('token');
tokenInput.value = localStorage.getItem('leetcode_remote_token') || '';
let source = null;
function render(s) {
  document.getElementById('agent').textContent = s.agent_status || 'ожидает';
  document.getElementById('project').textContent = s.project_name || 'нет проекта';
  document.getElementById('mode').textContent = s.workspace_mode || '-';
  document.getElementById('updated').textContent = new Date((s.updated_at || 0) * 1000).toLocaleTimeString();
  document.getElementById('runGate').style.display = s.pending_run_gate_summary ? 'block' : 'none';
  document.getElementById('runGateSummary').textContent = s.pending_run_gate_summary || '';
  document.getElementById('approval').style.display = s.pending_approval_summary ? 'block' : 'none';
  document.getElementById('approvalSummary').textContent = s.pending_approval_summary || '';
  document.getElementById('json').textContent = JSON.stringify(s, null, 2);
}
async function postJson(path, payload) {
  const token = tokenInput.value.trim();
  const res = await fetch(path, {
    method: 'POST',
    headers: { Authorization:'Bearer ' + token, 'Content-Type':'application/json' },
    body: JSON.stringify(payload)
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || ('HTTP ' + res.status));
  return data;
}
async function getJson(path) {
  const token = tokenInput.value.trim();
  const res = await fetch(path, {headers:{Authorization:'Bearer ' + token}});
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || ('HTTP ' + res.status));
  return data;
}
function setObserver(items, text) {
  const list = document.getElementById('observerList');
  list.innerHTML = '';
  for (const item of items) list.appendChild(item);
  document.getElementById('observerView').textContent = text || '';
}
async function loadToolLog() {
  try {
    const data = await getJson('/api/tool-log');
    const items = (data.entries || []).map((entry) => {
      const button = document.createElement('button');
      button.textContent = entry.title || 'log';
      button.onclick = () => document.getElementById('observerView').textContent = (entry.title || '') + '\n\n' + (entry.content || '');
      return button;
    });
    setObserver(items, JSON.stringify(data.entries || [], null, 2));
  } catch (error) { setObserver([], 'Ошибка: ' + error.message); }
}
async function loadHistory() {
  try {
    const data = await getJson('/api/history');
    const items = (data.runs || []).map((run) => {
      const button = document.createElement('button');
      button.textContent = `${run.status} · ${run.provider}/${run.model} · ${new Date((run.started_at || 0) * 1000).toLocaleString()}`;
      button.onclick = () => openRun(run.id, run);
      return button;
    });
    setObserver(items, JSON.stringify(data.runs || [], null, 2));
  } catch (error) { setObserver([], 'Ошибка: ' + error.message); }
}
async function openRun(id, fallback) {
  if (!id) {
    document.getElementById('observerView').textContent = JSON.stringify(fallback || {}, null, 2);
    return;
  }
  try {
    const data = await getJson('/api/history/run?id=' + encodeURIComponent(id));
    document.getElementById('observerView').textContent = JSON.stringify(data.run || fallback || {}, null, 2);
  } catch (error) {
    document.getElementById('observerView').textContent = JSON.stringify(fallback || {}, null, 2) + '\n\nDetail error: ' + error.message;
  }
}
async function loadFiles() {
  try {
    const data = await getJson('/api/files');
    const items = (data.files || []).slice(0, 300).map((path) => {
      const button = document.createElement('button');
      button.textContent = path;
      button.onclick = () => openFile(path);
      return button;
    });
    setObserver(items, `Workspace: ${data.workspace || 'нет'}\nФайлов: ${(data.files || []).length}`);
  } catch (error) { setObserver([], 'Ошибка: ' + error.message); }
}
async function openFile(path) {
  if (path.endsWith('/')) {
    document.getElementById('observerView').textContent = 'Это каталог: ' + path;
    return;
  }
  try {
    const data = await getJson('/api/files/content?path=' + encodeURIComponent(path));
    document.getElementById('observerView').textContent = `${data.path} · ${data.bytes} bytes\n\n${data.content}`;
  } catch (error) { document.getElementById('observerView').textContent = 'Ошибка: ' + error.message; }
}
async function loadCommands() {
  try {
    const data = await getJson('/api/commands');
    const commands = data.commands || [];
    const items = commands.map((command) => {
      const button = document.createElement('button');
      button.textContent = `${command.enabled ? '▶' : '·'} ${command.title} · ${command.category}`;
      button.disabled = !command.enabled;
      button.onclick = () => runCommand(command.id);
      return button;
    });
    setObserver(items, JSON.stringify(commands, null, 2));
  } catch (error) { setObserver([], 'Ошибка: ' + error.message); }
}
async function runCommand(id) {
  try {
    const data = await postJson('/api/commands', {id, source:'pwa'});
    document.getElementById('taskStatus').textContent = 'Команда поставлена: ' + data.id;
    await loadCommands();
  } catch (error) {
    document.getElementById('taskStatus').textContent = 'Ошибка команды: ' + error.message;
  }
}
async function submitTask() {
  const text = document.getElementById('task').value.trim();
  if (!text) { document.getElementById('taskStatus').textContent = 'Введите задачу.'; return; }
  try {
    const data = await postJson('/api/tasks', {message:text, source:'pwa'});
    document.getElementById('taskStatus').textContent = 'Задача поставлена: ' + data.id;
    document.getElementById('task').value = '';
  } catch (error) {
    document.getElementById('taskStatus').textContent = 'Ошибка: ' + error.message;
  }
}
async function answer(path, action) {
  try {
    await postJson(path, {action});
    document.getElementById('taskStatus').textContent = 'Ответ отправлен: ' + action;
  } catch (error) {
    document.getElementById('taskStatus').textContent = 'Ошибка: ' + error.message;
  }
}
async function connect() {
  const token = tokenInput.value.trim();
  localStorage.setItem('leetcode_remote_token', token);
  document.getElementById('status').textContent = 'Подключаюсь...';
  if (source) source.close();
  const res = await fetch('/api/state', {headers:{Authorization:'Bearer ' + token}});
  if (!res.ok) { document.getElementById('status').textContent = 'Нет доступа: проверьте token.'; return; }
  render(await res.json());
  document.getElementById('status').textContent = 'Подключено. Состояние обновляется live.';
  source = new EventSource('/api/events?token=' + encodeURIComponent(token));
  source.addEventListener('state', (event) => render(JSON.parse(event.data)));
  source.onerror = () => { document.getElementById('status').textContent = 'Поток прерван. Нажмите подключиться снова.'; };
}
if (tokenInput.value) connect();
</script>
</body>
</html>"#
}

pub fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn generated_remote_tokens_are_non_empty_and_prefixed() {
        let token = generate_remote_access_token();
        let device_token = generate_remote_device_token();

        assert!(token.starts_with("lrt-"));
        assert!(token.len() > 16);
        assert!(device_token.starts_with("ldt-"));
        assert!(device_token.len() > 16);
    }

    #[test]
    fn accepts_bearer_header_and_query_token() {
        let mut headers = HashMap::new();
        headers.insert("authorization".to_string(), "Bearer lrt-test".to_string());

        assert!(is_authorized(&headers, "", "lrt-test"));
        assert!(is_authorized(&HashMap::new(), "token=lrt-test", "lrt-test"));
        assert!(is_authorized(
            &HashMap::new(),
            "token=lrt-%74est",
            "lrt-test"
        ));
        assert!(!is_authorized(&HashMap::new(), "token=bad", "lrt-test"));
    }

    #[test]
    fn remote_server_serves_health_and_protects_state() {
        let shared_state = new_remote_shared_state();
        update_remote_shared_state(
            &shared_state,
            RemoteControlSnapshot {
                project_name: "RemoteTest".to_string(),
                agent_status: "ожидает".to_string(),
                remote_enabled: true,
                ..RemoteControlSnapshot::default()
            },
        );
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy: RemoteAccessPolicy::default(),
                actions: None,
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let health = request(&addr, "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(health.starts_with("HTTP/1.1 200 OK"));
        assert!(health.contains("remote-control"));

        let denied = request(&addr, "GET /api/state HTTP/1.1\r\nHost: localhost\r\n\r\n");
        assert!(denied.starts_with("HTTP/1.1 401 Unauthorized"));

        let state = request(
            &addr,
            "GET /api/state HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\n\r\n",
        );
        assert!(state.starts_with("HTTP/1.1 200 OK"));
        assert!(state.contains("RemoteTest"));

        server.stop();
    }

    #[test]
    fn remote_server_enqueues_submitted_tasks() {
        let shared_state = new_remote_shared_state();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy: RemoteAccessPolicy::default(),
                actions: Some(tx),
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();
        let body = r#"{"message":"Проверь проект","source":"test"}"#;
        let http_request = format!(
            "POST /api/tasks HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let response = request(&addr, &http_request);
        assert!(response.starts_with("HTTP/1.1 202 Accepted"));
        let action = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("receives submitted task");
        match action {
            RemoteControlAction::SubmitTask(task) => {
                assert_eq!(task.message, "Проверь проект");
                assert_eq!(task.source, "test");
                assert!(task.id.starts_with("remote-"));
            }
            _ => panic!("expected task action"),
        }

        server.stop();
    }

    #[test]
    fn remote_server_reads_workspace_files_read_only() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file_path = temp.path().join("README.md");
        std::fs::write(&file_path, "hello remote").expect("writes file");
        let shared_state = new_remote_shared_state();
        update_remote_shared_state(
            &shared_state,
            RemoteControlSnapshot {
                workspace_path: Some(temp.path().to_string_lossy().to_string()),
                file_rows: vec!["README.md".to_string()],
                ..RemoteControlSnapshot::default()
            },
        );
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy: RemoteAccessPolicy::default(),
                actions: None,
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let response = request(
            &addr,
            "GET /api/files/content?path=README.md HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\n\r\n",
        );
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("hello remote"));

        let denied = request(
            &addr,
            "GET /api/files/content?path=..%2Fsecret.txt HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\n\r\n",
        );
        assert!(denied.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(denied.contains("path traversal"));

        server.stop();
    }

    #[test]
    fn remote_server_exposes_run_detail_by_id() {
        let shared_state = new_remote_shared_state();
        update_remote_shared_state(
            &shared_state,
            RemoteControlSnapshot {
                agent_history_tail: vec![RemoteRunSummary {
                    id: "run-1".to_string(),
                    status: "done".to_string(),
                    started_at: 1,
                    duration_ms: 1200,
                    provider: "OpenAI".to_string(),
                    model: "gpt-5.4".to_string(),
                    user_request: "test".to_string(),
                    final_response: Some("ok".to_string()),
                    changed_files: Vec::new(),
                    errors: Vec::new(),
                    tool_count: 1,
                }],
                agent_history_details: vec![json!({
                    "id": "run-1",
                    "timeline_steps": [{"kind": "tool", "summary": "checked"}]
                })],
                ..RemoteControlSnapshot::default()
            },
        );
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy: RemoteAccessPolicy::default(),
                actions: None,
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let response = request(
            &addr,
            "GET /api/history/run?id=run-1 HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\n\r\n",
        );
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("timeline_steps"));

        server.stop();
    }

    #[test]
    fn remote_server_lists_and_queues_commands() {
        let shared_state = new_remote_shared_state();
        update_remote_shared_state(
            &shared_state,
            RemoteControlSnapshot {
                remote_commands: vec![RemoteCommandSummary {
                    id: "git:status".to_string(),
                    title: "Git status".to_string(),
                    category: "Git".to_string(),
                    description: "Refresh Git status".to_string(),
                    enabled: true,
                    kind: "single".to_string(),
                    risk: "low".to_string(),
                    requires_confirmation: false,
                    requires_approval: false,
                    requires_run: false,
                    requires_desktop: false,
                    steps: Vec::new(),
                }],
                ..RemoteControlSnapshot::default()
            },
        );
        let (tx, rx) = std::sync::mpsc::channel();
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy: RemoteAccessPolicy::default(),
                actions: Some(tx),
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let list = request(
            &addr,
            "GET /api/commands HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\n\r\n",
        );
        assert!(list.starts_with("HTTP/1.1 200 OK"));
        assert!(list.contains("git:status"));

        let body = r#"{"id":"git:status","source":"test"}"#;
        let http_request = format!(
            "POST /api/commands HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let response = request(&addr, &http_request);
        assert!(response.starts_with("HTTP/1.1 202 Accepted"));
        let action = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("receives remote command");
        match action {
            RemoteControlAction::RunCommand(command) => {
                assert_eq!(command.id, "git:status");
                assert_eq!(command.source, "test");
            }
            _ => panic!("expected command action"),
        }

        server.stop();
    }

    #[test]
    fn remote_server_requires_command_confirmation_preview() {
        let shared_state = new_remote_shared_state();
        update_remote_shared_state(
            &shared_state,
            RemoteControlSnapshot {
                remote_commands: vec![RemoteCommandSummary {
                    id: "macro:release".to_string(),
                    title: "Release macro".to_string(),
                    category: "Macro".to_string(),
                    description: "Run release workflow".to_string(),
                    enabled: true,
                    kind: "macro".to_string(),
                    risk: "high".to_string(),
                    requires_confirmation: true,
                    requires_approval: false,
                    requires_run: true,
                    requires_desktop: false,
                    steps: vec![
                        "cargo test".to_string(),
                        "cargo build --release".to_string(),
                    ],
                }],
                ..RemoteControlSnapshot::default()
            },
        );
        let (tx, rx) = std::sync::mpsc::channel();
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy: RemoteAccessPolicy::default(),
                actions: Some(tx),
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let body = r#"{"id":"macro:release","source":"test"}"#;
        let http_request = format!(
            "POST /api/commands HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let response = request(&addr, &http_request);
        assert!(response.starts_with("HTTP/1.1 409 Conflict"));
        assert!(response.contains("preview_required"));
        assert!(rx.try_recv().is_err());

        server.stop();
    }

    #[test]
    fn remote_server_pairs_device_with_one_time_code() {
        let shared_state = new_remote_shared_state();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut policy = RemoteAccessPolicy::default();
        policy.agent_id = "LC-TEST-0000-0000".to_string();
        policy.pairing_code = "ABC-123".to_string();
        policy.pairing_expires_at = unix_timestamp() + 600;
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy,
                actions: Some(tx),
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();
        let body = r#"{"agent_id":"LC-TEST-0000-0000","pairing_code":"abc-123","device_name":"Laptop","role_view":true,"role_chat":true,"role_approve":true}"#;
        let http_request = format!(
            "POST /api/pair HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let response = request(&addr, &http_request);
        assert!(response.starts_with("HTTP/1.1 201 Created"));
        assert!(response.contains("device_token"));
        let action = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("receives paired device");
        match action {
            RemoteControlAction::PairDevice(device) => {
                assert_eq!(device.name, "Laptop");
                assert!(device.token.starts_with("ldt-"));
                assert!(device.role_view);
                assert!(device.role_chat);
                assert!(device.role_approve);
                assert!(!device.role_files);
            }
            _ => panic!("expected paired device action"),
        }

        server.stop();
    }

    #[test]
    fn remote_device_token_respects_device_roles() {
        let shared_state = new_remote_shared_state();
        let (tx, rx) = std::sync::mpsc::channel();
        let mut policy = RemoteAccessPolicy::default();
        policy.devices.push(RemoteTrustedDevice {
            id: "device-1".to_string(),
            name: "Laptop".to_string(),
            token: "ldt-test".to_string(),
            role_view: true,
            role_chat: false,
            role_approve: false,
            role_files: false,
            role_run: false,
            role_desktop: false,
            created_at: 1,
            last_seen_at: 1,
            expires_at: 0,
            token_rotated_at: 1,
            revoked_at: 0,
            revoked: false,
        });
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-admin".to_string(),
                policy,
                actions: Some(tx),
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let state = request(
            &addr,
            "GET /api/state HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer ldt-test\r\n\r\n",
        );
        assert!(state.starts_with("HTTP/1.1 200 OK"));
        let seen = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("receives device seen");
        assert!(matches!(
            seen,
            RemoteControlAction::DeviceSeen {
                ref device_id,
                ..
            } if device_id == "device-1"
        ));

        let files = request(
            &addr,
            "GET /api/files HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer ldt-test\r\n\r\n",
        );
        assert!(files.starts_with("HTTP/1.1 403 Forbidden"));
        assert!(files.contains("files"));

        server.stop();
    }

    #[test]
    fn remote_device_token_expires() {
        let shared_state = new_remote_shared_state();
        let mut policy = RemoteAccessPolicy::default();
        policy.devices.push(RemoteTrustedDevice {
            id: "device-expired".to_string(),
            name: "Old Laptop".to_string(),
            token: "ldt-expired".to_string(),
            role_view: true,
            role_chat: true,
            role_approve: true,
            role_files: false,
            role_run: false,
            role_desktop: false,
            created_at: 1,
            last_seen_at: 1,
            expires_at: unix_timestamp().saturating_sub(1),
            token_rotated_at: 1,
            revoked_at: 0,
            revoked: false,
        });
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-admin".to_string(),
                policy,
                actions: None,
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let state = request(
            &addr,
            "GET /api/state HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer ldt-expired\r\n\r\n",
        );

        assert!(state.starts_with("HTTP/1.1 401 Unauthorized"));
        server.stop();
    }

    #[test]
    fn remote_security_denies_missing_role_and_bad_origin() {
        let shared_state = new_remote_shared_state();
        let mut policy = RemoteAccessPolicy::default();
        policy.files = false;
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy,
                actions: None,
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let denied_role = request(
            &addr,
            "GET /api/files HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\n\r\n",
        );
        assert!(denied_role.starts_with("HTTP/1.1 403 Forbidden"));
        assert!(denied_role.contains("files"));

        let denied_origin = request(
            &addr,
            "GET /api/state HTTP/1.1\r\nHost: localhost\r\nOrigin: https://example.invalid\r\nAuthorization: Bearer lrt-test\r\n\r\n",
        );
        assert!(denied_origin.starts_with("HTTP/1.1 403 Forbidden"));
        assert!(denied_origin.contains("origin"));

        server.stop();
    }

    #[test]
    fn remote_security_denies_command_without_chat_role() {
        let shared_state = new_remote_shared_state();
        let mut policy = RemoteAccessPolicy::default();
        policy.chat = false;
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy,
                actions: None,
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();
        let body = r#"{"id":"git:status"}"#;
        let http_request = format!(
            "POST /api/commands HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let denied = request(&addr, &http_request);
        assert!(denied.starts_with("HTTP/1.1 403 Forbidden"));
        assert!(denied.contains("chat"));

        server.stop();
    }

    #[test]
    fn remote_security_denies_run_command_without_run_role() {
        let shared_state = new_remote_shared_state();
        update_remote_shared_state(
            &shared_state,
            RemoteControlSnapshot {
                remote_commands: vec![RemoteCommandSummary {
                    id: "project:check".to_string(),
                    title: "Project check".to_string(),
                    category: "Project".to_string(),
                    description: "Run project check".to_string(),
                    enabled: true,
                    kind: "project_command".to_string(),
                    risk: "medium".to_string(),
                    requires_confirmation: true,
                    requires_approval: false,
                    requires_run: true,
                    requires_desktop: false,
                    steps: vec!["cargo check".to_string()],
                }],
                ..RemoteControlSnapshot::default()
            },
        );
        let mut policy = RemoteAccessPolicy::default();
        policy.devices.push(RemoteTrustedDevice {
            id: "device-run-denied".to_string(),
            name: "Thin Client".to_string(),
            token: "device-run-token".to_string(),
            role_view: true,
            role_chat: true,
            role_approve: false,
            role_files: false,
            role_run: false,
            role_desktop: false,
            created_at: unix_timestamp(),
            last_seen_at: 0,
            expires_at: 0,
            token_rotated_at: 0,
            revoked_at: 0,
            revoked: false,
        });
        let (tx, rx) = std::sync::mpsc::channel();
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-admin".to_string(),
                policy,
                actions: Some(tx),
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();
        let body = r#"{"id":"project:check","confirmed":true}"#;
        let http_request = format!(
            "POST /api/commands HTTP/1.1\r\nHost: localhost\r\nX-Leetcode-Device-Token: device-run-token\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        let denied = request(&addr, &http_request);
        assert!(denied.starts_with("HTTP/1.1 403 Forbidden"));
        assert!(denied.contains("run role"));
        let actions = rx.try_iter().collect::<Vec<_>>();
        assert!(!actions
            .iter()
            .any(|action| matches!(action, RemoteControlAction::RunCommand(_))));

        server.stop();
    }

    #[test]
    fn remote_security_rate_limits_api_requests() {
        let shared_state = new_remote_shared_state();
        let mut policy = RemoteAccessPolicy::default();
        policy.rate_limit_per_minute = 1;
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy,
                actions: None,
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let first = request(
            &addr,
            "GET /api/state HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\n\r\n",
        );
        assert!(first.starts_with("HTTP/1.1 200 OK"));

        let second = request(
            &addr,
            "GET /api/state HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\n\r\n",
        );
        assert!(second.starts_with("HTTP/1.1 429 Too Many Requests"));

        server.stop();
    }

    #[test]
    fn remote_security_rate_limits_ip_requests() {
        let shared_state = new_remote_shared_state();
        let mut policy = RemoteAccessPolicy::default();
        policy.rate_limit_per_minute = 0;
        policy.ip_rate_limit_per_minute = 1;
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy,
                actions: None,
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let first = request(
            &addr,
            "GET /api/state HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\n\r\n",
        );
        assert!(first.starts_with("HTTP/1.1 200 OK"));

        let second = request(
            &addr,
            "GET /api/state HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\n\r\n",
        );
        assert!(second.starts_with("HTTP/1.1 429 Too Many Requests"));
        assert!(second.contains("IP rate limit"));

        server.stop();
    }

    #[test]
    fn remote_security_rate_limits_device_requests() {
        let shared_state = new_remote_shared_state();
        let mut policy = RemoteAccessPolicy::default();
        policy.rate_limit_per_minute = 0;
        policy.ip_rate_limit_per_minute = 0;
        policy.device_rate_limit_per_minute = 1;
        policy.devices.push(RemoteTrustedDevice {
            id: "device-1".to_string(),
            name: "Phone".to_string(),
            token: "device-token".to_string(),
            role_view: true,
            role_chat: true,
            role_approve: true,
            role_files: false,
            role_run: false,
            role_desktop: false,
            created_at: unix_timestamp(),
            last_seen_at: 0,
            expires_at: 0,
            token_rotated_at: 0,
            revoked_at: 0,
            revoked: false,
        });
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy,
                actions: None,
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let first = request(
            &addr,
            "GET /api/state HTTP/1.1\r\nHost: localhost\r\nX-Leetcode-Device-Token: device-token\r\n\r\n",
        );
        assert!(first.starts_with("HTTP/1.1 200 OK"));

        let second = request(
            &addr,
            "GET /api/state HTTP/1.1\r\nHost: localhost\r\nX-Leetcode-Device-Token: device-token\r\n\r\n",
        );
        assert!(second.starts_with("HTTP/1.1 429 Too Many Requests"));
        assert!(second.contains("device rate limit"));

        server.stop();
    }

    #[test]
    fn remote_signed_session_authorizes_state_and_rejects_bad_tokens() {
        let shared_state = new_remote_shared_state();
        let mut server = start_remote_control_server(
            RemoteControlServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                token: "lrt-test".to_string(),
                policy: RemoteAccessPolicy::default(),
                actions: None,
            },
            shared_state,
        )
        .expect("starts remote server");
        let addr = server.bind_addr().to_string();

        let session_response = request(
            &addr,
            "POST /api/sessions HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer lrt-test\r\nContent-Length: 0\r\n\r\n",
        );
        assert!(session_response.starts_with("HTTP/1.1 201 Created"));
        let session_body = response_body(&session_response);
        let session_json = serde_json::from_str::<Value>(session_body).expect("session json");
        let session_token = session_json
            .get("session_token")
            .and_then(Value::as_str)
            .expect("session token");
        assert!(session_token.starts_with(REMOTE_SESSION_TOKEN_VERSION));

        let state = request(
            &addr,
            &format!(
                "GET /api/state HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {session_token}\r\n\r\n"
            ),
        );
        assert!(state.starts_with("HTTP/1.1 200 OK"));

        let mut tampered = session_token.to_string();
        tampered.push('x');
        let denied = request(
            &addr,
            &format!(
                "GET /api/state HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {tampered}\r\n\r\n"
            ),
        );
        assert!(denied.starts_with("HTTP/1.1 401 Unauthorized"));

        let now = unix_timestamp();
        let expired_payload = format!(
            "{REMOTE_SESSION_TOKEN_VERSION}.admin._.{}.{}.expired",
            now.saturating_sub(120),
            now.saturating_sub(60)
        );
        let expired_signature =
            sign_remote_session_payload("lrt-test", &expired_payload).expect("signature");
        let expired_token = format!("{expired_payload}.{expired_signature}");
        let expired = request(
            &addr,
            &format!(
                "GET /api/state HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {expired_token}\r\n\r\n"
            ),
        );
        assert!(expired.starts_with("HTTP/1.1 401 Unauthorized"));

        server.stop();
    }

    #[test]
    fn percent_decode_preserves_utf8_paths() {
        assert_eq!(percent_decode("%D1%84%D0%B0%D0%B9%D0%BB.txt"), "файл.txt");
    }

    fn response_body(response: &str) -> &str {
        response.split("\r\n\r\n").nth(1).unwrap_or_default()
    }

    fn request(addr: &str, request: &str) -> String {
        let mut stream = TcpStream::connect(addr).expect("connects to remote server");
        stream
            .write_all(request.as_bytes())
            .expect("writes request");
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(count) => bytes.extend_from_slice(&chunk[..count]),
                Err(err) if !bytes.is_empty() => {
                    let _ = err;
                    break;
                }
                Err(err) => panic!("reads response: {err}"),
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}
