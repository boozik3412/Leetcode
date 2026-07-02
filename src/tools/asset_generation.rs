use crate::agent::models::{GEMINI_PROVIDER_ID, OPENAI_PROVIDER_ID};
use crate::agent::types::{AppEvent, ToolResult};
use crate::assets::{
    asset_provider_env_var, attach_asset_context, audio_provider_name, default_audio_model,
    default_image_model, default_video_model, export_asset, image_provider_env_var,
    image_provider_name, image_request_from_job, load_jobs, normalize_image_provider,
    run_audio_job, run_image_job, run_spritesheet_job, run_video_job, upscale_asset,
    video_provider_name, AssetJob, AssetStatus, AudioAssetRequest, ImageAssetRequest,
    SpritesheetAssetRequest, VideoAssetRequest, GEMINI_IMAGE_PROVIDER_ID, OPENAI_AUDIO_PROVIDER_ID,
    OPENAI_IMAGE_PROVIDER_ID, OPENAI_VIDEO_PROVIDER_ID, REPLICATE_IMAGE_PROVIDER_ID,
    STABILITY_IMAGE_PROVIDER_ID,
};
use crate::config::AppConfig;
use crate::tools::policy::{request_approval, ApprovalMap};
use crate::workspace::Workspace;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::Sender;

#[derive(Debug, Deserialize)]
pub struct GenerateImageAssetArgs {
    pub prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub aspect_ratio: Option<String>,
    pub image_size: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateSpritesheetAssetArgs {
    pub prompt: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub aspect_ratio: Option<String>,
    pub image_size: Option<String>,
    pub columns: Option<u32>,
    pub rows: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateAudioAssetArgs {
    pub prompt: String,
    pub model: Option<String>,
    pub voice: Option<String>,
    pub format: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateVideoAssetArgs {
    pub prompt: String,
    pub model: Option<String>,
    pub size: Option<String>,
    pub seconds: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct UpscaleAssetArgs {
    pub source_path: String,
    pub scale: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ExportAssetArgs {
    pub source_path: String,
    pub target_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AttachAssetArgs {
    pub source_path: String,
}

#[derive(Debug, Deserialize)]
pub struct UseAssetAsAppIconArgs {
    pub source_path: String,
    pub target_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegenerateImageAssetArgs {
    pub job_id: String,
}

#[derive(Debug, Deserialize)]
pub struct VaryImageAssetArgs {
    pub job_id: String,
    pub prompt: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAssetFolderArgs {
    pub path: Option<String>,
}

pub async fn generate_image_asset(
    workspace: &Workspace,
    args: GenerateImageAssetArgs,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let prompt = args.prompt.trim().to_string();
    if prompt.is_empty() {
        return ToolResult::error("generate_image_asset prompt is empty");
    }

    let provider = args
        .provider
        .as_deref()
        .map(normalize_image_provider)
        .unwrap_or_else(|| default_configured_image_provider(config));
    let model = args
        .model
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| image_model_from_config(config, &provider));
    let aspect_ratio = args.aspect_ratio.unwrap_or_else(|| "1:1".to_string());
    let image_size = args.image_size.unwrap_or_else(|| "1K".to_string());
    let request = ImageAssetRequest {
        provider,
        prompt,
        model,
        aspect_ratio,
        image_size,
    };

    run_approved_image_request(
        workspace,
        request,
        config,
        events,
        approvals,
        "Generate image asset",
    )
    .await
}

pub async fn generate_spritesheet_asset(
    workspace: &Workspace,
    args: GenerateSpritesheetAssetArgs,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let prompt = args.prompt.trim().to_string();
    if prompt.is_empty() {
        return ToolResult::error("generate_spritesheet_asset prompt is empty");
    }
    let provider = args
        .provider
        .as_deref()
        .map(normalize_image_provider)
        .unwrap_or_else(|| default_configured_image_provider(config));
    let model = args
        .model
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| image_model_from_config(config, &provider));
    let request = SpritesheetAssetRequest {
        provider,
        prompt,
        model,
        aspect_ratio: args.aspect_ratio.unwrap_or_else(|| "1:1".to_string()),
        image_size: args.image_size.unwrap_or_else(|| "2K".to_string()),
        columns: args.columns.unwrap_or(4).clamp(1, 12),
        rows: args.rows.unwrap_or(4).clamp(1, 12),
    };
    let api_key = image_api_key_from_config(config, &request.provider);
    if api_key.trim().is_empty() {
        return ToolResult::error(format!(
            "{} key is empty. Save it in the Assets panel or set {}.",
            image_provider_name(&request.provider),
            image_provider_env_var(&request.provider)
        ));
    }
    if !request_approval(
        events,
        approvals,
        format!(
            "Generate spritesheet with {}",
            image_provider_name(&request.provider)
        ),
        format!(
            "Provider: {}\nModel: {}\nGrid: {}x{}\nPrompt:\n{}",
            image_provider_name(&request.provider),
            request.model,
            request.columns,
            request.rows,
            request.prompt
        ),
    ) {
        return ToolResult::error("generate_spritesheet_asset denied by user");
    }

    let job = AssetJob::new_spritesheet(&request);
    finish_asset_job(run_spritesheet_job(workspace.clone(), api_key, request, job).await)
}

pub async fn generate_audio_asset(
    workspace: &Workspace,
    args: GenerateAudioAssetArgs,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let prompt = args.prompt.trim().to_string();
    if prompt.is_empty() {
        return ToolResult::error("generate_audio_asset prompt is empty");
    }
    let request = AudioAssetRequest {
        provider: OPENAI_AUDIO_PROVIDER_ID.to_string(),
        prompt,
        model: args
            .model
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| audio_model_from_config(config)),
        voice: args.voice.unwrap_or_else(|| "alloy".to_string()),
        format: args.format.unwrap_or_else(|| "wav".to_string()),
    };
    let api_key = media_api_key_from_config(config, &request.provider);
    if api_key.trim().is_empty() {
        return ToolResult::error(format!(
            "{} key is empty. Save it or set {}.",
            audio_provider_name(&request.provider),
            asset_provider_env_var(&request.provider)
        ));
    }
    if !request_approval(
        events,
        approvals,
        format!(
            "Generate audio asset with {}",
            audio_provider_name(&request.provider)
        ),
        format!(
            "Provider: {}\nModel: {}\nVoice: {}\nFormat: {}\n\nPrompt:\n{}",
            audio_provider_name(&request.provider),
            request.model,
            request.voice,
            request.format,
            request.prompt
        ),
    ) {
        return ToolResult::error("generate_audio_asset denied by user");
    }

    let job = AssetJob::new_audio(&request);
    finish_asset_job(run_audio_job(workspace.clone(), api_key, request, job).await)
}

pub async fn generate_video_asset(
    workspace: &Workspace,
    args: GenerateVideoAssetArgs,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let prompt = args.prompt.trim().to_string();
    if prompt.is_empty() {
        return ToolResult::error("generate_video_asset prompt is empty");
    }
    let request = VideoAssetRequest {
        provider: OPENAI_VIDEO_PROVIDER_ID.to_string(),
        prompt,
        model: args
            .model
            .filter(|model| !model.trim().is_empty())
            .unwrap_or_else(|| video_model_from_config(config)),
        size: args.size.unwrap_or_else(|| "1280x720".to_string()),
        seconds: args.seconds.unwrap_or(8).clamp(1, 20),
    };
    let api_key = media_api_key_from_config(config, &request.provider);
    if api_key.trim().is_empty() {
        return ToolResult::error(format!(
            "{} key is empty. Save it or set {}.",
            video_provider_name(&request.provider),
            asset_provider_env_var(&request.provider)
        ));
    }
    if !request_approval(
        events,
        approvals,
        format!(
            "Generate video asset with {}",
            video_provider_name(&request.provider)
        ),
        format!(
            "Provider: {}\nModel: {}\nSize: {}\nSeconds: {}\n\nPrompt:\n{}",
            video_provider_name(&request.provider),
            request.model,
            request.size,
            request.seconds,
            request.prompt
        ),
    ) {
        return ToolResult::error("generate_video_asset denied by user");
    }

    let job = AssetJob::new_video(&request);
    finish_asset_job(run_video_job(workspace.clone(), api_key, request, job).await)
}

pub async fn regenerate_image_asset(
    workspace: &Workspace,
    args: RegenerateImageAssetArgs,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let Some(job) = find_asset_job(workspace, &args.job_id) else {
        return ToolResult::error(format!("asset job not found: {}", args.job_id));
    };
    let request = image_request_from_job(&job, None);

    run_approved_image_request(
        workspace,
        request,
        config,
        events,
        approvals,
        "Regenerate image asset",
    )
    .await
}

pub async fn vary_image_asset(
    workspace: &Workspace,
    args: VaryImageAssetArgs,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let Some(job) = find_asset_job(workspace, &args.job_id) else {
        return ToolResult::error(format!("asset job not found: {}", args.job_id));
    };
    let prompt = args.prompt.unwrap_or_else(|| {
        format!(
            "{}\n\nCreate a polished variation that keeps the same purpose, composition, and game/app asset usability, but changes visual details enough to offer a fresh option.",
            job.prompt
        )
    });
    let request = image_request_from_job(&job, Some(prompt));

    run_approved_image_request(
        workspace,
        request,
        config,
        events,
        approvals,
        "Create image asset variation",
    )
    .await
}

async fn run_approved_image_request(
    workspace: &Workspace,
    request: ImageAssetRequest,
    config: &AppConfig,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
    action_name: &str,
) -> ToolResult {
    let api_key = image_api_key_from_config(config, &request.provider);
    if api_key.trim().is_empty() {
        return ToolResult::error(format!(
            "{} key is empty. Save it in the Assets panel or set {}.",
            image_provider_name(&request.provider),
            image_provider_env_var(&request.provider)
        ));
    }

    if !request_approval(
        events,
        approvals,
        format!(
            "{action_name} with {} ({})",
            image_provider_name(&request.provider),
            request.model
        ),
        format!(
            "Provider: {}\nModel: {}\nAspect ratio: {}\nImage size: {}\n\nPrompt:\n{}",
            image_provider_name(&request.provider),
            request.model,
            request.aspect_ratio,
            request.image_size,
            request.prompt
        ),
    ) {
        return ToolResult::error(format!(
            "{} denied by user",
            action_name.to_ascii_lowercase()
        ));
    }

    let job = AssetJob::new_image(&request);
    let final_job = run_image_job(workspace.clone(), api_key, request, job).await;

    match final_job.status {
        AssetStatus::Done => finish_asset_job(final_job),
        AssetStatus::Failed => ToolResult::error(format!(
            "{} failed: {}",
            action_name.to_ascii_lowercase(),
            final_job
                .error
                .unwrap_or_else(|| "unknown error".to_string())
        )),
        AssetStatus::Pending | AssetStatus::Running => ToolResult::error(format!(
            "{} ended before the image job reached a final state",
            action_name.to_ascii_lowercase()
        )),
    }
}

pub fn upscale_existing_asset(
    workspace: &Workspace,
    args: UpscaleAssetArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let source_path = args.source_path.trim();
    if source_path.is_empty() {
        return ToolResult::error("upscale_asset source_path is empty");
    }
    let scale = args.scale.unwrap_or(2).clamp(2, 4);
    if !request_approval(
        events,
        approvals,
        format!("Upscale asset {scale}x"),
        format!("Source:\n{source_path}\n\nScale: {scale}x"),
    ) {
        return ToolResult::error("upscale_asset denied by user");
    }
    match upscale_asset(workspace, source_path, scale) {
        Ok(job) => finish_asset_job(job),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn export_existing_asset(
    workspace: &Workspace,
    args: ExportAssetArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let source_path = args.source_path.trim();
    if source_path.is_empty() {
        return ToolResult::error("export_asset source_path is empty");
    }
    if !request_approval(
        events,
        approvals,
        "Export asset",
        format!(
            "Source:\n{}\n\nTarget name: {}",
            source_path,
            args.target_name.as_deref().unwrap_or("(auto)")
        ),
    ) {
        return ToolResult::error("export_asset denied by user");
    }
    match export_asset(workspace, source_path, args.target_name.as_deref()) {
        Ok(job) => finish_asset_job(job),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

pub fn attach_asset(
    workspace: &Workspace,
    args: AttachAssetArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let source_path = args.source_path.trim();
    if source_path.is_empty() {
        return ToolResult::error("attach_asset source_path is empty");
    }
    if !request_approval(
        events,
        approvals,
        "Attach asset context",
        format!("Attach metadata for:\n{source_path}"),
    ) {
        return ToolResult::error("attach_asset denied by user");
    }
    match attach_asset_context(workspace, source_path) {
        Ok(context) => ToolResult::ok(
            serde_json::to_string_pretty(&context).unwrap_or_else(|_| "asset attached".to_string()),
        ),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

fn finish_asset_job(final_job: AssetJob) -> ToolResult {
    match final_job.status {
        AssetStatus::Done => ToolResult::ok(
            serde_json::to_string_pretty(&json!({
                "job_id": final_job.id,
                "kind": final_job.kind,
                "provider": final_job.provider,
                "model": final_job.model,
                "output_files": final_job.output_files,
                "metadata": final_job.metadata
            }))
            .unwrap_or_else(|_| "asset job finished".to_string()),
        ),
        AssetStatus::Failed => ToolResult::error(format!(
            "asset job failed: {}",
            final_job
                .error
                .unwrap_or_else(|| "unknown error".to_string())
        )),
        AssetStatus::Pending | AssetStatus::Running => {
            ToolResult::error("asset job ended before reaching a final state")
        }
    }
}

pub fn use_asset_as_app_icon(
    workspace: &Workspace,
    args: UseAssetAsAppIconArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let source_path = args.source_path.trim();
    if source_path.is_empty() {
        return ToolResult::error("use_asset_as_app_icon source_path is empty");
    }
    let target_path = args
        .target_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .unwrap_or("assets/app-icon.png");

    let source = match workspace.resolve_existing(source_path) {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    if !source.is_file() {
        return ToolResult::error("use_asset_as_app_icon source_path must point to a file");
    }
    if !is_supported_image_path(&source) {
        return ToolResult::error(
            "use_asset_as_app_icon source_path must be png, jpg, jpeg, or webp",
        );
    }

    if !request_approval(
        events,
        approvals,
        format!("Use asset as app icon: {target_path}"),
        format!("Source:\n{source_path}\n\nTarget:\n{target_path}"),
    ) {
        return ToolResult::error("use_asset_as_app_icon denied by user");
    }

    let target = match workspace.resolve_for_write(target_path) {
        Ok(path) => path,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    if let Some(parent) = target.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            return ToolResult::error(err.to_string());
        }
    }

    let bytes = match fs::read(&source) {
        Ok(bytes) => bytes,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    let image = match image::load_from_memory(&bytes) {
        Ok(image) => image,
        Err(err) => return ToolResult::error(err.to_string()),
    };
    if let Err(err) = image.save_with_format(&target, image::ImageFormat::Png) {
        return ToolResult::error(err.to_string());
    }

    ToolResult::ok(
        serde_json::to_string_pretty(&json!({
            "source_path": source_path,
            "target_path": target_path,
            "format": "png"
        }))
        .unwrap_or_else(|_| format!("saved {target_path}")),
    )
}

pub fn open_asset_folder(
    workspace: &Workspace,
    args: OpenAssetFolderArgs,
    events: &Sender<AppEvent>,
    approvals: &ApprovalMap,
) -> ToolResult {
    let rel_path = args
        .path
        .unwrap_or_else(|| "assets/generated/images".to_string());
    let target = if rel_path.trim().is_empty() {
        "assets/generated/images".to_string()
    } else {
        rel_path
    };

    let path = match workspace.resolve_existing(&target) {
        Ok(path) => path,
        Err(_) if target == "assets/generated/images" => {
            let path = match workspace.resolve_for_write(&target) {
                Ok(path) => path,
                Err(err) => return ToolResult::error(err.to_string()),
            };
            if let Err(err) = fs::create_dir_all(&path) {
                return ToolResult::error(err.to_string());
            }
            path
        }
        Err(err) => return ToolResult::error(err.to_string()),
    };

    if !request_approval(
        events,
        approvals,
        "Open asset folder",
        format!("Open or reveal:\n{}", path.display()),
    ) {
        return ToolResult::error("open_asset_folder denied by user");
    }

    #[cfg(target_os = "windows")]
    let result = if path.is_file() {
        Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map(|_| ())
    } else {
        Command::new("explorer").arg(&path).spawn().map(|_| ())
    };
    #[cfg(not(target_os = "windows"))]
    let result = Command::new("open")
        .arg(if path.is_file() {
            path.parent().unwrap_or_else(|| workspace.root())
        } else {
            &path
        })
        .spawn()
        .map(|_| ());

    match result {
        Ok(()) => ToolResult::ok(format!("opened {}", path.display())),
        Err(err) => ToolResult::error(err.to_string()),
    }
}

fn default_configured_image_provider(config: &AppConfig) -> String {
    for provider_id in [
        OPENAI_IMAGE_PROVIDER_ID,
        GEMINI_IMAGE_PROVIDER_ID,
        STABILITY_IMAGE_PROVIDER_ID,
        REPLICATE_IMAGE_PROVIDER_ID,
    ] {
        if !image_api_key_from_config(config, provider_id)
            .trim()
            .is_empty()
        {
            return provider_id.to_string();
        }
    }

    OPENAI_IMAGE_PROVIDER_ID.to_string()
}

fn find_asset_job(workspace: &Workspace, job_id: &str) -> Option<AssetJob> {
    load_jobs(workspace)
        .into_iter()
        .find(|job| job.id == job_id || job.id.starts_with(job_id))
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

fn audio_model_from_config(config: &AppConfig) -> String {
    config
        .providers
        .get(OPENAI_AUDIO_PROVIDER_ID)
        .and_then(|settings| {
            let model = settings.model.trim();
            if model.is_empty() {
                None
            } else {
                Some(model.to_string())
            }
        })
        .unwrap_or_else(|| default_audio_model(OPENAI_AUDIO_PROVIDER_ID).to_string())
}

fn video_model_from_config(config: &AppConfig) -> String {
    config
        .providers
        .get(OPENAI_VIDEO_PROVIDER_ID)
        .and_then(|settings| {
            let model = settings.model.trim();
            if model.is_empty() {
                None
            } else {
                Some(model.to_string())
            }
        })
        .unwrap_or_else(|| default_video_model(OPENAI_VIDEO_PROVIDER_ID).to_string())
}

fn is_supported_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderSettings;
    use std::collections::BTreeMap;

    #[test]
    fn defaults_to_first_configured_image_provider() {
        let mut providers = BTreeMap::new();
        providers.insert(
            STABILITY_IMAGE_PROVIDER_ID.to_string(),
            ProviderSettings {
                api_key: "sk-stability".to_string(),
                model: "stable-image-core".to_string(),
            },
        );
        let config = AppConfig {
            provider: OPENAI_PROVIDER_ID.to_string(),
            api_key: String::new(),
            model: "gpt-5.5".to_string(),
            providers,
            last_workspace: None,
            require_shell_approval: true,
            require_write_approval: true,
            task_route: "auto".to_string(),
        };

        assert_eq!(
            default_configured_image_provider(&config),
            STABILITY_IMAGE_PROVIDER_ID
        );
    }

    #[test]
    fn openai_image_provider_reuses_chat_key() {
        let config = AppConfig {
            provider: OPENAI_PROVIDER_ID.to_string(),
            api_key: "sk-openai".to_string(),
            model: "gpt-5.5".to_string(),
            providers: BTreeMap::new(),
            last_workspace: None,
            require_shell_approval: true,
            require_write_approval: true,
            task_route: "auto".to_string(),
        };

        assert_eq!(
            image_api_key_from_config(&config, OPENAI_IMAGE_PROVIDER_ID),
            "sk-openai"
        );
    }

    #[test]
    fn recognizes_supported_icon_sources() {
        assert!(is_supported_image_path(Path::new(
            "assets/generated/icon.png"
        )));
        assert!(is_supported_image_path(Path::new(
            "assets/generated/icon.webp"
        )));
        assert!(!is_supported_image_path(Path::new(
            "assets/generated/icon.txt"
        )));
    }
}
