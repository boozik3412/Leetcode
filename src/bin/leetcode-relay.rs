#[path = "../relay.rs"]
mod relay;

use relay::{
    generate_relay_device_token, new_action, normalize_agent_id, RelayAction, RelayActionKind,
    RelayClientApprovalRequest, RelayClientCommandRequest, RelayClientRequest,
    RelayClientTaskRequest, RelayDevice, RelayHostPollReply, RelayHostPollRequest, RelayPairReply,
    RelayPairRequest, RelayQueuedReply, RelayStateReply, DEFAULT_RELAY_URL,
    RELAY_HOST_SESSION_TTL_SECS,
};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Default)]
struct RelayState {
    hosts: HashMap<String, HostRecord>,
}

struct HostRecord {
    host_token: String,
    state: Value,
    pairing_code: String,
    pairing_expires_at: u64,
    devices: HashMap<String, RelayDevice>,
    actions: VecDeque<RelayAction>,
    updated_at: u64,
}

impl HostRecord {
    fn new(host_token: String) -> Self {
        Self {
            host_token,
            state: json!({}),
            pairing_code: String::new(),
            pairing_expires_at: 0,
            devices: HashMap::new(),
            actions: VecDeque::new(),
            updated_at: unix_timestamp(),
        }
    }
}

#[derive(Clone, Copy)]
enum DeviceRole {
    View,
    Chat,
    Approve,
}

fn main() -> anyhow::Result<()> {
    let bind = bind_addr_from_args();
    let listener = TcpListener::bind(&bind)?;
    let state = Arc::new(Mutex::new(RelayState::default()));
    println!("Leetcode Relay listening on http://{bind}");

    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let state = Arc::clone(&state);
        thread::spawn(move || handle_connection(stream, state));
    }
    Ok(())
}

fn bind_addr_from_args() -> String {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--bind" {
            if let Some(value) = args.next() {
                return value;
            }
        } else if let Some(value) = arg.strip_prefix("--bind=") {
            return value.to_string();
        }
    }
    DEFAULT_RELAY_URL
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .to_string()
}

fn handle_connection(mut stream: TcpStream, state: Arc<Mutex<RelayState>>) {
    let Ok(request) = read_http_request(&mut stream) else {
        write_json_response(
            &mut stream,
            400,
            &json!({"ok": false, "error": "bad request"}),
        );
        return;
    };

    if request.method == "OPTIONS" {
        write_json_response(&mut stream, 204, &json!({}));
        return;
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") => handle_health(&mut stream, state),
        ("POST", "/api/hosts/poll") => handle_host_poll(&mut stream, &request.body, state),
        ("POST", "/api/clients/pair") => handle_client_pair(&mut stream, &request.body, state),
        ("POST", "/api/clients/state") => handle_client_state(&mut stream, &request.body, state),
        ("POST", "/api/clients/tasks") => handle_client_task(&mut stream, &request.body, state),
        ("POST", "/api/clients/commands") => {
            handle_client_command(&mut stream, &request.body, state)
        }
        ("POST", "/api/clients/run-gate") => {
            handle_client_approval(&mut stream, &request.body, state, true)
        }
        ("POST", "/api/clients/approval") => {
            handle_client_approval(&mut stream, &request.body, state, false)
        }
        _ => write_json_response(
            &mut stream,
            404,
            &json!({"ok": false, "error": "not found"}),
        ),
    }
}

fn handle_health(stream: &mut TcpStream, state: Arc<Mutex<RelayState>>) {
    let now = unix_timestamp();
    let state = state.lock().expect("relay state poisoned");
    let host_count = state.hosts.len();
    let online_hosts = state
        .hosts
        .values()
        .filter(|host| host_online(host, now))
        .count();
    let queued_actions: usize = state.hosts.values().map(|host| host.actions.len()).sum();
    write_json_response(
        stream,
        200,
        &json!({
            "ok": true,
            "service": "leetcode-relay",
            "host_count": host_count,
            "online_hosts": online_hosts,
            "queued_actions": queued_actions,
            "host_session_ttl_secs": RELAY_HOST_SESSION_TTL_SECS,
            "updated_at": now
        }),
    );
}

fn handle_host_poll(stream: &mut TcpStream, body: &[u8], state: Arc<Mutex<RelayState>>) {
    let Ok(request) = serde_json::from_slice::<RelayHostPollRequest>(body) else {
        write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
        return;
    };
    let agent_id = normalize_agent_id(&request.agent_id);
    if agent_id.is_empty() || request.host_token.trim().is_empty() {
        write_json_response(
            stream,
            400,
            &json!({"ok": false, "error": "agent_id and host_token are required"}),
        );
        return;
    }

    let mut state = state.lock().expect("relay state poisoned");
    let host = state
        .hosts
        .entry(agent_id)
        .or_insert_with(|| HostRecord::new(request.host_token.clone()));
    if host.host_token != request.host_token {
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "host token mismatch"}),
        );
        return;
    }

    host.state = request.state;
    host.pairing_code = request.pairing_code.trim().to_ascii_uppercase();
    host.pairing_expires_at = request.pairing_expires_at;
    host.updated_at = unix_timestamp();
    let actions = host.actions.drain(..).take(100).collect::<Vec<_>>();
    write_json_response(
        stream,
        200,
        &RelayHostPollReply {
            ok: true,
            actions,
            server_time: unix_timestamp(),
            next_poll_after_ms: 2_000,
            error: None,
        },
    );
}

fn handle_client_pair(stream: &mut TcpStream, body: &[u8], state: Arc<Mutex<RelayState>>) {
    let Ok(request) = serde_json::from_slice::<RelayPairRequest>(body) else {
        write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
        return;
    };
    let agent_id = normalize_agent_id(&request.agent_id);
    let code = request.pairing_code.trim().to_ascii_uppercase();
    let now = unix_timestamp();
    let mut state = state.lock().expect("relay state poisoned");
    let Some(host) = state.hosts.get_mut(&agent_id) else {
        write_json_response(
            stream,
            404,
            &json!({"ok": false, "error": "agent is not connected to relay"}),
        );
        return;
    };
    if !host_online(host, now) {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "agent is offline on relay"}),
        );
        return;
    }
    if host.pairing_code.is_empty() || host.pairing_expires_at <= now {
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "pairing code is not active"}),
        );
        return;
    }
    if code != host.pairing_code {
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "pairing code is invalid"}),
        );
        return;
    }

    let name = if request.device_name.trim().is_empty() {
        "Leetcode Client".to_string()
    } else {
        request.device_name.trim().chars().take(80).collect()
    };
    let device = RelayDevice {
        id: format!("relay-device-{}", uuid::Uuid::new_v4().simple()),
        name,
        token: generate_relay_device_token(),
        role_view: request.role_view,
        role_chat: request.role_chat,
        role_approve: request.role_approve,
        role_files: request.role_files,
        created_at: now,
        last_seen_at: now,
        revoked: false,
    };
    host.devices.insert(device.id.clone(), device.clone());
    host.actions.push_back(new_action(
        RelayActionKind::PairDevice {
            device: device.clone(),
        },
        now,
    ));
    host.pairing_code.clear();
    host.pairing_expires_at = 0;

    write_json_response(
        stream,
        201,
        &RelayPairReply {
            ok: true,
            device_id: device.id,
            device_name: device.name,
            device_token: device.token,
            error: None,
        },
    );
}

fn handle_client_state(stream: &mut TcpStream, body: &[u8], state: Arc<Mutex<RelayState>>) {
    let Ok(request) = serde_json::from_slice::<RelayClientRequest>(body) else {
        write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
        return;
    };
    let mut state = state.lock().expect("relay state poisoned");
    let Ok(host) = authorized_host_mut(
        &mut state,
        &request.agent_id,
        &request.device_token,
        DeviceRole::View,
    ) else {
        write_json_response(stream, 403, &json!({"ok": false, "error": "access denied"}));
        return;
    };
    let now = unix_timestamp();
    let online = host_online(host, now);
    if !online {
        write_json_response(
            stream,
            503,
            &RelayStateReply {
                ok: false,
                state: host.state.clone(),
                host_online: false,
                host_updated_at: host.updated_at,
                host_age_secs: now.saturating_sub(host.updated_at),
                queued_actions: host.actions.len(),
                error: Some("agent is offline on relay".to_string()),
            },
        );
        return;
    }
    queue_device_seen(host, &request.device_token);
    write_json_response(
        stream,
        200,
        &RelayStateReply {
            ok: true,
            state: host.state.clone(),
            host_online: online,
            host_updated_at: host.updated_at,
            host_age_secs: now.saturating_sub(host.updated_at),
            queued_actions: host.actions.len(),
            error: None,
        },
    );
}

fn handle_client_task(stream: &mut TcpStream, body: &[u8], state: Arc<Mutex<RelayState>>) {
    let Ok(request) = serde_json::from_slice::<RelayClientTaskRequest>(body) else {
        write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
        return;
    };
    if request.message.trim().is_empty() {
        write_json_response(
            stream,
            400,
            &json!({"ok": false, "error": "message is empty"}),
        );
        return;
    }
    let mut state = state.lock().expect("relay state poisoned");
    let Ok(host) = authorized_host_mut(
        &mut state,
        &request.agent_id,
        &request.device_token,
        DeviceRole::Chat,
    ) else {
        write_json_response(stream, 403, &json!({"ok": false, "error": "access denied"}));
        return;
    };
    if !host_online(host, unix_timestamp()) {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "agent is offline on relay"}),
        );
        return;
    }
    queue_device_seen(host, &request.device_token);
    let task_id = format!("relay-task-{}", uuid::Uuid::new_v4().simple());
    host.actions.push_back(new_action(
        RelayActionKind::SubmitTask {
            task_id: task_id.clone(),
            message: request.message,
            source: empty_as(&request.source, "leetcode-client-relay"),
        },
        unix_timestamp(),
    ));
    write_json_response(
        stream,
        202,
        &RelayQueuedReply {
            ok: true,
            id: Some(task_id),
            status: Some("queued".to_string()),
            error: None,
        },
    );
}

fn handle_client_command(stream: &mut TcpStream, body: &[u8], state: Arc<Mutex<RelayState>>) {
    let Ok(request) = serde_json::from_slice::<RelayClientCommandRequest>(body) else {
        write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
        return;
    };
    let mut state = state.lock().expect("relay state poisoned");
    let Ok(host) = authorized_host_mut(
        &mut state,
        &request.agent_id,
        &request.device_token,
        DeviceRole::Chat,
    ) else {
        write_json_response(stream, 403, &json!({"ok": false, "error": "access denied"}));
        return;
    };
    if !host_online(host, unix_timestamp()) {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "agent is offline on relay"}),
        );
        return;
    }
    queue_device_seen(host, &request.device_token);
    let action_id = format!("relay-command-{}", uuid::Uuid::new_v4().simple());
    host.actions.push_back(new_action(
        RelayActionKind::RunCommand {
            id: request.id,
            source: empty_as(&request.source, "leetcode-client-relay"),
        },
        unix_timestamp(),
    ));
    write_json_response(
        stream,
        202,
        &RelayQueuedReply {
            ok: true,
            id: Some(action_id),
            status: Some("queued".to_string()),
            error: None,
        },
    );
}

fn handle_client_approval(
    stream: &mut TcpStream,
    body: &[u8],
    state: Arc<Mutex<RelayState>>,
    run_gate: bool,
) {
    let Ok(request) = serde_json::from_slice::<RelayClientApprovalRequest>(body) else {
        write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
        return;
    };
    let mut state = state.lock().expect("relay state poisoned");
    let Ok(host) = authorized_host_mut(
        &mut state,
        &request.agent_id,
        &request.device_token,
        DeviceRole::Approve,
    ) else {
        write_json_response(stream, 403, &json!({"ok": false, "error": "access denied"}));
        return;
    };
    if !host_online(host, unix_timestamp()) {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "agent is offline on relay"}),
        );
        return;
    }
    queue_device_seen(host, &request.device_token);
    let action_id = format!("relay-approval-{}", uuid::Uuid::new_v4().simple());
    let kind = if run_gate {
        RelayActionKind::AnswerRunGate {
            approved: request.approved,
        }
    } else {
        RelayActionKind::AnswerApproval {
            approved: request.approved,
        }
    };
    host.actions.push_back(new_action(kind, unix_timestamp()));
    write_json_response(
        stream,
        202,
        &RelayQueuedReply {
            ok: true,
            id: Some(action_id),
            status: Some("queued".to_string()),
            error: None,
        },
    );
}

fn authorized_host_mut<'a>(
    state: &'a mut RelayState,
    agent_id: &str,
    token: &str,
    role: DeviceRole,
) -> Result<&'a mut HostRecord, ()> {
    let agent_id = normalize_agent_id(agent_id);
    let host = state.hosts.get_mut(&agent_id).ok_or(())?;
    let token = token.trim();
    if token.is_empty() {
        return Err(());
    }
    let allowed = host.devices.values().any(|device| {
        device.token == token
            && !device.revoked
            && match role {
                DeviceRole::View => device.role_view,
                DeviceRole::Chat => device.role_chat,
                DeviceRole::Approve => device.role_approve,
            }
    });
    if allowed {
        Ok(host)
    } else {
        Err(())
    }
}

fn queue_device_seen(host: &mut HostRecord, token: &str) {
    let now = unix_timestamp();
    let Some(device) = host
        .devices
        .values_mut()
        .find(|device| device.token == token && !device.revoked)
    else {
        return;
    };
    if now.saturating_sub(device.last_seen_at) < 60 {
        return;
    }
    device.last_seen_at = now;
    host.actions.push_back(new_action(
        RelayActionKind::DeviceSeen {
            device_id: device.id.clone(),
        },
        now,
    ));
}

fn host_online(host: &HostRecord, now: u64) -> bool {
    now.saturating_sub(host.updated_at) <= RELAY_HOST_SESSION_TTL_SECS
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> anyhow::Result<HttpRequest> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts
        .next()
        .unwrap_or("/")
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().unwrap_or(0);
        } else if let Some(value) = trimmed.strip_prefix("content-length:") {
            content_length = value.trim().parse::<usize>().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(HttpRequest { method, path, body })
}

fn write_json_response(stream: &mut TcpStream, status: u16, body: &impl serde::Serialize) {
    let reason = match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Error",
    };
    let body = if status == 204 {
        Vec::new()
    } else {
        serde_json::to_vec(body).unwrap_or_else(|_| b"{\"ok\":false}".to_vec())
    };
    let headers = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: Content-Type, Authorization\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(headers.as_bytes());
    let _ = stream.write_all(&body);
}

fn empty_as(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.trim().to_string()
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_bind_arg_defaults_to_default_url_host() {
        assert!(DEFAULT_RELAY_URL.contains("17990"));
    }
}
