use crate::config::AppConfig;
use crate::http::build_http_client;
use crate::workspace::Workspace;
use base64::{engine::general_purpose, Engine as _};
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};

const JOBS_PATH: &str = "assets/generated/asset_jobs.json";
const REPLICATE_POLL_LIMIT: usize = 45;

pub const OPENAI_IMAGE_PROVIDER_ID: &str = "openai-image";
pub const OPENAI_AUDIO_PROVIDER_ID: &str = "openai-audio";
pub const OPENAI_VIDEO_PROVIDER_ID: &str = "openai-video";
pub const GEMINI_IMAGE_PROVIDER_ID: &str = "gemini-image";
pub const STABILITY_IMAGE_PROVIDER_ID: &str = "stability-image";
pub const REPLICATE_IMAGE_PROVIDER_ID: &str = "replicate-image";

#[derive(Clone, Debug)]
pub struct ImageProviderSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub default_model: &'static str,
    pub env_var: &'static str,
    pub notes: &'static str,
}

pub fn image_provider_specs() -> &'static [ImageProviderSpec] {
    const SPECS: &[ImageProviderSpec] = &[
        ImageProviderSpec {
            id: OPENAI_IMAGE_PROVIDER_ID,
            name: "ChatGPT Image",
            default_model: "gpt-image-2",
            env_var: "OPENAI_API_KEY",
            notes: "OpenAI Images API",
        },
        ImageProviderSpec {
            id: GEMINI_IMAGE_PROVIDER_ID,
            name: "Nano Banana",
            default_model: "gemini-3.1-flash-image",
            env_var: "GEMINI_API_KEY",
            notes: "Gemini Interactions API",
        },
        ImageProviderSpec {
            id: STABILITY_IMAGE_PROVIDER_ID,
            name: "Stability AI",
            default_model: "stable-image-core",
            env_var: "STABILITY_API_KEY",
            notes: "Stable Image Core",
        },
        ImageProviderSpec {
            id: REPLICATE_IMAGE_PROVIDER_ID,
            name: "Replicate FLUX",
            default_model: "black-forest-labs/flux-schnell",
            env_var: "REPLICATE_API_TOKEN",
            notes: "Replicate model predictions",
        },
    ];

    SPECS
}

pub fn audio_provider_name(provider_id: &str) -> &'static str {
    match provider_id {
        OPENAI_AUDIO_PROVIDER_ID | "openai" => "OpenAI Audio",
        _ => "Audio Provider",
    }
}

pub fn video_provider_name(provider_id: &str) -> &'static str {
    match provider_id {
        OPENAI_VIDEO_PROVIDER_ID | "openai" => "OpenAI Video",
        _ => "Video Provider",
    }
}

pub fn default_audio_model(provider_id: &str) -> &'static str {
    match provider_id {
        OPENAI_AUDIO_PROVIDER_ID | "openai" => "gpt-audio-1.5",
        _ => "gpt-audio-1.5",
    }
}

pub fn default_video_model(provider_id: &str) -> &'static str {
    match provider_id {
        OPENAI_VIDEO_PROVIDER_ID | "openai" => "sora-2",
        _ => "sora-2",
    }
}

pub fn asset_provider_env_var(provider_id: &str) -> &'static str {
    match provider_id {
        OPENAI_AUDIO_PROVIDER_ID | OPENAI_VIDEO_PROVIDER_ID => "OPENAI_API_KEY",
        _ => image_provider_env_var(provider_id),
    }
}

pub fn image_provider_name(provider_id: &str) -> &'static str {
    match provider_id {
        "openai" => return "ChatGPT Image",
        "gemini" => return "Nano Banana",
        _ => {}
    }

    image_provider_specs()
        .iter()
        .find(|provider| provider.id == provider_id)
        .map(|provider| provider.name)
        .unwrap_or("Image Provider")
}

pub fn default_image_model(provider_id: &str) -> &'static str {
    image_provider_specs()
        .iter()
        .find(|provider| provider.id == provider_id)
        .map(|provider| provider.default_model)
        .unwrap_or("gpt-image-2")
}

pub fn image_provider_env_var(provider_id: &str) -> &'static str {
    image_provider_specs()
        .iter()
        .find(|provider| provider.id == provider_id)
        .map(|provider| provider.env_var)
        .unwrap_or("API_KEY")
}

pub fn normalize_image_provider(provider_id: &str) -> String {
    let provider_id = provider_id.trim().to_ascii_lowercase();
    if provider_id == "openai" {
        return OPENAI_IMAGE_PROVIDER_ID.to_string();
    }
    if provider_id == "gemini" {
        return GEMINI_IMAGE_PROVIDER_ID.to_string();
    }

    if image_provider_specs()
        .iter()
        .any(|provider| provider.id == provider_id)
    {
        provider_id
    } else {
        OPENAI_IMAGE_PROVIDER_ID.to_string()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Image,
    Spritesheet,
    Audio,
    Video,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    Pending,
    Running,
    Done,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetJob {
    pub id: String,
    pub kind: AssetKind,
    pub status: AssetStatus,
    pub provider: String,
    pub model: String,
    pub prompt: String,
    pub parameters: Value,
    pub output_files: Vec<String>,
    pub metadata: Value,
    pub error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug)]
pub struct ImageAssetRequest {
    pub provider: String,
    pub prompt: String,
    pub model: String,
    pub aspect_ratio: String,
    pub image_size: String,
}

#[derive(Clone, Debug)]
pub struct SpritesheetAssetRequest {
    pub provider: String,
    pub prompt: String,
    pub model: String,
    pub aspect_ratio: String,
    pub image_size: String,
    pub columns: u32,
    pub rows: u32,
}

#[derive(Clone, Debug)]
pub struct AudioAssetRequest {
    pub provider: String,
    pub prompt: String,
    pub model: String,
    pub voice: String,
    pub format: String,
}

#[derive(Clone, Debug)]
pub struct VideoAssetRequest {
    pub provider: String,
    pub prompt: String,
    pub model: String,
    pub size: String,
    pub seconds: u32,
}

#[derive(Clone, Debug)]
pub enum AssetEvent {
    JobUpdated(AssetJob),
    Done,
}

struct GeneratedImage {
    bytes: Vec<u8>,
    mime_type: String,
    api: &'static str,
    metadata: Value,
}

struct GeneratedBinary {
    bytes: Vec<u8>,
    mime_type: String,
    api: &'static str,
    metadata: Value,
}

impl AssetJob {
    pub fn new_image(request: &ImageAssetRequest) -> Self {
        let now = unix_timestamp();
        let provider = normalize_image_provider(&request.provider);
        let model = if request.model.trim().is_empty() {
            default_image_model(&provider).to_string()
        } else {
            request.model.clone()
        };

        Self {
            id: format!("img-{}", uuid::Uuid::new_v4()),
            kind: AssetKind::Image,
            status: AssetStatus::Pending,
            provider,
            model,
            prompt: request.prompt.clone(),
            parameters: json!({
                "aspect_ratio": request.aspect_ratio,
                "image_size": request.image_size,
                "mime_type": "image/png"
            }),
            output_files: Vec::new(),
            metadata: json!({}),
            error: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn new_spritesheet(request: &SpritesheetAssetRequest) -> Self {
        let now = unix_timestamp();
        let provider = normalize_image_provider(&request.provider);
        let model = if request.model.trim().is_empty() {
            default_image_model(&provider).to_string()
        } else {
            request.model.clone()
        };

        Self {
            id: format!("sheet-{}", uuid::Uuid::new_v4()),
            kind: AssetKind::Spritesheet,
            status: AssetStatus::Pending,
            provider,
            model,
            prompt: request.prompt.clone(),
            parameters: json!({
                "aspect_ratio": request.aspect_ratio,
                "image_size": request.image_size,
                "columns": request.columns.max(1),
                "rows": request.rows.max(1),
                "mime_type": "image/png"
            }),
            output_files: Vec::new(),
            metadata: json!({}),
            error: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn new_audio(request: &AudioAssetRequest) -> Self {
        let now = unix_timestamp();
        let provider = if request.provider.trim().is_empty() {
            OPENAI_AUDIO_PROVIDER_ID.to_string()
        } else {
            request.provider.trim().to_ascii_lowercase()
        };
        let model = if request.model.trim().is_empty() {
            default_audio_model(&provider).to_string()
        } else {
            request.model.clone()
        };

        Self {
            id: format!("aud-{}", uuid::Uuid::new_v4()),
            kind: AssetKind::Audio,
            status: AssetStatus::Pending,
            provider,
            model,
            prompt: request.prompt.clone(),
            parameters: json!({
                "voice": request.voice,
                "format": request.format
            }),
            output_files: Vec::new(),
            metadata: json!({}),
            error: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn new_video(request: &VideoAssetRequest) -> Self {
        let now = unix_timestamp();
        let provider = if request.provider.trim().is_empty() {
            OPENAI_VIDEO_PROVIDER_ID.to_string()
        } else {
            request.provider.trim().to_ascii_lowercase()
        };
        let model = if request.model.trim().is_empty() {
            default_video_model(&provider).to_string()
        } else {
            request.model.clone()
        };

        Self {
            id: format!("vid-{}", uuid::Uuid::new_v4()),
            kind: AssetKind::Video,
            status: AssetStatus::Pending,
            provider,
            model,
            prompt: request.prompt.clone(),
            parameters: json!({
                "size": request.size,
                "seconds": request.seconds.clamp(1, 20),
                "mime_type": "video/mp4"
            }),
            output_files: Vec::new(),
            metadata: json!({}),
            error: None,
            created_at: now,
            updated_at: now,
        }
    }
}

pub fn load_jobs(workspace: &Workspace) -> Vec<AssetJob> {
    let Ok(text) = workspace.read_text(JOBS_PATH, 2_000_000) else {
        return Vec::new();
    };

    serde_json::from_str::<Vec<AssetJob>>(&text).unwrap_or_default()
}

pub fn upsert_job(workspace: &Workspace, job: &AssetJob) -> anyhow::Result<()> {
    let mut jobs = load_jobs(workspace);
    if let Some(existing) = jobs.iter_mut().find(|existing| existing.id == job.id) {
        *existing = job.clone();
    } else {
        jobs.push(job.clone());
    }

    jobs.sort_by_key(|job| job.created_at);
    workspace.write_text(JOBS_PATH, &serde_json::to_string_pretty(&jobs)?)?;
    Ok(())
}

pub fn image_request_from_job(
    job: &AssetJob,
    prompt_override: Option<String>,
) -> ImageAssetRequest {
    let aspect_ratio = job
        .parameters
        .get("aspect_ratio")
        .and_then(Value::as_str)
        .unwrap_or("1:1")
        .to_string();
    let image_size = job
        .parameters
        .get("image_size")
        .and_then(Value::as_str)
        .unwrap_or("1K")
        .to_string();

    ImageAssetRequest {
        provider: job.provider.clone(),
        prompt: prompt_override.unwrap_or_else(|| job.prompt.clone()),
        model: job.model.clone(),
        aspect_ratio,
        image_size,
    }
}

pub async fn run_image_job(
    workspace: Workspace,
    api_key: String,
    config: AppConfig,
    mut request: ImageAssetRequest,
    mut job: AssetJob,
) -> AssetJob {
    request.provider = normalize_image_provider(&request.provider);
    if request.model.trim().is_empty() {
        request.model = default_image_model(&request.provider).to_string();
    }

    job.status = AssetStatus::Running;
    job.provider = request.provider.clone();
    job.model = request.model.clone();
    job.updated_at = unix_timestamp();
    let _ = upsert_job(&workspace, &job);

    match generate_image(&api_key, &request, &config).await {
        Ok(generated) => match save_generated_image(&workspace, &request, &job, &generated) {
            Ok(output_file) => {
                job.status = AssetStatus::Done;
                job.output_files = vec![output_file];
                job.metadata = json!({
                    "provider": image_provider_name(&request.provider),
                    "provider_id": request.provider,
                    "api": generated.api,
                    "model": request.model,
                    "mime_type": generated.mime_type,
                    "parameters": {
                        "aspect_ratio": request.aspect_ratio,
                        "image_size": request.image_size
                    },
                    "details": generated.metadata,
                    "license": license_metadata(&request.provider)
                });
                job.error = None;
            }
            Err(err) => {
                job.status = AssetStatus::Failed;
                job.error = Some(err.to_string());
            }
        },
        Err(err) => {
            job.status = AssetStatus::Failed;
            job.error = Some(err.to_string());
        }
    }

    job.updated_at = unix_timestamp();
    let _ = upsert_job(&workspace, &job);
    job
}

pub async fn run_spritesheet_job(
    workspace: Workspace,
    api_key: String,
    config: AppConfig,
    request: SpritesheetAssetRequest,
    mut job: AssetJob,
) -> AssetJob {
    let image_request = ImageAssetRequest {
        provider: request.provider.clone(),
        prompt: format!(
            "{}\n\nCreate a clean game spritesheet laid out as a {} columns by {} rows grid. Keep each cell consistent, isolated, and ready for slicing. Avoid text labels.",
            request.prompt,
            request.columns.max(1),
            request.rows.max(1)
        ),
        model: request.model.clone(),
        aspect_ratio: request.aspect_ratio.clone(),
        image_size: request.image_size.clone(),
    };

    job.status = AssetStatus::Running;
    job.updated_at = unix_timestamp();
    let _ = upsert_job(&workspace, &job);

    match generate_image(&api_key, &image_request, &config).await {
        Ok(generated) => match save_generated_image(&workspace, &image_request, &job, &generated) {
            Ok(output_file) => {
                job.status = AssetStatus::Done;
                job.output_files = vec![output_file];
                job.metadata = json!({
                    "provider": image_provider_name(&job.provider),
                    "provider_id": job.provider,
                    "asset_kind": "spritesheet",
                    "api": generated.api,
                    "model": job.model,
                    "parameters": {
                        "columns": request.columns.max(1),
                        "rows": request.rows.max(1),
                        "aspect_ratio": request.aspect_ratio,
                        "image_size": request.image_size
                    },
                    "details": generated.metadata,
                    "license": license_metadata(&job.provider)
                });
                job.error = None;
            }
            Err(err) => {
                job.status = AssetStatus::Failed;
                job.error = Some(err.to_string());
            }
        },
        Err(err) => {
            job.status = AssetStatus::Failed;
            job.error = Some(err.to_string());
        }
    }

    job.updated_at = unix_timestamp();
    let _ = upsert_job(&workspace, &job);
    job
}

pub async fn run_audio_job(
    workspace: Workspace,
    api_key: String,
    config: AppConfig,
    mut request: AudioAssetRequest,
    mut job: AssetJob,
) -> AssetJob {
    if request.provider.trim().is_empty() {
        request.provider = OPENAI_AUDIO_PROVIDER_ID.to_string();
    }
    if request.model.trim().is_empty() {
        request.model = default_audio_model(&request.provider).to_string();
    }

    job.status = AssetStatus::Running;
    job.provider = request.provider.clone();
    job.model = request.model.clone();
    job.updated_at = unix_timestamp();
    let _ = upsert_job(&workspace, &job);

    match generate_audio(&api_key, &request, &config).await {
        Ok(generated) => match save_generated_binary(
            &workspace,
            "assets/generated/audio",
            &request.prompt,
            &job,
            &generated,
        ) {
            Ok(output_file) => {
                job.status = AssetStatus::Done;
                job.output_files = vec![output_file];
                job.metadata = json!({
                    "provider": audio_provider_name(&request.provider),
                    "provider_id": request.provider,
                    "api": generated.api,
                    "model": request.model,
                    "mime_type": generated.mime_type,
                    "parameters": {
                        "voice": request.voice,
                        "format": request.format
                    },
                    "details": generated.metadata,
                    "license": license_metadata(&job.provider)
                });
                job.error = None;
            }
            Err(err) => {
                job.status = AssetStatus::Failed;
                job.error = Some(err.to_string());
            }
        },
        Err(err) => {
            job.status = AssetStatus::Failed;
            job.error = Some(err.to_string());
        }
    }

    job.updated_at = unix_timestamp();
    let _ = upsert_job(&workspace, &job);
    job
}

pub async fn run_video_job(
    workspace: Workspace,
    api_key: String,
    config: AppConfig,
    mut request: VideoAssetRequest,
    mut job: AssetJob,
) -> AssetJob {
    if request.provider.trim().is_empty() {
        request.provider = OPENAI_VIDEO_PROVIDER_ID.to_string();
    }
    if request.model.trim().is_empty() {
        request.model = default_video_model(&request.provider).to_string();
    }

    job.status = AssetStatus::Running;
    job.provider = request.provider.clone();
    job.model = request.model.clone();
    job.updated_at = unix_timestamp();
    let _ = upsert_job(&workspace, &job);

    match generate_video(&api_key, &request, &config).await {
        Ok(generated) => match save_generated_binary(
            &workspace,
            "assets/generated/video",
            &request.prompt,
            &job,
            &generated,
        ) {
            Ok(output_file) => {
                job.status = AssetStatus::Done;
                job.output_files = vec![output_file];
                job.metadata = json!({
                    "provider": video_provider_name(&request.provider),
                    "provider_id": request.provider,
                    "api": generated.api,
                    "model": request.model,
                    "mime_type": generated.mime_type,
                    "parameters": {
                        "size": request.size,
                        "seconds": request.seconds.clamp(1, 20)
                    },
                    "details": generated.metadata,
                    "license": license_metadata(&job.provider)
                });
                job.error = None;
            }
            Err(err) => {
                job.status = AssetStatus::Failed;
                job.error = Some(err.to_string());
            }
        },
        Err(err) => {
            job.status = AssetStatus::Failed;
            job.error = Some(err.to_string());
        }
    }

    job.updated_at = unix_timestamp();
    let _ = upsert_job(&workspace, &job);
    job
}

async fn generate_image(
    api_key: &str,
    request: &ImageAssetRequest,
    config: &AppConfig,
) -> anyhow::Result<GeneratedImage> {
    if api_key.trim().is_empty() {
        anyhow::bail!(
            "{} is empty. Save an API key before generating image assets.",
            image_provider_env_var(&request.provider)
        );
    }
    if request.prompt.trim().is_empty() {
        anyhow::bail!("Asset prompt is empty");
    }

    let client = build_http_client(config)?;
    match request.provider.as_str() {
        OPENAI_IMAGE_PROVIDER_ID => generate_openai_image(&client, api_key, request).await,
        GEMINI_IMAGE_PROVIDER_ID => generate_gemini_image(&client, api_key, request).await,
        STABILITY_IMAGE_PROVIDER_ID => generate_stability_image(&client, api_key, request).await,
        REPLICATE_IMAGE_PROVIDER_ID => generate_replicate_image(&client, api_key, request).await,
        _ => anyhow::bail!("Unsupported image provider: {}", request.provider),
    }
}

async fn generate_audio(
    api_key: &str,
    request: &AudioAssetRequest,
    config: &AppConfig,
) -> anyhow::Result<GeneratedBinary> {
    if api_key.trim().is_empty() {
        anyhow::bail!("OPENAI_API_KEY is empty. Save a key before generating audio assets.");
    }
    if request.prompt.trim().is_empty() {
        anyhow::bail!("Audio prompt is empty");
    }
    if request.provider != OPENAI_AUDIO_PROVIDER_ID {
        anyhow::bail!("Unsupported audio provider: {}", request.provider);
    }

    let format = normalize_audio_format(&request.format);
    let body = json!({
        "model": request.model,
        "modalities": ["text", "audio"],
        "audio": {
            "voice": normalize_voice(&request.voice),
            "format": format
        },
        "messages": [{
            "role": "user",
            "content": request.prompt
        }]
    });

    let client = build_http_client(config)?;
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("OpenAI audio API error {status}: {text}");
    }

    let value = serde_json::from_str::<Value>(&text)?;
    let encoded = value
        .pointer("/choices/0/message/audio/data")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("OpenAI audio response did not include audio data"))?;

    Ok(GeneratedBinary {
        bytes: general_purpose::STANDARD.decode(encoded)?,
        mime_type: mime_for_audio_format(format).to_string(),
        api: "chat/completions.audio",
        metadata: json!({
            "transcript": value.pointer("/choices/0/message/audio/transcript").and_then(Value::as_str)
        }),
    })
}

async fn generate_video(
    api_key: &str,
    request: &VideoAssetRequest,
    config: &AppConfig,
) -> anyhow::Result<GeneratedBinary> {
    if api_key.trim().is_empty() {
        anyhow::bail!("OPENAI_API_KEY is empty. Save a key before generating video assets.");
    }
    if request.prompt.trim().is_empty() {
        anyhow::bail!("Video prompt is empty");
    }
    if request.provider != OPENAI_VIDEO_PROVIDER_ID {
        anyhow::bail!("Unsupported video provider: {}", request.provider);
    }

    let client = build_http_client(config)?;
    let form = multipart::Form::new()
        .text("model", request.model.clone())
        .text("prompt", request.prompt.clone())
        .text("size", normalize_video_size(&request.size).to_string())
        .text("seconds", request.seconds.clamp(1, 20).to_string());
    let response = client
        .post("https://api.openai.com/v1/videos")
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("OpenAI video API error {status}: {text}");
    }

    let mut value = serde_json::from_str::<Value>(&text)?;
    let video_id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("OpenAI video response did not include id"))?
        .to_string();

    for _ in 0..120 {
        let status_text = value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match status_text {
            "completed" | "succeeded" => {
                let content_url = format!("https://api.openai.com/v1/videos/{video_id}/content");
                let content = client.get(content_url).bearer_auth(api_key).send().await?;
                let content_status = content.status();
                if !content_status.is_success() {
                    let text = content.text().await.unwrap_or_default();
                    anyhow::bail!("OpenAI video content download failed {content_status}: {text}");
                }
                return Ok(GeneratedBinary {
                    bytes: content.bytes().await?.to_vec(),
                    mime_type: "video/mp4".to_string(),
                    api: "videos",
                    metadata: json!({
                        "video_id": video_id,
                        "final_status": status_text,
                        "job": value
                    }),
                });
            }
            "failed" | "cancelled" | "canceled" => {
                anyhow::bail!("OpenAI video generation failed: {value}");
            }
            _ => {}
        }

        sleep(Duration::from_secs(5)).await;
        let response = client
            .get(format!("https://api.openai.com/v1/videos/{video_id}"))
            .bearer_auth(api_key)
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("OpenAI video polling error {status}: {text}");
        }
        value = serde_json::from_str::<Value>(&text)?;
    }

    anyhow::bail!("OpenAI video generation did not finish before timeout");
}

async fn generate_openai_image(
    client: &reqwest::Client,
    api_key: &str,
    request: &ImageAssetRequest,
) -> anyhow::Result<GeneratedImage> {
    let size = openai_size_for_aspect_ratio(&request.aspect_ratio);
    let body = json!({
        "model": request.model,
        "prompt": request.prompt,
        "size": size,
        "n": 1
    });

    let response = client
        .post("https://api.openai.com/v1/images/generations")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("OpenAI image API error {status}: {text}");
    }

    let value = serde_json::from_str::<Value>(&text)?;
    if let Some(encoded) = value.pointer("/data/0/b64_json").and_then(Value::as_str) {
        return Ok(GeneratedImage {
            bytes: general_purpose::STANDARD.decode(encoded)?,
            mime_type: "image/png".to_string(),
            api: "images/generations",
            metadata: json!({ "size": size }),
        });
    }

    if let Some(url) = value.pointer("/data/0/url").and_then(Value::as_str) {
        let (bytes, mime_type) = download_generated_file(client, url).await?;
        return Ok(GeneratedImage {
            bytes,
            mime_type,
            api: "images/generations",
            metadata: json!({ "size": size, "source_url": url }),
        });
    }

    anyhow::bail!("OpenAI image response did not include b64_json or url output");
}

async fn generate_gemini_image(
    client: &reqwest::Client,
    api_key: &str,
    request: &ImageAssetRequest,
) -> anyhow::Result<GeneratedImage> {
    let body = json!({
        "model": request.model,
        "input": [
            {
                "type": "text",
                "text": request.prompt
            }
        ],
        "response_format": {
            "type": "image",
            "mime_type": "image/png",
            "aspect_ratio": request.aspect_ratio,
            "image_size": request.image_size
        }
    });

    let response = client
        .post("https://generativelanguage.googleapis.com/v1beta/interactions")
        .header("x-goog-api-key", api_key)
        .json(&body)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("Gemini image API error {status}: {text}");
    }

    let value = serde_json::from_str::<Value>(&text)?;
    let (encoded, mime_type) = extract_image_data(&value)
        .ok_or_else(|| anyhow::anyhow!("Gemini response did not include output_image data"))?;

    Ok(GeneratedImage {
        bytes: general_purpose::STANDARD.decode(encoded)?,
        mime_type: mime_type.to_string(),
        api: "interactions",
        metadata: json!({}),
    })
}

async fn generate_stability_image(
    client: &reqwest::Client,
    api_key: &str,
    request: &ImageAssetRequest,
) -> anyhow::Result<GeneratedImage> {
    let form = multipart::Form::new()
        .text("prompt", request.prompt.clone())
        .text("output_format", "png")
        .text(
            "aspect_ratio",
            stability_aspect_ratio(&request.aspect_ratio).to_string(),
        );

    let response = client
        .post("https://api.stability.ai/v2beta/stable-image/generate/core")
        .bearer_auth(api_key)
        .header("accept", "image/*")
        .multipart(form)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Stability image API error {status}: {text}");
    }

    Ok(GeneratedImage {
        bytes: response.bytes().await?.to_vec(),
        mime_type: "image/png".to_string(),
        api: "stable-image-core",
        metadata: json!({
            "aspect_ratio": stability_aspect_ratio(&request.aspect_ratio)
        }),
    })
}

async fn generate_replicate_image(
    client: &reqwest::Client,
    api_key: &str,
    request: &ImageAssetRequest,
) -> anyhow::Result<GeneratedImage> {
    let model_path = request.model.trim().trim_matches('/');
    let model_parts = model_path.split('/').collect::<Vec<_>>();
    if model_parts.len() != 2 || model_parts.iter().any(|part| part.trim().is_empty()) {
        anyhow::bail!(
            "Replicate model must look like owner/name, for example black-forest-labs/flux-schnell"
        );
    }

    let url = format!(
        "https://api.replicate.com/v1/models/{}/{}/predictions",
        model_parts[0], model_parts[1]
    );
    let body = json!({
        "input": {
            "prompt": request.prompt,
            "aspect_ratio": replicate_aspect_ratio(&request.aspect_ratio),
            "output_format": "png"
        }
    });

    let mut value = post_replicate_prediction(client, api_key, &url, &body).await?;
    for _ in 0..REPLICATE_POLL_LIMIT {
        if let Some(output_url) = first_url_in_value(value.get("output").unwrap_or(&Value::Null)) {
            let (bytes, mime_type) = download_generated_file(client, output_url).await?;
            return Ok(GeneratedImage {
                bytes,
                mime_type,
                api: "models.predictions.create",
                metadata: json!({
                    "prediction_id": value.get("id").and_then(Value::as_str),
                    "source_url": output_url
                }),
            });
        }

        match value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "failed" | "canceled" => {
                let error = value
                    .get("error")
                    .map(Value::to_string)
                    .unwrap_or_else(|| "unknown prediction error".to_string());
                anyhow::bail!("Replicate prediction failed: {error}");
            }
            _ => {}
        }

        let Some(get_url) = value.pointer("/urls/get").and_then(Value::as_str) else {
            anyhow::bail!("Replicate response did not include output or polling URL");
        };

        sleep(Duration::from_secs(2)).await;
        value = get_replicate_prediction(client, api_key, get_url).await?;
    }

    anyhow::bail!("Replicate prediction did not finish before timeout");
}

async fn post_replicate_prediction(
    client: &reqwest::Client,
    api_key: &str,
    url: &str,
    body: &Value,
) -> anyhow::Result<Value> {
    let response = client
        .post(url)
        .bearer_auth(api_key)
        .header("Prefer", "wait")
        .json(body)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("Replicate prediction API error {status}: {text}");
    }

    Ok(serde_json::from_str::<Value>(&text)?)
}

async fn get_replicate_prediction(
    client: &reqwest::Client,
    api_key: &str,
    url: &str,
) -> anyhow::Result<Value> {
    let response = client.get(url).bearer_auth(api_key).send().await?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("Replicate polling API error {status}: {text}");
    }

    Ok(serde_json::from_str::<Value>(&text)?)
}

async fn download_generated_file(
    client: &reqwest::Client,
    url: &str,
) -> anyhow::Result<(Vec<u8>, String)> {
    let response = client.get(url).send().await?;
    let status = response.status();
    let mime_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/png")
        .split(';')
        .next()
        .unwrap_or("image/png")
        .to_string();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("Generated file download failed {status}: {text}");
    }

    Ok((response.bytes().await?.to_vec(), mime_type))
}

fn save_generated_image(
    workspace: &Workspace,
    request: &ImageAssetRequest,
    job: &AssetJob,
    generated: &GeneratedImage,
) -> anyhow::Result<String> {
    let extension = extension_for_mime(&generated.mime_type);
    let file_name = format!("{}-{}.{}", slugify(&request.prompt), job.id, extension);
    let rel_path = format!("assets/generated/images/{file_name}");
    let output_path = workspace.resolve_for_write(&rel_path)?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, &generated.bytes)?;

    Ok(rel_path)
}

fn save_generated_binary(
    workspace: &Workspace,
    folder: &str,
    prompt: &str,
    job: &AssetJob,
    generated: &GeneratedBinary,
) -> anyhow::Result<String> {
    let extension = extension_for_mime(&generated.mime_type);
    let file_name = format!("{}-{}.{}", slugify(prompt), job.id, extension);
    let rel_path = format!("{folder}/{file_name}");
    let output_path = workspace.resolve_for_write(&rel_path)?;
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, &generated.bytes)?;

    Ok(rel_path)
}

fn extract_image_data(value: &Value) -> Option<(&str, &str)> {
    if let Some(data) = value.pointer("/output_image/data").and_then(Value::as_str) {
        let mime = value
            .pointer("/output_image/mime_type")
            .or_else(|| value.pointer("/output_image/mimeType"))
            .and_then(Value::as_str)
            .unwrap_or("image/png");
        return Some((data, mime));
    }

    find_inline_image_data(value)
}

fn find_inline_image_data(value: &Value) -> Option<(&str, &str)> {
    match value {
        Value::Object(map) => {
            if let Some(inline) = map.get("inlineData").or_else(|| map.get("inline_data")) {
                let data = inline.get("data").and_then(Value::as_str)?;
                let mime = inline
                    .get("mimeType")
                    .or_else(|| inline.get("mime_type"))
                    .and_then(Value::as_str)
                    .unwrap_or("image/png");
                return Some((data, mime));
            }

            map.values().find_map(find_inline_image_data)
        }
        Value::Array(items) => items.iter().find_map(find_inline_image_data),
        _ => None,
    }
}

fn first_url_in_value(value: &Value) -> Option<&str> {
    match value {
        Value::String(text) if text.starts_with("http://") || text.starts_with("https://") => {
            Some(text)
        }
        Value::Array(items) => items.iter().find_map(first_url_in_value),
        Value::Object(map) => map.values().find_map(first_url_in_value),
        _ => None,
    }
}

pub fn absolute_output_path(workspace: &Workspace, rel_path: &str) -> Option<PathBuf> {
    workspace.resolve_existing(rel_path).ok()
}

pub fn export_asset(
    workspace: &Workspace,
    source_path: &str,
    target_name: Option<&str>,
) -> anyhow::Result<AssetJob> {
    let source = workspace.resolve_existing(source_path)?;
    if !source.is_file() {
        anyhow::bail!("export source must be a file");
    }

    let now = unix_timestamp();
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("bin");
    let stem = target_name
        .filter(|name| !name.trim().is_empty())
        .map(slugify)
        .unwrap_or_else(|| {
            source
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(slugify)
                .unwrap_or_else(|| "asset-export".to_string())
        });
    let mut job = AssetJob {
        id: format!("export-{}", uuid::Uuid::new_v4()),
        kind: kind_for_path(&source),
        status: AssetStatus::Running,
        provider: "local-export".to_string(),
        model: "local-copy".to_string(),
        prompt: format!("Export {source_path}"),
        parameters: json!({
            "source_path": source_path,
            "target_name": target_name
        }),
        output_files: Vec::new(),
        metadata: json!({}),
        error: None,
        created_at: now,
        updated_at: now,
    };
    let rel_path = format!("assets/generated/exports/{}-{}.{}", stem, job.id, extension);
    let target = workspace.resolve_for_write(&rel_path)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&source, &target)?;
    job.status = AssetStatus::Done;
    job.output_files = vec![rel_path];
    job.metadata = json!({
        "operation": "export",
        "source_path": source_path,
        "license": license_metadata("local-export")
    });
    job.updated_at = unix_timestamp();
    upsert_job(workspace, &job)?;
    Ok(job)
}

pub fn upscale_asset(
    workspace: &Workspace,
    source_path: &str,
    scale: u32,
) -> anyhow::Result<AssetJob> {
    let source = workspace.resolve_existing(source_path)?;
    if !is_image_path(&source) {
        anyhow::bail!("upscale source must be an image");
    }
    let scale = scale.clamp(2, 4);
    let now = unix_timestamp();
    let mut job = AssetJob {
        id: format!("upscale-{}", uuid::Uuid::new_v4()),
        kind: AssetKind::Image,
        status: AssetStatus::Running,
        provider: "local-upscale".to_string(),
        model: "lanczos3".to_string(),
        prompt: format!("Upscale {source_path} by {scale}x"),
        parameters: json!({
            "source_path": source_path,
            "scale": scale
        }),
        output_files: Vec::new(),
        metadata: json!({}),
        error: None,
        created_at: now,
        updated_at: now,
    };

    let image = image::open(&source)?;
    let width = image.width().saturating_mul(scale);
    let height = image.height().saturating_mul(scale);
    let resized = image.resize(width, height, image::imageops::FilterType::Lanczos3);
    let rel_path = format!(
        "assets/generated/exports/{}-{}.png",
        slugify(source_path),
        job.id
    );
    let target = workspace.resolve_for_write(&rel_path)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    resized.save_with_format(&target, image::ImageFormat::Png)?;
    job.status = AssetStatus::Done;
    job.output_files = vec![rel_path];
    job.metadata = json!({
        "operation": "upscale",
        "source_path": source_path,
        "scale": scale,
        "width": width,
        "height": height,
        "license": license_metadata("local-upscale")
    });
    job.updated_at = unix_timestamp();
    upsert_job(workspace, &job)?;
    Ok(job)
}

pub fn attach_asset_context(workspace: &Workspace, source_path: &str) -> anyhow::Result<Value> {
    let source = workspace.resolve_existing(source_path)?;
    if !source.is_file() {
        anyhow::bail!("attach source must be a file");
    }
    let metadata = load_jobs(workspace)
        .into_iter()
        .find(|job| job.output_files.iter().any(|output| output == source_path))
        .map(|job| {
            json!({
                "job_id": job.id,
                "kind": job.kind,
                "provider": job.provider,
                "model": job.model,
                "prompt": job.prompt,
                "parameters": job.parameters,
                "metadata": job.metadata
            })
        })
        .unwrap_or_else(|| json!({}));
    let bytes = fs::metadata(&source)?.len();
    let context = json!({
        "path": source_path,
        "bytes": bytes,
        "is_image": is_image_path(&source),
        "metadata": metadata
    });
    let attachments_path = "assets/generated/attachments/attached_assets.json";
    let mut attachments = workspace
        .read_text(attachments_path, 2_000_000)
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<Value>>(&text).ok())
        .unwrap_or_default();
    attachments.push(context.clone());
    workspace.write_text(
        attachments_path,
        &serde_json::to_string_pretty(&attachments)?,
    )?;
    Ok(context)
}

pub fn is_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp")
    )
}

fn kind_for_path(path: &Path) -> AssetKind {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("wav" | "mp3" | "ogg" | "opus") => AssetKind::Audio,
        Some("mp4" | "mov" | "webm") => AssetKind::Video,
        _ => AssetKind::Image,
    }
}

pub fn openai_size_for_aspect_ratio(aspect_ratio: &str) -> &'static str {
    match aspect_ratio {
        "16:9" | "3:2" | "4:3" => "1536x1024",
        "9:16" | "2:3" | "3:4" => "1024x1536",
        _ => "1024x1024",
    }
}

fn stability_aspect_ratio(aspect_ratio: &str) -> &str {
    match aspect_ratio {
        "16:9" | "3:2" | "2:3" | "4:3" | "3:4" | "9:16" | "1:1" => aspect_ratio,
        _ => "1:1",
    }
}

fn replicate_aspect_ratio(aspect_ratio: &str) -> &str {
    match aspect_ratio {
        "16:9" | "3:2" | "2:3" | "4:3" | "3:4" | "9:16" | "1:1" => aspect_ratio,
        _ => "1:1",
    }
}

fn extension_for_mime(mime_type: &str) -> &'static str {
    match mime_type {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "audio/mpeg" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/ogg" => "ogg",
        "video/mp4" => "mp4",
        _ => "png",
    }
}

fn normalize_audio_format(format: &str) -> &'static str {
    match format.trim().to_ascii_lowercase().as_str() {
        "mp3" => "mp3",
        "ogg" | "opus" => "opus",
        _ => "wav",
    }
}

fn mime_for_audio_format(format: &str) -> &'static str {
    match format {
        "mp3" => "audio/mpeg",
        "opus" => "audio/ogg",
        _ => "audio/wav",
    }
}

fn normalize_voice(voice: &str) -> &'static str {
    match voice.trim().to_ascii_lowercase().as_str() {
        "ash" => "ash",
        "ballad" => "ballad",
        "coral" => "coral",
        "echo" => "echo",
        "sage" => "sage",
        "shimmer" => "shimmer",
        "verse" => "verse",
        _ => "alloy",
    }
}

fn normalize_video_size(size: &str) -> &'static str {
    match size.trim().to_ascii_lowercase().as_str() {
        "720x1280" | "9:16" => "720x1280",
        "1280x720" | "16:9" => "1280x720",
        "1920x1080" => "1920x1080",
        "1080x1920" => "1080x1920",
        _ => "1280x720",
    }
}

fn license_metadata(provider_id: &str) -> Value {
    let (provider, terms_url) = match provider_id {
        OPENAI_IMAGE_PROVIDER_ID | OPENAI_AUDIO_PROVIDER_ID | OPENAI_VIDEO_PROVIDER_ID => {
            ("OpenAI", "https://openai.com/policies/terms-of-use/")
        }
        GEMINI_IMAGE_PROVIDER_ID => ("Google Gemini", "https://ai.google.dev/gemini-api/terms"),
        STABILITY_IMAGE_PROVIDER_ID => ("Stability AI", "https://stability.ai/terms-of-use"),
        REPLICATE_IMAGE_PROVIDER_ID => ("Replicate", "https://replicate.com/terms"),
        _ => ("Unknown", ""),
    };
    json!({
        "provider": provider,
        "terms_url": terms_url,
        "note": "Review provider terms and project license requirements before production use."
    })
}

fn slugify(text: &str) -> String {
    let mut slug = text
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() || matches!(ch, '-' | '_' | '.') {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>();

    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    slug = slug.trim_matches('-').chars().take(48).collect();
    if slug.is_empty() {
        "asset".to_string()
    } else {
        slug
    }
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
    fn provider_specs_include_primary_image_services() {
        let ids = image_provider_specs()
            .iter()
            .map(|provider| provider.id)
            .collect::<Vec<_>>();

        assert!(ids.contains(&OPENAI_IMAGE_PROVIDER_ID));
        assert!(ids.contains(&GEMINI_IMAGE_PROVIDER_ID));
        assert!(ids.contains(&STABILITY_IMAGE_PROVIDER_ID));
        assert!(ids.contains(&REPLICATE_IMAGE_PROVIDER_ID));
    }

    #[test]
    fn new_image_job_keeps_selected_provider() {
        let request = ImageAssetRequest {
            provider: STABILITY_IMAGE_PROVIDER_ID.to_string(),
            prompt: "pixel art potion".to_string(),
            model: "stable-image-core".to_string(),
            aspect_ratio: "1:1".to_string(),
            image_size: "1K".to_string(),
        };

        let job = AssetJob::new_image(&request);

        assert_eq!(job.provider, STABILITY_IMAGE_PROVIDER_ID);
        assert_eq!(job.model, "stable-image-core");
    }

    #[test]
    fn rebuilds_image_request_from_job_metadata() {
        let request = ImageAssetRequest {
            provider: OPENAI_IMAGE_PROVIDER_ID.to_string(),
            prompt: "app icon".to_string(),
            model: "gpt-image-2".to_string(),
            aspect_ratio: "16:9".to_string(),
            image_size: "2K".to_string(),
        };
        let job = AssetJob::new_image(&request);

        let rebuilt = image_request_from_job(&job, Some("variation".to_string()));

        assert_eq!(rebuilt.provider, OPENAI_IMAGE_PROVIDER_ID);
        assert_eq!(rebuilt.prompt, "variation");
        assert_eq!(rebuilt.aspect_ratio, "16:9");
        assert_eq!(rebuilt.image_size, "2K");
    }

    #[test]
    fn maps_openai_image_sizes_from_common_ratios() {
        assert_eq!(openai_size_for_aspect_ratio("1:1"), "1024x1024");
        assert_eq!(openai_size_for_aspect_ratio("16:9"), "1536x1024");
        assert_eq!(openai_size_for_aspect_ratio("9:16"), "1024x1536");
    }

    #[test]
    fn extracts_output_image_data() {
        let value = json!({
            "output_image": {
                "data": "aGVsbG8=",
                "mime_type": "image/png"
            }
        });

        assert_eq!(extract_image_data(&value), Some(("aGVsbG8=", "image/png")));
    }

    #[test]
    fn extracts_nested_inline_data() {
        let value = json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "inlineData": {
                            "data": "aGVsbG8=",
                            "mimeType": "image/jpeg"
                        }
                    }]
                }
            }]
        });

        assert_eq!(extract_image_data(&value), Some(("aGVsbG8=", "image/jpeg")));
    }

    #[test]
    fn finds_replicate_output_url() {
        let value = json!([
            {
                "url": "https://replicate.delivery/example.png"
            }
        ]);

        assert_eq!(
            first_url_in_value(&value),
            Some("https://replicate.delivery/example.png")
        );
    }

    #[test]
    fn slug_is_ascii_and_bounded() {
        assert_eq!(
            slugify("Cute 2D knight sprite, idle pose!!!"),
            "cute-2d-knight-sprite-idle-pose"
        );
        assert_eq!(slugify("РїРµСЂСЃРѕРЅР°Р¶"), "asset");
    }
}
