use crate::config::AppConfig;
use crate::http::build_http_client;
use crate::unreal::unreal_snapshot;
use crate::workspace::Workspace;
use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use reqwest::multipart;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MESHY_3D_PROVIDER_ID: &str = "meshy-3d";
pub const TRIPO_3D_PROVIDER_ID: &str = "tripo-3d";
pub const THREE_D_JOBS_PATH: &str = "assets/generated/leetcode/assets3d/jobs.json";
pub const THREE_D_OUTPUT_DIR: &str = "assets/generated/3d";
pub const THREE_D_IMPORT_DIR: &str = "assets/generated/leetcode/unreal/imports";

#[derive(Clone, Copy, Debug)]
pub struct ThreeDProviderSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub default_model: &'static str,
    pub env_var: &'static str,
    pub supports_text: bool,
    pub supports_image: bool,
    pub terms_url: &'static str,
}

pub fn three_d_provider_specs() -> &'static [ThreeDProviderSpec] {
    const SPECS: &[ThreeDProviderSpec] = &[
        ThreeDProviderSpec {
            id: MESHY_3D_PROVIDER_ID,
            name: "Meshy",
            default_model: "latest",
            env_var: "MESHY_API_KEY",
            supports_text: true,
            supports_image: true,
            terms_url: "https://www.meshy.ai/terms",
        },
        ThreeDProviderSpec {
            id: TRIPO_3D_PROVIDER_ID,
            name: "Tripo",
            default_model: "P1-20260311",
            env_var: "TRIPO_API_KEY",
            supports_text: true,
            supports_image: true,
            terms_url: "https://www.tripo3d.ai/terms-of-service",
        },
    ];
    SPECS
}

pub fn three_d_provider_spec(provider: &str) -> Option<&'static ThreeDProviderSpec> {
    three_d_provider_specs()
        .iter()
        .find(|spec| spec.id == normalize_provider(provider))
}

pub fn normalize_provider(provider: &str) -> String {
    match provider.trim().to_ascii_lowercase().as_str() {
        "meshy" | "meshy-3d" => MESHY_3D_PROVIDER_ID.to_string(),
        "tripo" | "tripo-3d" => TRIPO_3D_PROVIDER_ID.to_string(),
        _ => MESHY_3D_PROVIDER_ID.to_string(),
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreeDInputKind {
    Text,
    Image,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreeDJobStatus {
    Pending,
    Running,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ThreeDPipelineStage {
    Submitted,
    Geometry,
    Texturing,
    Download,
    Validation,
    Ready,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreeDAssetJob {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub input_kind: ThreeDInputKind,
    pub prompt: String,
    pub source_image: Option<String>,
    pub target_format: String,
    pub target_polycount: u32,
    pub enable_pbr: bool,
    pub pose_mode: String,
    pub license_confirmed: bool,
    pub provider_task_id: String,
    pub provider_task_kind: String,
    pub status: ThreeDJobStatus,
    pub stage: ThreeDPipelineStage,
    pub progress: u8,
    #[serde(default)]
    pub output_files: Vec<String>,
    pub validation: Option<ThreeDValidationReport>,
    #[serde(default)]
    pub provider_payload: Value,
    pub error: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SubmitThreeDAssetArgs {
    pub prompt: String,
    pub image_path: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub target_format: Option<String>,
    pub target_polycount: Option<u32>,
    pub enable_pbr: Option<bool>,
    pub pose_mode: Option<String>,
    pub license_confirmed: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RefreshThreeDAssetArgs {
    pub job_id: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ValidateThreeDAssetArgs {
    pub source_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ThreeDValidationReport {
    pub source_path: String,
    pub format: String,
    pub file_size: u64,
    pub geometry_valid: bool,
    pub import_ready: bool,
    pub mesh_count: usize,
    pub primitive_count: usize,
    pub vertex_count: u64,
    pub triangle_count: u64,
    pub material_count: usize,
    pub texture_count: usize,
    pub skin_count: usize,
    pub animation_count: usize,
    pub lod_count: usize,
    pub has_uv0: bool,
    pub has_normals: bool,
    pub has_tangents: bool,
    pub has_pbr_material: bool,
    pub bounds_meters: Option<[f64; 3]>,
    pub scale_note: String,
    pub collision_recommendation: String,
    pub nanite_recommended: bool,
    pub rig_stage: String,
    pub animation_stage: String,
    pub provenance_present: bool,
    pub license_confirmed: bool,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UnrealImportThreeDArgs {
    pub source_path: String,
    pub destination_path: Option<String>,
    pub asset_type: Option<String>,
    pub skeleton_path: Option<String>,
    pub replace_existing: Option<bool>,
    pub import_lods: Option<bool>,
    pub enable_nanite: Option<bool>,
    pub collision: Option<String>,
    pub license_confirmed: Option<bool>,
    pub allow_validation_warnings: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnrealImportManifest {
    pub schema_version: u32,
    pub id: String,
    pub source_file: String,
    pub source_workspace_path: String,
    pub destination_path: String,
    pub asset_type: String,
    pub skeleton_path: Option<String>,
    pub replace_existing: bool,
    pub import_lods: bool,
    pub enable_nanite: bool,
    pub collision: String,
    pub validation: ThreeDValidationReport,
    pub result_path: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnrealImportCommand {
    pub manifest_path: String,
    pub result_path: String,
    pub shell_command: String,
    pub timeout_secs: u64,
}

pub fn load_3d_jobs(workspace: &Workspace) -> Vec<ThreeDAssetJob> {
    workspace
        .read_text(THREE_D_JOBS_PATH, 4_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_3d_jobs(workspace: &Workspace, jobs: &[ThreeDAssetJob]) -> Result<()> {
    workspace.write_text(THREE_D_JOBS_PATH, &serde_json::to_string_pretty(jobs)?)
}

pub fn upsert_3d_job(workspace: &Workspace, job: &ThreeDAssetJob) -> Result<()> {
    let mut jobs = load_3d_jobs(workspace);
    if let Some(existing) = jobs.iter_mut().find(|existing| existing.id == job.id) {
        *existing = job.clone();
    } else {
        jobs.push(job.clone());
    }
    jobs.sort_by_key(|item| item.created_at);
    save_3d_jobs(workspace, &jobs)
}

pub fn asset_3d_snapshot(workspace: &Workspace) -> Value {
    let jobs = load_3d_jobs(workspace);
    json!({
        "providers": three_d_provider_specs().iter().map(|spec| json!({
            "id": spec.id,
            "name": spec.name,
            "default_model": spec.default_model,
            "env_var": spec.env_var,
            "supports_text": spec.supports_text,
            "supports_image": spec.supports_image,
        })).collect::<Vec<_>>(),
        "jobs": jobs,
    })
}

pub async fn submit_3d_asset(
    workspace: &Workspace,
    args: SubmitThreeDAssetArgs,
    config: &AppConfig,
) -> Result<ThreeDAssetJob> {
    let provider = normalize_provider(args.provider.as_deref().unwrap_or(MESHY_3D_PROVIDER_ID));
    let spec = three_d_provider_spec(&provider)
        .ok_or_else(|| anyhow::anyhow!("Unsupported 3D provider: {provider}"))?;
    let api_key = config.api_key_for_provider(&provider);
    if api_key.trim().is_empty() {
        anyhow::bail!("Missing {} ({})", spec.name, spec.env_var);
    }
    let prompt = args.prompt.trim().to_string();
    let source_image = args
        .image_path
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(normalize_relative_path);
    let input_kind = if source_image.is_some() {
        ThreeDInputKind::Image
    } else {
        ThreeDInputKind::Text
    };
    if input_kind == ThreeDInputKind::Text && prompt.is_empty() {
        anyhow::bail!("A text-to-3D prompt is required");
    }
    if input_kind == ThreeDInputKind::Image && !spec.supports_image {
        anyhow::bail!("{} does not support image-to-3D", spec.name);
    }

    let target_format = normalize_format(args.target_format.as_deref().unwrap_or("glb"))?;
    let target_polycount = args.target_polycount.unwrap_or(20_000).clamp(48, 500_000);
    let enable_pbr = args.enable_pbr.unwrap_or(true);
    let pose_mode = normalize_pose_mode(args.pose_mode.as_deref().unwrap_or(""))?;
    let model = args
        .model
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| spec.default_model.to_string());
    let client = build_http_client(config)?;
    let (provider_task_id, provider_task_kind, provider_payload) = match provider.as_str() {
        MESHY_3D_PROVIDER_ID => {
            submit_meshy(
                workspace,
                &client,
                &api_key,
                input_kind,
                &prompt,
                source_image.as_deref(),
                &model,
                &target_format,
                target_polycount,
                enable_pbr,
                &pose_mode,
            )
            .await?
        }
        TRIPO_3D_PROVIDER_ID => {
            submit_tripo(
                workspace,
                &client,
                &api_key,
                input_kind,
                &prompt,
                source_image.as_deref(),
                &model,
                target_polycount,
                enable_pbr,
            )
            .await?
        }
        _ => anyhow::bail!("Unsupported 3D provider: {provider}"),
    };
    let now = unix_timestamp();
    let job = ThreeDAssetJob {
        id: format!("3d-{}", uuid::Uuid::new_v4()),
        provider,
        model,
        input_kind,
        prompt,
        source_image,
        target_format,
        target_polycount,
        enable_pbr,
        pose_mode,
        license_confirmed: args.license_confirmed.unwrap_or(false),
        provider_task_id,
        provider_task_kind,
        status: ThreeDJobStatus::Pending,
        stage: ThreeDPipelineStage::Submitted,
        progress: 0,
        output_files: Vec::new(),
        validation: None,
        provider_payload,
        error: None,
        created_at: now,
        updated_at: now,
    };
    upsert_3d_job(workspace, &job)?;
    Ok(job)
}

pub async fn refresh_3d_asset(
    workspace: &Workspace,
    args: RefreshThreeDAssetArgs,
    config: &AppConfig,
) -> Result<ThreeDAssetJob> {
    let mut job = load_3d_jobs(workspace)
        .into_iter()
        .find(|item| item.id == args.job_id)
        .ok_or_else(|| anyhow::anyhow!("3D job not found: {}", args.job_id))?;
    if matches!(job.status, ThreeDJobStatus::Ready | ThreeDJobStatus::Failed) {
        return Ok(job);
    }
    let spec = three_d_provider_spec(&job.provider)
        .ok_or_else(|| anyhow::anyhow!("Unsupported 3D provider: {}", job.provider))?;
    let api_key = config.api_key_for_provider(&job.provider);
    if api_key.trim().is_empty() {
        anyhow::bail!("Missing {} ({})", spec.name, spec.env_var);
    }
    let client = build_http_client(config)?;
    job.status = ThreeDJobStatus::Running;
    let refresh = match job.provider.as_str() {
        MESHY_3D_PROVIDER_ID => refresh_meshy(&client, &api_key, &mut job).await,
        TRIPO_3D_PROVIDER_ID => refresh_tripo(&client, &api_key, &mut job).await,
        _ => anyhow::bail!("Unsupported 3D provider: {}", job.provider),
    };
    if let Err(err) = refresh {
        job.status = ThreeDJobStatus::Failed;
        job.stage = ThreeDPipelineStage::Failed;
        job.error = Some(err.to_string());
    }
    if job.status == ThreeDJobStatus::Ready && job.output_files.is_empty() {
        job.stage = ThreeDPipelineStage::Download;
        match download_job_output(workspace, &client, &mut job, spec).await {
            Ok(()) => {
                job.stage = ThreeDPipelineStage::Validation;
                let report = validate_3d_asset_path(workspace, &job.output_files[0])?;
                job.validation = Some(report);
                job.stage = ThreeDPipelineStage::Ready;
                job.progress = 100;
            }
            Err(err) => {
                job.status = ThreeDJobStatus::Failed;
                job.stage = ThreeDPipelineStage::Failed;
                job.error = Some(err.to_string());
            }
        }
    }
    job.updated_at = unix_timestamp();
    upsert_3d_job(workspace, &job)?;
    Ok(job)
}

async fn submit_meshy(
    workspace: &Workspace,
    client: &reqwest::Client,
    api_key: &str,
    input_kind: ThreeDInputKind,
    prompt: &str,
    image_path: Option<&str>,
    model: &str,
    target_format: &str,
    target_polycount: u32,
    enable_pbr: bool,
    pose_mode: &str,
) -> Result<(String, String, Value)> {
    let (url, body, kind) = match input_kind {
        ThreeDInputKind::Text => (
            "https://api.meshy.ai/openapi/v2/text-to-3d",
            json!({
                "mode": "preview",
                "prompt": prompt,
                "ai_model": model,
                "should_remesh": true,
                "target_polycount": target_polycount,
                "target_formats": [target_format],
                "pose_mode": pose_mode,
            }),
            "meshy_text_preview",
        ),
        ThreeDInputKind::Image => {
            let path = image_path.context("Image path is required")?;
            let image_url = workspace_image_data_uri(workspace, path, 20 * 1024 * 1024)?;
            (
                "https://api.meshy.ai/openapi/v1/image-to-3d",
                json!({
                    "image_url": image_url,
                    "ai_model": model,
                    "should_texture": enable_pbr,
                    "enable_pbr": enable_pbr,
                    "should_remesh": true,
                    "target_polycount": target_polycount,
                    "target_formats": [target_format],
                    "pose_mode": pose_mode,
                }),
                "meshy_image",
            )
        }
    };
    let value = response_json(
        client
            .post(url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await?,
    )
    .await?;
    let task_id = value
        .get("result")
        .and_then(Value::as_str)
        .context("Meshy response does not contain result task id")?;
    Ok((task_id.to_string(), kind.to_string(), value))
}

async fn submit_tripo(
    workspace: &Workspace,
    client: &reqwest::Client,
    api_key: &str,
    input_kind: ThreeDInputKind,
    prompt: &str,
    image_path: Option<&str>,
    model: &str,
    target_polycount: u32,
    enable_pbr: bool,
) -> Result<(String, String, Value)> {
    let body = match input_kind {
        ThreeDInputKind::Text => json!({
            "type": "text_to_model",
            "prompt": prompt,
            "model_version": model,
            "face_limit": target_polycount.min(20_000),
            "texture": enable_pbr,
            "pbr": enable_pbr,
            "export_uv": true,
        }),
        ThreeDInputKind::Image => {
            let path = workspace.resolve_existing(image_path.context("Image path is required")?)?;
            let metadata = fs::metadata(&path)?;
            if metadata.len() > 20 * 1024 * 1024 {
                anyhow::bail!("Tripo image exceeds 20 MB");
            }
            let mime = image_mime(&path)?;
            let file_name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("reference.png")
                .to_string();
            let part = multipart::Part::bytes(fs::read(&path)?)
                .file_name(file_name)
                .mime_str(mime)?;
            let upload = response_json(
                client
                    .post("https://api.tripo3d.ai/v2/openapi/upload/sts")
                    .bearer_auth(api_key)
                    .multipart(multipart::Form::new().part("file", part))
                    .send()
                    .await?,
            )
            .await?;
            let token = upload
                .pointer("/data/image_token")
                .or_else(|| upload.pointer("/data/file_token"))
                .and_then(Value::as_str)
                .context("Tripo upload response does not contain image_token")?;
            json!({
                "type": "image_to_model",
                "file": { "type": mime, "file_token": token },
                "model_version": model,
                "face_limit": target_polycount.min(20_000),
                "texture": enable_pbr,
                "pbr": enable_pbr,
                "export_uv": true,
                "enable_image_autofix": true,
            })
        }
    };
    let value = response_json(
        client
            .post("https://api.tripo3d.ai/v2/openapi/task")
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await?,
    )
    .await?;
    let task_id = value
        .pointer("/data/task_id")
        .or_else(|| value.get("task_id"))
        .and_then(Value::as_str)
        .context("Tripo response does not contain task_id")?;
    Ok((task_id.to_string(), "tripo_generation".to_string(), value))
}

async fn refresh_meshy(
    client: &reqwest::Client,
    api_key: &str,
    job: &mut ThreeDAssetJob,
) -> Result<()> {
    let endpoint = if job.provider_task_kind == "meshy_image" {
        "image-to-3d"
    } else {
        "text-to-3d"
    };
    let url = format!(
        "https://api.meshy.ai/openapi/{}/{}/{}",
        if endpoint == "image-to-3d" {
            "v1"
        } else {
            "v2"
        },
        endpoint,
        job.provider_task_id
    );
    let value = response_json(client.get(url).bearer_auth(api_key).send().await?).await?;
    job.progress = value
        .get("progress")
        .and_then(Value::as_u64)
        .unwrap_or(job.progress as u64)
        .min(100) as u8;
    job.provider_payload = value.clone();
    match value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_uppercase()
        .as_str()
    {
        "SUCCEEDED" => {
            if job.provider_task_kind == "meshy_text_preview" && job.enable_pbr {
                let body = json!({
                    "mode": "refine",
                    "preview_task_id": job.provider_task_id,
                    "enable_pbr": true,
                    "target_formats": [job.target_format],
                    "ai_model": job.model,
                });
                let refine = response_json(
                    client
                        .post("https://api.meshy.ai/openapi/v2/text-to-3d")
                        .bearer_auth(api_key)
                        .json(&body)
                        .send()
                        .await?,
                )
                .await?;
                job.provider_task_id = refine
                    .get("result")
                    .and_then(Value::as_str)
                    .context("Meshy refine response does not contain task id")?
                    .to_string();
                job.provider_task_kind = "meshy_text_refine".to_string();
                job.provider_payload = refine;
                job.stage = ThreeDPipelineStage::Texturing;
                job.progress = 0;
            } else {
                job.status = ThreeDJobStatus::Ready;
                job.stage = ThreeDPipelineStage::Download;
            }
        }
        "FAILED" | "EXPIRED" | "CANCELED" => {
            anyhow::bail!(
                "Meshy task failed: {}",
                provider_error_message(&value).unwrap_or("unknown error")
            );
        }
        _ => {
            job.stage = if job.provider_task_kind.ends_with("refine") {
                ThreeDPipelineStage::Texturing
            } else {
                ThreeDPipelineStage::Geometry
            };
        }
    }
    Ok(())
}

async fn refresh_tripo(
    client: &reqwest::Client,
    api_key: &str,
    job: &mut ThreeDAssetJob,
) -> Result<()> {
    let url = format!(
        "https://api.tripo3d.ai/v2/openapi/task/{}",
        job.provider_task_id
    );
    let value = response_json(client.get(url).bearer_auth(api_key).send().await?).await?;
    job.progress = value
        .pointer("/data/progress")
        .or_else(|| value.get("progress"))
        .and_then(Value::as_u64)
        .unwrap_or(job.progress as u64)
        .min(100) as u8;
    job.provider_payload = value.clone();
    match value
        .pointer("/data/status")
        .or_else(|| value.get("status"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "success" => {
            job.status = ThreeDJobStatus::Ready;
            job.stage = ThreeDPipelineStage::Download;
        }
        "failed" | "banned" | "expired" | "cancelled" => {
            anyhow::bail!(
                "Tripo task failed: {}",
                provider_error_message(&value).unwrap_or("unknown error")
            );
        }
        _ => job.stage = ThreeDPipelineStage::Geometry,
    }
    Ok(())
}

async fn download_job_output(
    workspace: &Workspace,
    client: &reqwest::Client,
    job: &mut ThreeDAssetJob,
    provider: &ThreeDProviderSpec,
) -> Result<()> {
    let urls = collect_model_urls(&job.provider_payload);
    let selected = select_model_url(&urls, &job.target_format)
        .context("Provider result does not contain a supported model URL")?;
    let response = client.get(&selected.1).send().await?;
    if !response.status().is_success() {
        anyhow::bail!("Model download failed with HTTP {}", response.status());
    }
    if response
        .content_length()
        .is_some_and(|size| size > 300 * 1024 * 1024)
    {
        anyhow::bail!("Generated 3D model exceeds 300 MB");
    }
    let bytes = response.bytes().await?;
    if bytes.len() > 300 * 1024 * 1024 {
        anyhow::bail!("Generated 3D model exceeds 300 MB");
    }
    let extension = supported_extension(&selected.0).unwrap_or_else(|| job.target_format.clone());
    let base = safe_file_stem(&job.prompt);
    let relative = format!(
        "{}/{}-{}.{}",
        THREE_D_OUTPUT_DIR,
        if base.is_empty() { "asset-3d" } else { &base },
        compact_id(&job.id),
        extension
    );
    let output = workspace.resolve_for_write(&relative)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&output, bytes)?;
    let provenance = json!({
        "schema_version": 1,
        "job_id": job.id,
        "provider": job.provider,
        "provider_name": provider.name,
        "provider_task_id": job.provider_task_id,
        "model": job.model,
        "input_kind": job.input_kind,
        "prompt": job.prompt,
        "source_image": job.source_image,
        "download_url": selected.1,
        "terms_url": provider.terms_url,
        "license": "provider terms",
        "license_confirmed": job.license_confirmed,
        "generated_at": unix_timestamp(),
        "parameters": {
            "target_polycount": job.target_polycount,
            "pbr": job.enable_pbr,
            "pose_mode": job.pose_mode,
        }
    });
    fs::write(
        sidecar_path(&output),
        serde_json::to_vec_pretty(&provenance)?,
    )?;
    job.output_files = vec![relative];
    Ok(())
}

pub fn validate_3d_asset_path(
    workspace: &Workspace,
    relative_path: &str,
) -> Result<ThreeDValidationReport> {
    let path = workspace.resolve_existing(relative_path)?;
    validate_3d_asset_file(&path, &normalize_relative_path(relative_path))
}

pub fn validate_3d_asset_file(path: &Path, display_path: &str) -> Result<ThreeDValidationReport> {
    let format = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !matches!(
        format.as_str(),
        "glb" | "gltf" | "fbx" | "usd" | "usda" | "usdc"
    ) {
        anyhow::bail!("Unsupported 3D format: {format}");
    }
    let metadata = fs::metadata(path)?;
    let mut report = ThreeDValidationReport {
        source_path: display_path.to_string(),
        format: format.clone(),
        file_size: metadata.len(),
        scale_note: "Scale could not be measured from this format".to_string(),
        collision_recommendation:
            "auto convex for props; custom UCX for gameplay-critical collision".to_string(),
        rig_stage: "not detected".to_string(),
        animation_stage: "not detected".to_string(),
        ..Default::default()
    };
    if metadata.len() == 0 {
        report.errors.push("The model file is empty".to_string());
    }
    match format.as_str() {
        "glb" | "gltf" => validate_gltf(path, &mut report)?,
        "fbx" => validate_fbx(path, &mut report)?,
        "usd" | "usda" | "usdc" => validate_usd(path, &mut report)?,
        _ => {}
    }
    apply_texture_set_validation(path, &mut report);
    apply_provenance_validation(path, &mut report);
    if report.mesh_count == 0 {
        report
            .errors
            .push("No mesh geometry was detected".to_string());
    }
    if report.mesh_count > 0 && !report.has_uv0 {
        report
            .warnings
            .push("UV0 was not detected; PBR textures may not map correctly".to_string());
    }
    if report.mesh_count > 0 && !report.has_normals {
        report
            .warnings
            .push("Normals were not detected; Unreal must recompute them".to_string());
    }
    if report.triangle_count > 250_000 {
        report.nanite_recommended = true;
        report
            .warnings
            .push("High triangle count: enable Nanite or generate LODs".to_string());
    }
    if report.lod_count == 0 && report.triangle_count > 20_000 {
        report
            .warnings
            .push("No explicit LOD chain was detected".to_string());
    }
    report.geometry_valid = report.errors.is_empty() && report.mesh_count > 0;
    report.import_ready =
        report.geometry_valid && report.provenance_present && report.license_confirmed;
    if report.geometry_valid && !report.provenance_present {
        report
            .warnings
            .push("Provenance sidecar is missing".to_string());
    }
    if report.geometry_valid && !report.license_confirmed {
        report
            .warnings
            .push("Asset license/provider terms are not confirmed".to_string());
    }
    Ok(report)
}

fn validate_gltf(path: &Path, report: &mut ThreeDValidationReport) -> Result<()> {
    let value = read_gltf_json(path)?;
    let accessors = value
        .get("accessors")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let meshes = value
        .get("meshes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    report.mesh_count = meshes.len();
    report.material_count = array_len(&value, "materials");
    report.texture_count = array_len(&value, "textures").max(array_len(&value, "images"));
    report.skin_count = array_len(&value, "skins");
    report.animation_count = array_len(&value, "animations");
    report.rig_stage = if report.skin_count > 0 {
        "skin detected; skeleton import required".to_string()
    } else {
        "static mesh".to_string()
    };
    report.animation_stage = if report.animation_count > 0 {
        "animation clips detected; import as a separate stage".to_string()
    } else {
        "no animation clips".to_string()
    };
    let mut bounds_min = [f64::INFINITY; 3];
    let mut bounds_max = [f64::NEG_INFINITY; 3];
    for mesh in &meshes {
        if mesh
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|name| name.to_ascii_lowercase().contains("lod"))
        {
            report.lod_count += 1;
        }
        for primitive in mesh
            .get("primitives")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            report.primitive_count += 1;
            let attributes = primitive.get("attributes").and_then(Value::as_object);
            report.has_uv0 |= attributes.is_some_and(|value| value.contains_key("TEXCOORD_0"));
            report.has_normals |= attributes.is_some_and(|value| value.contains_key("NORMAL"));
            report.has_tangents |= attributes.is_some_and(|value| value.contains_key("TANGENT"));
            let position_accessor = attributes
                .and_then(|value| value.get("POSITION"))
                .and_then(Value::as_u64)
                .and_then(|index| accessors.get(index as usize));
            let vertices = position_accessor
                .and_then(|value| value.get("count"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            report.vertex_count += vertices;
            if let Some(accessor) = position_accessor {
                merge_accessor_bounds(accessor, &mut bounds_min, &mut bounds_max);
            }
            let indices = primitive
                .get("indices")
                .and_then(Value::as_u64)
                .and_then(|index| accessors.get(index as usize))
                .and_then(|value| value.get("count"))
                .and_then(Value::as_u64)
                .unwrap_or(vertices);
            let mode = primitive.get("mode").and_then(Value::as_u64).unwrap_or(4);
            if mode == 4 {
                report.triangle_count += indices / 3;
            }
        }
    }
    report.lod_count += value
        .pointer("/extensions/MSFT_lod/ids")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    report.has_pbr_material = value
        .get("materials")
        .and_then(Value::as_array)
        .is_some_and(|materials| {
            materials.iter().any(|material| {
                material.get("pbrMetallicRoughness").is_some()
                    || material.get("normalTexture").is_some()
            })
        });
    if bounds_min[0].is_finite() && bounds_max[0].is_finite() {
        let bounds = [
            bounds_max[0] - bounds_min[0],
            bounds_max[1] - bounds_min[1],
            bounds_max[2] - bounds_min[2],
        ];
        report.bounds_meters = Some(bounds);
        let max_extent = bounds.into_iter().fold(0.0_f64, f64::max);
        report.scale_note = if max_extent < 0.01 {
            "Very small glTF bounds; verify meter-to-centimeter conversion".to_string()
        } else if max_extent > 1_000.0 {
            "Very large glTF bounds; verify source units before import".to_string()
        } else {
            "glTF uses meters; Unreal import should convert to centimeters".to_string()
        };
    }
    Ok(())
}

fn validate_fbx(path: &Path, report: &mut ThreeDValidationReport) -> Result<()> {
    let bytes = fs::read(path)?;
    let binary = bytes.starts_with(b"Kaydara FBX Binary");
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(4_000_000)]);
    if !binary && !text.contains("FBXHeaderExtension") {
        report
            .errors
            .push("FBX header is invalid or unsupported".to_string());
        return Ok(());
    }
    report.mesh_count = count_occurrences(&text, "Model::").max(1);
    report.material_count = count_occurrences(&text, "Material::");
    report.texture_count = count_occurrences(&text, "Texture::");
    report.skin_count = count_occurrences(&text, "Deformer::Skin");
    report.animation_count = count_occurrences(&text, "AnimationStack::");
    report.has_uv0 = text.contains("LayerElementUV") || binary;
    report.has_normals = text.contains("LayerElementNormal") || binary;
    report.rig_stage = if report.skin_count > 0 {
        "FBX skin detected; import skeleton before animation clips".to_string()
    } else {
        "static FBX mesh".to_string()
    };
    report.animation_stage = if report.animation_count > 0 {
        "FBX animation stack detected".to_string()
    } else {
        "no FBX animation stack detected".to_string()
    };
    report.warnings.push(
        "FBX validation is structural; Unreal Interchange performs the authoritative parse"
            .to_string(),
    );
    Ok(())
}

fn validate_usd(path: &Path, report: &mut ThreeDValidationReport) -> Result<()> {
    let bytes = fs::read(path)?;
    let binary = bytes.starts_with(b"PXR-USDC");
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(4_000_000)]);
    if !binary && !text.trim_start().starts_with("#usda") {
        report
            .errors
            .push("USD header is invalid or unsupported".to_string());
        return Ok(());
    }
    report.mesh_count = count_occurrences(&text, "def Mesh").max(usize::from(binary));
    report.material_count = count_occurrences(&text, "def Material");
    report.animation_count = count_occurrences(&text, "timeSamples");
    report.has_uv0 = text.contains("primvars:st") || binary;
    report.has_normals = text.contains("normals") || binary;
    report.scale_note = "Read metersPerUnit/upAxis during Unreal USD import".to_string();
    report.warnings.push(
        "USD validation is structural; the Unreal USD/Interchange importer is authoritative"
            .to_string(),
    );
    Ok(())
}

fn read_gltf_json(path: &Path) -> Result<Value> {
    if path.extension().and_then(|value| value.to_str()) == Some("gltf") {
        return Ok(serde_json::from_slice(&fs::read(path)?)?);
    }
    let bytes = fs::read(path)?;
    if bytes.len() < 20 || &bytes[0..4] != b"glTF" {
        anyhow::bail!("Invalid GLB header");
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().expect("slice length"));
    if version != 2 {
        anyhow::bail!("Unsupported GLB version: {version}");
    }
    let declared = u32::from_le_bytes(bytes[8..12].try_into().expect("slice length")) as usize;
    if declared > bytes.len() {
        anyhow::bail!("Truncated GLB file");
    }
    let mut cursor = 12;
    while cursor + 8 <= declared {
        let length = u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().expect("slice length"))
            as usize;
        let kind = u32::from_le_bytes(
            bytes[cursor + 4..cursor + 8]
                .try_into()
                .expect("slice length"),
        );
        cursor += 8;
        if cursor + length > declared {
            anyhow::bail!("Invalid GLB chunk length");
        }
        if kind == 0x4E4F534A {
            let json_bytes = &bytes[cursor..cursor + length];
            return Ok(serde_json::from_slice(
                json_bytes.strip_suffix(&[0]).unwrap_or(json_bytes),
            )?);
        }
        cursor += length;
    }
    anyhow::bail!("GLB JSON chunk is missing")
}

pub fn build_unreal_import_command(
    workspace: &Workspace,
    args: UnrealImportThreeDArgs,
) -> Result<UnrealImportCommand> {
    let source = workspace.resolve_existing(&args.source_path)?;
    let mut validation =
        validate_3d_asset_file(&source, &normalize_relative_path(&args.source_path))?;
    if args.license_confirmed.unwrap_or(false) {
        validation.license_confirmed = true;
        validation.import_ready = validation.geometry_valid && validation.provenance_present;
    }
    if !validation.geometry_valid {
        anyhow::bail!("3D validation failed: {}", validation.errors.join("; "));
    }
    if !validation.import_ready && !args.allow_validation_warnings.unwrap_or(false) {
        anyhow::bail!(
            "3D asset is not import-ready: confirm license/provenance or set allow_validation_warnings"
        );
    }
    let snapshot = unreal_snapshot(workspace);
    let project = snapshot
        .project
        .as_ref()
        .context("No .uproject found in the workspace")?;
    let editor_cmd = snapshot
        .selected_engine
        .as_ref()
        .and_then(|engine| engine.tools.editor_cmd.as_deref())
        .context("UnrealEditor-Cmd was not found")?;
    let script = workspace.resolve_existing("scripts/unreal/import_3d_asset.py")?;
    let destination_path = normalize_unreal_content_path(
        args.destination_path
            .as_deref()
            .unwrap_or("/Game/Generated/Leetcode"),
    )?;
    let asset_type = normalize_asset_type(args.asset_type.as_deref().unwrap_or("static_mesh"))?;
    let collision = normalize_collision(args.collision.as_deref().unwrap_or("auto"))?;
    let id = uuid::Uuid::new_v4().to_string();
    let manifest_relative = format!("{THREE_D_IMPORT_DIR}/{id}.json");
    let result_relative = format!("{THREE_D_IMPORT_DIR}/{id}.result.json");
    let manifest_path = workspace.resolve_for_write(&manifest_relative)?;
    let result_path = workspace.resolve_for_write(&result_relative)?;
    let manifest = UnrealImportManifest {
        schema_version: 1,
        id,
        source_file: path_string(&source),
        source_workspace_path: normalize_relative_path(&args.source_path),
        destination_path,
        asset_type,
        skeleton_path: args.skeleton_path.filter(|value| !value.trim().is_empty()),
        replace_existing: args.replace_existing.unwrap_or(true),
        import_lods: args.import_lods.unwrap_or(true),
        enable_nanite: args.enable_nanite.unwrap_or(validation.nanite_recommended),
        collision,
        validation,
        result_path: path_string(&result_path),
        created_at: unix_timestamp(),
    };
    workspace.write_text(
        &manifest_relative,
        &serde_json::to_string_pretty(&manifest)?,
    )?;
    let shell_command = format!(
        "$env:LEETCODE_3D_IMPORT_MANIFEST={}; & {} {} -Unattended -NoSplash -NoP4 -ExecutePythonScript={} -log -UTF8Output",
        powershell_quote(&path_string(&manifest_path)),
        powershell_quote(editor_cmd),
        powershell_quote(&project.path),
        powershell_quote(&path_string(&script)),
    );
    Ok(UnrealImportCommand {
        manifest_path: manifest_relative,
        result_path: result_relative,
        shell_command,
        timeout_secs: 1_800,
    })
}

fn apply_texture_set_validation(path: &Path, report: &mut ThreeDValidationReport) {
    let Some(parent) = path.parent() else { return };
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    let mut pbr_kinds = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        for kind in [
            "basecolor",
            "albedo",
            "normal",
            "roughness",
            "metallic",
            "ao",
            "occlusion",
        ] {
            if name.contains(kind) && !pbr_kinds.contains(&kind) {
                pbr_kinds.push(kind);
            }
        }
    }
    if !pbr_kinds.is_empty() {
        report.has_pbr_material = true;
        report.texture_count = report.texture_count.max(pbr_kinds.len());
    }
}

fn apply_provenance_validation(path: &Path, report: &mut ThreeDValidationReport) {
    let sidecar = sidecar_path(path);
    let value = fs::read(&sidecar)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    report.provenance_present = value.is_some();
    report.license_confirmed = value
        .as_ref()
        .and_then(|item| item.get("license_confirmed"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
}

fn workspace_image_data_uri(
    workspace: &Workspace,
    relative: &str,
    max_bytes: u64,
) -> Result<String> {
    let path = workspace.resolve_existing(relative)?;
    let metadata = fs::metadata(&path)?;
    if metadata.len() > max_bytes {
        anyhow::bail!("Reference image exceeds {} MB", max_bytes / 1024 / 1024);
    }
    let mime = image_mime(&path)?;
    Ok(format!(
        "data:{mime};base64,{}",
        general_purpose::STANDARD.encode(fs::read(path)?)
    ))
}

fn image_mime(path: &Path) -> Result<&'static str> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Ok("image/png"),
        "jpg" | "jpeg" => Ok("image/jpeg"),
        "webp" => Ok("image/webp"),
        value => anyhow::bail!("Unsupported reference image format: {value}"),
    }
}

async fn response_json(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let text = response.text().await?;
    let value = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({"message": text}));
    if !status.is_success() {
        anyhow::bail!(
            "3D provider HTTP {}: {}",
            status,
            provider_error_message(&value).unwrap_or("unknown error")
        );
    }
    Ok(value)
}

fn provider_error_message(value: &Value) -> Option<&str> {
    value
        .get("message")
        .or_else(|| value.get("error"))
        .or_else(|| value.pointer("/data/error"))
        .or_else(|| value.pointer("/data/message"))
        .and_then(|item| {
            item.as_str()
                .or_else(|| item.get("message").and_then(Value::as_str))
        })
}

fn collect_model_urls(value: &Value) -> Vec<(String, String)> {
    fn visit(value: &Value, result: &mut Vec<(String, String)>) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    if let Some(url) = value.as_str() {
                        if url.starts_with("http://") || url.starts_with("https://") {
                            let extension = supported_extension(url)
                                .unwrap_or_else(|| key.to_ascii_lowercase());
                            if matches!(
                                extension.as_str(),
                                "glb" | "gltf" | "fbx" | "usd" | "usdz" | "obj"
                            ) {
                                result.push((extension, url.to_string()));
                            }
                        }
                    } else {
                        visit(value, result);
                    }
                }
            }
            Value::Array(values) => values.iter().for_each(|value| visit(value, result)),
            _ => {}
        }
    }
    let mut result = Vec::new();
    visit(value, &mut result);
    result.sort();
    result.dedup();
    result
}

fn select_model_url(urls: &[(String, String)], preferred: &str) -> Option<(String, String)> {
    urls.iter()
        .find(|(extension, _)| extension == preferred)
        .or_else(|| urls.iter().find(|(extension, _)| extension == "glb"))
        .or_else(|| urls.first())
        .cloned()
}

fn supported_extension(value: &str) -> Option<String> {
    let without_query = value.split(['?', '#']).next().unwrap_or(value);
    Path::new(without_query)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| {
            matches!(
                value.as_str(),
                "glb" | "gltf" | "fbx" | "usd" | "usda" | "usdc" | "usdz" | "obj"
            )
        })
}

fn merge_accessor_bounds(accessor: &Value, min: &mut [f64; 3], max: &mut [f64; 3]) {
    let accessor_min = accessor.get("min").and_then(Value::as_array);
    let accessor_max = accessor.get("max").and_then(Value::as_array);
    for index in 0..3 {
        if let Some(value) = accessor_min
            .and_then(|values| values.get(index))
            .and_then(Value::as_f64)
        {
            min[index] = min[index].min(value);
        }
        if let Some(value) = accessor_max
            .and_then(|values| values.get(index))
            .and_then(Value::as_f64)
        {
            max[index] = max[index].max(value);
        }
    }
}

fn array_len(value: &Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0)
}

fn count_occurrences(value: &str, pattern: &str) -> usize {
    value.match_indices(pattern).count()
}

fn sidecar_path(path: &Path) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(".asset.json");
    PathBuf::from(value)
}

fn normalize_format(value: &str) -> Result<String> {
    let value = value.trim().trim_start_matches('.').to_ascii_lowercase();
    if matches!(value.as_str(), "glb" | "gltf" | "fbx" | "usd") {
        Ok(value)
    } else {
        anyhow::bail!("Unsupported target 3D format: {value}")
    }
}

fn normalize_pose_mode(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if matches!(value.as_str(), "" | "a-pose" | "t-pose") {
        Ok(value)
    } else {
        anyhow::bail!("pose_mode must be empty, a-pose, or t-pose")
    }
}

fn normalize_asset_type(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase().replace('-', "_");
    if matches!(
        value.as_str(),
        "static_mesh" | "skeletal_mesh" | "animation"
    ) {
        Ok(value)
    } else {
        anyhow::bail!("asset_type must be static_mesh, skeletal_mesh, or animation")
    }
}

fn normalize_collision(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if matches!(value.as_str(), "auto" | "simple" | "complex" | "none") {
        Ok(value)
    } else {
        anyhow::bail!("collision must be auto, simple, complex, or none")
    }
}

fn normalize_unreal_content_path(value: &str) -> Result<String> {
    let value = value.trim().replace('\\', "/");
    if !value.starts_with("/Game/")
        || value.contains("..")
        || value.contains(['\n', '\r', '\"', '\''])
    {
        anyhow::bail!("destination_path must be a safe /Game/... path")
    }
    Ok(value.trim_end_matches('/').to_string())
}

fn normalize_relative_path(value: &str) -> String {
    value.trim().replace('\\', "/")
}

fn safe_file_stem(value: &str) -> String {
    value
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() || matches!(ch, '-' | '_') {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("-")
}

fn compact_id(value: &str) -> &str {
    value.rsplit('-').next().unwrap_or(value)
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempdir().expect("temp dir");
        let workspace = Workspace::new(dir.path().to_path_buf()).expect("workspace");
        (dir, workspace)
    }

    #[test]
    fn validates_gltf_geometry_pbr_rig_and_animation() {
        let (_dir, workspace) = workspace();
        workspace
            .write_text(
                "hero.gltf",
                &serde_json::to_string(&json!({
                    "asset": {"version": "2.0"},
                    "accessors": [
                        {"count": 300, "min": [-0.5, -0.5, 0.0], "max": [0.5, 0.5, 1.8]},
                        {"count": 900}
                    ],
                    "meshes": [{"name": "Hero_LOD0", "primitives": [{
                        "attributes": {"POSITION": 0, "NORMAL": 0, "TANGENT": 0, "TEXCOORD_0": 0},
                        "indices": 1,
                        "material": 0
                    }]}],
                    "materials": [{"pbrMetallicRoughness": {}}],
                    "textures": [{}],
                    "skins": [{}],
                    "animations": [{}]
                }))
                .expect("json"),
            )
            .expect("write gltf");
        workspace
            .write_text(
                "hero.gltf.asset.json",
                r#"{"provider":"fixture","license_confirmed":true}"#,
            )
            .expect("write provenance");

        let report = validate_3d_asset_path(&workspace, "hero.gltf").expect("report");
        assert!(report.geometry_valid);
        assert!(report.import_ready);
        assert_eq!(report.mesh_count, 1);
        assert_eq!(report.triangle_count, 300);
        assert!(report.has_uv0);
        assert!(report.has_pbr_material);
        assert_eq!(report.skin_count, 1);
        assert_eq!(report.animation_count, 1);
        assert_eq!(report.bounds_meters, Some([1.0, 1.0, 1.8]));
    }

    #[test]
    fn model_url_selection_prefers_requested_then_glb() {
        let value = json!({
            "model_urls": {
                "fbx": "https://example.test/model.fbx?token=1",
                "glb": "https://example.test/model.glb?token=1"
            }
        });
        let urls = collect_model_urls(&value);
        assert_eq!(select_model_url(&urls, "fbx").unwrap().0, "fbx");
        assert_eq!(select_model_url(&urls, "usd").unwrap().0, "glb");
    }

    #[test]
    fn missing_provenance_blocks_import_ready_not_geometry() {
        let (_dir, workspace) = workspace();
        workspace
            .write_text(
                "prop.gltf",
                r#"{"asset":{"version":"2.0"},"accessors":[{"count":3}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}]}"#,
            )
            .expect("write gltf");
        let report = validate_3d_asset_path(&workspace, "prop.gltf").expect("report");
        assert!(report.geometry_valid);
        assert!(!report.import_ready);
        assert!(!report.provenance_present);
    }

    #[test]
    fn provider_aliases_and_safe_paths_are_normalized() {
        assert_eq!(normalize_provider("Tripo"), TRIPO_3D_PROVIDER_ID);
        assert_eq!(normalize_format(".GLB").unwrap(), "glb");
        assert!(normalize_unreal_content_path("/Game/Generated/Hero").is_ok());
        assert!(normalize_unreal_content_path("../Content").is_err());
    }
}
