use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
    AnswerRunGate { approved: bool },
    AnswerApproval { approved: bool },
    Audit(RemoteAuditEvent),
}

#[derive(Clone, Debug)]
pub struct RemoteAccessPolicy {
    pub view: bool,
    pub chat: bool,
    pub approve: bool,
    pub files: bool,
    pub allowed_origins: Vec<String>,
    pub rate_limit_per_minute: u32,
    pub audit: bool,
}

impl Default for RemoteAccessPolicy {
    fn default() -> Self {
        Self {
            view: true,
            chat: true,
            approve: true,
            files: true,
            allowed_origins: Vec::new(),
            rate_limit_per_minute: 120,
            audit: true,
        }
    }
}

impl RemoteAccessPolicy {
    fn allows(&self, role: RemoteAccessRole) -> bool {
        match role {
            RemoteAccessRole::View => self.view,
            RemoteAccessRole::Chat => self.chat,
            RemoteAccessRole::Approve => self.approve,
            RemoteAccessRole::Files => self.files,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum RemoteAccessRole {
    View,
    Chat,
    Approve,
    Files,
}

impl RemoteAccessRole {
    fn label(self) -> &'static str {
        match self {
            RemoteAccessRole::View => "view",
            RemoteAccessRole::Chat => "chat",
            RemoteAccessRole::Approve => "approve",
            RemoteAccessRole::Files => "files",
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
    pub remote_last_action: String,
    pub pending_run_gate_summary: Option<String>,
    pub pending_approval_summary: Option<String>,
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
            remote_last_action: String::new(),
            pending_run_gate_summary: None,
            pending_approval_summary: None,
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
                Ok((stream, _addr)) => {
                    let state = Arc::clone(&shared_state);
                    let token = token.clone();
                    let policy = policy.clone();
                    let actions = actions.clone();
                    let rate_limit = Arc::clone(&rate_limit);
                    let stop = Arc::clone(&server_stop);
                    let _ = thread::Builder::new()
                        .name("leetcode-remote-client".to_string())
                        .spawn(move || {
                            handle_client(stream, state, token, policy, actions, rate_limit, stop)
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
                path,
            ) {
                return;
            }
            handle_submit_task(&mut stream, &body, actions.as_ref());
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
struct RemoteRateLimitState {
    window_started_at: u64,
    count: u32,
}

impl Default for RemoteRateLimitState {
    fn default() -> Self {
        Self {
            window_started_at: unix_timestamp(),
            count: 0,
        }
    }
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
    path: &str,
) -> bool {
    if !is_authorized(headers, query, token) {
        write_unauthorized(stream);
        return false;
    }

    if !origin_is_allowed(headers, policy) {
        send_remote_audit(actions, policy.audit, "origin_denied", path);
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "origin is not allowed"}),
        );
        return false;
    }

    if !policy.allows(role) {
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

    if !check_remote_rate_limit(rate_limit, policy.rate_limit_per_minute) {
        send_remote_audit(actions, policy.audit, "rate_limited", path);
        write_json_response(
            stream,
            429,
            &json!({"ok": false, "error": "remote API rate limit exceeded"}),
        );
        return false;
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

fn check_remote_rate_limit(rate_limit: &Arc<Mutex<RemoteRateLimitState>>, per_minute: u32) -> bool {
    if per_minute == 0 {
        return true;
    }
    let Ok(mut state) = rate_limit.lock() else {
        return true;
    };
    let now = unix_timestamp();
    if now.saturating_sub(state.window_started_at) >= 60 {
        state.window_started_at = now;
        state.count = 0;
    }
    if state.count >= per_minute {
        return false;
    }
    state.count = state.count.saturating_add(1);
    true
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

fn split_target(target: &str) -> (&str, &str) {
    if let Some((path, query)) = target.split_once('?') {
        (path, query)
    } else {
        (target, "")
    }
}

fn is_authorized(headers: &HashMap<String, String>, query: &str, token: &str) -> bool {
    if let Some(header) = headers.get("authorization") {
        if header.trim() == format!("Bearer {token}") {
            return true;
        }
    }
    if let Some(header) = headers.get("x-leetcode-remote-token") {
        if header.trim() == token {
            return true;
        }
    }
    query.split('&').any(|part| {
        let Some((name, value)) = part.split_once('=') else {
            return false;
        };
        name == "token" && percent_decode(value) == token
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
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        429 => "Too Many Requests",
        503 => "Service Unavailable",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: application/json; charset=utf-8\r\nCache-Control: no-store\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, X-Leetcode-Remote-Token, Content-Type\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
        "HTTP/1.1 {status} {status_text}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Authorization, X-Leetcode-Remote-Token, Content-Type\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
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
    <p>Лёгкая панель состояния локального агента. Действия и approvals будут добавлены следующим этапом.</p>
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
    <p>Read-only обзор того, что делает агент: логи, история запусков и файлы текущего проекта.</p>
    <div class="row">
      <button onclick="loadToolLog()">Логи</button>
      <button onclick="loadHistory()">История</button>
      <button onclick="loadFiles()">Файлы</button>
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

        assert!(token.starts_with("lrt-"));
        assert!(token.len() > 16);
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
    fn percent_decode_preserves_utf8_paths() {
        assert_eq!(percent_decode("%D1%84%D0%B0%D0%B9%D0%BB.txt"), "файл.txt");
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
