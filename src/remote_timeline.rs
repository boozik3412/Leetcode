use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const REMOTE_TIMELINE_PATH: &str = "remote_sessions.jsonl";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteSessionEvent {
    pub schema_version: u32,
    pub id: String,
    pub created_at: u64,
    pub session_id: String,
    pub channel: String,
    pub actor: String,
    pub event: String,
    pub status: String,
    pub summary: String,
    pub detail: String,
    pub related_id: Option<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl RemoteSessionEvent {
    pub fn new(
        channel: impl Into<String>,
        actor: impl Into<String>,
        event: impl Into<String>,
        status: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        let created_at = unix_timestamp();
        let channel = truncate_chars(&channel.into(), 80);
        let actor = truncate_chars(&actor.into(), 140);
        let event = truncate_chars(&event.into(), 120);
        Self {
            schema_version: 1,
            id: format!("remote-{created_at}-{}", uuid::Uuid::new_v4().simple()),
            created_at,
            session_id: remote_session_id(&channel, &actor),
            channel,
            actor,
            event,
            status: truncate_chars(&status.into(), 80),
            summary: truncate_chars(&summary.into(), 500),
            detail: String::new(),
            related_id: None,
            metadata: BTreeMap::new(),
        }
    }

    pub fn with_created_at(mut self, created_at: u64) -> Self {
        self.created_at = created_at;
        self.session_id = remote_session_id(&self.channel, &self.actor);
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = truncate_chars(&detail.into(), 2_000);
        self
    }

    pub fn with_related_id(mut self, related_id: impl Into<String>) -> Self {
        let related_id = related_id.into();
        if !related_id.trim().is_empty() {
            self.related_id = Some(truncate_chars(&related_id, 180));
        }
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = truncate_chars(&key.into(), 80);
        let value = truncate_chars(&value.into(), 500);
        if !key.trim().is_empty() && !value.trim().is_empty() {
            self.metadata.insert(key, value);
        }
        self
    }
}

pub fn remote_timeline_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("leetcode").join(REMOTE_TIMELINE_PATH))
}

pub fn append_remote_session_event(event: &RemoteSessionEvent) -> anyhow::Result<()> {
    let Some(path) = remote_timeline_path() else {
        anyhow::bail!("could not resolve remote timeline directory");
    };
    append_remote_session_event_to_path(&path, event)
}

pub fn load_remote_session_events_tail(limit: usize) -> Vec<RemoteSessionEvent> {
    let Some(path) = remote_timeline_path() else {
        return Vec::new();
    };
    load_remote_session_events_tail_from_path(&path, limit)
}

pub fn clear_remote_session_events() -> anyhow::Result<()> {
    let Some(path) = remote_timeline_path() else {
        anyhow::bail!("could not resolve remote timeline directory");
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, "")?;
    Ok(())
}

fn append_remote_session_event_to_path(
    path: &Path,
    event: &RemoteSessionEvent,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(event)?;
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn load_remote_session_events_tail_from_path(path: &Path, limit: usize) -> Vec<RemoteSessionEvent> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut events = text
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<RemoteSessionEvent>(line).ok())
        .take(limit)
        .collect::<Vec<_>>();
    events.reverse();
    events
}

fn remote_session_id(channel: &str, actor: &str) -> String {
    let actor = if actor.trim().is_empty() {
        "unknown"
    } else {
        actor.trim()
    };
    format!("{}:{}", slug_part(channel, 48), slug_part(actor, 96))
}

fn slug_part(value: &str, max_chars: usize) -> String {
    let mut slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else if matches!(ch, '-' | '_' | '.' | ':') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug
        .trim_matches('-')
        .chars()
        .take(max_chars)
        .collect::<String>();
    if slug.is_empty() {
        "unknown".to_string()
    } else {
        slug
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut output = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        output.push_str("...");
    }
    output
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
    fn appends_and_reads_tail_in_order() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("remote_sessions.jsonl");
        for index in 0..5 {
            let event = RemoteSessionEvent::new(
                "relay",
                format!("device-{index}"),
                "task",
                "queued",
                format!("task {index}"),
            )
            .with_created_at(100 + index);
            append_remote_session_event_to_path(&path, &event).expect("append");
        }

        let events = load_remote_session_events_tail_from_path(&path, 3);

        assert_eq!(events.len(), 3);
        assert_eq!(events[0].summary, "task 2");
        assert_eq!(events[2].summary, "task 4");
    }

    #[test]
    fn builds_stable_session_id_from_channel_and_actor() {
        let event = RemoteSessionEvent::new("Relay PWA", "iPhone 15 Pro", "task", "queued", "ok");

        assert_eq!(event.session_id, "relay-pwa:iphone-15-pro");
    }
}
