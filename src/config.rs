use crate::agent::models::{
    default_model_for_provider, ANTHROPIC_PROVIDER_ID, DEEPSEEK_PROVIDER_ID, GEMINI_PROVIDER_ID,
    OPENAI_PROVIDER_ID,
};
use crate::agent::routing::ROUTE_AUTO;
use crate::assets::{
    GEMINI_IMAGE_PROVIDER_ID, OPENAI_AUDIO_PROVIDER_ID, OPENAI_IMAGE_PROVIDER_ID,
    OPENAI_VIDEO_PROVIDER_ID, REPLICATE_IMAGE_PROVIDER_ID, STABILITY_IMAGE_PROVIDER_ID,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub providers: BTreeMap<String, ProviderSettings>,
    pub last_workspace: Option<PathBuf>,
    pub policy_profile: String,
    pub require_shell_approval: bool,
    pub require_write_approval: bool,
    pub require_paid_api_approval: bool,
    pub require_desktop_approval: bool,
    pub require_external_approval: bool,
    pub require_orchestration_approval: bool,
    pub allow_destructive_shell: bool,
    pub task_route: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProviderSettings {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedConfig {
    #[serde(default = "default_provider")]
    provider: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    providers: BTreeMap<String, ProviderSettings>,
    #[serde(default = "default_model")]
    model: String,
    #[serde(default = "default_task_route")]
    task_route: String,
    last_workspace: Option<PathBuf>,
    #[serde(default = "default_policy_profile")]
    policy_profile: String,
    #[serde(default = "default_true")]
    require_shell_approval: bool,
    #[serde(default = "default_true")]
    require_write_approval: bool,
    #[serde(default = "default_true")]
    require_paid_api_approval: bool,
    #[serde(default = "default_true")]
    require_desktop_approval: bool,
    #[serde(default = "default_true")]
    require_external_approval: bool,
    #[serde(default = "default_true")]
    require_orchestration_approval: bool,
    #[serde(default)]
    allow_destructive_shell: bool,
}

impl Default for PersistedConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            api_key: String::new(),
            providers: BTreeMap::new(),
            model: default_model(),
            task_route: default_task_route(),
            last_workspace: None,
            policy_profile: default_policy_profile(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
        }
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let persisted = config_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<PersistedConfig>(&text).ok())
            .unwrap_or_default();

        let provider = normalize_provider(&persisted.provider);
        let mut providers = persisted.providers;

        {
            let openai = providers.entry(OPENAI_PROVIDER_ID.to_string()).or_default();
            if openai.model.trim().is_empty() {
                openai.model = persisted.model.clone();
            }
            if openai.api_key.trim().is_empty() {
                openai.api_key = persisted.api_key.clone();
            }
        }
        apply_env_api_key(&mut providers, OPENAI_PROVIDER_ID, "OPENAI_API_KEY");
        apply_env_api_key(&mut providers, ANTHROPIC_PROVIDER_ID, "ANTHROPIC_API_KEY");
        apply_env_api_key(&mut providers, DEEPSEEK_PROVIDER_ID, "DEEPSEEK_API_KEY");
        apply_env_api_key(&mut providers, GEMINI_PROVIDER_ID, "GEMINI_API_KEY");
        apply_env_api_key(&mut providers, OPENAI_IMAGE_PROVIDER_ID, "OPENAI_API_KEY");
        apply_env_api_key(&mut providers, OPENAI_AUDIO_PROVIDER_ID, "OPENAI_API_KEY");
        apply_env_api_key(&mut providers, OPENAI_VIDEO_PROVIDER_ID, "OPENAI_API_KEY");
        apply_env_api_key(&mut providers, GEMINI_IMAGE_PROVIDER_ID, "GEMINI_API_KEY");
        apply_env_api_key(
            &mut providers,
            STABILITY_IMAGE_PROVIDER_ID,
            "STABILITY_API_KEY",
        );
        apply_env_api_key(
            &mut providers,
            REPLICATE_IMAGE_PROVIDER_ID,
            "REPLICATE_API_TOKEN",
        );

        providers.entry(provider.clone()).or_default();
        let active_settings = providers.get(&provider).cloned().unwrap_or_default();

        Self {
            provider: provider.clone(),
            api_key: if active_settings.api_key.trim().is_empty() {
                String::new()
            } else {
                active_settings.api_key
            },
            model: if active_settings.model.trim().is_empty() {
                default_model_for_provider(&provider).to_string()
            } else {
                active_settings.model
            },
            providers,
            last_workspace: persisted.last_workspace,
            policy_profile: normalize_policy_profile(&persisted.policy_profile),
            require_shell_approval: persisted.require_shell_approval,
            require_write_approval: persisted.require_write_approval,
            require_paid_api_approval: persisted.require_paid_api_approval,
            require_desktop_approval: persisted.require_desktop_approval,
            require_external_approval: persisted.require_external_approval,
            require_orchestration_approval: persisted.require_orchestration_approval,
            allow_destructive_shell: persisted.allow_destructive_shell,
            task_route: normalize_task_route(&persisted.task_route),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = config_path() else {
            anyhow::bail!("Could not resolve config directory");
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut providers = self.providers.clone();
        providers.insert(
            self.provider_id().to_string(),
            ProviderSettings {
                api_key: self.api_key.clone(),
                model: self.model.clone(),
            },
        );

        let persisted = PersistedConfig {
            provider: self.provider_id().to_string(),
            api_key: self.api_key.clone(),
            providers,
            model: self.model.clone(),
            task_route: normalize_task_route(&self.task_route),
            last_workspace: self.last_workspace.clone(),
            policy_profile: normalize_policy_profile(&self.policy_profile),
            require_shell_approval: self.require_shell_approval,
            require_write_approval: self.require_write_approval,
            require_paid_api_approval: self.require_paid_api_approval,
            require_desktop_approval: self.require_desktop_approval,
            require_external_approval: self.require_external_approval,
            require_orchestration_approval: self.require_orchestration_approval,
            allow_destructive_shell: self.allow_destructive_shell,
        };

        fs::write(path, serde_json::to_string_pretty(&persisted)?)?;
        Ok(())
    }

    pub fn provider_id(&self) -> &str {
        if self.provider.trim().is_empty() {
            OPENAI_PROVIDER_ID
        } else {
            self.provider.trim()
        }
    }

    pub fn api_key_for_provider(&self, provider_id: &str) -> String {
        if self.provider_id() == provider_id {
            self.api_key.clone()
        } else {
            self.providers
                .get(provider_id)
                .map(|settings| settings.api_key.clone())
                .unwrap_or_default()
        }
    }

    pub fn model_for_provider(&self, provider_id: &str) -> String {
        if self.provider_id() == provider_id && !self.model.trim().is_empty() {
            return self.model.clone();
        }

        self.providers
            .get(provider_id)
            .and_then(|settings| {
                if settings.model.trim().is_empty() {
                    None
                } else {
                    Some(settings.model.clone())
                }
            })
            .unwrap_or_else(|| default_model_for_provider(provider_id).to_string())
    }

    pub fn set_active_provider_settings(
        &mut self,
        provider_id: &str,
        model: String,
        api_key: String,
    ) {
        let provider_id = normalize_provider(provider_id);
        self.provider = provider_id.clone();
        self.model = if model.trim().is_empty() {
            default_model_for_provider(&provider_id).to_string()
        } else {
            model
        };
        self.api_key = api_key;
        self.providers.insert(
            provider_id,
            ProviderSettings {
                api_key: self.api_key.clone(),
                model: self.model.clone(),
            },
        );
    }

    pub fn set_provider_settings(&mut self, provider_id: &str, model: String, api_key: String) {
        let provider_id = provider_id.trim().to_ascii_lowercase();
        if provider_id.is_empty() {
            return;
        }

        self.providers
            .insert(provider_id, ProviderSettings { api_key, model });
    }

    pub fn select_provider(&mut self, provider_id: &str) {
        let provider_id = normalize_provider(provider_id);
        self.provider = provider_id.clone();
        let settings = self
            .providers
            .get(&provider_id)
            .cloned()
            .unwrap_or_default();
        self.api_key = settings.api_key;
        self.model = if settings.model.trim().is_empty() {
            default_model_for_provider(&provider_id).to_string()
        } else {
            settings.model
        };
    }

    pub fn set_policy_profile(&mut self, policy_profile: &str) {
        self.policy_profile = normalize_policy_profile(policy_profile);
        match self.policy_profile.as_str() {
            PERMISSION_ASK => {
                self.require_shell_approval = true;
                self.require_write_approval = true;
                self.require_paid_api_approval = true;
                self.require_desktop_approval = true;
                self.require_external_approval = true;
                self.require_orchestration_approval = true;
                self.allow_destructive_shell = false;
            }
            PERMISSION_AUTO => {
                self.require_shell_approval = false;
                self.require_write_approval = false;
                self.require_paid_api_approval = true;
                self.require_desktop_approval = true;
                self.require_external_approval = true;
                self.require_orchestration_approval = false;
                self.allow_destructive_shell = false;
            }
            PERMISSION_WORK => {
                self.require_shell_approval = false;
                self.require_write_approval = false;
                self.require_paid_api_approval = false;
                self.require_desktop_approval = true;
                self.require_external_approval = true;
                self.require_orchestration_approval = false;
                self.allow_destructive_shell = false;
            }
            PERMISSION_FULL => {
                self.require_shell_approval = false;
                self.require_write_approval = false;
                self.require_paid_api_approval = false;
                self.require_desktop_approval = false;
                self.require_external_approval = false;
                self.require_orchestration_approval = false;
                self.allow_destructive_shell = true;
            }
            POLICY_CUSTOM => {}
            _ => unreachable!("policy profile is normalized"),
        }
    }

    pub fn effective_require_shell_approval(&self) -> bool {
        match normalize_policy_profile(&self.policy_profile).as_str() {
            PERMISSION_ASK => true,
            PERMISSION_AUTO | PERMISSION_WORK | PERMISSION_FULL => false,
            POLICY_CUSTOM => self.require_shell_approval,
            _ => unreachable!("policy profile is normalized"),
        }
    }

    pub fn effective_require_write_approval(&self) -> bool {
        match normalize_policy_profile(&self.policy_profile).as_str() {
            PERMISSION_ASK => true,
            PERMISSION_AUTO | PERMISSION_WORK | PERMISSION_FULL => false,
            POLICY_CUSTOM => self.require_write_approval,
            _ => unreachable!("policy profile is normalized"),
        }
    }

    pub fn effective_require_paid_api_approval(&self) -> bool {
        match normalize_policy_profile(&self.policy_profile).as_str() {
            PERMISSION_ASK | PERMISSION_AUTO => true,
            PERMISSION_WORK | PERMISSION_FULL => false,
            POLICY_CUSTOM => self.require_paid_api_approval,
            _ => unreachable!("policy profile is normalized"),
        }
    }

    pub fn effective_require_desktop_approval(&self) -> bool {
        match normalize_policy_profile(&self.policy_profile).as_str() {
            PERMISSION_ASK | PERMISSION_AUTO | PERMISSION_WORK => true,
            PERMISSION_FULL => false,
            POLICY_CUSTOM => self.require_desktop_approval,
            _ => unreachable!("policy profile is normalized"),
        }
    }

    pub fn effective_require_external_approval(&self) -> bool {
        match normalize_policy_profile(&self.policy_profile).as_str() {
            PERMISSION_ASK | PERMISSION_AUTO | PERMISSION_WORK => true,
            PERMISSION_FULL => false,
            POLICY_CUSTOM => self.require_external_approval,
            _ => unreachable!("policy profile is normalized"),
        }
    }

    pub fn effective_require_orchestration_approval(&self) -> bool {
        match normalize_policy_profile(&self.policy_profile).as_str() {
            PERMISSION_ASK => true,
            PERMISSION_AUTO | PERMISSION_WORK | PERMISSION_FULL => false,
            POLICY_CUSTOM => self.require_orchestration_approval,
            _ => unreachable!("policy profile is normalized"),
        }
    }

    pub fn effective_allow_destructive_shell(&self) -> bool {
        match normalize_policy_profile(&self.policy_profile).as_str() {
            PERMISSION_FULL => true,
            PERMISSION_ASK | PERMISSION_AUTO | PERMISSION_WORK => false,
            POLICY_CUSTOM => self.allow_destructive_shell,
            _ => unreachable!("policy profile is normalized"),
        }
    }
}

pub const PERMISSION_ASK: &str = "ask";
pub const PERMISSION_AUTO: &str = "auto";
pub const PERMISSION_WORK: &str = "work";
pub const PERMISSION_FULL: &str = "full";
pub const POLICY_CUSTOM: &str = "custom";

pub fn policy_profile_labels() -> &'static [(&'static str, &'static str)] {
    &[
        (PERMISSION_ASK, "Запрос"),
        (PERMISSION_AUTO, "Авто"),
        (PERMISSION_WORK, "Работа"),
        (PERMISSION_FULL, "Полный"),
        (POLICY_CUSTOM, "Особый"),
    ]
}

pub fn permission_mode_description(mode: &str) -> &'static str {
    match normalize_policy_profile(mode).as_str() {
        PERMISSION_ASK => "Запрашивать подтверждение для всех действий с эффектами.",
        PERMISSION_AUTO => "Автоматически работать в проекте; спрашивать для платных API, desktop и внешних открытий.",
        PERMISSION_WORK => "Автоматически менять проект и вызывать asset API; спрашивать для desktop и внешних открытий.",
        PERMISSION_FULL => "Полный доступ: выполнять действия без подтверждений, сохраняя проверки путей workspace.",
        POLICY_CUSTOM => "Особая ручная комбинация разрешений.",
        _ => "Режим разрешений.",
    }
}

fn default_provider() -> String {
    OPENAI_PROVIDER_ID.to_string()
}

fn default_model() -> String {
    default_model_for_provider(OPENAI_PROVIDER_ID).to_string()
}

fn default_task_route() -> String {
    ROUTE_AUTO.to_string()
}

fn default_policy_profile() -> String {
    PERMISSION_ASK.to_string()
}

fn default_true() -> bool {
    true
}

fn normalize_policy_profile(policy_profile: &str) -> String {
    let normalized = policy_profile.trim().to_ascii_lowercase().replace('-', "_");

    match normalized.as_str() {
        PERMISSION_ASK | "normal" | "strict" => PERMISSION_ASK.to_string(),
        PERMISSION_AUTO | "safe" => PERMISSION_AUTO.to_string(),
        PERMISSION_WORK => PERMISSION_WORK.to_string(),
        PERMISSION_FULL | "full_access" => PERMISSION_FULL.to_string(),
        POLICY_CUSTOM => POLICY_CUSTOM.to_string(),
        _ => PERMISSION_ASK.to_string(),
    }
}

fn normalize_task_route(task_route: &str) -> String {
    let task_route = task_route.trim().to_ascii_lowercase().replace('-', "_");
    if task_route.is_empty() {
        ROUTE_AUTO.to_string()
    } else {
        task_route
    }
}

fn normalize_provider(provider_id: &str) -> String {
    let provider_id = provider_id.trim();
    if provider_id.is_empty() {
        OPENAI_PROVIDER_ID.to_string()
    } else {
        provider_id.to_ascii_lowercase()
    }
}

fn apply_env_api_key(
    providers: &mut BTreeMap<String, ProviderSettings>,
    provider_id: &str,
    env_var: &str,
) {
    let Ok(api_key) = std::env::var(env_var) else {
        return;
    };
    if api_key.trim().is_empty() {
        return;
    }

    providers
        .entry(provider_id.to_string())
        .or_default()
        .api_key = api_key;
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

pub fn read_journal_tail(limit: usize) -> Vec<String> {
    let Some(path) = journal_path() else {
        return Vec::new();
    };
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut lines = text
        .lines()
        .rev()
        .take(limit)
        .map(render_journal_line)
        .collect::<Vec<_>>();
    lines.reverse();
    lines
}

pub fn clear_journal() -> anyhow::Result<()> {
    let Some(path) = journal_path() else {
        anyhow::bail!("Could not resolve journal directory");
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, "")?;
    Ok(())
}

fn render_journal_line(line: &str) -> String {
    let mut parts = line.splitn(2, '\t');
    let timestamp = parts.next().unwrap_or_default();
    let entry = parts.next().unwrap_or_default();
    if entry.is_empty() {
        timestamp.to_string()
    } else {
        format!("{timestamp}  {entry}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_config_without_persisted_api_key() {
        let json = r#"{
          "model": "gpt-5.5",
          "task_route": "auto",
          "last_workspace": null,
          "policy_profile": "normal",
          "require_shell_approval": true,
          "require_write_approval": true
        }"#;

        let config = serde_json::from_str::<PersistedConfig>(json).expect("valid config");

        assert!(config.api_key.is_empty());
        assert_eq!(config.model, "gpt-5.5");
        assert_eq!(config.task_route, "auto");
        assert_eq!(
            normalize_policy_profile(&config.policy_profile),
            PERMISSION_ASK
        );
    }

    #[test]
    fn serializes_persisted_api_key() {
        let config = PersistedConfig {
            provider: OPENAI_PROVIDER_ID.to_string(),
            api_key: "sk-test".to_string(),
            providers: BTreeMap::new(),
            model: "gpt-5.5".to_string(),
            task_route: "auto".to_string(),
            last_workspace: None,
            policy_profile: PERMISSION_ASK.to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
        };

        let json = serde_json::to_string(&config).expect("serializes config");

        assert!(json.contains("\"api_key\":\"sk-test\""));
    }

    #[test]
    fn serializes_provider_settings() {
        let mut providers = BTreeMap::new();
        providers.insert(
            OPENAI_PROVIDER_ID.to_string(),
            ProviderSettings {
                api_key: "sk-openai".to_string(),
                model: "gpt-5.4".to_string(),
            },
        );
        let config = PersistedConfig {
            provider: OPENAI_PROVIDER_ID.to_string(),
            api_key: "sk-openai".to_string(),
            providers,
            model: "gpt-5.4".to_string(),
            task_route: "coding".to_string(),
            last_workspace: None,
            policy_profile: PERMISSION_ASK.to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
        };

        let json = serde_json::to_string(&config).expect("serializes config");

        assert!(json.contains("\"providers\""));
        assert!(json.contains("\"openai\""));
        assert!(json.contains("\"model\":\"gpt-5.4\""));
    }

    #[test]
    fn selects_default_model_for_new_provider() {
        let mut config = AppConfig {
            provider: OPENAI_PROVIDER_ID.to_string(),
            api_key: String::new(),
            model: "gpt-5.5".to_string(),
            providers: BTreeMap::new(),
            last_workspace: None,
            policy_profile: PERMISSION_ASK.to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
            task_route: "auto".to_string(),
        };

        config.select_provider(GEMINI_PROVIDER_ID);

        assert_eq!(config.provider_id(), GEMINI_PROVIDER_ID);
        assert_eq!(config.model, default_model_for_provider(GEMINI_PROVIDER_ID));
    }

    #[test]
    fn auto_mode_allows_workspace_work_but_keeps_paid_and_desktop_approval() {
        let mut config = AppConfig {
            provider: OPENAI_PROVIDER_ID.to_string(),
            api_key: String::new(),
            model: default_model_for_provider(OPENAI_PROVIDER_ID).to_string(),
            providers: BTreeMap::new(),
            last_workspace: None,
            policy_profile: PERMISSION_ASK.to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
            task_route: "auto".to_string(),
        };

        config.set_policy_profile(PERMISSION_AUTO);

        assert!(!config.effective_require_shell_approval());
        assert!(!config.effective_require_write_approval());
        assert!(config.effective_require_paid_api_approval());
        assert!(config.effective_require_desktop_approval());
        assert!(!config.effective_allow_destructive_shell());
    }

    #[test]
    fn full_mode_removes_approval_gates() {
        let mut config = AppConfig {
            provider: OPENAI_PROVIDER_ID.to_string(),
            api_key: String::new(),
            model: default_model_for_provider(OPENAI_PROVIDER_ID).to_string(),
            providers: BTreeMap::new(),
            last_workspace: None,
            policy_profile: PERMISSION_ASK.to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
            task_route: "auto".to_string(),
        };

        config.set_policy_profile(PERMISSION_FULL);

        assert!(!config.effective_require_shell_approval());
        assert!(!config.effective_require_write_approval());
        assert!(!config.effective_require_paid_api_approval());
        assert!(!config.effective_require_desktop_approval());
        assert!(config.effective_allow_destructive_shell());
    }
}
