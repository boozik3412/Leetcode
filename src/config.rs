use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub api_key: String,
    pub model: String,
    pub last_workspace: Option<PathBuf>,
    pub require_shell_approval: bool,
    pub require_write_approval: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedConfig {
    model: String,
    last_workspace: Option<PathBuf>,
    require_shell_approval: bool,
    require_write_approval: bool,
}

impl Default for PersistedConfig {
    fn default() -> Self {
        Self {
            model: "gpt-5.5".to_string(),
            last_workspace: None,
            require_shell_approval: true,
            require_write_approval: true,
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let persisted = config_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<PersistedConfig>(&text).ok())
            .unwrap_or_default();

        Self {
            api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            model: persisted.model,
            last_workspace: persisted.last_workspace,
            require_shell_approval: persisted.require_shell_approval,
            require_write_approval: persisted.require_write_approval,
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = config_path() else {
            anyhow::bail!("Could not resolve config directory");
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let persisted = PersistedConfig {
            model: self.model.clone(),
            last_workspace: self.last_workspace.clone(),
            require_shell_approval: self.require_shell_approval,
            require_write_approval: self.require_write_approval,
        };

        fs::write(path, serde_json::to_string_pretty(&persisted)?)?;
        Ok(())
    }
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("leetcode").join("config.json"))
}

pub fn journal_path() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("leetcode").join("journal.log"))
}

pub fn append_journal(entry: impl AsRef<str>) {
    let Some(path) = journal_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let sanitized = entry.as_ref().replace('\n', "\\n");
        let _ = writeln!(file, "{timestamp}\t{sanitized}");
    }
}
