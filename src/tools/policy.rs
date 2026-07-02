use crate::agent::types::AppEvent;
use std::collections::HashMap;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

pub type ApprovalMap = Arc<Mutex<HashMap<String, Sender<bool>>>>;

#[derive(Clone, Debug)]
pub struct PolicyConfig {
    pub require_shell_approval: bool,
    pub require_write_approval: bool,
}

pub fn request_approval(
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    summary: impl Into<String>,
    detail: impl Into<String>,
) -> bool {
    let id = Uuid::new_v4().to_string();
    let summary = summary.into();
    let detail = detail.into();
    let (tx, rx) = mpsc::channel();

    approvals
        .lock()
        .expect("approval map poisoned")
        .insert(id.clone(), tx);

    let sent = events.send(AppEvent::ApprovalRequested {
        id: id.clone(),
        summary,
        detail,
    });

    if sent.is_err() {
        approvals.lock().expect("approval map poisoned").remove(&id);
        return false;
    }

    let approved = rx.recv_timeout(Duration::from_secs(300)).unwrap_or(false);
    approvals.lock().expect("approval map poisoned").remove(&id);
    approved
}

pub fn looks_destructive_shell(cmd: &str) -> bool {
    let lower = cmd.to_ascii_lowercase();
    let risky_needles = [
        "remove-item",
        "rm -rf",
        "del /",
        "rmdir /s",
        "rd /s",
        "format ",
        "diskpart",
        "shutdown",
        "reg delete",
        "takeown",
        "icacls",
    ];

    risky_needles.iter().any(|needle| lower.contains(needle))
}
