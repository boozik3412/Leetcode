use crate::agent::types::AppEvent;
use crate::config::AppConfig;
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
    pub require_paid_api_approval: bool,
    pub require_desktop_approval: bool,
    pub require_external_approval: bool,
    pub require_orchestration_approval: bool,
    pub allow_destructive_shell: bool,
}

impl PolicyConfig {
    pub fn from_config(config: &AppConfig) -> Self {
        Self {
            require_shell_approval: config.effective_require_shell_approval(),
            require_write_approval: config.effective_require_write_approval(),
            require_paid_api_approval: config.effective_require_paid_api_approval(),
            require_desktop_approval: config.effective_require_desktop_approval(),
            require_external_approval: config.effective_require_external_approval(),
            require_orchestration_approval: config.effective_require_orchestration_approval(),
            allow_destructive_shell: config.effective_allow_destructive_shell(),
        }
    }

    pub fn require_shell_for(&self, cmd: &str) -> bool {
        self.require_shell_approval
            || (looks_destructive_shell(cmd) && !self.allow_destructive_shell)
    }
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

pub fn request_approval_if(
    required: bool,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    summary: impl Into<String>,
    detail: impl Into<String>,
) -> bool {
    !required || request_approval(events, approvals, summary, detail)
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
