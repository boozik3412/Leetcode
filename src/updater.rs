use crate::config::AppConfig;
use crate::http::build_http_client;
use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub enum UpdateEvent {
    Progress(String),
    Available {
        current_version: String,
        latest_version: String,
    },
    DeferredByRollout {
        current_version: String,
        latest_version: String,
        rollout_percent: u8,
    },
    AlreadyCurrent {
        current_version: String,
        latest_version: String,
    },
    CheckFailed(String),
    Restarting {
        latest_version: String,
        install_dir: String,
    },
    Error(String),
}

#[derive(Clone, Debug)]
pub enum UpdateCheck {
    Available {
        current_version: String,
        latest_version: String,
    },
    DeferredByRollout {
        current_version: String,
        latest_version: String,
        rollout_percent: u8,
    },
    AlreadyCurrent {
        current_version: String,
        latest_version: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateManifest {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub package: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub installer: Option<String>,
    #[serde(default)]
    pub uninstaller: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub signature_algorithm: Option<String>,
    #[serde(default)]
    pub rollback_version: Option<String>,
    #[serde(default)]
    pub rollback_package: Option<String>,
    #[serde(default)]
    pub rollback_sha256: Option<String>,
    #[serde(default)]
    pub rollout_percent: Option<u8>,
    #[serde(default)]
    pub rollout_seed: Option<String>,
    #[serde(default)]
    pub minimum_supported_version: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Clone, Debug)]
enum UpdateLocation {
    Http(reqwest::Url),
    File(PathBuf),
}

pub async fn check_for_update(
    config: AppConfig,
    manifest_location: String,
    current_version: String,
) -> anyhow::Result<UpdateCheck> {
    let client = build_http_client(&config)?;
    let manifest_location = parse_update_location(&manifest_location)?;
    let manifest_bytes = read_location(&client, &manifest_location).await?;
    let manifest: UpdateManifest =
        serde_json::from_slice(&manifest_bytes).context("latest.json не является валидным JSON")?;
    validate_manifest(&manifest)?;

    if version_is_newer(&manifest.version, &current_version) {
        if !manifest_rollout_allows(&manifest, &rollout_key_from_config(&config)) {
            return Ok(UpdateCheck::DeferredByRollout {
                current_version,
                latest_version: manifest.version,
                rollout_percent: manifest.rollout_percent.unwrap_or(100).min(100),
            });
        }
        Ok(UpdateCheck::Available {
            current_version,
            latest_version: manifest.version,
        })
    } else {
        Ok(UpdateCheck::AlreadyCurrent {
            current_version,
            latest_version: manifest.version,
        })
    }
}

pub async fn update_and_restart(
    config: AppConfig,
    manifest_location: String,
    current_version: String,
    current_exe: PathBuf,
    current_pid: u32,
    events: Sender<UpdateEvent>,
) -> anyhow::Result<()> {
    send_progress(&events, "читаю manifest обновления");
    let client = build_http_client(&config)?;
    let manifest_location = parse_update_location(&manifest_location)?;
    let manifest_bytes = read_location(&client, &manifest_location).await?;
    let manifest: UpdateManifest =
        serde_json::from_slice(&manifest_bytes).context("latest.json не является валидным JSON")?;
    validate_manifest(&manifest)?;

    if !version_is_newer(&manifest.version, &current_version) {
        let _ = events.send(UpdateEvent::AlreadyCurrent {
            current_version,
            latest_version: manifest.version,
        });
        return Ok(());
    }
    if !manifest_rollout_allows(&manifest, &rollout_key_from_config(&config)) {
        let percent = manifest.rollout_percent.unwrap_or(100).min(100);
        bail!(
            "обновление {} найдено, но эта установка пока не входит в staged rollout {}%",
            manifest.version,
            percent
        );
    }

    let install_dir = validate_install_dir(&current_exe)?;
    let exe_name = current_exe
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("leetcode.exe")
        .to_string();
    let package_location = resolve_package_location(&manifest_location, &manifest.package)?;

    send_progress(
        &events,
        format!("найдена версия {}, скачиваю пакет", manifest.version.trim()),
    );
    let package_bytes = read_location(&client, &package_location).await?;
    if let Some(expected_size) = manifest.size_bytes {
        let actual_size = package_bytes.len() as u64;
        if expected_size != actual_size {
            bail!("размер пакета не совпал: ожидалось {expected_size}, получено {actual_size}");
        }
    }

    send_progress(&events, "проверяю SHA256");
    let actual_hash = sha256_hex(&package_bytes);
    let expected_hash = normalize_sha256(&manifest.sha256)?;
    if actual_hash != expected_hash {
        bail!("SHA256 не совпал: ожидалось {expected_hash}, получено {actual_hash}");
    }

    let staging_dir = update_staging_dir()?;
    fs::create_dir_all(&staging_dir)?;
    let package_path = staging_dir.join("leetcode-portable.zip");
    let script_path = staging_dir.join("apply-leetcode-update.ps1");
    let log_path = update_log_path()?;
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&package_path, package_bytes)?;
    fs::write(&script_path, UPDATE_SCRIPT)?;

    send_progress(&events, "запускаю updater и перезапускаю приложение");
    launch_update_script(
        &script_path,
        current_pid,
        &package_path,
        &install_dir,
        &exe_name,
        &log_path,
    )?;

    let _ = events.send(UpdateEvent::Restarting {
        latest_version: manifest.version,
        install_dir: install_dir.display().to_string(),
    });
    Ok(())
}

pub fn version_is_newer(latest: &str, current: &str) -> bool {
    let latest_parts = parse_version_parts(latest);
    let current_parts = parse_version_parts(current);
    let width = latest_parts.len().max(current_parts.len()).max(1);
    for index in 0..width {
        let latest_part = latest_parts.get(index).copied().unwrap_or(0);
        let current_part = current_parts.get(index).copied().unwrap_or(0);
        if latest_part > current_part {
            return true;
        }
        if latest_part < current_part {
            return false;
        }
    }
    false
}

fn parse_version_parts(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches('v')
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .take(4)
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

fn validate_manifest(manifest: &UpdateManifest) -> anyhow::Result<()> {
    if manifest.app.trim() != "Leetcode" {
        bail!("manifest относится не к Leetcode: {}", manifest.app);
    }
    if manifest.platform.trim() != "windows-x64" {
        bail!(
            "неподдерживаемая платформа обновления: {}",
            manifest.platform
        );
    }
    if manifest.version.trim().is_empty() {
        bail!("manifest не содержит version");
    }
    if manifest.package.trim().is_empty() {
        bail!("manifest не содержит package");
    }
    if manifest.rollout_percent.unwrap_or(100) > 100 {
        bail!("manifest contains invalid rollout_percent");
    }
    if let Some(hash) = &manifest.rollback_sha256 {
        if !hash.trim().is_empty() {
            normalize_sha256(hash)?;
        }
    }
    normalize_sha256(&manifest.sha256)?;
    Ok(())
}

fn rollout_key_from_config(config: &AppConfig) -> String {
    if !config.agent_id.trim().is_empty() {
        return config.agent_id.trim().to_string();
    }
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "anonymous-install".to_string())
}

pub fn manifest_rollout_allows(manifest: &UpdateManifest, install_key: &str) -> bool {
    let percent = manifest.rollout_percent.unwrap_or(100).min(100);
    if percent >= 100 {
        return true;
    }
    if percent == 0 {
        return false;
    }
    rollout_bucket(manifest, install_key) < percent
}

fn rollout_bucket(manifest: &UpdateManifest, install_key: &str) -> u8 {
    let seed = manifest
        .rollout_seed
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&manifest.version);
    let mut hasher = Sha256::new();
    hasher.update(seed.as_bytes());
    hasher.update(b":");
    hasher.update(install_key.trim().as_bytes());
    let digest = hasher.finalize();
    let value = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    (value % 100) as u8
}

fn normalize_sha256(value: &str) -> anyhow::Result<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("manifest содержит некорректный sha256");
    }
    Ok(normalized)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn parse_update_location(value: &str) -> anyhow::Result<UpdateLocation> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("URL manifest обновления пуст");
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return Ok(UpdateLocation::Http(reqwest::Url::parse(trimmed)?));
    }
    if trimmed.starts_with("file://") {
        let url = reqwest::Url::parse(trimmed)?;
        return url
            .to_file_path()
            .map(UpdateLocation::File)
            .map_err(|_| anyhow::anyhow!("не удалось разобрать file:// путь manifest"));
    }
    Ok(UpdateLocation::File(PathBuf::from(trimmed)))
}

async fn read_location(
    client: &reqwest::Client,
    location: &UpdateLocation,
) -> anyhow::Result<Vec<u8>> {
    match location {
        UpdateLocation::Http(url) => {
            let response = client
                .get(url.clone())
                .send()
                .await
                .with_context(|| format!("не удалось скачать {url}"))?;
            let status = response.status();
            if !status.is_success() {
                bail!("сервер вернул {status} для {url}");
            }
            Ok(response.bytes().await?.to_vec())
        }
        UpdateLocation::File(path) => {
            fs::read(path).with_context(|| format!("не удалось прочитать {}", path.display()))
        }
    }
}

fn resolve_package_location(
    manifest_location: &UpdateLocation,
    package: &str,
) -> anyhow::Result<UpdateLocation> {
    if package.starts_with("http://")
        || package.starts_with("https://")
        || package.starts_with("file://")
    {
        return parse_update_location(package);
    }
    let package_path = Path::new(package);
    if package_path.is_absolute() {
        return Ok(UpdateLocation::File(package_path.to_path_buf()));
    }
    match manifest_location {
        UpdateLocation::Http(url) => Ok(UpdateLocation::Http(url.join(package)?)),
        UpdateLocation::File(path) => Ok(UpdateLocation::File(
            path.parent()
                .unwrap_or_else(|| Path::new("."))
                .join(package),
        )),
    }
}

fn validate_install_dir(current_exe: &Path) -> anyhow::Result<PathBuf> {
    let install_dir = current_exe
        .parent()
        .context("не удалось определить папку установленного приложения")?
        .to_path_buf();
    let normalized = install_dir
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    if normalized.contains("\\target\\debug") || normalized.contains("\\target\\release") {
        bail!(
            "автообновление отключено для dev-сборки. Проверьте установленную версию из %LOCALAPPDATA%\\Programs\\Leetcode"
        );
    }
    Ok(install_dir)
}

fn update_staging_dir() -> anyhow::Result<PathBuf> {
    Ok(std::env::temp_dir().join(format!("leetcode-update-{}", unix_timestamp())))
}

fn update_log_path() -> anyhow::Result<PathBuf> {
    let base = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("leetcode")
        .join("updates");
    Ok(base.join("last-update.log"))
}

fn launch_update_script(
    script_path: &Path,
    current_pid: u32,
    package_path: &Path,
    install_dir: &Path,
    exe_name: &str,
    log_path: &Path,
) -> anyhow::Result<()> {
    Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-WindowStyle")
        .arg("Hidden")
        .arg("-File")
        .arg(script_path)
        .arg("-ProcessId")
        .arg(current_pid.to_string())
        .arg("-ZipPath")
        .arg(package_path)
        .arg("-InstallDir")
        .arg(install_dir)
        .arg("-ExeName")
        .arg(exe_name)
        .arg("-LogPath")
        .arg(log_path)
        .spawn()
        .context("не удалось запустить внешний updater")?;
    Ok(())
}

fn send_progress(events: &Sender<UpdateEvent>, message: impl Into<String>) {
    let _ = events.send(UpdateEvent::Progress(message.into()));
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

const UPDATE_SCRIPT: &str = r#"
param(
    [Parameter(Mandatory=$true)][int]$ProcessId,
    [Parameter(Mandatory=$true)][string]$ZipPath,
    [Parameter(Mandatory=$true)][string]$InstallDir,
    [Parameter(Mandatory=$true)][string]$ExeName,
    [Parameter(Mandatory=$true)][string]$LogPath
)

$ErrorActionPreference = "Stop"

function Write-UpdateLog([string]$Message) {
    $parent = Split-Path -Parent $LogPath
    if ($parent -and -not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    $line = "$(Get-Date -Format o) $Message"
    Add-Content -Encoding UTF8 -Path $LogPath -Value $line
}

try {
    Write-UpdateLog "waiting for process $ProcessId"
    $deadline = (Get-Date).AddSeconds(45)
    while (Get-Process -Id $ProcessId -ErrorAction SilentlyContinue) {
        if ((Get-Date) -gt $deadline) {
            Write-UpdateLog "process wait timed out"
            break
        }
        Start-Sleep -Milliseconds 250
    }

    $stagingRoot = Join-Path (Split-Path -Parent $ZipPath) "expanded"
    if (Test-Path -LiteralPath $stagingRoot) {
        Remove-Item -LiteralPath $stagingRoot -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $stagingRoot | Out-Null
    Write-UpdateLog "expanding $ZipPath"
    Expand-Archive -LiteralPath $ZipPath -DestinationPath $stagingRoot -Force

    $sourceExe = Join-Path $stagingRoot $ExeName
    if (-not (Test-Path -LiteralPath $sourceExe)) {
        throw "Package does not contain $ExeName"
    }
    if (-not (Test-Path -LiteralPath $InstallDir)) {
        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    }

    $backupRoot = Join-Path (Split-Path -Parent $LogPath) ("backup-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
    New-Item -ItemType Directory -Force -Path $backupRoot | Out-Null
    if (Test-Path -LiteralPath $InstallDir) {
        Get-ChildItem -LiteralPath $InstallDir -Force | ForEach-Object {
            Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $backupRoot $_.Name) -Recurse -Force
        }
    }

    Write-UpdateLog "copying update into $InstallDir"
    Get-ChildItem -LiteralPath $stagingRoot -Force | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $InstallDir $_.Name) -Recurse -Force
    }

    $targetExe = Join-Path $InstallDir $ExeName
    Write-UpdateLog "starting $targetExe"
    Start-Process -FilePath $targetExe -WorkingDirectory $InstallDir
    Write-UpdateLog "update complete"
} catch {
    Write-UpdateLog "update failed: $($_.Exception.Message)"
    throw
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_semver_like_versions() {
        assert!(version_is_newer("0.1.1", "0.1.0"));
        assert!(version_is_newer("v1.0.0", "0.9.9"));
        assert!(!version_is_newer("0.1.0", "0.1.0"));
        assert!(!version_is_newer("0.1.0", "0.1.1"));
    }

    #[test]
    fn validates_sha256_shape() {
        assert!(normalize_sha256(&"a".repeat(64)).is_ok());
        assert!(normalize_sha256("nope").is_err());
    }

    #[test]
    fn resolves_relative_http_package_url() {
        let manifest = UpdateLocation::Http(
            reqwest::Url::parse("https://example.com/releases/latest/latest.json").unwrap(),
        );

        let resolved = resolve_package_location(&manifest, "leetcode-portable.zip").unwrap();

        match resolved {
            UpdateLocation::Http(url) => {
                assert_eq!(
                    url.as_str(),
                    "https://example.com/releases/latest/leetcode-portable.zip"
                )
            }
            UpdateLocation::File(_) => panic!("expected http location"),
        }
    }

    #[test]
    fn rejects_dev_target_install_dir() {
        let path = PathBuf::from(r"C:\repo\target\debug\leetcode.exe");

        assert!(validate_install_dir(&path).is_err());
    }

    #[test]
    fn check_for_update_reports_available_version() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("latest.json");
        let manifest = UpdateManifest {
            schema_version: 1,
            app: "Leetcode".to_string(),
            version: "0.1.1".to_string(),
            channel: "test".to_string(),
            platform: "windows-x64".to_string(),
            package: "leetcode-portable.zip".to_string(),
            sha256: "a".repeat(64),
            size_bytes: None,
            installer: None,
            uninstaller: None,
            published_at: None,
            signature: None,
            signature_algorithm: None,
            rollback_version: None,
            rollback_package: None,
            rollback_sha256: None,
            rollout_percent: None,
            rollout_seed: None,
            minimum_supported_version: None,
            notes: None,
        };
        fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(check_for_update(
                AppConfig::load(),
                manifest_path.display().to_string(),
                "0.1.0".to_string(),
            ))
            .unwrap();

        match result {
            UpdateCheck::Available {
                current_version,
                latest_version,
            } => {
                assert_eq!(current_version, "0.1.0");
                assert_eq!(latest_version, "0.1.1");
            }
            UpdateCheck::DeferredByRollout { .. } => panic!("expected available update"),
            UpdateCheck::AlreadyCurrent { .. } => panic!("expected available update"),
        }
    }

    #[test]
    fn rollout_percent_gates_updates_deterministically() {
        let mut manifest = UpdateManifest {
            schema_version: 1,
            app: "Leetcode".to_string(),
            version: "0.2.0".to_string(),
            channel: "test".to_string(),
            platform: "windows-x64".to_string(),
            package: "leetcode-portable.zip".to_string(),
            sha256: "a".repeat(64),
            size_bytes: None,
            installer: None,
            uninstaller: None,
            published_at: None,
            signature: None,
            signature_algorithm: None,
            rollback_version: None,
            rollback_package: None,
            rollback_sha256: None,
            rollout_percent: Some(0),
            rollout_seed: Some("seed".to_string()),
            minimum_supported_version: None,
            notes: None,
        };

        assert!(!manifest_rollout_allows(&manifest, "LC-TEST"));
        manifest.rollout_percent = Some(100);
        assert!(manifest_rollout_allows(&manifest, "LC-TEST"));
        manifest.rollout_percent = Some(50);
        assert_eq!(
            manifest_rollout_allows(&manifest, "LC-TEST"),
            manifest_rollout_allows(&manifest, "LC-TEST")
        );
    }
}
