#[path = "../relay.rs"]
mod relay;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use relay::{
    generate_relay_device_token, new_action, normalize_agent_id, RelayAction, RelayActionKind,
    RelayClientApprovalRequest, RelayClientCommandRequest, RelayClientRequest,
    RelayClientSessionReply, RelayClientSessionRequest, RelayClientTaskRequest, RelayDevice,
    RelayHostPairingDecisionRequest, RelayHostPollReply, RelayHostPollRequest,
    RelayPairDecisionReply, RelayPairReply, RelayPairRequest, RelayPairStatusRequest,
    RelayPairingRequest, RelayQueuedReply, RelayStateReply, DEFAULT_RELAY_URL,
    RELAY_CLIENT_SESSION_TTL_SECS, RELAY_HOST_SESSION_TTL_SECS,
};
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;
const RELAY_SESSION_TOKEN_VERSION: &str = "lrs1";

#[derive(Default)]
struct RelayState {
    hosts: HashMap<String, HostRecord>,
}

struct HostRecord {
    host_token: String,
    state: Value,
    pairing_code: String,
    pairing_expires_at: u64,
    pending_pairings: HashMap<String, PendingPairingRecord>,
    devices: HashMap<String, RelayDevice>,
    actions: VecDeque<RelayAction>,
    updated_at: u64,
}

struct PendingPairingRecord {
    request: RelayPairingRequest,
    status: PairingRequestStatus,
    device: Option<RelayDevice>,
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PairingRequestStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

impl PairingRequestStatus {
    fn as_str(self) -> &'static str {
        match self {
            PairingRequestStatus::Pending => "pending",
            PairingRequestStatus::Approved => "approved",
            PairingRequestStatus::Denied => "denied",
            PairingRequestStatus::Expired => "expired",
        }
    }
}

impl HostRecord {
    fn new(host_token: String) -> Self {
        Self {
            host_token,
            state: json!({}),
            pairing_code: String::new(),
            pairing_expires_at: 0,
            pending_pairings: HashMap::new(),
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
    Run,
    Desktop,
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
        ("GET", "/") => write_html_response(&mut stream, relay_mobile_pwa_html()),
        ("GET", "/manifest.webmanifest") => write_json_response(
            &mut stream,
            200,
            &json!({
                "name": "Leetcode Relay",
                "short_name": "Leetcode",
                "start_url": "/",
                "display": "standalone",
                "background_color": "#090d12",
                "theme_color": "#1f9fc4"
            }),
        ),
        ("GET", "/health") => handle_health(&mut stream, state),
        ("POST", "/api/hosts/poll") => handle_host_poll(&mut stream, &request.body, state),
        ("POST", "/api/hosts/pairing/decision") => {
            handle_host_pairing_decision(&mut stream, &request.body, state)
        }
        ("POST", "/api/clients/pair") => handle_client_pair(&mut stream, &request.body, state),
        ("POST", "/api/clients/pair/status") => {
            handle_client_pair_status(&mut stream, &request.body, state)
        }
        ("POST", "/api/clients/sessions") => {
            handle_client_session(&mut stream, &request.body, state)
        }
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
    sync_trusted_devices(host, request.trusted_devices);
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
    let pairing_request = RelayPairingRequest {
        request_id: format!("relay-pair-{}", uuid::Uuid::new_v4().simple()),
        device_name: name,
        role_view: request.role_view,
        role_chat: request.role_chat,
        role_approve: request.role_approve,
        role_files: request.role_files,
        role_run: request.role_run,
        role_desktop: request.role_desktop,
        created_at: now,
        expires_at: now + 10 * 60,
    };
    host.actions.push_back(new_action(
        RelayActionKind::PairingRequest {
            request: pairing_request.clone(),
        },
        now,
    ));
    host.pending_pairings.insert(
        pairing_request.request_id.clone(),
        PendingPairingRecord {
            request: pairing_request.clone(),
            status: PairingRequestStatus::Pending,
            device: None,
            error: None,
        },
    );
    host.pairing_code.clear();
    host.pairing_expires_at = 0;

    write_json_response(
        stream,
        201,
        &RelayPairReply {
            ok: true,
            device_id: String::new(),
            device_name: pairing_request.device_name,
            device_token: String::new(),
            status: "pending".to_string(),
            request_id: pairing_request.request_id,
            poll_after_ms: 2_000,
            error: None,
        },
    );
}

fn handle_client_pair_status(stream: &mut TcpStream, body: &[u8], state: Arc<Mutex<RelayState>>) {
    let Ok(request) = serde_json::from_slice::<RelayPairStatusRequest>(body) else {
        write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
        return;
    };
    let agent_id = normalize_agent_id(&request.agent_id);
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
    let request_id = request.request_id.trim().to_string();
    let Some(record) = host.pending_pairings.get_mut(&request_id) else {
        write_json_response(
            stream,
            404,
            &RelayPairReply {
                ok: false,
                device_id: String::new(),
                device_name: String::new(),
                device_token: String::new(),
                status: "unknown".to_string(),
                request_id: request.request_id,
                poll_after_ms: 2_000,
                error: Some("pairing request not found".to_string()),
            },
        );
        return;
    };
    if record.status == PairingRequestStatus::Pending && record.request.expires_at <= now {
        record.status = PairingRequestStatus::Expired;
        record.error = Some("pairing request expired".to_string());
    }
    let device = record.device.clone();
    write_json_response(
        stream,
        200,
        &RelayPairReply {
            ok: record.status == PairingRequestStatus::Approved,
            device_id: device
                .as_ref()
                .map(|device| device.id.clone())
                .unwrap_or_default(),
            device_name: device
                .as_ref()
                .map(|device| device.name.clone())
                .unwrap_or_else(|| record.request.device_name.clone()),
            device_token: device
                .as_ref()
                .map(|device| device.token.clone())
                .unwrap_or_default(),
            status: record.status.as_str().to_string(),
            request_id: record.request.request_id.clone(),
            poll_after_ms: 2_000,
            error: record.error.clone(),
        },
    );
}

fn handle_client_session(stream: &mut TcpStream, body: &[u8], state: Arc<Mutex<RelayState>>) {
    let Ok(request) = serde_json::from_slice::<RelayClientSessionRequest>(body) else {
        write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
        return;
    };
    let agent_id = normalize_agent_id(&request.agent_id);
    let device_token = request.device_token.trim();
    if agent_id.is_empty() || device_token.is_empty() {
        write_json_response(
            stream,
            400,
            &json!({"ok": false, "error": "agent_id and device_token are required"}),
        );
        return;
    }

    let state = state.lock().expect("relay state poisoned");
    let Some(host) = state.hosts.get(&agent_id) else {
        write_json_response(stream, 403, &json!({"ok": false, "error": "access denied"}));
        return;
    };
    let now = unix_timestamp();
    let Some(device) = host
        .devices
        .values()
        .find(|device| device.token == device_token && relay_device_is_active(device, now))
    else {
        write_json_response(stream, 403, &json!({"ok": false, "error": "access denied"}));
        return;
    };
    let Some(session_token) = issue_relay_session_token(&agent_id, host, device, now) else {
        write_json_response(
            stream,
            503,
            &json!({"ok": false, "error": "session token could not be signed"}),
        );
        return;
    };
    write_json_response(
        stream,
        201,
        &RelayClientSessionReply {
            ok: true,
            session_token,
            expires_at: now.saturating_add(RELAY_CLIENT_SESSION_TTL_SECS),
            ttl_secs: RELAY_CLIENT_SESSION_TTL_SECS,
            error: None,
        },
    );
}

fn handle_host_pairing_decision(
    stream: &mut TcpStream,
    body: &[u8],
    state: Arc<Mutex<RelayState>>,
) {
    let Ok(request) = serde_json::from_slice::<RelayHostPairingDecisionRequest>(body) else {
        write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
        return;
    };
    let agent_id = normalize_agent_id(&request.agent_id);
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
    if host.host_token != request.host_token {
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "host token mismatch"}),
        );
        return;
    }
    let request_id = request.request_id.trim().to_string();
    let mut device_to_insert: Option<RelayDevice> = None;
    let reply = {
        let Some(record) = host.pending_pairings.get_mut(&request_id) else {
            write_json_response(
                stream,
                404,
                &RelayPairDecisionReply {
                    ok: false,
                    status: "unknown".to_string(),
                    device: None,
                    error: Some("pairing request not found".to_string()),
                },
            );
            return;
        };
        if record.status != PairingRequestStatus::Pending {
            RelayPairDecisionReply {
                ok: record.status == PairingRequestStatus::Approved,
                status: record.status.as_str().to_string(),
                device: record.device.clone(),
                error: record.error.clone(),
            }
        } else {
            if record.request.expires_at <= now {
                record.status = PairingRequestStatus::Expired;
                record.error = Some("pairing request expired".to_string());
            } else if request.approved {
                let device = RelayDevice {
                    id: format!("relay-device-{}", uuid::Uuid::new_v4().simple()),
                    name: record.request.device_name.clone(),
                    token: generate_relay_device_token(),
                    role_view: request.role_view,
                    role_chat: request.role_chat,
                    role_approve: request.role_approve,
                    role_files: request.role_files,
                    role_run: request.role_run,
                    role_desktop: request.role_desktop,
                    created_at: now,
                    last_seen_at: now,
                    expires_at: request.device_expires_at,
                    token_rotated_at: now,
                    revoked_at: 0,
                    revoked: false,
                };
                record.status = PairingRequestStatus::Approved;
                record.device = Some(device.clone());
                record.error = None;
                device_to_insert = Some(device);
            } else {
                record.status = PairingRequestStatus::Denied;
                record.error = Some("pairing request denied by host".to_string());
            }
            RelayPairDecisionReply {
                ok: record.status == PairingRequestStatus::Approved,
                status: record.status.as_str().to_string(),
                device: record.device.clone(),
                error: record.error.clone(),
            }
        }
    };
    if let Some(device) = device_to_insert {
        host.devices.insert(device.id.clone(), device);
    }
    write_json_response(stream, 200, &reply);
}

fn handle_client_state(stream: &mut TcpStream, body: &[u8], state: Arc<Mutex<RelayState>>) {
    let Ok(request) = serde_json::from_slice::<RelayClientRequest>(body) else {
        write_json_response(stream, 400, &json!({"ok": false, "error": "invalid json"}));
        return;
    };
    let credential = relay_client_credential(&request.device_token, &request.session_token);
    let mut state = state.lock().expect("relay state poisoned");
    let Ok(host) = authorized_host_mut(&mut state, &request.agent_id, credential, DeviceRole::View)
    else {
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
    queue_device_seen(host, &request.agent_id, credential);
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
    let credential = relay_client_credential(&request.device_token, &request.session_token);
    if request.message.trim().is_empty() {
        write_json_response(
            stream,
            400,
            &json!({"ok": false, "error": "message is empty"}),
        );
        return;
    }
    let mut state = state.lock().expect("relay state poisoned");
    let Ok(host) = authorized_host_mut(&mut state, &request.agent_id, credential, DeviceRole::Chat)
    else {
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
    queue_device_seen(host, &request.agent_id, credential);
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
    let credential = relay_client_credential(&request.device_token, &request.session_token);
    let mut state = state.lock().expect("relay state poisoned");
    let Ok(host) = authorized_host_mut(&mut state, &request.agent_id, credential, DeviceRole::Chat)
    else {
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
    let command_id = request.id.trim().to_string();
    if command_id.is_empty() {
        write_json_response(
            stream,
            400,
            &json!({"ok": false, "error": "command id is empty"}),
        );
        return;
    }
    let Some(command_summary) = relay_remote_command_summary(host, &command_id) else {
        write_json_response(
            stream,
            404,
            &json!({"ok": false, "error": "command is not available"}),
        );
        return;
    };
    if !relay_command_bool(&command_summary, "enabled") {
        write_json_response(
            stream,
            409,
            &json!({"ok": false, "error": "command is disabled", "command": command_summary}),
        );
        return;
    }
    if relay_command_bool(&command_summary, "requires_confirmation") && !request.confirmed {
        write_json_response(
            stream,
            409,
            &json!({
                "ok": false,
                "error": "command confirmation is required",
                "status": "preview_required",
                "command": command_summary
            }),
        );
        return;
    }
    if relay_command_bool(&command_summary, "requires_run")
        && !relay_device_allows(host, &request.agent_id, credential, DeviceRole::Run)
    {
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "command requires run role"}),
        );
        return;
    }
    if relay_command_bool(&command_summary, "requires_desktop")
        && !relay_device_allows(host, &request.agent_id, credential, DeviceRole::Desktop)
    {
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "command requires desktop role"}),
        );
        return;
    }
    if relay_command_bool(&command_summary, "requires_approval")
        && !relay_device_allows(host, &request.agent_id, credential, DeviceRole::Approve)
    {
        write_json_response(
            stream,
            403,
            &json!({"ok": false, "error": "command requires approve role"}),
        );
        return;
    }
    queue_device_seen(host, &request.agent_id, credential);
    let action_id = format!("relay-command-{}", uuid::Uuid::new_v4().simple());
    host.actions.push_back(new_action(
        RelayActionKind::RunCommand {
            id: command_id,
            source: empty_as(&request.source, "leetcode-client-relay"),
            confirmed: request.confirmed,
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
    let credential = relay_client_credential(&request.device_token, &request.session_token);
    let mut state = state.lock().expect("relay state poisoned");
    let Ok(host) = authorized_host_mut(
        &mut state,
        &request.agent_id,
        credential,
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
    queue_device_seen(host, &request.agent_id, credential);
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
    credential: &str,
    role: DeviceRole,
) -> Result<&'a mut HostRecord, ()> {
    let agent_id = normalize_agent_id(agent_id);
    let host = state.hosts.get_mut(&agent_id).ok_or(())?;
    if credential.trim().is_empty() {
        return Err(());
    }
    if relay_authorized_device_id(host, &agent_id, credential, role).is_some() {
        Ok(host)
    } else {
        Err(())
    }
}

fn relay_remote_command_summary(host: &HostRecord, command_id: &str) -> Option<Value> {
    host.state
        .get("remote_commands")
        .and_then(Value::as_array)?
        .iter()
        .find(|command| {
            command
                .get("id")
                .and_then(Value::as_str)
                .map(|id| id == command_id)
                .unwrap_or(false)
        })
        .cloned()
}

fn relay_command_bool(command: &Value, key: &str) -> bool {
    command.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn relay_device_allows(
    host: &HostRecord,
    agent_id: &str,
    credential: &str,
    role: DeviceRole,
) -> bool {
    relay_authorized_device_id(host, agent_id, credential, role).is_some()
}

fn queue_device_seen(host: &mut HostRecord, agent_id: &str, credential: &str) {
    let now = unix_timestamp();
    let Some(device_id) = relay_authorized_device_id(host, agent_id, credential, DeviceRole::View)
    else {
        return;
    };
    let Some(device) = host.devices.get_mut(&device_id) else {
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

fn relay_client_credential<'a>(device_token: &'a str, session_token: &'a str) -> &'a str {
    if !session_token.trim().is_empty() {
        session_token.trim()
    } else {
        device_token.trim()
    }
}

fn relay_authorized_device_id(
    host: &HostRecord,
    agent_id: &str,
    credential: &str,
    role: DeviceRole,
) -> Option<String> {
    let credential = credential.trim();
    if credential.is_empty() {
        return None;
    }
    let now = unix_timestamp();
    for device in host.devices.values() {
        if device.token == credential
            && relay_device_is_active(device, now)
            && relay_device_has_role(device, role)
        {
            return Some(device.id.clone());
        }
    }
    let device_id = relay_session_device_id(credential, agent_id, host, now)?;
    let device = host.devices.get(&device_id)?;
    (relay_device_is_active(device, now) && relay_device_has_role(device, role))
        .then_some(device_id)
}

fn relay_device_has_role(device: &RelayDevice, role: DeviceRole) -> bool {
    match role {
        DeviceRole::View => device.role_view,
        DeviceRole::Chat => device.role_chat,
        DeviceRole::Approve => device.role_approve,
        DeviceRole::Run => device.role_run,
        DeviceRole::Desktop => device.role_desktop,
    }
}

fn issue_relay_session_token(
    agent_id: &str,
    host: &HostRecord,
    device: &RelayDevice,
    now: u64,
) -> Option<String> {
    let expires_at = now.saturating_add(RELAY_CLIENT_SESSION_TTL_SECS);
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let payload = format!(
        "{}.{}.{}.{}.{}.{}",
        RELAY_SESSION_TOKEN_VERSION,
        normalize_agent_id(agent_id),
        device.id,
        now,
        expires_at,
        nonce
    );
    let secret = relay_session_secret(&host.host_token, &device.token);
    let signature = sign_relay_session_payload(&secret, &payload)?;
    Some(format!("{payload}.{signature}"))
}

fn relay_session_device_id(
    session_token: &str,
    agent_id: &str,
    host: &HostRecord,
    now: u64,
) -> Option<String> {
    let parts = session_token.trim().split('.').collect::<Vec<_>>();
    if parts.len() != 7 || parts.first().copied() != Some(RELAY_SESSION_TOKEN_VERSION) {
        return None;
    }
    let token_agent_id = normalize_agent_id(parts[1]);
    if token_agent_id != normalize_agent_id(agent_id) {
        return None;
    }
    let device_id = parts[2];
    let issued_at = parts[3].parse::<u64>().ok()?;
    let expires_at = parts[4].parse::<u64>().ok()?;
    if issued_at > now.saturating_add(60) || expires_at <= now || expires_at <= issued_at {
        return None;
    }
    let device = host.devices.get(device_id)?;
    if device.token_rotated_at > 0 && issued_at < device.token_rotated_at {
        return None;
    }
    let payload = parts[..6].join(".");
    let secret = relay_session_secret(&host.host_token, &device.token);
    verify_relay_session_signature(&secret, &payload, parts[6]).then(|| device_id.to_string())
}

fn relay_session_secret(host_token: &str, device_token: &str) -> String {
    format!("{}:{}", host_token.trim(), device_token.trim())
}

fn sign_relay_session_payload(secret: &str, payload: &str) -> Option<String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(payload.as_bytes());
    Some(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
}

fn verify_relay_session_signature(secret: &str, payload: &str, signature: &str) -> bool {
    let Ok(signature) = URL_SAFE_NO_PAD.decode(signature.as_bytes()) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload.as_bytes());
    mac.verify_slice(&signature).is_ok()
}

fn sync_trusted_devices(host: &mut HostRecord, devices: Vec<RelayDevice>) {
    for mut device in devices {
        if device.id.trim().is_empty() || device.token.trim().is_empty() {
            continue;
        }
        device.id = device.id.trim().chars().take(120).collect();
        device.name = empty_as(&device.name, "Устройство")
            .chars()
            .take(120)
            .collect();
        device.token = device.token.trim().to_string();
        host.devices.insert(device.id.clone(), device);
    }
}

fn relay_device_is_active(device: &RelayDevice, now: u64) -> bool {
    !device.revoked && (device.expires_at == 0 || device.expires_at > now)
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
        503 => "Service Unavailable",
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

fn write_html_response(stream: &mut TcpStream, body: &str) {
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(headers.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}

fn empty_as(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value.trim().to_string()
    }
}

fn relay_mobile_pwa_html() -> &'static str {
    r##"<!doctype html>
<html lang="ru">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
<meta name="apple-mobile-web-app-capable" content="yes" />
<meta name="apple-mobile-web-app-title" content="Leetcode" />
<meta name="theme-color" content="#0b1118" />
<link rel="manifest" href="/manifest.webmanifest" />
<title>Leetcode Relay</title>
<style>
:root{color-scheme:dark;font-family:Inter,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;background:#090d12;color:#e7eef6}
*{box-sizing:border-box} body{margin:0;min-height:100vh;background:#090d12;color:#e7eef6}
main{width:min(760px,100%);margin:0 auto;padding:18px 14px calc(24px + env(safe-area-inset-bottom));display:grid;gap:14px}
.top{display:flex;align-items:flex-start;justify-content:space-between;gap:12px;padding:8px 2px 2px}
h1{margin:0;font-size:25px;line-height:1.05;font-weight:680;letter-spacing:0}
h2{margin:0 0 8px;font-size:16px;font-weight:650}.muted{color:#8d99a8}.small{font-size:12px}
.status{display:inline-flex;align-items:center;gap:8px;border:1px solid #26303d;background:#121923;border-radius:999px;padding:7px 10px;font-size:13px;color:#cbd6e2;white-space:nowrap}
.dot{width:8px;height:8px;border-radius:50%;background:#8d99a8}.dot.online{background:#59c38d}.dot.offline{background:#d66969}
.panel{border-top:1px solid #26303d;padding:14px 0}.panel:first-of-type{border-top:0}
.connect{display:grid;grid-template-columns:1fr 1fr;gap:8px}
input,textarea{width:100%;background:#070b10;color:#e7eef6;border:1px solid #2d3a49;border-radius:10px;padding:12px;font:inherit;font-size:16px;outline:none}
textarea{min-height:118px;resize:vertical;line-height:1.35}
input:focus,textarea:focus{border-color:#2aa7ca;box-shadow:0 0 0 2px rgba(42,167,202,.14)}
.actions{display:flex;gap:8px;flex-wrap:wrap;margin-top:10px}
button{border:1px solid #2d3a49;background:#1c2633;color:#e7eef6;border-radius:10px;padding:11px 13px;font:inherit;font-size:15px}
button.primary{background:#2289a7;border-color:#2ba8ca;color:white}button.good{background:#1f7d57;border-color:#2da875}button.warn{background:#56333a;border-color:#8b4651}
button:disabled{opacity:.45}.metrics{display:grid;grid-template-columns:repeat(3,1fr);gap:10px}
.metric{min-width:0}.metric b{display:block;font-size:22px;line-height:1.1;font-weight:640;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.metric span{display:block;margin-top:3px;color:#8d99a8;font-size:12px}
.taskbar{position:sticky;bottom:0;background:linear-gradient(180deg,rgba(9,13,18,0),#090d12 16px);padding-top:18px}
.pending{display:none;margin-top:10px;border-left:3px solid #59c38d;padding:8px 0 8px 12px}
.diagnostics{white-space:pre-wrap;margin-top:12px;border-top:1px solid #26303d;padding-top:10px;color:#cbd6e2;font-size:12px;line-height:1.45}
.list{display:grid;gap:8px}.item{display:block;width:100%;text-align:left;background:#111821;border:1px solid #26303d;border-radius:10px;padding:10px;color:#d9e3ed}
.tabs{display:flex;gap:8px;overflow:auto;padding-bottom:2px}.tab{white-space:nowrap;background:transparent}.tab.active{background:#1f6f87;border-color:#2aa7ca}
pre{white-space:pre-wrap;word-break:break-word;margin:10px 0 0;background:#05080c;border:1px solid #26303d;border-radius:10px;padding:10px;max-height:280px;overflow:auto;color:#cfd9e5}
@media(max-width:620px){main{padding-left:12px;padding-right:12px}.connect{grid-template-columns:1fr}.metrics{grid-template-columns:1fr 1fr}h1{font-size:23px}.top{align-items:center}}
</style>
</head>
<body>
<main>
  <section class="top">
    <div>
      <h1>Leetcode Relay</h1>
      <p class="muted small">мобильный доступ к агенту по Agent ID</p>
    </div>
    <div class="status"><span id="onlineDot" class="dot"></span><span id="onlineText">не подключено</span></div>
  </section>

  <section class="panel">
    <h2>Подключение</h2>
    <div class="connect">
      <input id="agentId" placeholder="Agent ID" autocomplete="off" autocapitalize="characters" />
      <input id="pairingCode" placeholder="Pairing code" autocomplete="one-time-code" autocapitalize="characters" />
      <input id="deviceName" placeholder="Имя устройства" />
      <input id="deviceToken" type="password" placeholder="Device token появится после пары" />
    </div>
    <div class="actions">
      <button class="primary" onclick="pairDevice()">Подключить по коду</button>
      <button onclick="checkPairStatus()">Проверить подтверждение</button>
      <button onclick="saveConnection()">Сохранить</button>
      <button onclick="loadState()">Обновить</button>
    </div>
    <p id="status" class="muted small">Откройте ссылку из Leetcode или введите Agent ID и pairing code.</p>
  </section>

  <section class="panel">
    <div class="metrics">
      <div class="metric"><b id="agentStatus">-</b><span>агент</span></div>
      <div class="metric"><b id="projectName">-</b><span>проект</span></div>
      <div class="metric"><b id="snapshotAge">-</b><span>snapshot</span></div>
      <div class="metric"><b id="modelName">-</b><span>модель</span></div>
      <div class="metric"><b id="queueCount">0</b><span>очередь relay</span></div>
      <div class="metric"><b id="runCount">0</b><span>запуски</span></div>
    </div>
    <div id="diagnostics" class="diagnostics">Диагностика появится после подключения.</div>
    <div id="runGate" class="pending">
      <b>План ждёт подтверждения</b>
      <p id="runGateSummary" class="muted"></p>
      <div class="actions"><button class="good" onclick="answer('/api/clients/run-gate',true)">Подтверждаю</button><button class="warn" onclick="answer('/api/clients/run-gate',false)">Отклонить</button></div>
    </div>
    <div id="approval" class="pending">
      <b>Инструмент ждёт разрешения</b>
      <p id="approvalSummary" class="muted"></p>
      <div class="actions"><button class="good" onclick="answer('/api/clients/approval',true)">Разрешить</button><button class="warn" onclick="answer('/api/clients/approval',false)">Запретить</button></div>
    </div>
  </section>

  <section class="panel">
    <div class="tabs">
      <button id="tab-runs" class="tab active" onclick="showTab('runs')">Запуски</button>
      <button id="tab-commands" class="tab" onclick="showTab('commands')">Команды</button>
      <button id="tab-log" class="tab" onclick="showTab('log')">Логи</button>
    </div>
    <div id="list" class="list"></div>
    <pre id="details">Пока нет данных.</pre>
  </section>

  <section class="taskbar">
    <textarea id="task" placeholder="Что сделать агенту?"></textarea>
    <div class="actions">
      <button class="primary" onclick="submitTask()">Отправить</button>
      <button onclick="document.getElementById('task').value=''">Очистить</button>
    </div>
  </section>
</main>
<script>
const $ = (id) => document.getElementById(id);
let lastState = null;
let activeTab = 'runs';
let timer = null;
let relaySession = {token:'', expiresAt:0};
function params(){return new URLSearchParams(location.search)}
function loadConnection(){
  $('agentId').value = params().get('agent_id') || localStorage.getItem('leetcode_relay_agent_id') || '';
  $('pairingCode').value = params().get('pairing_code') || '';
  $('deviceName').value = params().get('device_name') || localStorage.getItem('leetcode_relay_device_name') || (navigator.platform || 'iPhone');
  $('deviceToken').value = localStorage.getItem('leetcode_relay_device_token_' + $('agentId').value.trim().toUpperCase()) || '';
}
function pendingKey(){
  return 'leetcode_relay_pair_request_' + $('agentId').value.trim().toUpperCase();
}
function currentPending(){
  return localStorage.getItem(pendingKey()) || '';
}
function setPendingPairRequest(requestId){
  if(requestId) localStorage.setItem(pendingKey(), requestId);
  else localStorage.removeItem(pendingKey());
}
function saveConnection(){
  const agent = $('agentId').value.trim().toUpperCase();
  localStorage.setItem('leetcode_relay_agent_id', agent);
  localStorage.setItem('leetcode_relay_device_name', $('deviceName').value.trim() || 'iPhone');
  if ($('deviceToken').value.trim()) localStorage.setItem('leetcode_relay_device_token_' + agent, $('deviceToken').value.trim());
  setStatus('сохранено');
}
function requestBase(){
  return {agent_id:$('agentId').value.trim().toUpperCase(), device_token:$('deviceToken').value.trim()};
}
async function authedRequestBase(){
  const base = requestBase();
  if(!base.agent_id || !base.device_token) return base;
  const now = Math.floor(Date.now()/1000);
  if(relaySession.token && relaySession.expiresAt > now + 30){
    return {...base, session_token: relaySession.token};
  }
  try{
    const session = await relayPost('/api/clients/sessions', base);
    if(session.session_token){
      relaySession = {token: session.session_token, expiresAt: session.expires_at || (now + 600)};
      return {...base, session_token: relaySession.token};
    }
  }catch(_error){}
  return base;
}
async function relayPost(path, payload, allowOfflineState=false){
  const res = await fetch(path,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(payload)});
  const data = await res.json().catch(()=>({}));
  if (!res.ok && !(allowOfflineState && data && Object.prototype.hasOwnProperty.call(data,'host_online'))) throw new Error(data.error || ('HTTP ' + res.status));
  return data;
}
function setStatus(text, online){
  $('status').textContent = text;
  $('onlineText').textContent = text;
  $('onlineDot').className = 'dot ' + (online === true ? 'online' : online === false ? 'offline' : '');
}
async function pairDevice(){
  try{
    const body = {
      agent_id:$('agentId').value.trim().toUpperCase(),
      pairing_code:$('pairingCode').value.trim().toUpperCase(),
      device_name:$('deviceName').value.trim() || 'iPhone',
      role_view:true, role_chat:true, role_approve:true, role_files:false, role_run:false, role_desktop:false
    };
    const data = await relayPost('/api/clients/pair', body);
    $('pairingCode').value = '';
    if(data.device_token){
      $('deviceToken').value = data.device_token || '';
      setPendingPairRequest('');
      saveConnection();
      await loadState();
      return;
    }
    if(data.status === 'pending' && data.request_id){
      setPendingPairRequest(data.request_id);
      saveConnection();
      setStatus('запрос отправлен: подтвердите устройство в Leetcode', true);
      return;
    }
    throw new Error(data.error || data.status || 'pairing не завершён');
  }catch(error){setStatus('ошибка подключения: ' + error.message, false)}
}
async function checkPairStatus(){
  const requestId = currentPending();
  if(!requestId){setStatus('нет ожидающего подтверждения');return}
  try{
    const data = await relayPost('/api/clients/pair/status',{
      agent_id:$('agentId').value.trim().toUpperCase(),
      request_id:requestId
    });
    if(data.device_token){
      $('deviceToken').value = data.device_token || '';
      setPendingPairRequest('');
      saveConnection();
      setStatus('устройство подтверждено', true);
      await loadState();
      return;
    }
    if(data.status === 'pending'){
      setStatus('ожидает подтверждения в Leetcode', true);
      return;
    }
    setPendingPairRequest('');
    setStatus('подключение не завершено: ' + (data.error || data.status || 'unknown'), false);
  }catch(error){setStatus('ошибка проверки: ' + error.message, false)}
}
async function loadState(){
  const agent = $('agentId').value.trim();
  const token = $('deviceToken').value.trim();
  if(!agent || !token){setStatus('нужен Agent ID и device token');return}
  try{
    const data = await relayPost('/api/clients/state', await authedRequestBase(), true);
    lastState = data;
    render(data);
  }catch(error){setStatus('ошибка: ' + error.message, false)}
}
function remoteDiagnosticsText(data, s){
  const lines = [];
  const hostAge = data.host_age_secs ?? 0;
  lines.push(`Host: ${data.host_online ? 'online' : 'offline'} · snapshot ${hostAge} c · очередь ${data.queued_actions ?? 0}`);
  if(s.remote_enabled){
    lines.push(`Remote API: ${s.remote_server_running ? 'running' : 'stopped'} · ${s.remote_api_url || 'url нет'}`);
  }else{
    lines.push('Remote API: выключен для direct-клиента');
  }
  if(s.relay_enabled){
    const latency = s.relay_last_latency_ms ? `${s.relay_last_latency_ms} ms` : 'нет данных';
    const lastSync = s.relay_last_success_at ? `${Math.max(0, Math.floor(Date.now()/1000) - s.relay_last_success_at)} c назад` : 'нет успешного sync';
    lines.push(`Relay: ${s.relay_status || 'ожидает'} · latency ${latency} · sync ${lastSync}`);
  }else{
    lines.push('Relay: выключен, iPhone PWA не сможет подключиться по Agent ID');
  }
  if(!data.host_online) lines.push('Что проверить: основной Leetcode должен быть запущен, Relay включён, device token не отозван.');
  if(s.remote_pairing_expires_at && s.remote_pairing_expires_at > Math.floor(Date.now()/1000)) lines.push('Pairing code активен для новых устройств.');
  else lines.push('Pairing code не активен: создайте новый код в основном Leetcode.');
  return lines.join('\n');
}
function render(data){
  const s = data.state || {};
  const online = !!data.host_online;
  setStatus(online ? 'relay online' : 'relay offline', online);
  $('agentStatus').textContent = s.agent_status || 'ожидает';
  $('projectName').textContent = s.project_name || 'нет проекта';
  $('snapshotAge').textContent = (data.host_age_secs ?? 0) + ' c';
  $('modelName').textContent = [s.provider,s.model].filter(Boolean).join(' · ') || '-';
  $('queueCount').textContent = data.queued_actions ?? 0;
  $('runCount').textContent = (s.agent_history_tail || []).length;
  $('diagnostics').textContent = remoteDiagnosticsText(data, s);
  $('runGate').style.display = s.pending_run_gate_summary ? 'block' : 'none';
  $('runGateSummary').textContent = s.pending_run_gate_summary || '';
  $('approval').style.display = s.pending_approval_summary ? 'block' : 'none';
  $('approvalSummary').textContent = s.pending_approval_summary || '';
  showTab(activeTab);
}
function showTab(tab){
  activeTab = tab;
  for (const name of ['runs','commands','log']) $('tab-' + name).classList.toggle('active', name === tab);
  const list = $('list'); list.innerHTML = '';
  const s = (lastState && lastState.state) || {};
  if(tab === 'runs'){
    for(const run of (s.agent_history_tail || []).slice().reverse()){
      const b = document.createElement('button'); b.className='item';
      b.textContent = `${run.status || 'run'} · ${run.provider || ''}/${run.model || ''} · ${new Date((run.started_at||0)*1000).toLocaleString()}`;
      b.onclick = () => $('details').textContent = JSON.stringify(run,null,2);
      list.appendChild(b);
    }
    if(!list.children.length) $('details').textContent = 'Истории запусков пока нет.';
  }
  if(tab === 'commands'){
    for(const cmd of (s.remote_commands || []).filter(c=>c.enabled).slice(0,20)){
      const b = document.createElement('button'); b.className='item';
      b.textContent = `${cmd.title} · ${cmd.category || 'команда'}`;
      const risk = cmd.risk || 'low';
      const kind = cmd.kind || 'single';
      b.textContent = `${risk} · ${kind} · ${cmd.title} · ${cmd.category || 'команда'}`;
      b.onclick = () => previewAndRunCommand(cmd);
      list.appendChild(b);
    }
    if(!list.children.length) $('details').textContent = 'Команд пока нет.';
  }
  if(tab === 'log'){
    for(const entry of (s.tool_log_tail || []).slice().reverse()){
      const b = document.createElement('button'); b.className='item';
      b.textContent = entry.title || 'лог';
      b.onclick = () => $('details').textContent = (entry.title || '') + '\n\n' + (entry.content || '');
      list.appendChild(b);
    }
    if(!list.children.length) $('details').textContent = 'Логов пока нет.';
  }
}
async function submitTask(){
  const message = $('task').value.trim();
  if(!message){setStatus('введите задачу');return}
  try{
    const data = await relayPost('/api/clients/tasks',{...(await authedRequestBase()),message,source:'iphone-pwa'});
    $('task').value='';
    setStatus('задача поставлена: ' + (data.id || 'queued'), true);
    await loadState();
  }catch(error){setStatus('ошибка задачи: ' + error.message, false)}
}
function commandPreviewText(cmd){
  const steps = (cmd.steps || []).map((step,index)=>`${index+1}. ${step}`).join('\n');
  return [
    `Команда: ${cmd.title || cmd.id}`,
    `ID: ${cmd.id}`,
    `Тип: ${cmd.kind || 'single'}`,
    `Категория: ${cmd.category || 'команда'}`,
    `Риск: ${cmd.risk || 'low'}`,
    `Подтверждение: ${cmd.requires_confirmation ? 'нужно' : 'не нужно'}`,
    `Роль approve: ${cmd.requires_approval ? 'нужна' : 'не нужна'}`,
    `Роль run: ${cmd.requires_run ? 'нужна' : 'не нужна'}`,
    `Роль desktop: ${cmd.requires_desktop ? 'нужна' : 'не нужна'}`,
    cmd.description ? `Описание: ${cmd.description}` : '',
    steps ? `Шаги:\n${steps}` : ''
  ].filter(Boolean).join('\n');
}
async function previewAndRunCommand(cmd){
  const preview = commandPreviewText(cmd);
  $('details').textContent = preview;
  if((cmd.requires_confirmation || cmd.requires_approval || cmd.requires_run || cmd.requires_desktop) && !confirm(preview + '\n\nЗапустить команду?')) return;
  await runCommand(cmd.id, cmd.requires_confirmation || cmd.requires_approval || cmd.requires_run || cmd.requires_desktop);
}
async function runCommand(id, confirmed){
  try{
    const data = await relayPost('/api/clients/commands',{...(await authedRequestBase()),id,source:'iphone-pwa',confirmed:!!confirmed});
    $('details').textContent = 'Команда поставлена: ' + (data.id || 'queued');
    await loadState();
  }catch(error){$('details').textContent = 'Ошибка команды: ' + error.message}
}
async function answer(path, approved){
  try{
    const data = await relayPost(path,{...(await authedRequestBase()),approved});
    setStatus('ответ отправлен: ' + (data.id || 'queued'), true);
    await loadState();
  }catch(error){setStatus('ошибка ответа: ' + error.message, false)}
}
loadConnection();
if($('deviceToken').value.trim()) loadState();
else if(currentPending()) checkPairStatus();
timer = setInterval(() => {
  if($('deviceToken').value.trim()) loadState();
  else if(currentPending()) checkPairStatus();
}, 2500);
</script>
</body>
</html>"##
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

    #[test]
    fn relay_mobile_pwa_exposes_client_endpoints() {
        let html = relay_mobile_pwa_html();
        assert!(html.contains("/api/clients/sessions"));
        assert!(html.contains("/api/clients/state"));
        assert!(html.contains("/api/clients/tasks"));
        assert!(html.contains("/api/clients/pair/status"));
        assert!(html.contains("pairing_code"));
        assert!(html.contains("apple-mobile-web-app-capable"));
        assert!(html.contains("diagnostics"));
        assert!(html.contains("remoteDiagnosticsText"));
    }

    #[test]
    fn relay_session_token_authorizes_device_roles() {
        let now = unix_timestamp();
        let mut host = HostRecord::new("host-token".to_string());
        let device = RelayDevice {
            id: "device-1".to_string(),
            name: "Phone".to_string(),
            token: "rd-test-token".to_string(),
            role_view: true,
            role_chat: true,
            role_approve: false,
            role_files: false,
            role_run: false,
            role_desktop: false,
            created_at: now,
            last_seen_at: 0,
            expires_at: 0,
            token_rotated_at: 0,
            revoked_at: 0,
            revoked: false,
        };
        host.devices.insert(device.id.clone(), device.clone());

        let session =
            issue_relay_session_token("LC-TEST", &host, &device, now).expect("session token");
        assert_eq!(
            relay_authorized_device_id(&host, "LC-TEST", &session, DeviceRole::View).as_deref(),
            Some("device-1")
        );
        assert_eq!(
            relay_authorized_device_id(&host, "LC-TEST", &session, DeviceRole::Chat).as_deref(),
            Some("device-1")
        );
        assert!(
            relay_authorized_device_id(&host, "LC-TEST", &session, DeviceRole::Approve).is_none()
        );
        assert!(
            relay_authorized_device_id(&host, "LC-OTHER", &session, DeviceRole::View).is_none()
        );

        let mut tampered = session.clone();
        tampered.push('x');
        assert!(
            relay_authorized_device_id(&host, "LC-TEST", &tampered, DeviceRole::View).is_none()
        );
    }
}
