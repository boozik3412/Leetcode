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
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub provider: String,
    pub api_key: String,
    pub model: String,
    pub providers: BTreeMap<String, ProviderSettings>,
    pub last_workspace: Option<PathBuf>,
    pub projects: Vec<ProjectUiState>,
    pub policy_profile: String,
    pub require_shell_approval: bool,
    pub require_write_approval: bool,
    pub require_paid_api_approval: bool,
    pub require_desktop_approval: bool,
    pub require_external_approval: bool,
    pub require_orchestration_approval: bool,
    pub allow_destructive_shell: bool,
    pub task_route: String,
    pub proxy_enabled: bool,
    pub proxy_url: String,
    pub proxy_use_system: bool,
    pub proxy_scheme: String,
    pub proxy_host: String,
    pub proxy_port: String,
    pub proxy_username: String,
    pub proxy_password: String,
    pub proxy_no_proxy: String,
    pub remote_enabled: bool,
    pub remote_bind_host: String,
    pub remote_port: u16,
    pub remote_access_token: String,
    pub remote_role_view: bool,
    pub remote_role_chat: bool,
    pub remote_role_approve: bool,
    pub remote_role_files: bool,
    pub remote_allowed_origins: String,
    pub remote_rate_limit_per_minute: u32,
    pub remote_audit_enabled: bool,
    pub context_recent_messages: usize,
    pub context_relevant_messages: usize,
    pub context_recent_runs: usize,
    pub layout_workspace_mode: String,
    pub layout_right_panel_view: String,
    pub layout_file_panel_collapsed: bool,
    pub command_palette_recent: Vec<String>,
    pub command_palette_favorites: Vec<String>,
    pub command_palette_macros: Vec<CommandPaletteMacro>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProviderSettings {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProjectUiState {
    pub path: PathBuf,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub expanded: bool,
    #[serde(default)]
    pub expanded_dirs: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandPaletteMacro {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub confirm_each_step: bool,
    #[serde(default)]
    pub command_ids: Vec<String>,
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
    #[serde(default)]
    projects: Vec<ProjectUiState>,
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
    #[serde(default)]
    proxy_enabled: bool,
    #[serde(default)]
    proxy_url: String,
    #[serde(default = "default_true")]
    proxy_use_system: bool,
    #[serde(default = "default_proxy_scheme")]
    proxy_scheme: String,
    #[serde(default)]
    proxy_host: String,
    #[serde(default)]
    proxy_port: String,
    #[serde(default)]
    proxy_username: String,
    #[serde(default)]
    proxy_password: String,
    #[serde(default)]
    proxy_no_proxy: String,
    #[serde(default)]
    remote_enabled: bool,
    #[serde(default = "default_remote_bind_host")]
    remote_bind_host: String,
    #[serde(default = "default_remote_port")]
    remote_port: u16,
    #[serde(default)]
    remote_access_token: String,
    #[serde(default = "default_true")]
    remote_role_view: bool,
    #[serde(default = "default_true")]
    remote_role_chat: bool,
    #[serde(default = "default_true")]
    remote_role_approve: bool,
    #[serde(default = "default_true")]
    remote_role_files: bool,
    #[serde(default)]
    remote_allowed_origins: String,
    #[serde(default = "default_remote_rate_limit_per_minute")]
    remote_rate_limit_per_minute: u32,
    #[serde(default = "default_true")]
    remote_audit_enabled: bool,
    #[serde(default = "default_context_recent_messages")]
    context_recent_messages: usize,
    #[serde(default = "default_context_relevant_messages")]
    context_relevant_messages: usize,
    #[serde(default = "default_context_recent_runs")]
    context_recent_runs: usize,
    #[serde(default = "default_layout_workspace_mode")]
    layout_workspace_mode: String,
    #[serde(default = "default_layout_right_panel_view")]
    layout_right_panel_view: String,
    #[serde(default)]
    layout_file_panel_collapsed: bool,
    #[serde(default)]
    command_palette_recent: Vec<String>,
    #[serde(default)]
    command_palette_favorites: Vec<String>,
    #[serde(default)]
    command_palette_macros: Vec<CommandPaletteMacro>,
}

impl Default for PersistedConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            api_key: String::new(),
            providers: BTreeMap::new(),
            model: default_model(),
            task_route: default_task_route(),
            projects: Vec::new(),
            last_workspace: None,
            policy_profile: default_policy_profile(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
            proxy_enabled: false,
            proxy_url: String::new(),
            proxy_use_system: true,
            proxy_scheme: default_proxy_scheme(),
            proxy_host: String::new(),
            proxy_port: String::new(),
            proxy_username: String::new(),
            proxy_password: String::new(),
            proxy_no_proxy: String::new(),
            remote_enabled: false,
            remote_bind_host: default_remote_bind_host(),
            remote_port: default_remote_port(),
            remote_access_token: String::new(),
            remote_role_view: true,
            remote_role_chat: true,
            remote_role_approve: true,
            remote_role_files: true,
            remote_allowed_origins: String::new(),
            remote_rate_limit_per_minute: default_remote_rate_limit_per_minute(),
            remote_audit_enabled: true,
            context_recent_messages: default_context_recent_messages(),
            context_relevant_messages: default_context_relevant_messages(),
            context_recent_runs: default_context_recent_runs(),
            layout_workspace_mode: default_layout_workspace_mode(),
            layout_right_panel_view: default_layout_right_panel_view(),
            layout_file_panel_collapsed: false,
            command_palette_recent: Vec::new(),
            command_palette_favorites: Vec::new(),
            command_palette_macros: Vec::new(),
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
        let (proxy_scheme, proxy_host, proxy_port, proxy_username, proxy_password) =
            normalized_proxy_parts(&persisted);
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
        let mut projects = normalize_project_states(persisted.projects);
        if let Some(path) = persisted.last_workspace.clone() {
            remember_project_state(&mut projects, path);
        }
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
            projects,
            policy_profile: normalize_policy_profile(&persisted.policy_profile),
            require_shell_approval: persisted.require_shell_approval,
            require_write_approval: persisted.require_write_approval,
            require_paid_api_approval: persisted.require_paid_api_approval,
            require_desktop_approval: persisted.require_desktop_approval,
            require_external_approval: persisted.require_external_approval,
            require_orchestration_approval: persisted.require_orchestration_approval,
            allow_destructive_shell: persisted.allow_destructive_shell,
            task_route: normalize_task_route(&persisted.task_route),
            proxy_enabled: persisted.proxy_enabled,
            proxy_url: persisted.proxy_url.trim().to_string(),
            proxy_use_system: persisted.proxy_use_system,
            proxy_scheme,
            proxy_host,
            proxy_port,
            proxy_username,
            proxy_password,
            proxy_no_proxy: persisted.proxy_no_proxy.trim().to_string(),
            remote_enabled: persisted.remote_enabled,
            remote_bind_host: normalize_remote_bind_host(&persisted.remote_bind_host),
            remote_port: normalize_remote_port(persisted.remote_port),
            remote_access_token: persisted.remote_access_token.trim().to_string(),
            remote_role_view: persisted.remote_role_view,
            remote_role_chat: persisted.remote_role_chat,
            remote_role_approve: persisted.remote_role_approve,
            remote_role_files: persisted.remote_role_files,
            remote_allowed_origins: normalize_remote_allowed_origins(
                &persisted.remote_allowed_origins,
            ),
            remote_rate_limit_per_minute: normalize_remote_rate_limit_per_minute(
                persisted.remote_rate_limit_per_minute,
            ),
            remote_audit_enabled: persisted.remote_audit_enabled,
            context_recent_messages: normalize_context_recent_messages(
                persisted.context_recent_messages,
            ),
            context_relevant_messages: normalize_context_relevant_messages(
                persisted.context_relevant_messages,
            ),
            context_recent_runs: normalize_context_recent_runs(persisted.context_recent_runs),
            layout_workspace_mode: normalize_layout_workspace_mode(
                &persisted.layout_workspace_mode,
            ),
            layout_right_panel_view: normalize_layout_right_panel_view(
                &persisted.layout_right_panel_view,
            ),
            layout_file_panel_collapsed: persisted.layout_file_panel_collapsed,
            command_palette_recent: normalize_command_palette_ids(
                persisted.command_palette_recent,
                24,
            ),
            command_palette_favorites: normalize_command_palette_ids(
                persisted.command_palette_favorites,
                80,
            ),
            command_palette_macros: normalize_command_palette_macros(
                persisted.command_palette_macros,
            ),
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
            projects: normalize_project_states(self.projects.clone()),
            last_workspace: self.last_workspace.clone(),
            policy_profile: normalize_policy_profile(&self.policy_profile),
            require_shell_approval: self.require_shell_approval,
            require_write_approval: self.require_write_approval,
            require_paid_api_approval: self.require_paid_api_approval,
            require_desktop_approval: self.require_desktop_approval,
            require_external_approval: self.require_external_approval,
            require_orchestration_approval: self.require_orchestration_approval,
            allow_destructive_shell: self.allow_destructive_shell,
            proxy_enabled: self.proxy_enabled,
            proxy_url: self.manual_proxy_url().unwrap_or_default(),
            proxy_use_system: self.proxy_use_system,
            proxy_scheme: normalize_proxy_scheme(&self.proxy_scheme),
            proxy_host: self.proxy_host.trim().to_string(),
            proxy_port: self.proxy_port.trim().to_string(),
            proxy_username: self.proxy_username.trim().to_string(),
            proxy_password: self.proxy_password.clone(),
            proxy_no_proxy: self.proxy_no_proxy.trim().to_string(),
            remote_enabled: self.remote_enabled,
            remote_bind_host: normalize_remote_bind_host(&self.remote_bind_host),
            remote_port: normalize_remote_port(self.remote_port),
            remote_access_token: self.remote_access_token.trim().to_string(),
            remote_role_view: self.remote_role_view,
            remote_role_chat: self.remote_role_chat,
            remote_role_approve: self.remote_role_approve,
            remote_role_files: self.remote_role_files,
            remote_allowed_origins: normalize_remote_allowed_origins(&self.remote_allowed_origins),
            remote_rate_limit_per_minute: normalize_remote_rate_limit_per_minute(
                self.remote_rate_limit_per_minute,
            ),
            remote_audit_enabled: self.remote_audit_enabled,
            context_recent_messages: normalize_context_recent_messages(
                self.context_recent_messages,
            ),
            context_relevant_messages: normalize_context_relevant_messages(
                self.context_relevant_messages,
            ),
            context_recent_runs: normalize_context_recent_runs(self.context_recent_runs),
            layout_workspace_mode: normalize_layout_workspace_mode(&self.layout_workspace_mode),
            layout_right_panel_view: normalize_layout_right_panel_view(
                &self.layout_right_panel_view,
            ),
            layout_file_panel_collapsed: self.layout_file_panel_collapsed,
            command_palette_recent: normalize_command_palette_ids(
                self.command_palette_recent.clone(),
                24,
            ),
            command_palette_favorites: normalize_command_palette_ids(
                self.command_palette_favorites.clone(),
                80,
            ),
            command_palette_macros: normalize_command_palette_macros(
                self.command_palette_macros.clone(),
            ),
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

    pub fn remember_project(&mut self, path: PathBuf) {
        remember_project_state(&mut self.projects, path);
    }

    pub fn project_state(&self, path: &Path) -> Option<&ProjectUiState> {
        let key = project_path_key(path);
        self.projects
            .iter()
            .find(|project| project_path_key(&project.path) == key)
    }

    pub fn project_state_mut(&mut self, path: &Path) -> Option<&mut ProjectUiState> {
        let key = project_path_key(path);
        self.projects
            .iter_mut()
            .find(|project| project_path_key(&project.path) == key)
    }

    pub fn remove_project(&mut self, path: &Path) -> bool {
        let key = project_path_key(path);
        let before = self.projects.len();
        self.projects
            .retain(|project| project_path_key(&project.path) != key);
        before != self.projects.len()
    }

    pub fn set_project_display_name(&mut self, path: &Path, display_name: &str) -> bool {
        let Some(project) = self.project_state_mut(path) else {
            return false;
        };
        project.display_name = display_name.trim().to_string();
        true
    }

    pub fn toggle_project_pinned(&mut self, path: &Path) -> bool {
        let Some(project) = self.project_state_mut(path) else {
            return false;
        };
        project.pinned = !project.pinned;
        true
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

    pub fn normalize_proxy_settings(&mut self) {
        self.proxy_scheme = normalize_proxy_scheme(&self.proxy_scheme);
        self.proxy_host = self.proxy_host.trim().to_string();
        self.proxy_port = self.proxy_port.trim().to_string();
        self.proxy_username = self.proxy_username.trim().to_string();
        self.proxy_no_proxy = self.proxy_no_proxy.trim().to_string();
        self.proxy_url = self.manual_proxy_url().unwrap_or_default();
    }

    pub fn manual_proxy_url(&self) -> Option<String> {
        if !self.proxy_host.trim().is_empty() {
            let scheme = normalize_proxy_scheme(&self.proxy_scheme);
            let host = self.proxy_host.trim();
            let port = self.proxy_port.trim();
            let authority = if port.is_empty() {
                host.to_string()
            } else {
                format!("{host}:{port}")
            };
            return Some(format!("{scheme}://{authority}"));
        }

        let proxy_url = self.proxy_url.trim();
        if proxy_url.is_empty() {
            None
        } else {
            Some(proxy_url.to_string())
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
        PERMISSION_AUTO => "Автоматически работать в проекте; спрашивать для платных API, рабочего стола и внешних открытий.",
        PERMISSION_WORK => "Автоматически менять проект и вызывать API ассетов; спрашивать для рабочего стола и внешних открытий.",
        PERMISSION_FULL => "Полный доступ: выполнять действия без подтверждений, сохраняя проверки путей рабочей папки.",
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

fn default_proxy_scheme() -> String {
    "http".to_string()
}

fn default_remote_bind_host() -> String {
    "127.0.0.1".to_string()
}

fn default_remote_port() -> u16 {
    17890
}

fn default_remote_rate_limit_per_minute() -> u32 {
    120
}

fn default_context_recent_messages() -> usize {
    14
}

fn default_context_relevant_messages() -> usize {
    8
}

fn default_context_recent_runs() -> usize {
    5
}

fn default_layout_workspace_mode() -> String {
    "chat".to_string()
}

fn default_layout_right_panel_view() -> String {
    "context".to_string()
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

fn normalize_remote_bind_host(host: &str) -> String {
    let host = host.trim();
    if host.is_empty() {
        default_remote_bind_host()
    } else {
        host.to_string()
    }
}

fn normalize_remote_port(port: u16) -> u16 {
    if port == 0 {
        default_remote_port()
    } else {
        port
    }
}

fn normalize_remote_allowed_origins(value: &str) -> String {
    value
        .split(|ch| matches!(ch, '\n' | ',' | ';'))
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .take(32)
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_remote_rate_limit_per_minute(value: u32) -> u32 {
    if value == 0 {
        0
    } else {
        value.clamp(10, 5_000)
    }
}

fn normalize_context_recent_messages(value: usize) -> usize {
    value.min(80)
}

fn normalize_context_relevant_messages(value: usize) -> usize {
    value.min(40)
}

fn normalize_context_recent_runs(value: usize) -> usize {
    value.min(20)
}

fn normalize_layout_workspace_mode(value: &str) -> String {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "chat" | "code" | "assets" | "project" => {
            value.trim().to_ascii_lowercase().replace('-', "_")
        }
        _ => default_layout_workspace_mode(),
    }
}

fn normalize_layout_right_panel_view(value: &str) -> String {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "overview" | "context" | "roadmap" | "release" | "project" | "assets" | "control"
        | "logs" => value.trim().to_ascii_lowercase().replace('-', "_"),
        _ => default_layout_right_panel_view(),
    }
}

fn normalize_command_palette_ids(ids: Vec<String>, limit: usize) -> Vec<String> {
    let mut normalized = Vec::new();
    for id in ids {
        let id = id.trim().to_string();
        if id.is_empty() || normalized.iter().any(|existing| existing == &id) {
            continue;
        }
        normalized.push(id);
        if normalized.len() >= limit {
            break;
        }
    }
    normalized
}

fn normalize_command_palette_macros(macros: Vec<CommandPaletteMacro>) -> Vec<CommandPaletteMacro> {
    let mut normalized = Vec::new();
    for (index, mut command_macro) in macros.into_iter().enumerate() {
        command_macro.name = command_macro.name.trim().to_string();
        command_macro.description = command_macro.description.trim().to_string();
        command_macro.command_ids = normalize_command_palette_ids(command_macro.command_ids, 12)
            .into_iter()
            .filter(|id| !id.starts_with("macro:"))
            .collect();
        command_macro.id =
            normalize_command_macro_id(&command_macro.id, &command_macro.name, index);

        if command_macro.name.is_empty() || command_macro.command_ids.is_empty() {
            continue;
        }
        if normalized
            .iter()
            .any(|existing: &CommandPaletteMacro| existing.id == command_macro.id)
        {
            continue;
        }
        normalized.push(command_macro);
        if normalized.len() >= 40 {
            break;
        }
    }
    normalized
}

fn normalize_command_macro_id(id: &str, name: &str, index: usize) -> String {
    let source = if id.trim().is_empty() { name } else { id };
    let slug = source
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else if ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if compact.is_empty() {
        format!("macro-{index}")
    } else {
        compact
    }
}

fn normalize_project_states(projects: Vec<ProjectUiState>) -> Vec<ProjectUiState> {
    let mut normalized = Vec::new();
    for project in projects {
        remember_project_state_with_state(&mut normalized, project);
    }
    normalized
}

fn remember_project_state(projects: &mut Vec<ProjectUiState>, path: PathBuf) {
    remember_project_state_with_state(
        projects,
        ProjectUiState {
            path,
            display_name: String::new(),
            pinned: false,
            expanded: false,
            expanded_dirs: Vec::new(),
        },
    );
}

fn remember_project_state_with_state(
    projects: &mut Vec<ProjectUiState>,
    mut state: ProjectUiState,
) {
    state.path = readable_project_path(state.path);
    state.display_name = state.display_name.trim().to_string();
    let key = project_path_key(&state.path);
    if key.is_empty() {
        return;
    }
    state.expanded_dirs = normalize_expanded_dirs(state.expanded_dirs);

    if let Some(existing) = projects
        .iter_mut()
        .find(|project| project_path_key(&project.path) == key)
    {
        if project_path_string(&existing.path).starts_with("//?/")
            || project_path_string(&existing.path).starts_with("\\\\?\\")
        {
            existing.path = state.path;
        }
        if existing.display_name.trim().is_empty() && !state.display_name.trim().is_empty() {
            existing.display_name = state.display_name;
        }
        existing.pinned |= state.pinned;
        existing.expanded |= state.expanded;
        for dir in state.expanded_dirs {
            if !existing
                .expanded_dirs
                .iter()
                .any(|existing| existing == &dir)
            {
                existing.expanded_dirs.push(dir);
            }
        }
        return;
    }

    projects.push(state);
}

fn normalize_expanded_dirs(dirs: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for dir in dirs {
        let dir = normalize_project_dir(&dir);
        if dir.is_empty() || normalized.iter().any(|existing| existing == &dir) {
            continue;
        }
        normalized.push(dir);
    }
    normalized
}

fn normalize_project_dir(dir: &str) -> String {
    let dir = dir.trim().replace('\\', "/");
    let dir = dir.trim_matches('/').trim();
    if dir.is_empty() || dir == "." || dir.contains("..") {
        String::new()
    } else {
        format!("{dir}/")
    }
}

fn project_path_key(path: &Path) -> String {
    strip_windows_extended_prefix(&path.to_string_lossy().replace('\\', "/"))
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn readable_project_path(path: PathBuf) -> PathBuf {
    let stripped = strip_windows_extended_prefix(&path.to_string_lossy().replace('\\', "/"));
    if stripped.len() > 3 {
        PathBuf::from(stripped.trim_end_matches('/'))
    } else {
        PathBuf::from(stripped)
    }
}

fn project_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn strip_windows_extended_prefix(path: &str) -> String {
    path.strip_prefix("//?/")
        .or_else(|| path.strip_prefix("\\\\?\\"))
        .unwrap_or(path)
        .to_string()
}

pub fn normalize_proxy_scheme(proxy_scheme: &str) -> String {
    match proxy_scheme.trim().to_ascii_lowercase().as_str() {
        "https" => "https".to_string(),
        "socks5" => "socks5".to_string(),
        "socks5h" => "socks5h".to_string(),
        _ => "http".to_string(),
    }
}

fn normalized_proxy_parts(persisted: &PersistedConfig) -> (String, String, String, String, String) {
    let mut scheme = normalize_proxy_scheme(&persisted.proxy_scheme);
    let mut host = persisted.proxy_host.trim().to_string();
    let mut port = persisted.proxy_port.trim().to_string();
    let mut username = persisted.proxy_username.trim().to_string();
    let mut password = persisted.proxy_password.clone();

    if host.is_empty() {
        if let Ok(url) = reqwest::Url::parse(persisted.proxy_url.trim()) {
            scheme = normalize_proxy_scheme(url.scheme());
            host = url.host_str().unwrap_or_default().to_string();
            port = url.port().map(|port| port.to_string()).unwrap_or_default();
            if !url.username().is_empty() {
                username = url.username().to_string();
            }
            if let Some(url_password) = url.password() {
                password = url_password.to_string();
            }
        }
    }

    (scheme, host, port, username, password)
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
            projects: Vec::new(),
            last_workspace: None,
            policy_profile: PERMISSION_ASK.to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
            proxy_enabled: false,
            proxy_url: String::new(),
            proxy_use_system: true,
            proxy_scheme: default_proxy_scheme(),
            proxy_host: String::new(),
            proxy_port: String::new(),
            proxy_username: String::new(),
            proxy_password: String::new(),
            proxy_no_proxy: String::new(),
            remote_enabled: false,
            remote_bind_host: default_remote_bind_host(),
            remote_port: default_remote_port(),
            remote_access_token: String::new(),
            remote_role_view: true,
            remote_role_chat: true,
            remote_role_approve: true,
            remote_role_files: true,
            remote_allowed_origins: String::new(),
            remote_rate_limit_per_minute: default_remote_rate_limit_per_minute(),
            remote_audit_enabled: true,
            context_recent_messages: default_context_recent_messages(),
            context_relevant_messages: default_context_relevant_messages(),
            context_recent_runs: default_context_recent_runs(),
            layout_workspace_mode: default_layout_workspace_mode(),
            layout_right_panel_view: default_layout_right_panel_view(),
            layout_file_panel_collapsed: false,
            command_palette_recent: Vec::new(),
            command_palette_favorites: Vec::new(),
            command_palette_macros: Vec::new(),
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
            projects: Vec::new(),
            last_workspace: None,
            policy_profile: PERMISSION_ASK.to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
            proxy_enabled: false,
            proxy_url: String::new(),
            proxy_use_system: true,
            proxy_scheme: default_proxy_scheme(),
            proxy_host: String::new(),
            proxy_port: String::new(),
            proxy_username: String::new(),
            proxy_password: String::new(),
            proxy_no_proxy: String::new(),
            remote_enabled: false,
            remote_bind_host: default_remote_bind_host(),
            remote_port: default_remote_port(),
            remote_access_token: String::new(),
            remote_role_view: true,
            remote_role_chat: true,
            remote_role_approve: true,
            remote_role_files: true,
            remote_allowed_origins: String::new(),
            remote_rate_limit_per_minute: default_remote_rate_limit_per_minute(),
            remote_audit_enabled: true,
            context_recent_messages: default_context_recent_messages(),
            context_relevant_messages: default_context_relevant_messages(),
            context_recent_runs: default_context_recent_runs(),
            layout_workspace_mode: default_layout_workspace_mode(),
            layout_right_panel_view: default_layout_right_panel_view(),
            layout_file_panel_collapsed: false,
            command_palette_recent: Vec::new(),
            command_palette_favorites: Vec::new(),
            command_palette_macros: Vec::new(),
        };

        let json = serde_json::to_string(&config).expect("serializes config");

        assert!(json.contains("\"providers\""));
        assert!(json.contains("\"openai\""));
        assert!(json.contains("\"model\":\"gpt-5.4\""));
    }

    #[test]
    fn migrates_legacy_proxy_url_into_structured_fields() {
        let persisted = PersistedConfig {
            proxy_enabled: true,
            proxy_url: "socks5h://user:pass@127.0.0.1:1080".to_string(),
            ..PersistedConfig::default()
        };

        let (scheme, host, port, username, password) = normalized_proxy_parts(&persisted);

        assert_eq!(scheme, "socks5h");
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, "1080");
        assert_eq!(username, "user");
        assert_eq!(password, "pass");
    }

    #[test]
    fn normalizes_project_navigation_state() {
        let projects = normalize_project_states(vec![
            ProjectUiState {
                path: PathBuf::from("C:/Projects/Game"),
                display_name: "Game Client".to_string(),
                pinned: false,
                expanded: true,
                expanded_dirs: vec![
                    "src".to_string(),
                    "src/".to_string(),
                    "../escape".to_string(),
                ],
            },
            ProjectUiState {
                path: PathBuf::from("//?/C:/Projects/Game/"),
                display_name: String::new(),
                pinned: true,
                expanded: false,
                expanded_dirs: vec!["assets/generated".to_string()],
            },
        ]);

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].path, PathBuf::from("C:/Projects/Game"));
        assert_eq!(projects[0].display_name, "Game Client");
        assert!(projects[0].pinned);
        assert!(projects[0].expanded);
        assert_eq!(projects[0].expanded_dirs, vec!["src/", "assets/generated/"]);
    }

    #[test]
    fn serializes_and_normalizes_layout_state() {
        let config = PersistedConfig {
            layout_workspace_mode: "code".to_string(),
            layout_right_panel_view: "project".to_string(),
            layout_file_panel_collapsed: true,
            command_palette_recent: Vec::new(),
            command_palette_favorites: Vec::new(),
            command_palette_macros: Vec::new(),
            ..PersistedConfig::default()
        };

        let json = serde_json::to_string(&config).expect("serializes config");

        assert!(json.contains("\"layout_workspace_mode\":\"code\""));
        assert!(json.contains("\"layout_right_panel_view\":\"project\""));
        assert!(json.contains("\"layout_file_panel_collapsed\":true"));
        assert_eq!(normalize_layout_workspace_mode("bad-value"), "chat");
        assert_eq!(normalize_layout_right_panel_view("bad-value"), "context");
    }

    #[test]
    fn normalizes_remote_control_defaults() {
        assert_eq!(normalize_remote_bind_host(""), "127.0.0.1");
        assert_eq!(normalize_remote_bind_host(" 0.0.0.0 "), "0.0.0.0");
        assert_eq!(normalize_remote_port(0), 17890);
        assert_eq!(normalize_remote_port(18080), 18080);
        assert_eq!(
            normalize_remote_allowed_origins(" https://a.test, http://b.test\n; "),
            "https://a.test\nhttp://b.test"
        );
        assert_eq!(normalize_remote_rate_limit_per_minute(0), 0);
        assert_eq!(normalize_remote_rate_limit_per_minute(1), 10);
        assert_eq!(normalize_remote_rate_limit_per_minute(6_000), 5_000);
    }

    #[test]
    fn normalizes_command_palette_state() {
        let macros = normalize_command_palette_macros(vec![
            CommandPaletteMacro {
                id: String::new(),
                name: "Daily flow".to_string(),
                description: "Run daily checks".to_string(),
                confirm_each_step: true,
                command_ids: vec![
                    "git:status".to_string(),
                    "git:status".to_string(),
                    "macro:loop".to_string(),
                    "project:refresh".to_string(),
                ],
            },
            CommandPaletteMacro {
                id: "empty".to_string(),
                name: "Empty".to_string(),
                description: String::new(),
                confirm_each_step: false,
                command_ids: Vec::new(),
            },
        ]);

        assert_eq!(
            normalize_command_palette_ids(
                vec![
                    "git:status".to_string(),
                    "git:status".to_string(),
                    " ".to_string(),
                    "project:refresh".to_string(),
                ],
                8,
            ),
            vec!["git:status", "project:refresh"]
        );
        assert_eq!(macros.len(), 1);
        assert_eq!(macros[0].id, "daily-flow");
        assert!(macros[0].confirm_each_step);
        assert_eq!(macros[0].command_ids, vec!["git:status", "project:refresh"]);
    }

    #[test]
    fn selects_default_model_for_new_provider() {
        let mut config = AppConfig {
            provider: OPENAI_PROVIDER_ID.to_string(),
            api_key: String::new(),
            model: "gpt-5.5".to_string(),
            providers: BTreeMap::new(),
            last_workspace: None,
            projects: Vec::new(),
            policy_profile: PERMISSION_ASK.to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
            task_route: "auto".to_string(),
            proxy_enabled: false,
            proxy_url: String::new(),
            proxy_use_system: true,
            proxy_scheme: default_proxy_scheme(),
            proxy_host: String::new(),
            proxy_port: String::new(),
            proxy_username: String::new(),
            proxy_password: String::new(),
            proxy_no_proxy: String::new(),
            remote_enabled: false,
            remote_bind_host: default_remote_bind_host(),
            remote_port: default_remote_port(),
            remote_access_token: String::new(),
            remote_role_view: true,
            remote_role_chat: true,
            remote_role_approve: true,
            remote_role_files: true,
            remote_allowed_origins: String::new(),
            remote_rate_limit_per_minute: default_remote_rate_limit_per_minute(),
            remote_audit_enabled: true,
            context_recent_messages: default_context_recent_messages(),
            context_relevant_messages: default_context_relevant_messages(),
            context_recent_runs: default_context_recent_runs(),
            layout_workspace_mode: default_layout_workspace_mode(),
            layout_right_panel_view: default_layout_right_panel_view(),
            layout_file_panel_collapsed: false,
            command_palette_recent: Vec::new(),
            command_palette_favorites: Vec::new(),
            command_palette_macros: Vec::new(),
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
            projects: Vec::new(),
            policy_profile: PERMISSION_ASK.to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
            task_route: "auto".to_string(),
            proxy_enabled: false,
            proxy_url: String::new(),
            proxy_use_system: true,
            proxy_scheme: default_proxy_scheme(),
            proxy_host: String::new(),
            proxy_port: String::new(),
            proxy_username: String::new(),
            proxy_password: String::new(),
            proxy_no_proxy: String::new(),
            remote_enabled: false,
            remote_bind_host: default_remote_bind_host(),
            remote_port: default_remote_port(),
            remote_access_token: String::new(),
            remote_role_view: true,
            remote_role_chat: true,
            remote_role_approve: true,
            remote_role_files: true,
            remote_allowed_origins: String::new(),
            remote_rate_limit_per_minute: default_remote_rate_limit_per_minute(),
            remote_audit_enabled: true,
            context_recent_messages: default_context_recent_messages(),
            context_relevant_messages: default_context_relevant_messages(),
            context_recent_runs: default_context_recent_runs(),
            layout_workspace_mode: default_layout_workspace_mode(),
            layout_right_panel_view: default_layout_right_panel_view(),
            layout_file_panel_collapsed: false,
            command_palette_recent: Vec::new(),
            command_palette_favorites: Vec::new(),
            command_palette_macros: Vec::new(),
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
            projects: Vec::new(),
            policy_profile: PERMISSION_ASK.to_string(),
            require_shell_approval: true,
            require_write_approval: true,
            require_paid_api_approval: true,
            require_desktop_approval: true,
            require_external_approval: true,
            require_orchestration_approval: true,
            allow_destructive_shell: false,
            task_route: "auto".to_string(),
            proxy_enabled: false,
            proxy_url: String::new(),
            proxy_use_system: true,
            proxy_scheme: default_proxy_scheme(),
            proxy_host: String::new(),
            proxy_port: String::new(),
            proxy_username: String::new(),
            proxy_password: String::new(),
            proxy_no_proxy: String::new(),
            remote_enabled: false,
            remote_bind_host: default_remote_bind_host(),
            remote_port: default_remote_port(),
            remote_access_token: String::new(),
            remote_role_view: true,
            remote_role_chat: true,
            remote_role_approve: true,
            remote_role_files: true,
            remote_allowed_origins: String::new(),
            remote_rate_limit_per_minute: default_remote_rate_limit_per_minute(),
            remote_audit_enabled: true,
            context_recent_messages: default_context_recent_messages(),
            context_relevant_messages: default_context_relevant_messages(),
            context_recent_runs: default_context_recent_runs(),
            layout_workspace_mode: default_layout_workspace_mode(),
            layout_right_panel_view: default_layout_right_panel_view(),
            layout_file_panel_collapsed: false,
            command_palette_recent: Vec::new(),
            command_palette_favorites: Vec::new(),
            command_palette_macros: Vec::new(),
        };

        config.set_policy_profile(PERMISSION_FULL);

        assert!(!config.effective_require_shell_approval());
        assert!(!config.effective_require_write_approval());
        assert!(!config.effective_require_paid_api_approval());
        assert!(!config.effective_require_desktop_approval());
        assert!(config.effective_allow_destructive_shell());
    }
}
