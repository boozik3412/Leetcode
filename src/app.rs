use crate::agent::models::{
    models_for_provider, provider_name, provider_specs, GEMINI_PROVIDER_ID, OPENAI_PROVIDER_ID,
};
use crate::agent::routing::route_labels;
use crate::agent::types::{AppEvent, ChatLine, ChatRole, ToolLogLine};
use crate::agent::{run_user_turn, AgentState};
use crate::assets::{
    absolute_output_path, asset_provider_env_var, attach_asset_context, audio_provider_name,
    default_audio_model, default_image_model, default_video_model, export_asset,
    image_provider_env_var, image_provider_name, image_provider_specs, image_request_from_job,
    is_image_path, load_jobs, run_audio_job, run_image_job, run_spritesheet_job, run_video_job,
    upscale_asset, video_provider_name, AssetEvent, AssetJob, AssetStatus, AudioAssetRequest,
    ImageAssetRequest, SpritesheetAssetRequest, VideoAssetRequest, GEMINI_IMAGE_PROVIDER_ID,
    OPENAI_AUDIO_PROVIDER_ID, OPENAI_IMAGE_PROVIDER_ID, OPENAI_VIDEO_PROVIDER_ID,
};
use crate::config::{
    append_journal, clear_journal, policy_profile_labels, read_journal_tail, AppConfig,
};
use crate::game_workflows::{
    parse_workflow_kind, run_game_workflow, workflow_specs, GameWorkflowRequest,
};
use crate::orchestration::{
    agent_role_specs, export_trace, load_orchestration_state, orchestration_snapshot,
    parse_agent_role, record_handoff,
};
use crate::project::{detect_project_profiles, ProjectCommand, ProjectProfile};
use crate::terminal::{
    clear_terminal_output, read_terminal_snapshot, start_terminal_session, stop_terminal_session,
    write_terminal_input,
};
use crate::tools::policy::{ApprovalMap, PolicyConfig};
use crate::tools::shell::{run_shell, RunShellArgs};
use crate::workspace::Workspace;
use eframe::egui::{self, RichText, TextEdit};
use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct LeetcodeApp {
    config: AppConfig,
    provider_input: String,
    api_key_input: String,
    model_input: String,
    workspace: Option<Workspace>,
    file_rows: Vec<String>,
    selected_file: Option<String>,
    selected_preview: String,
    original_file_content: String,
    selected_file_editable: bool,
    editor_status: String,
    input: String,
    chat: Vec<ChatLine>,
    tool_log: Vec<ToolLogLine>,
    journal_lines: Vec<String>,
    journal_status: String,
    git_summary: String,
    project_profiles: Vec<ProjectProfile>,
    project_events_rx: Option<Receiver<AppEvent>>,
    project_is_running: bool,
    project_cancel: Option<Arc<AtomicBool>>,
    project_status: String,
    desktop_status: String,
    desktop_last_screenshot: Option<String>,
    desktop_active_window: String,
    terminal_input: String,
    terminal_output: String,
    terminal_status: String,
    terminal_running: bool,
    orchestration_status: String,
    asset_provider_input: String,
    asset_kind_input: String,
    asset_api_key_input: String,
    asset_model_input: String,
    asset_prompt: String,
    asset_aspect_ratio: String,
    asset_image_size: String,
    asset_jobs: Vec<AssetJob>,
    asset_events_rx: Option<Receiver<AssetEvent>>,
    asset_is_running: bool,
    asset_status: String,
    asset_previews: HashMap<String, egui::TextureHandle>,
    events_rx: Option<Receiver<AppEvent>>,
    is_running: bool,
    cancel: Option<Arc<AtomicBool>>,
    agent_state: Arc<Mutex<AgentState>>,
    approvals: ApprovalMap,
    pending_approval: Option<PendingApproval>,
}

#[derive(Clone, Debug)]
struct PendingApproval {
    id: String,
    summary: String,
    detail: String,
}

impl LeetcodeApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let config = AppConfig::load();
        let workspace = config
            .last_workspace
            .clone()
            .and_then(|path| Workspace::new(path).ok());
        let file_rows = workspace
            .as_ref()
            .map(|workspace| workspace.ui_file_rows(600))
            .unwrap_or_default();
        let asset_jobs = workspace.as_ref().map(load_jobs).unwrap_or_default();
        let project_profiles = workspace
            .as_ref()
            .map(detect_project_profiles)
            .unwrap_or_default();

        let api_key_input = config.api_key.clone();
        let model_input = config.model.clone();
        let provider_input = config.provider.clone();
        let asset_provider_input = OPENAI_IMAGE_PROVIDER_ID.to_string();
        let asset_api_key_input = image_api_key_from_config(&config, &asset_provider_input);
        let asset_model_input = image_model_from_config(&config, &asset_provider_input);
        let journal_lines = read_journal_tail(200);
        let approvals = Arc::new(Mutex::new(HashMap::new()));

        Self {
            config,
            provider_input,
            api_key_input,
            model_input,
            workspace,
            file_rows,
            selected_file: None,
            selected_preview: String::new(),
            original_file_content: String::new(),
            selected_file_editable: false,
            editor_status: String::new(),
            input: String::new(),
            chat: vec![ChatLine::system(
                "Выбери проект, проверь модель/API key и отправь задачу агенту.",
            )],
            tool_log: Vec::new(),
            journal_lines,
            journal_status: String::new(),
            git_summary: String::new(),
            project_profiles,
            project_events_rx: None,
            project_is_running: false,
            project_cancel: None,
            project_status: String::new(),
            desktop_status: String::new(),
            desktop_last_screenshot: None,
            desktop_active_window: String::new(),
            terminal_input: String::new(),
            terminal_output: String::new(),
            terminal_status: String::new(),
            terminal_running: false,
            orchestration_status: String::new(),
            asset_provider_input,
            asset_kind_input: "image".to_string(),
            asset_api_key_input,
            asset_model_input,
            asset_prompt: String::new(),
            asset_aspect_ratio: "1:1".to_string(),
            asset_image_size: "1K".to_string(),
            asset_jobs,
            asset_events_rx: None,
            asset_is_running: false,
            asset_status: String::new(),
            asset_previews: HashMap::new(),
            events_rx: None,
            is_running: false,
            cancel: None,
            agent_state: Arc::new(Mutex::new(AgentState::default())),
            approvals,
            pending_approval: None,
        }
    }

    fn choose_workspace(&mut self) {
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };

        match Workspace::new(path.clone()) {
            Ok(workspace) => {
                self.sync_config_from_inputs();
                self.sync_asset_provider_settings();
                self.config.last_workspace = Some(path);
                self.workspace = Some(workspace);
                self.refresh_file_rows();
                self.refresh_project_profiles();
                self.asset_jobs = self.workspace.as_ref().map(load_jobs).unwrap_or_default();
                self.asset_previews.clear();
                self.selected_file = None;
                self.selected_preview.clear();
                self.original_file_content.clear();
                self.selected_file_editable = false;
                self.editor_status.clear();
                self.refresh_git_summary();
                self.agent_state
                    .lock()
                    .expect("agent state poisoned")
                    .reset();
                let _ = self.config.save();
            }
            Err(err) => self.chat.push(ChatLine::system(format!(
                "Не удалось открыть workspace: {err}"
            ))),
        }
    }

    fn refresh_file_rows(&mut self) {
        self.file_rows = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.ui_file_rows(600))
            .unwrap_or_default();
    }

    fn refresh_project_profiles(&mut self) {
        self.project_profiles = self
            .workspace
            .as_ref()
            .map(detect_project_profiles)
            .unwrap_or_default();
    }

    fn sync_config_from_inputs(&mut self) {
        self.config.set_active_provider_settings(
            &self.provider_input,
            self.model_input.trim().to_string(),
            self.api_key_input.trim().to_string(),
        );
    }

    fn sync_asset_provider_settings(&mut self) {
        let provider_id = match self.asset_kind_input.as_str() {
            "audio" => OPENAI_AUDIO_PROVIDER_ID,
            "video" => OPENAI_VIDEO_PROVIDER_ID,
            _ => self.asset_provider_input.as_str(),
        };
        let default_model = match self.asset_kind_input.as_str() {
            "audio" => default_audio_model(provider_id),
            "video" => default_video_model(provider_id),
            _ => default_image_model(provider_id),
        };
        let model = if self.asset_model_input.trim().is_empty() {
            default_model.to_string()
        } else {
            self.asset_model_input.trim().to_string()
        };
        self.asset_model_input = model.clone();
        self.config.set_provider_settings(
            provider_id,
            model,
            self.asset_api_key_input.trim().to_string(),
        );
    }

    fn sync_asset_provider_settings_for(&mut self, provider_id: &str) {
        let model = if self.asset_model_input.trim().is_empty() {
            default_image_model(provider_id).to_string()
        } else {
            self.asset_model_input.trim().to_string()
        };
        self.config.set_provider_settings(
            provider_id,
            model,
            self.asset_api_key_input.trim().to_string(),
        );
    }

    fn switch_provider_from_ui(&mut self, provider_id: String) {
        self.config.select_provider(&provider_id);
        self.provider_input = self.config.provider.clone();
        self.api_key_input = self.config.api_key.clone();
        self.model_input = self.config.model.clone();
    }

    fn switch_asset_provider_from_ui(&mut self, provider_id: String) {
        self.asset_provider_input = provider_id;
        self.asset_api_key_input =
            image_api_key_from_config(&self.config, &self.asset_provider_input);
        self.asset_model_input = image_model_from_config(&self.config, &self.asset_provider_input);
    }

    fn switch_asset_kind_from_ui(&mut self) {
        match self.asset_kind_input.as_str() {
            "audio" => {
                self.asset_api_key_input =
                    media_api_key_from_config(&self.config, OPENAI_AUDIO_PROVIDER_ID);
                self.asset_model_input = media_model_from_config(
                    &self.config,
                    OPENAI_AUDIO_PROVIDER_ID,
                    default_audio_model(OPENAI_AUDIO_PROVIDER_ID),
                );
            }
            "video" => {
                self.asset_api_key_input =
                    media_api_key_from_config(&self.config, OPENAI_VIDEO_PROVIDER_ID);
                self.asset_model_input = media_model_from_config(
                    &self.config,
                    OPENAI_VIDEO_PROVIDER_ID,
                    default_video_model(OPENAI_VIDEO_PROVIDER_ID),
                );
                if !["1280x720", "720x1280", "1920x1080", "1080x1920"]
                    .contains(&self.asset_image_size.as_str())
                {
                    self.asset_image_size = "1280x720".to_string();
                }
            }
            _ => {
                self.asset_api_key_input =
                    image_api_key_from_config(&self.config, &self.asset_provider_input);
                self.asset_model_input =
                    image_model_from_config(&self.config, &self.asset_provider_input);
                if !["0.5K", "1K", "2K", "4K"].contains(&self.asset_image_size.as_str()) {
                    self.asset_image_size = "1K".to_string();
                }
            }
        }
    }

    fn refresh_git_summary(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.git_summary.clear();
            return;
        };

        let status = Command::new("git")
            .arg("status")
            .arg("--short")
            .current_dir(workspace.root())
            .output();
        let diff = Command::new("git")
            .arg("diff")
            .arg("--stat")
            .current_dir(workspace.root())
            .output();

        let status_text = match status {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().is_empty() {
                    "status: clean".to_string()
                } else {
                    format!("status:\n{stdout}")
                }
            }
            Ok(output) => format!(
                "status failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ),
            Err(err) => format!("status failed: {err}"),
        };

        let diff_text = match diff {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.trim().is_empty() {
                    "diff: no unstaged diff".to_string()
                } else {
                    format!("diff:\n{stdout}")
                }
            }
            Ok(output) => format!("diff failed:\n{}", String::from_utf8_lossy(&output.stderr)),
            Err(err) => format!("diff failed: {err}"),
        };

        self.git_summary = format!("{status_text}\n\n{diff_text}");
    }

    fn refresh_journal(&mut self) {
        self.journal_lines = read_journal_tail(200);
        self.journal_status = format!("showing last {} entries", self.journal_lines.len());
    }

    fn clear_journal_from_ui(&mut self) {
        match clear_journal() {
            Ok(()) => {
                self.journal_lines.clear();
                self.journal_status = "journal cleared".to_string();
            }
            Err(err) => {
                self.journal_status = format!("journal clear failed: {err}");
            }
        }
    }

    fn start_project_command(&mut self, command: ProjectCommand) {
        if self.project_is_running {
            return;
        }

        let Some(workspace) = self.workspace.clone() else {
            self.project_status = "workspace is not selected".to_string();
            return;
        };

        self.sync_config_from_inputs();
        self.sync_asset_provider_settings();
        let _ = self.config.save();

        let (tx, rx) = mpsc::channel();
        let approvals = self.approvals.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let policy = PolicyConfig {
            require_shell_approval: self.config.effective_require_shell_approval(),
            require_write_approval: self.config.effective_require_write_approval(),
        };
        let tool_id = format!("project-{}", command.id);
        let label = command.label.clone();
        let summary = format!("{}: {}", command.label, command.command);
        let args = RunShellArgs {
            cmd: command.command,
            cwd: Some(command.cwd),
            shell: None,
            timeout_secs: Some(command.timeout_secs),
        };

        self.project_events_rx = Some(rx);
        self.project_is_running = true;
        self.project_cancel = Some(cancel);
        self.project_status = format!("running {label}");

        thread::spawn(move || {
            let _ = tx.send(AppEvent::ToolStarted {
                id: tool_id.clone(),
                name: "project_command".to_string(),
                summary,
            });
            let result = tokio::runtime::Runtime::new()
                .expect("failed to start tokio runtime")
                .block_on(run_shell(
                    &workspace,
                    args,
                    tx.clone(),
                    approvals,
                    worker_cancel,
                    policy,
                    tool_id.clone(),
                ));
            let ok = result.ok;
            let output = result.output;
            let _ = tx.send(AppEvent::ToolFinished {
                id: tool_id,
                output: output.clone(),
            });
            if !ok {
                let _ = tx.send(AppEvent::Error(output));
            }
            let _ = tx.send(AppEvent::Done);
        });
    }

    fn stop_project_command(&mut self) {
        if let Some(cancel) = &self.project_cancel {
            cancel.store(true, Ordering::SeqCst);
        }
        if self.pending_approval.is_some() {
            self.answer_approval(false);
        }
        self.project_status = "stop requested".to_string();
    }

    fn create_game_workflow_from_ui(&mut self, workflow_id: &str) {
        let Some(workspace) = &self.workspace else {
            self.project_status = "workspace is not selected".to_string();
            return;
        };
        let Some(workflow) = parse_workflow_kind(workflow_id) else {
            self.project_status = format!("unknown workflow: {workflow_id}");
            return;
        };
        let title = self
            .workspace
            .as_ref()
            .map(|workspace| workspace.display_name())
            .unwrap_or_else(|| "Game Workflow".to_string());
        let brief = if self.input.trim().is_empty() {
            format!("Workflow created from Leetcode for {title}.")
        } else {
            self.input.trim().to_string()
        };

        match run_game_workflow(
            workspace,
            GameWorkflowRequest {
                workflow,
                title,
                brief,
            },
        ) {
            Ok(result) => {
                self.project_status = format!("created {}", result.path);
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => self.project_status = format!("workflow failed: {err}"),
        }
    }

    fn open_preview_url_from_ui(&mut self, url: &str) {
        #[cfg(target_os = "windows")]
        let result = Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(url)
            .spawn()
            .map(|_| ());
        #[cfg(target_os = "macos")]
        let result = Command::new("open").arg(url).spawn().map(|_| ());
        #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
        let result = Command::new("xdg-open").arg(url).spawn().map(|_| ());

        self.project_status = match result {
            Ok(()) => format!("opened {url}"),
            Err(err) => format!("open preview failed: {err}"),
        };
    }

    fn start_image_asset_job(&mut self) {
        if self.asset_is_running {
            return;
        }

        let prompt = self.asset_prompt.trim().to_string();
        if prompt.is_empty() {
            self.asset_status = "asset prompt is empty".to_string();
            return;
        }

        self.sync_config_from_inputs();
        self.sync_asset_provider_settings();
        let _ = self.config.save();

        match self.asset_kind_input.as_str() {
            "spritesheet" => {
                let request = SpritesheetAssetRequest {
                    provider: self.asset_provider_input.clone(),
                    prompt,
                    model: self.asset_model_input.trim().to_string(),
                    aspect_ratio: self.asset_aspect_ratio.clone(),
                    image_size: self.asset_image_size.clone(),
                    columns: 4,
                    rows: 4,
                };
                self.start_spritesheet_asset_request(request);
            }
            "audio" => {
                let request = AudioAssetRequest {
                    provider: OPENAI_AUDIO_PROVIDER_ID.to_string(),
                    prompt,
                    model: if self.asset_model_input.trim().is_empty() {
                        default_audio_model(OPENAI_AUDIO_PROVIDER_ID).to_string()
                    } else {
                        self.asset_model_input.trim().to_string()
                    },
                    voice: "alloy".to_string(),
                    format: "wav".to_string(),
                };
                self.start_audio_asset_request(request);
            }
            "video" => {
                let request = VideoAssetRequest {
                    provider: OPENAI_VIDEO_PROVIDER_ID.to_string(),
                    prompt,
                    model: if self.asset_model_input.trim().is_empty() {
                        default_video_model(OPENAI_VIDEO_PROVIDER_ID).to_string()
                    } else {
                        self.asset_model_input.trim().to_string()
                    },
                    size: self.asset_image_size.clone(),
                    seconds: 8,
                };
                self.start_video_asset_request(request);
            }
            _ => {
                let request = ImageAssetRequest {
                    provider: self.asset_provider_input.clone(),
                    prompt,
                    model: self.asset_model_input.trim().to_string(),
                    aspect_ratio: self.asset_aspect_ratio.clone(),
                    image_size: self.asset_image_size.clone(),
                };
                self.start_image_asset_request(request);
            }
        }
    }

    fn start_image_asset_request(&mut self, request: ImageAssetRequest) {
        if self.asset_is_running {
            return;
        }

        let Some(workspace) = self.workspace.clone() else {
            self.asset_status = "workspace is not selected".to_string();
            return;
        };

        let api_key = image_api_key_from_config(&self.config, &request.provider);
        if api_key.trim().is_empty() {
            self.asset_status = format!(
                "Save a {} key ({}) before generating image assets",
                image_provider_name(&request.provider),
                image_provider_env_var(&request.provider)
            );
            return;
        }

        let job = AssetJob::new_image(&request);
        self.upsert_asset_job(job.clone());
        self.asset_status = format!("running {}", job.id);

        let (tx, rx) = mpsc::channel();
        self.asset_events_rx = Some(rx);
        self.asset_is_running = true;

        thread::spawn(move || {
            let final_job = tokio::runtime::Runtime::new()
                .expect("failed to start tokio runtime")
                .block_on(run_image_job(workspace, api_key, request, job));
            let _ = tx.send(AssetEvent::JobUpdated(final_job));
            let _ = tx.send(AssetEvent::Done);
        });
    }

    fn start_spritesheet_asset_request(&mut self, request: SpritesheetAssetRequest) {
        if self.asset_is_running {
            return;
        }

        let Some(workspace) = self.workspace.clone() else {
            self.asset_status = "workspace is not selected".to_string();
            return;
        };
        let api_key = image_api_key_from_config(&self.config, &request.provider);
        if api_key.trim().is_empty() {
            self.asset_status = format!(
                "Save a {} key ({}) before generating spritesheets",
                image_provider_name(&request.provider),
                image_provider_env_var(&request.provider)
            );
            return;
        }

        let job = AssetJob::new_spritesheet(&request);
        self.upsert_asset_job(job.clone());
        self.asset_status = format!("running {}", job.id);
        let (tx, rx) = mpsc::channel();
        self.asset_events_rx = Some(rx);
        self.asset_is_running = true;

        thread::spawn(move || {
            let final_job = tokio::runtime::Runtime::new()
                .expect("failed to start tokio runtime")
                .block_on(run_spritesheet_job(workspace, api_key, request, job));
            let _ = tx.send(AssetEvent::JobUpdated(final_job));
            let _ = tx.send(AssetEvent::Done);
        });
    }

    fn start_audio_asset_request(&mut self, request: AudioAssetRequest) {
        if self.asset_is_running {
            return;
        }

        let Some(workspace) = self.workspace.clone() else {
            self.asset_status = "workspace is not selected".to_string();
            return;
        };
        let api_key = media_api_key_from_config(&self.config, &request.provider);
        if api_key.trim().is_empty() {
            self.asset_status = format!(
                "Save a {} key ({}) before generating audio",
                audio_provider_name(&request.provider),
                asset_provider_env_var(&request.provider)
            );
            return;
        }

        let job = AssetJob::new_audio(&request);
        self.upsert_asset_job(job.clone());
        self.asset_status = format!("running {}", job.id);
        let (tx, rx) = mpsc::channel();
        self.asset_events_rx = Some(rx);
        self.asset_is_running = true;

        thread::spawn(move || {
            let final_job = tokio::runtime::Runtime::new()
                .expect("failed to start tokio runtime")
                .block_on(run_audio_job(workspace, api_key, request, job));
            let _ = tx.send(AssetEvent::JobUpdated(final_job));
            let _ = tx.send(AssetEvent::Done);
        });
    }

    fn start_video_asset_request(&mut self, request: VideoAssetRequest) {
        if self.asset_is_running {
            return;
        }

        let Some(workspace) = self.workspace.clone() else {
            self.asset_status = "workspace is not selected".to_string();
            return;
        };
        let api_key = media_api_key_from_config(&self.config, &request.provider);
        if api_key.trim().is_empty() {
            self.asset_status = format!(
                "Save a {} key ({}) before generating video",
                video_provider_name(&request.provider),
                asset_provider_env_var(&request.provider)
            );
            return;
        }

        let job = AssetJob::new_video(&request);
        self.upsert_asset_job(job.clone());
        self.asset_status = format!("running {}", job.id);
        let (tx, rx) = mpsc::channel();
        self.asset_events_rx = Some(rx);
        self.asset_is_running = true;

        thread::spawn(move || {
            let final_job = tokio::runtime::Runtime::new()
                .expect("failed to start tokio runtime")
                .block_on(run_video_job(workspace, api_key, request, job));
            let _ = tx.send(AssetEvent::JobUpdated(final_job));
            let _ = tx.send(AssetEvent::Done);
        });
    }

    fn regenerate_asset_job(&mut self, job: &AssetJob) {
        self.start_image_asset_request(image_request_from_job(job, None));
    }

    fn vary_asset_job(&mut self, job: &AssetJob) {
        let prompt = format!(
            "{}\n\nCreate a polished variation that keeps the same purpose, composition, and game/app asset usability, but changes visual details enough to offer a fresh option.",
            job.prompt
        );
        self.start_image_asset_request(image_request_from_job(job, Some(prompt)));
    }

    fn load_asset_job_into_form(&mut self, job: &AssetJob) {
        let request = image_request_from_job(job, None);
        self.asset_provider_input = request.provider;
        self.asset_model_input = request.model;
        self.asset_prompt = request.prompt;
        self.asset_aspect_ratio = request.aspect_ratio;
        self.asset_image_size = request.image_size;
        self.asset_api_key_input =
            image_api_key_from_config(&self.config, &self.asset_provider_input);
        self.asset_status = "loaded asset prompt".to_string();
    }

    fn open_asset_folder(&mut self, rel_path: &str) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "workspace is not selected".to_string();
            return;
        };
        let Some(path) = absolute_output_path(workspace, rel_path) else {
            self.asset_status = "asset file not found".to_string();
            return;
        };

        #[cfg(target_os = "windows")]
        let result = Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map(|_| ());
        #[cfg(not(target_os = "windows"))]
        let result = Command::new("open")
            .arg(path.parent().unwrap_or_else(|| workspace.root()))
            .spawn()
            .map(|_| ());

        self.asset_status = match result {
            Ok(()) => "opened asset folder".to_string(),
            Err(err) => format!("open asset folder failed: {err}"),
        };
    }

    fn open_generated_assets_folder(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "workspace is not selected".to_string();
            return;
        };
        let folder = match workspace.resolve_for_write("assets/generated/images") {
            Ok(path) => path,
            Err(err) => {
                self.asset_status = format!("asset folder failed: {err}");
                return;
            }
        };
        if let Err(err) = fs::create_dir_all(&folder) {
            self.asset_status = format!("asset folder failed: {err}");
            return;
        }

        #[cfg(target_os = "windows")]
        let result = Command::new("explorer").arg(&folder).spawn().map(|_| ());
        #[cfg(not(target_os = "windows"))]
        let result = Command::new("open").arg(&folder).spawn().map(|_| ());

        self.asset_status = match result {
            Ok(()) => "opened generated images".to_string(),
            Err(err) => format!("open generated images failed: {err}"),
        };
    }

    fn use_asset_as_app_icon(&mut self, rel_path: &str) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "workspace is not selected".to_string();
            return;
        };
        let Some(source) = absolute_output_path(workspace, rel_path) else {
            self.asset_status = "asset file not found".to_string();
            return;
        };
        if !is_image_path(&source) {
            self.asset_status = "asset is not an image".to_string();
            return;
        }

        let target = match workspace.resolve_for_write("assets/app-icon.png") {
            Ok(path) => path,
            Err(err) => {
                self.asset_status = format!("app icon target failed: {err}");
                return;
            }
        };
        if let Some(parent) = target.parent() {
            if let Err(err) = fs::create_dir_all(parent) {
                self.asset_status = format!("app icon directory failed: {err}");
                return;
            }
        }

        let result = fs::read(&source)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| image::load_from_memory(&bytes).map_err(anyhow::Error::from))
            .and_then(|image| {
                image
                    .save_with_format(&target, image::ImageFormat::Png)
                    .map_err(anyhow::Error::from)
            });
        match result {
            Ok(()) => {
                self.asset_status = "saved assets/app-icon.png".to_string();
                self.asset_previews.remove("assets/app-icon.png");
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => self.asset_status = format!("save app icon failed: {err}"),
        }
    }

    fn upscale_asset_output(&mut self, rel_path: &str) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "workspace is not selected".to_string();
            return;
        };
        match upscale_asset(workspace, rel_path, 2) {
            Ok(job) => {
                self.asset_status = format!("upscaled {}", job.id);
                self.upsert_asset_job(job);
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => self.asset_status = format!("upscale failed: {err}"),
        }
    }

    fn export_asset_output(&mut self, rel_path: &str) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "workspace is not selected".to_string();
            return;
        };
        match export_asset(workspace, rel_path, None) {
            Ok(job) => {
                self.asset_status = format!("exported {}", job.id);
                self.upsert_asset_job(job);
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => self.asset_status = format!("export failed: {err}"),
        }
    }

    fn attach_asset_output(&mut self, rel_path: &str) {
        let Some(workspace) = &self.workspace else {
            self.asset_status = "workspace is not selected".to_string();
            return;
        };
        match attach_asset_context(workspace, rel_path) {
            Ok(_) => {
                self.asset_status = "attached asset context".to_string();
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => self.asset_status = format!("attach failed: {err}"),
        }
    }

    fn drain_asset_events(&mut self) {
        let mut events = Vec::new();
        if let Some(rx) = &self.asset_events_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }

        for event in events {
            match event {
                AssetEvent::JobUpdated(job) => {
                    self.asset_status = match job.status {
                        AssetStatus::Done => format!("done {}", job.id),
                        AssetStatus::Failed => format!(
                            "failed {}: {}",
                            job.id,
                            job.error.as_deref().unwrap_or("unknown error")
                        ),
                        AssetStatus::Running => format!("running {}", job.id),
                        AssetStatus::Pending => format!("queued {}", job.id),
                    };
                    self.upsert_asset_job(job);
                    self.refresh_file_rows();
                    self.refresh_git_summary();
                }
                AssetEvent::Done => {
                    self.asset_is_running = false;
                }
            }
        }
    }

    fn upsert_asset_job(&mut self, job: AssetJob) {
        if let Some(existing) = self
            .asset_jobs
            .iter_mut()
            .find(|existing| existing.id == job.id)
        {
            *existing = job;
        } else {
            self.asset_jobs.push(job);
        }
        self.asset_jobs.sort_by_key(|job| job.created_at);
    }

    fn texture_for_asset(
        &mut self,
        ctx: &egui::Context,
        rel_path: &str,
    ) -> Option<&egui::TextureHandle> {
        if self.asset_previews.contains_key(rel_path) {
            return self.asset_previews.get(rel_path);
        }

        let workspace = self.workspace.as_ref()?;
        let path = absolute_output_path(workspace, rel_path)?;
        if !is_image_path(&path) {
            return None;
        }
        let bytes = std::fs::read(path).ok()?;
        let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let pixels = image.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
        let texture = ctx.load_texture(
            format!("asset-preview-{rel_path}"),
            color_image,
            egui::TextureOptions::LINEAR,
        );
        self.asset_previews.insert(rel_path.to_string(), texture);
        self.asset_previews.get(rel_path)
    }

    fn load_file_preview(&mut self, rel: &str) {
        self.selected_file = Some(rel.to_string());
        let Some(workspace) = &self.workspace else {
            self.selected_preview = "Workspace is not selected".to_string();
            self.original_file_content.clear();
            self.selected_file_editable = false;
            self.editor_status = "workspace is not selected".to_string();
            return;
        };

        match workspace.read_text(rel, 2_000_000) {
            Ok(text) => {
                self.selected_preview = text.clone();
                self.original_file_content = text;
                self.selected_file_editable = true;
                self.editor_status = "loaded".to_string();
            }
            Err(err) => {
                self.selected_preview = format!("Could not open editable file: {err}");
                self.original_file_content.clear();
                self.selected_file_editable = false;
                self.editor_status = "not editable".to_string();
            }
        }
    }

    fn editor_dirty(&self) -> bool {
        self.selected_file_editable && self.selected_preview != self.original_file_content
    }

    fn save_selected_file(&mut self) {
        let Some(path) = self.selected_file.clone() else {
            return;
        };
        let Some(workspace) = &self.workspace else {
            self.editor_status = "workspace is not selected".to_string();
            return;
        };

        match workspace.write_text(&path, &self.selected_preview) {
            Ok(()) => {
                self.original_file_content = self.selected_preview.clone();
                self.editor_status = "saved".to_string();
                self.refresh_file_rows();
            }
            Err(err) => {
                self.editor_status = format!("save failed: {err}");
            }
        }
    }

    fn revert_selected_file(&mut self) {
        if self.selected_file_editable {
            self.selected_preview = self.original_file_content.clone();
            self.editor_status = "reverted".to_string();
        }
    }

    fn reload_selected_file(&mut self) {
        let Some(path) = self.selected_file.clone() else {
            return;
        };
        self.load_file_preview(&path);
    }

    fn send_current_input(&mut self) {
        let message = self.input.trim().to_string();
        if message.is_empty() || self.is_running {
            return;
        }
        if self.workspace.is_none() {
            self.chat
                .push(ChatLine::system("Сначала выбери папку проекта."));
            return;
        }

        self.sync_config_from_inputs();
        self.sync_asset_provider_settings();
        let _ = self.config.save();

        self.input.clear();
        self.chat.push(ChatLine::user(message.clone()));
        append_journal(format!("user_input\t{}", compact(&message, 500)));
        self.refresh_journal();
        self.tool_log.push(ToolLogLine {
            title: "run".to_string(),
            content: "Agent run started".to_string(),
        });

        let (tx, rx) = mpsc::channel();
        let config = self.config.clone();
        let workspace = self.workspace.clone();
        let state = self.agent_state.clone();
        let approvals = self.approvals.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();

        self.events_rx = Some(rx);
        self.cancel = Some(cancel);
        self.is_running = true;

        thread::spawn(move || {
            let result = tokio::runtime::Runtime::new()
                .expect("failed to start tokio runtime")
                .block_on(run_user_turn(
                    message,
                    config,
                    workspace,
                    state,
                    tx.clone(),
                    approvals,
                    worker_cancel,
                ));

            if let Err(err) = result {
                let _ = tx.send(AppEvent::Error(err.to_string()));
            }
            let _ = tx.send(AppEvent::Done);
        });
    }

    fn stop_run(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::SeqCst);
        }
        if self.pending_approval.is_some() {
            self.answer_approval(false);
        }
        self.tool_log.push(ToolLogLine {
            title: "stop".to_string(),
            content: "Stop requested".to_string(),
        });
    }

    fn reset_conversation(&mut self) {
        self.agent_state
            .lock()
            .expect("agent state poisoned")
            .reset();
        self.chat.clear();
        self.chat.push(ChatLine::system(
            "Диалог сброшен. Workspace и настройки сохранены.",
        ));
        self.tool_log.clear();
    }

    fn drain_events(&mut self) {
        let mut events = Vec::new();
        if let Some(rx) = &self.events_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        let had_events = !events.is_empty();

        for event in events {
            append_journal(format!("event\t{}", compact(&format!("{event:?}"), 2_000)));
            match event {
                AppEvent::AssistantText(text) => {
                    self.chat.push(ChatLine::assistant(text));
                }
                AppEvent::AssistantDelta(delta) => {
                    if let Some(last) = self.chat.last_mut() {
                        if matches!(last.role, ChatRole::Assistant) {
                            last.content.push_str(&delta);
                        } else {
                            self.chat.push(ChatLine::assistant(delta));
                        }
                    } else {
                        self.chat.push(ChatLine::assistant(delta));
                    }
                }
                AppEvent::ToolStarted { id, name, summary } => {
                    self.tool_log.push(ToolLogLine {
                        title: format!("{name} {id}"),
                        content: summary,
                    });
                }
                AppEvent::ToolOutput { id, chunk } => {
                    self.tool_log.push(ToolLogLine {
                        title: format!("output {id}"),
                        content: chunk,
                    });
                }
                AppEvent::ToolFinished { id, output } => {
                    self.update_desktop_state_from_tool_output(&output);
                    self.tool_log.push(ToolLogLine {
                        title: format!("done {id}"),
                        content: compact(&output, 2_000),
                    });
                    if let Some(workspace) = &self.workspace {
                        self.asset_jobs = load_jobs(workspace);
                    }
                    self.refresh_file_rows();
                    self.refresh_project_profiles();
                }
                AppEvent::ApprovalRequested {
                    id,
                    summary,
                    detail,
                } => {
                    self.pending_approval = Some(PendingApproval {
                        id,
                        summary,
                        detail,
                    });
                }
                AppEvent::Error(err) => {
                    self.chat.push(ChatLine::system(format!("Ошибка: {err}")));
                }
                AppEvent::Done => {
                    self.is_running = false;
                    self.cancel = None;
                    self.refresh_file_rows();
                    self.refresh_project_profiles();
                    self.refresh_git_summary();
                    if self.selected_file.is_some()
                        && self.selected_file_editable
                        && !self.editor_dirty()
                    {
                        self.reload_selected_file();
                    }
                    self.tool_log.push(ToolLogLine {
                        title: "run".to_string(),
                        content: "Agent run finished".to_string(),
                    });
                }
            }
        }

        if had_events {
            self.refresh_journal();
        }
    }

    fn update_desktop_state_from_tool_output(&mut self, output: &str) {
        let trimmed = output.trim();
        if trimmed.starts_with("assets/generated/screenshots/") && trimmed.ends_with(".png") {
            self.desktop_last_screenshot = Some(trimmed.to_string());
            self.desktop_status = format!("screenshot: {trimmed}");
            return;
        }

        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            return;
        };

        if let Some(path) = value
            .get("after_screenshot")
            .and_then(serde_json::Value::as_str)
        {
            self.desktop_last_screenshot = Some(path.to_string());
            self.desktop_status = value
                .get("action")
                .and_then(serde_json::Value::as_str)
                .map(|action| format!("desktop step: {action}"))
                .unwrap_or_else(|| "desktop step finished".to_string());
        } else if let Some(path) = value
            .get("before_screenshot")
            .and_then(serde_json::Value::as_str)
        {
            self.desktop_last_screenshot = Some(path.to_string());
        }

        if let Some(window) = value
            .get("after_window")
            .or_else(|| value.get("active_window"))
            .or_else(|| {
                if value.get("title").is_some() && value.get("process_name").is_some() {
                    Some(&value)
                } else {
                    None
                }
            })
        {
            self.desktop_active_window = summarize_window_value(window);
        }
    }

    fn drain_project_events(&mut self) {
        let mut events = Vec::new();
        if let Some(rx) = &self.project_events_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }
        let had_events = !events.is_empty();

        for event in events {
            append_journal(format!(
                "project_event\t{}",
                compact(&format!("{event:?}"), 2_000)
            ));
            match event {
                AppEvent::ToolStarted { id, name, summary } => {
                    self.tool_log.push(ToolLogLine {
                        title: format!("{name} {id}"),
                        content: summary,
                    });
                }
                AppEvent::ToolOutput { id, chunk } => {
                    self.tool_log.push(ToolLogLine {
                        title: format!("output {id}"),
                        content: chunk,
                    });
                }
                AppEvent::ToolFinished { id, output } => {
                    self.tool_log.push(ToolLogLine {
                        title: format!("done {id}"),
                        content: compact(&output, 2_000),
                    });
                    self.project_status = "command finished".to_string();
                    self.refresh_file_rows();
                    self.refresh_project_profiles();
                    self.refresh_git_summary();
                }
                AppEvent::ApprovalRequested {
                    id,
                    summary,
                    detail,
                } => {
                    self.pending_approval = Some(PendingApproval {
                        id,
                        summary,
                        detail,
                    });
                }
                AppEvent::Error(err) => {
                    self.project_status = format!("error: {err}");
                    self.tool_log.push(ToolLogLine {
                        title: "project error".to_string(),
                        content: err,
                    });
                }
                AppEvent::Done => {
                    self.project_is_running = false;
                    self.project_cancel = None;
                    self.refresh_file_rows();
                    self.refresh_project_profiles();
                    self.refresh_git_summary();
                }
                AppEvent::AssistantText(_) | AppEvent::AssistantDelta(_) => {}
            }
        }

        if had_events {
            self.refresh_journal();
        }
    }

    fn answer_approval(&mut self, approved: bool) {
        let Some(prompt) = self.pending_approval.take() else {
            return;
        };

        let sender = self
            .approvals
            .lock()
            .expect("approval map poisoned")
            .remove(&prompt.id);
        if let Some(sender) = sender {
            let _ = sender.send(approved);
        }

        self.tool_log.push(ToolLogLine {
            title: "approval".to_string(),
            content: if approved {
                format!("Approved: {}", prompt.summary)
            } else {
                format!("Denied: {}", prompt.summary)
            },
        });
        append_journal(format!(
            "approval\t{}\t{}",
            if approved { "approved" } else { "denied" },
            prompt.summary
        ));
        self.refresh_journal();
    }

    fn show_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Provider");
                let old_provider = self.provider_input.clone();
                egui::ComboBox::from_id_salt("provider_select")
                    .selected_text(provider_name(&self.provider_input))
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        for provider in provider_specs()
                            .iter()
                            .filter(|provider| provider.implemented)
                        {
                            ui.selectable_value(
                                &mut self.provider_input,
                                provider.id.to_string(),
                                provider.name,
                            );
                        }
                    });
                if self.provider_input != old_provider {
                    self.switch_provider_from_ui(self.provider_input.clone());
                }

                ui.label("Модель");
                ui.add_sized([150.0, 22.0], TextEdit::singleline(&mut self.model_input));
                let model_options = models_for_provider(&self.provider_input).collect::<Vec<_>>();
                if !model_options.is_empty() {
                    egui::ComboBox::from_id_salt("model_select")
                        .selected_text("models")
                        .width(76.0)
                        .show_ui(ui, |ui| {
                            for model in model_options {
                                ui.selectable_value(
                                    &mut self.model_input,
                                    model.id.to_string(),
                                    model.name,
                                );
                            }
                        });
                }

                ui.label("Route");
                egui::ComboBox::from_id_salt("task_route_select")
                    .selected_text(
                        route_labels()
                            .iter()
                            .find(|(id, _)| *id == self.config.task_route)
                            .map(|(_, label)| *label)
                            .unwrap_or("Auto"),
                    )
                    .width(86.0)
                    .show_ui(ui, |ui| {
                        for (id, label) in route_labels() {
                            ui.selectable_value(
                                &mut self.config.task_route,
                                (*id).to_string(),
                                *label,
                            );
                        }
                    });

                ui.label("Policy");
                let old_policy_profile = self.config.policy_profile.clone();
                egui::ComboBox::from_id_salt("policy_profile_select")
                    .selected_text(
                        policy_profile_labels()
                            .iter()
                            .find(|(id, _)| *id == self.config.policy_profile)
                            .map(|(_, label)| *label)
                            .unwrap_or("Normal"),
                    )
                    .width(76.0)
                    .show_ui(ui, |ui| {
                        for (id, label) in policy_profile_labels() {
                            ui.selectable_value(
                                &mut self.config.policy_profile,
                                (*id).to_string(),
                                *label,
                            );
                        }
                    });
                if self.config.policy_profile != old_policy_profile {
                    let selected = self.config.policy_profile.clone();
                    self.config.set_policy_profile(&selected);
                }

                ui.separator();
                ui.label("API key");
                ui.add_sized(
                    [230.0, 22.0],
                    TextEdit::singleline(&mut self.api_key_input).password(true),
                );

                let shell_changed = ui
                    .checkbox(&mut self.config.require_shell_approval, "Confirm shell")
                    .changed();
                let write_changed = ui
                    .checkbox(&mut self.config.require_write_approval, "Confirm write")
                    .changed();
                if shell_changed || write_changed {
                    self.config.set_custom_policy();
                }

                if ui.button("Сохранить").clicked() {
                    self.sync_config_from_inputs();
                    self.sync_asset_provider_settings();
                    let _ = self.config.save();
                }

                ui.separator();
                if ui
                    .add_enabled(!self.is_running, egui::Button::new("Выбрать проект"))
                    .clicked()
                {
                    self.choose_workspace();
                }

                if let Some(workspace) = &self.workspace {
                    ui.label(RichText::new(workspace.display_name()).strong());
                } else {
                    ui.label(RichText::new("проект не выбран").weak());
                }

                ui.separator();
                if ui
                    .add_enabled(!self.is_running, egui::Button::new("Сброс"))
                    .clicked()
                {
                    self.reset_conversation();
                }
                if self.is_running {
                    if ui.button("Stop").clicked() {
                        self.stop_run();
                    }
                    ui.spinner();
                }
            });
        });
    }

    fn show_file_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("files")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Файлы");
                    if ui.button("Обновить").clicked() {
                        self.refresh_file_rows();
                    }
                });

                egui::ScrollArea::vertical()
                    .id_salt("file_tree_scroll")
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for idx in 0..self.file_rows.len() {
                            let row = self.file_rows[idx].clone();
                            let selected = self.selected_file.as_deref() == Some(row.as_str());
                            if ui.selectable_label(selected, row.as_str()).clicked()
                                && !row.ends_with('/')
                                && row != "..."
                            {
                                self.load_file_preview(&row);
                            }
                        }
                    });

                ui.separator();
                if let Some(file) = &self.selected_file {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(file).strong());
                        if self.editor_dirty() {
                            ui.label(RichText::new("modified").italics());
                        } else if !self.editor_status.is_empty() {
                            ui.label(RichText::new(&self.editor_status).weak());
                        }
                    });
                } else {
                    ui.label(RichText::new("Файл не выбран").weak());
                }

                ui.horizontal(|ui| {
                    let dirty = self.editor_dirty();
                    if ui
                        .add_enabled(
                            self.selected_file_editable && dirty,
                            egui::Button::new("Save"),
                        )
                        .clicked()
                    {
                        self.save_selected_file();
                    }
                    if ui
                        .add_enabled(
                            self.selected_file_editable && dirty,
                            egui::Button::new("Revert"),
                        )
                        .clicked()
                    {
                        self.revert_selected_file();
                    }
                    if ui
                        .add_enabled(self.selected_file.is_some(), egui::Button::new("Reload"))
                        .clicked()
                    {
                        self.reload_selected_file();
                    }
                });

                egui::ScrollArea::vertical()
                    .id_salt("file_preview_scroll")
                    .show(ui, |ui| {
                        ui.add(
                            TextEdit::multiline(&mut self.selected_preview)
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Monospace)
                                .interactive(self.selected_file_editable),
                        );
                    });
            });
    }

    fn show_runtime_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Runtime", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!(
                    "agent: {}",
                    if self.is_running { "running" } else { "idle" }
                ));
                ui.label(format!(
                    "project: {}",
                    if self.project_is_running {
                        "running"
                    } else {
                        "idle"
                    }
                ));
                ui.label(format!(
                    "assets: {}",
                    if self.asset_is_running {
                        "running"
                    } else {
                        "idle"
                    }
                ));
                ui.label(format!(
                    "terminal: {}",
                    if self.terminal_running {
                        "running"
                    } else {
                        "stopped"
                    }
                ));
            });

            let policy_label = policy_profile_labels()
                .iter()
                .find(|(id, _)| *id == self.config.policy_profile)
                .map(|(_, label)| *label)
                .unwrap_or("Normal");
            ui.label(format!(
                "policy: {policy_label} | shell approval: {} | write approval: {}",
                yes_no(self.config.effective_require_shell_approval()),
                yes_no(self.config.effective_require_write_approval())
            ));

            if let Some(prompt) = &self.pending_approval {
                ui.label(RichText::new("waiting for approval").strong());
                ui.label(compact(&prompt.summary, 160));
            }

            let state_summary = self
                .agent_state
                .lock()
                .map(|state| {
                    format!(
                        "provider: {} | model: {} | response: {}",
                        state.provider_id.as_deref().unwrap_or("none"),
                        state.model_id.as_deref().unwrap_or("none"),
                        state
                            .previous_response_id
                            .as_deref()
                            .map(|id| compact(id, 48))
                            .unwrap_or_else(|| "none".to_string())
                    )
                })
                .unwrap_or_else(|_| "agent state unavailable".to_string());
            ui.label(RichText::new(state_summary).weak());
        });
    }

    fn show_journal_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Journal", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Refresh").clicked() {
                    self.refresh_journal();
                }
                if ui.button("Clear").clicked() {
                    self.clear_journal_from_ui();
                }
            });

            if !self.journal_status.is_empty() {
                ui.label(RichText::new(&self.journal_status).weak());
            }

            let mut journal_text = if self.journal_lines.is_empty() {
                "No journal entries yet".to_string()
            } else {
                self.journal_lines.join("\n")
            };
            ui.add(
                TextEdit::multiline(&mut journal_text)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(10)
                    .interactive(false),
            );
        });
    }

    fn show_tool_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("tools")
            .resizable(true)
            .default_width(380.0)
            .show(ctx, |ui| {
                self.show_runtime_panel(ui);
                ui.separator();
                self.show_project_panel(ui);
                ui.separator();
                self.show_desktop_panel(ui, ctx);
                ui.separator();
                self.show_terminal_panel(ui);
                ui.separator();
                self.show_orchestration_panel(ui);
                ui.separator();
                self.show_journal_panel(ui);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.heading("Git");
                    if ui.button("Обновить").clicked() {
                        self.refresh_git_summary();
                    }
                });
                ui.add(
                    egui::Label::new(
                        RichText::new(if self.git_summary.trim().is_empty() {
                            "git status пока не загружен"
                        } else {
                            &self.git_summary
                        })
                        .text_style(egui::TextStyle::Monospace),
                    )
                    .wrap(),
                );
                ui.separator();
                ui.heading("Инструменты");
                self.show_asset_panel(ui, ctx);
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("tool_log_scroll")
                    .show(ui, |ui| {
                        for line in &self.tool_log {
                            ui.label(RichText::new(&line.title).strong());
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&line.content)
                                        .text_style(egui::TextStyle::Monospace),
                                )
                                .wrap(),
                            );
                            ui.separator();
                        }
                    });
            });
    }

    fn show_project_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Project");
            if self.project_is_running {
                ui.spinner();
            }
            if ui
                .add_enabled(self.workspace.is_some(), egui::Button::new("Refresh"))
                .clicked()
            {
                self.refresh_project_profiles();
            }
            if ui
                .add_enabled(self.project_is_running, egui::Button::new("Stop"))
                .clicked()
            {
                self.stop_project_command();
            }
        });

        if !self.project_status.is_empty() {
            ui.label(RichText::new(&self.project_status).weak());
        }

        if self.workspace.is_none() {
            ui.label(RichText::new("No workspace selected").weak());
            return;
        }

        if self.project_profiles.is_empty() {
            ui.label(RichText::new("No project profile detected").weak());
            self.show_game_workflow_buttons(ui);
            return;
        }

        let profiles = self.project_profiles.clone();
        for profile in profiles {
            let profile_commands = profile.commands.clone();
            let preview_hooks = profile.previews.clone();
            ui.group(|ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&profile.kind).strong());
                    ui.label(RichText::new(&profile.name).weak());
                });
                if !profile.markers.is_empty() {
                    ui.label(
                        RichText::new(format!("markers: {}", profile.markers.join(", ")))
                            .text_style(egui::TextStyle::Monospace)
                            .weak(),
                    );
                }

                if profile.commands.is_empty() {
                    ui.label(RichText::new("No quick commands for this profile yet").weak());
                } else {
                    ui.horizontal_wrapped(|ui| {
                        for command in profile.commands {
                            let response = ui
                                .add_enabled(
                                    !self.project_is_running,
                                    egui::Button::new(&command.label),
                                )
                                .on_hover_text(format!(
                                    "{}\n{}",
                                    command.description, command.command
                                ));
                            if response.clicked() {
                                self.start_project_command(command);
                            }
                        }
                    });
                }

                if !preview_hooks.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Preview").weak());
                        for hook in preview_hooks {
                            let response = ui
                                .add_enabled(
                                    !self.project_is_running,
                                    egui::Button::new(&hook.label),
                                )
                                .on_hover_text(&hook.description);
                            if response.clicked() {
                                if let Some(url) = hook.url.as_deref() {
                                    self.open_preview_url_from_ui(url);
                                } else if let Some(command_id) = hook.command_id.as_deref() {
                                    if let Some(command) = profile_commands
                                        .iter()
                                        .find(|command| command.id == command_id)
                                        .cloned()
                                    {
                                        self.start_project_command(command);
                                    }
                                }
                            }
                        }
                    });
                }
            });
            ui.add_space(6.0);
        }

        self.show_game_workflow_buttons(ui);
    }

    fn show_desktop_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.collapsing("Desktop", |ui| {
            if self.desktop_status.is_empty()
                && self.desktop_active_window.is_empty()
                && self.desktop_last_screenshot.is_none()
            {
                ui.label(RichText::new("No desktop step captured yet").weak());
            }

            if !self.desktop_active_window.is_empty() {
                ui.label(RichText::new(&self.desktop_active_window).weak());
            }
            if !self.desktop_status.is_empty() {
                ui.label(RichText::new(&self.desktop_status).weak());
            }

            if let Some(path) = self.desktop_last_screenshot.clone() {
                if let Some(texture) = self.texture_for_asset(ctx, &path) {
                    let size = texture.size_vec2();
                    let scale = (180.0 / size.x.max(size.y)).min(1.0);
                    ui.image((texture.id(), size * scale));
                }
                ui.label(RichText::new(path).text_style(egui::TextStyle::Monospace));
            }
        });
    }

    fn refresh_terminal_snapshot(&mut self) {
        let snapshot = read_terminal_snapshot(Some(400), None);
        self.terminal_running = snapshot.running;
        self.terminal_status = match (&snapshot.shell, &snapshot.cwd) {
            (Some(shell), Some(cwd)) => {
                format!("{} | {} | {}", snapshot.status, shell, cwd)
            }
            _ => snapshot.status,
        };
        self.terminal_output = snapshot
            .lines
            .iter()
            .map(|line| format!("{:>5} [{}] {}", line.seq, line.stream, line.text))
            .collect::<Vec<_>>()
            .join("\n");
    }

    fn start_terminal_from_ui(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.terminal_status = "workspace is not selected".to_string();
            return;
        };
        match start_terminal_session(workspace, None, Some("powershell")) {
            Ok(_) => self.refresh_terminal_snapshot(),
            Err(err) => self.terminal_status = format!("terminal start failed: {err}"),
        }
    }

    fn write_terminal_from_ui(&mut self) {
        let input = self.terminal_input.trim_end().to_string();
        if input.is_empty() {
            return;
        }
        match write_terminal_input(&input, true) {
            Ok(_) => {
                self.terminal_input.clear();
                self.refresh_terminal_snapshot();
            }
            Err(err) => self.terminal_status = format!("terminal write failed: {err}"),
        }
    }

    fn stop_terminal_from_ui(&mut self) {
        match stop_terminal_session() {
            Ok(_) => self.refresh_terminal_snapshot(),
            Err(err) => self.terminal_status = format!("terminal stop failed: {err}"),
        }
    }

    fn clear_terminal_from_ui(&mut self) {
        let _ = clear_terminal_output();
        self.refresh_terminal_snapshot();
    }

    fn show_terminal_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Terminal", |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        self.workspace.is_some() && !self.terminal_running,
                        egui::Button::new("Start"),
                    )
                    .clicked()
                {
                    self.start_terminal_from_ui();
                }
                if ui
                    .add_enabled(self.terminal_running, egui::Button::new("Stop"))
                    .clicked()
                {
                    self.stop_terminal_from_ui();
                }
                if ui.button("Clear").clicked() {
                    self.clear_terminal_from_ui();
                }
                if ui.button("Refresh").clicked() {
                    self.refresh_terminal_snapshot();
                }
            });
            if !self.terminal_status.is_empty() {
                ui.label(RichText::new(&self.terminal_status).weak());
            }

            let response = ui.add(
                TextEdit::singleline(&mut self.terminal_input)
                    .hint_text("Command")
                    .desired_width(f32::INFINITY),
            );
            let enter_pressed =
                response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (enter_pressed || ui.button("Send").clicked()) && self.terminal_running {
                self.write_terminal_from_ui();
            }

            let mut output = self.terminal_output.clone();
            ui.add(
                TextEdit::multiline(&mut output)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(12)
                    .interactive(false),
            );
        });
    }

    fn show_game_workflow_buttons(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Game workflows", |ui| {
            ui.horizontal_wrapped(|ui| {
                for spec in workflow_specs() {
                    if ui
                        .add_enabled(self.workspace.is_some(), egui::Button::new(spec.label))
                        .on_hover_text(spec.description)
                        .clicked()
                    {
                        self.create_game_workflow_from_ui(spec.id);
                    }
                }
            });
        });
    }

    fn create_agent_handoff_from_ui(&mut self, role_id: &str) {
        let Some(workspace) = self.workspace.clone() else {
            self.orchestration_status = "workspace is not selected".to_string();
            return;
        };
        let Some(role) = parse_agent_role(role_id) else {
            self.orchestration_status = format!("unknown agent role: {role_id}");
            return;
        };

        let task = if self.input.trim().is_empty() {
            format!(
                "Review the current {} workspace and propose the next useful actions.",
                workspace.display_name()
            )
        } else {
            self.input.trim().to_string()
        };
        let context = format!(
            "Workspace: {}\nSelected file: {}\nCurrent prompt: {}",
            workspace.display_name(),
            self.selected_file.as_deref().unwrap_or("none"),
            if self.input.trim().is_empty() {
                "none"
            } else {
                self.input.trim()
            }
        );

        match record_handoff(
            &workspace,
            role,
            "Leetcode UI".to_string(),
            task,
            context,
            "Specialist recommendation, risks, and next actions".to_string(),
        ) {
            Ok(record) => {
                self.orchestration_status = format!("handoff recorded: {}", record.id);
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => {
                self.orchestration_status = format!("handoff failed: {err}");
            }
        }
    }

    fn export_trace_from_ui(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.orchestration_status = "workspace is not selected".to_string();
            return;
        };

        match export_trace(workspace) {
            Ok(path) => {
                self.orchestration_status = format!("trace exported: {path}");
                self.refresh_file_rows();
                self.refresh_git_summary();
            }
            Err(err) => {
                self.orchestration_status = format!("trace export failed: {err}");
            }
        }
    }

    fn add_orchestration_snapshot_to_log(&mut self) {
        let Some(workspace) = &self.workspace else {
            self.orchestration_status = "workspace is not selected".to_string();
            return;
        };

        let snapshot = orchestration_snapshot(workspace);
        let content = serde_json::to_string_pretty(&snapshot)
            .unwrap_or_else(|_| "failed to serialize orchestration snapshot".to_string());
        self.tool_log.push(ToolLogLine {
            title: "orchestration snapshot".to_string(),
            content,
        });
        self.orchestration_status = "snapshot added to tool log".to_string();
    }

    fn show_orchestration_panel(&mut self, ui: &mut egui::Ui) {
        ui.collapsing("Agents", |ui| {
            if self.workspace.is_none() {
                ui.label(RichText::new("No workspace selected").weak());
                return;
            }

            let state = self.workspace.as_ref().map(load_orchestration_state);
            if let Some(state) = &state {
                ui.label(
                    RichText::new(format!(
                        "handoffs: {} | subagents: {} | summaries: {} | evals: {}",
                        state.handoffs.len(),
                        state.subagent_runs.len(),
                        state.run_summaries.len(),
                        state.evals.len()
                    ))
                    .weak(),
                );
                if !state.context.summary.trim().is_empty() {
                    ui.label(compact(&state.context.summary, 180));
                }
            }

            ui.horizontal_wrapped(|ui| {
                if ui.button("Snapshot").clicked() {
                    self.add_orchestration_snapshot_to_log();
                }
                if ui.button("Export trace").clicked() {
                    self.export_trace_from_ui();
                }
            });

            if !self.orchestration_status.is_empty() {
                ui.label(RichText::new(&self.orchestration_status).weak());
            }

            ui.separator();
            ui.horizontal_wrapped(|ui| {
                for spec in agent_role_specs() {
                    if ui
                        .add_enabled(self.workspace.is_some(), egui::Button::new(spec.label))
                        .on_hover_text(spec.purpose)
                        .clicked()
                    {
                        self.create_agent_handoff_from_ui(spec.id);
                    }
                }
            });
        });
    }

    fn show_asset_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.separator();
        ui.horizontal(|ui| {
            ui.heading("Assets");
            if self.asset_is_running {
                ui.spinner();
            }
            if ui
                .add_enabled(self.workspace.is_some(), egui::Button::new("Open folder"))
                .clicked()
            {
                self.open_generated_assets_folder();
            }
            if ui
                .add_enabled(self.workspace.is_some(), egui::Button::new("Refresh"))
                .clicked()
            {
                if let Some(workspace) = &self.workspace {
                    self.asset_jobs = load_jobs(workspace);
                }
            }
        });

        let old_asset_provider = self.asset_provider_input.clone();
        let old_asset_kind = self.asset_kind_input.clone();
        ui.horizontal(|ui| {
            ui.label("Kind");
            egui::ComboBox::from_id_salt("asset_kind_select")
                .selected_text(match self.asset_kind_input.as_str() {
                    "spritesheet" => "Spritesheet",
                    "audio" => "Audio",
                    "video" => "Video",
                    _ => "Image",
                })
                .width(118.0)
                .show_ui(ui, |ui| {
                    for (id, label) in [
                        ("image", "Image"),
                        ("spritesheet", "Spritesheet"),
                        ("audio", "Audio"),
                        ("video", "Video"),
                    ] {
                        ui.selectable_value(&mut self.asset_kind_input, id.to_string(), label);
                    }
                });
        });
        if self.asset_kind_input != old_asset_kind {
            self.sync_asset_provider_settings_for(&old_asset_provider);
            self.switch_asset_kind_from_ui();
        }
        if matches!(self.asset_kind_input.as_str(), "image" | "spritesheet") {
            ui.horizontal(|ui| {
                ui.label("Image provider");
                egui::ComboBox::from_id_salt("asset_provider_select")
                    .selected_text(image_provider_name(&self.asset_provider_input))
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for provider in image_provider_specs() {
                            ui.selectable_value(
                                &mut self.asset_provider_input,
                                provider.id.to_string(),
                                provider.name,
                            );
                        }
                    });
            });
            if self.asset_provider_input != old_asset_provider {
                self.sync_asset_provider_settings_for(&old_asset_provider);
                self.switch_asset_provider_from_ui(self.asset_provider_input.clone());
            }

            if let Some(provider) = image_provider_specs()
                .iter()
                .find(|provider| provider.id == self.asset_provider_input)
            {
                ui.label(
                    RichText::new(format!("{} | {}", provider.notes, provider.env_var)).weak(),
                );
            }
        } else {
            let provider_label = if self.asset_kind_input == "video" {
                video_provider_name(OPENAI_VIDEO_PROVIDER_ID)
            } else {
                audio_provider_name(OPENAI_AUDIO_PROVIDER_ID)
            };
            ui.label(RichText::new(format!("{provider_label} | OPENAI_API_KEY")).weak());
        }

        ui.horizontal(|ui| {
            ui.label("Model");
            ui.add_sized(
                [ui.available_width().max(160.0), 22.0],
                TextEdit::singleline(&mut self.asset_model_input),
            );
        });

        ui.horizontal(|ui| {
            ui.label(
                if matches!(self.asset_kind_input.as_str(), "image" | "spritesheet") {
                    "Image key"
                } else {
                    "Media key"
                },
            );
            let key_width = (ui.available_width() - 76.0).max(120.0);
            ui.add_sized(
                [key_width, 22.0],
                TextEdit::singleline(&mut self.asset_api_key_input).password(true),
            );
            if ui.button("Save").clicked() {
                self.sync_asset_provider_settings();
                let _ = self.config.save();
            }
        });

        ui.add(
            TextEdit::multiline(&mut self.asset_prompt)
                .hint_text("Prompt for a game/app asset")
                .desired_width(f32::INFINITY)
                .desired_rows(3),
        );

        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("asset_aspect_ratio")
                .selected_text(&self.asset_aspect_ratio)
                .width(72.0)
                .show_ui(ui, |ui| {
                    for ratio in ["1:1", "3:2", "2:3", "4:3", "3:4", "16:9", "9:16"] {
                        ui.selectable_value(&mut self.asset_aspect_ratio, ratio.to_string(), ratio);
                    }
                });
            egui::ComboBox::from_id_salt("asset_image_size")
                .selected_text(&self.asset_image_size)
                .width(72.0)
                .show_ui(ui, |ui| {
                    let sizes: &[&str] = if self.asset_kind_input == "video" {
                        &["1280x720", "720x1280", "1920x1080", "1080x1920"]
                    } else {
                        &["0.5K", "1K", "2K", "4K"]
                    };
                    for size in sizes {
                        ui.selectable_value(&mut self.asset_image_size, (*size).to_string(), *size);
                    }
                });
            if ui
                .add_enabled(
                    !self.asset_is_running && self.workspace.is_some(),
                    egui::Button::new(match self.asset_kind_input.as_str() {
                        "spritesheet" => "Generate spritesheet",
                        "audio" => "Generate audio",
                        "video" => "Generate video",
                        _ => "Generate image",
                    }),
                )
                .clicked()
            {
                self.start_image_asset_job();
            }
        });

        if !self.asset_status.is_empty() {
            ui.label(RichText::new(&self.asset_status).weak());
        }

        egui::ScrollArea::vertical()
            .id_salt("asset_jobs_scroll")
            .max_height(260.0)
            .show(ui, |ui| {
                if self.asset_jobs.is_empty() {
                    ui.label(RichText::new("No generated assets yet").weak());
                    return;
                }

                let jobs = self
                    .asset_jobs
                    .iter()
                    .rev()
                    .take(12)
                    .cloned()
                    .collect::<Vec<_>>();
                for job in jobs {
                    self.show_asset_card(ui, ctx, job);
                }
            });
    }

    fn show_asset_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, job: AssetJob) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("{:?}", job.status)).strong());
                ui.label(RichText::new(image_provider_name(&job.provider)).weak());
                ui.label(RichText::new(&job.model).weak());
            });

            ui.label(compact(&job.prompt, 170));
            if let Some(error) = &job.error {
                ui.label(RichText::new(compact(error, 180)).weak());
            }

            let first_output = job.output_files.first().cloned();
            if let Some(output) = &first_output {
                if let Some(texture) = self.texture_for_asset(ctx, output) {
                    let size = texture.size_vec2();
                    let scale = (132.0 / size.x.max(size.y)).min(1.0);
                    ui.image((texture.id(), size * scale));
                }
                ui.label(RichText::new(output).text_style(egui::TextStyle::Monospace));
            } else {
                ui.label(RichText::new("No output file").weak());
            }

            ui.horizontal_wrapped(|ui| {
                if ui
                    .add_enabled(
                        !self.asset_is_running && self.workspace.is_some(),
                        egui::Button::new("Regenerate"),
                    )
                    .clicked()
                {
                    self.regenerate_asset_job(&job);
                }
                if ui
                    .add_enabled(
                        !self.asset_is_running && self.workspace.is_some(),
                        egui::Button::new("Variation"),
                    )
                    .clicked()
                {
                    self.vary_asset_job(&job);
                }
                if let Some(output) = first_output.as_deref() {
                    if ui.button("Use icon").clicked() {
                        self.use_asset_as_app_icon(output);
                    }
                    if ui.button("Upscale").clicked() {
                        self.upscale_asset_output(output);
                    }
                    if ui.button("Export").clicked() {
                        self.export_asset_output(output);
                    }
                    if ui.button("Attach").clicked() {
                        self.attach_asset_output(output);
                    }
                    if ui.button("Open folder").clicked() {
                        self.open_asset_folder(output);
                    }
                }
                if ui.button("Load prompt").clicked() {
                    self.load_asset_job_into_form(&job);
                }
            });
        });
        ui.add_space(6.0);
    }

    fn show_chat_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Leetcode");
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("chat_transcript_scroll")
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.chat {
                        let label = match line.role {
                            ChatRole::User => "Вы",
                            ChatRole::Assistant => "Assistant",
                            ChatRole::System => "System",
                        };
                        let text = match line.role {
                            ChatRole::User => RichText::new(label).strong(),
                            ChatRole::Assistant => RichText::new(label).strong(),
                            ChatRole::System => RichText::new(label).weak(),
                        };
                        ui.label(text);
                        ui.add(egui::Label::new(line.content.as_str()).wrap());
                        ui.add_space(8.0);
                    }
                });
        });
    }

    fn show_input_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("input_bar")
            .exact_height(88.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let send_width = 112.0;
                    let input_width = (ui.available_width() - send_width - 12.0).max(220.0);
                    let response = ui.add_sized(
                        [input_width, 68.0],
                        TextEdit::multiline(&mut self.input)
                            .id_salt("main_prompt_input")
                            .hint_text("Что сделать в выбранном проекте? Ctrl+Enter для отправки")
                            .desired_width(f32::INFINITY),
                    );

                    let send_clicked = ui
                        .add_sized(
                            [send_width, 68.0],
                            egui::Button::new(RichText::new("Отправить").strong()),
                        )
                        .clicked()
                        && !self.is_running;
                    let enter_pressed = response.has_focus()
                        && ui.input(|input| {
                            input.key_pressed(egui::Key::Enter) && input.modifiers.ctrl
                        });

                    if (send_clicked || enter_pressed) && !self.is_running {
                        self.send_current_input();
                    }
                });
            });
    }

    fn show_approval_window(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.pending_approval.clone() else {
            return;
        };

        egui::Window::new("Подтверждение действия")
            .collapsible(false)
            .resizable(true)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(prompt.summary).strong());
                ui.separator();
                let mut detail = prompt.detail.clone();
                ui.add(
                    TextEdit::multiline(&mut detail)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(10)
                        .interactive(false),
                );
                ui.horizontal(|ui| {
                    if ui.button("Разрешить").clicked() {
                        self.answer_approval(true);
                    }
                    if ui.button("Отклонить").clicked() {
                        self.answer_approval(false);
                    }
                });
            });
    }
}

impl eframe::App for LeetcodeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();
        self.drain_project_events();
        self.drain_asset_events();
        self.refresh_terminal_snapshot();
        self.show_top_bar(ctx);
        self.show_input_bar(ctx);
        self.show_file_panel(ctx);
        self.show_tool_panel(ctx);
        self.show_chat_panel(ctx);
        self.show_approval_window(ctx);

        if self.is_running
            || self.project_is_running
            || self.asset_is_running
            || self.terminal_running
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

fn image_api_key_from_config(config: &AppConfig, provider_id: &str) -> String {
    let direct_key = config.api_key_for_provider(provider_id);
    if !direct_key.trim().is_empty() {
        return direct_key;
    }

    match provider_id {
        OPENAI_IMAGE_PROVIDER_ID => config.api_key_for_provider(OPENAI_PROVIDER_ID),
        GEMINI_IMAGE_PROVIDER_ID => config.api_key_for_provider(GEMINI_PROVIDER_ID),
        _ => String::new(),
    }
}

fn media_api_key_from_config(config: &AppConfig, provider_id: &str) -> String {
    let direct_key = config.api_key_for_provider(provider_id);
    if !direct_key.trim().is_empty() {
        return direct_key;
    }

    match provider_id {
        OPENAI_AUDIO_PROVIDER_ID | OPENAI_VIDEO_PROVIDER_ID => {
            config.api_key_for_provider(OPENAI_PROVIDER_ID)
        }
        _ => String::new(),
    }
}

fn image_model_from_config(config: &AppConfig, provider_id: &str) -> String {
    config
        .providers
        .get(provider_id)
        .and_then(|settings| {
            let model = settings.model.trim();
            if model.is_empty() {
                None
            } else {
                Some(model.to_string())
            }
        })
        .unwrap_or_else(|| default_image_model(provider_id).to_string())
}

fn media_model_from_config(config: &AppConfig, provider_id: &str, default_model: &str) -> String {
    config
        .providers
        .get(provider_id)
        .and_then(|settings| {
            let model = settings.model.trim();
            if model.is_empty() {
                None
            } else {
                Some(model.to_string())
            }
        })
        .unwrap_or_else(|| default_model.to_string())
}

fn summarize_window_value(value: &serde_json::Value) -> String {
    let title = value
        .get("title")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("Untitled");
    let process = value
        .get("process_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let pid = value
        .get("process_id")
        .and_then(serde_json::Value::as_u64)
        .map(|pid| pid.to_string())
        .unwrap_or_default();

    if pid.is_empty() {
        format!("active: {title} ({process})")
    } else {
        format!("active: {title} ({process}, pid {pid})")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn compact(text: &str, max_chars: usize) -> String {
    let mut compacted = text.chars().take(max_chars).collect::<String>();
    if text.chars().count() > max_chars {
        compacted.push_str("\n... truncated ...");
    }
    compacted
}
