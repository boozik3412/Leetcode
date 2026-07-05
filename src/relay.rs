#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DEFAULT_RELAY_URL: &str = "http://127.0.0.1:17990";

pub fn generate_relay_host_token() -> String {
    format!("rh-{}", uuid::Uuid::new_v4().simple())
}

pub fn generate_relay_device_token() -> String {
    format!("rd-{}", uuid::Uuid::new_v4().simple())
}

pub fn normalize_agent_id(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayHostPollRequest {
    pub agent_id: String,
    pub host_token: String,
    #[serde(default)]
    pub pairing_code: String,
    #[serde(default)]
    pub pairing_expires_at: u64,
    #[serde(default)]
    pub state: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayHostPollReply {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub actions: Vec<RelayAction>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayPairRequest {
    pub agent_id: String,
    pub pairing_code: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default = "default_true")]
    pub role_view: bool,
    #[serde(default = "default_true")]
    pub role_chat: bool,
    #[serde(default)]
    pub role_approve: bool,
    #[serde(default)]
    pub role_files: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayPairReply {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub device_token: String,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayClientRequest {
    pub agent_id: String,
    pub device_token: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayClientTaskRequest {
    pub agent_id: String,
    pub device_token: String,
    pub message: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayClientCommandRequest {
    pub agent_id: String,
    pub device_token: String,
    pub id: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayClientApprovalRequest {
    pub agent_id: String,
    pub device_token: String,
    pub approved: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayAction {
    pub id: String,
    pub created_at: u64,
    #[serde(flatten)]
    pub kind: RelayActionKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayActionKind {
    SubmitTask {
        task_id: String,
        message: String,
        source: String,
    },
    RunCommand {
        id: String,
        source: String,
    },
    AnswerRunGate {
        approved: bool,
    },
    AnswerApproval {
        approved: bool,
    },
    PairDevice {
        device: RelayDevice,
    },
    DeviceSeen {
        device_id: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayDevice {
    pub id: String,
    pub name: String,
    pub token: String,
    pub role_view: bool,
    pub role_chat: bool,
    pub role_approve: bool,
    pub role_files: bool,
    pub created_at: u64,
    #[serde(default)]
    pub last_seen_at: u64,
    #[serde(default)]
    pub revoked: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayStateReply {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub state: Value,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayQueuedReply {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

pub fn new_action(kind: RelayActionKind, created_at: u64) -> RelayAction {
    RelayAction {
        id: format!("relay-action-{}", uuid::Uuid::new_v4().simple()),
        created_at,
        kind,
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_action_roundtrips_with_flat_type() {
        let action = new_action(
            RelayActionKind::SubmitTask {
                task_id: "task-1".to_string(),
                message: "hello".to_string(),
                source: "test".to_string(),
            },
            10,
        );

        let json = serde_json::to_string(&action).unwrap();
        assert!(json.contains("\"type\":\"submit_task\""));
        let parsed: RelayAction = serde_json::from_str(&json).unwrap();
        assert!(matches!(parsed.kind, RelayActionKind::SubmitTask { .. }));
    }
}
