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

    match generate_image(&api_key, &request).await {
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
                    "license_note": "Generated asset metadata only; check provider terms before production use."
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

    match request.provider.as_str() {
        OPENAI_IMAGE_PROVIDER_ID => generate_openai_image(api_key, request).await,
        GEMINI_IMAGE_PROVIDER_ID => generate_gemini_image(api_key, request).await,
        STABILITY_IMAGE_PROVIDER_ID => generate_stability_image(api_key, request).await,
        REPLICATE_IMAGE_PROVIDER_ID => generate_replicate_image(api_key, request).await,
        _ => anyhow::bail!("Unsupported image provider: {}", request.provider),
    }
}

async fn generate_openai_image(
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

    let client = reqwest::Client::new();
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
        let (bytes, mime_type) = download_generated_file(&client, url).await?;
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

    let response = reqwest::Client::new()
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

    let response = reqwest::Client::new()
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
    api_key: &str,
    request: &ImageAssetRequest,
) -> anyhow::Result<GeneratedImage> {
    let client = reqwest::Client::new();
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

    let mut value = post_replicate_prediction(&client, api_key, &url, &body).await?;
    for _ in 0..REPLICATE_POLL_LIMIT {
        if let Some(output_url) = first_url_in_value(value.get("output").unwrap_or(&Value::Null)) {
            let (bytes, mime_type) = download_generated_file(&client, output_url).await?;
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
        value = get_replicate_prediction(&client, api_key, get_url).await?;
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

pub fn is_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "webp")
    )
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
        _ => "png",
    }
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
