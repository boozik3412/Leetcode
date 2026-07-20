use crate::agent::types::ToolResult;
use crate::workspace::Workspace;
use image::GenericImageView;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const VISUAL_STATE_PATH: &str = "assets/generated/leetcode/visual-regression/state.json";
const VISUAL_BASELINE_ROOT: &str = "assets/generated/leetcode/visual-regression/baselines";
const DEFAULT_MEAN_ERROR_LIMIT: f64 = 0.025;
const DEFAULT_CHANGED_PIXEL_LIMIT: f64 = 0.10;
const PIXEL_CHANGE_THRESHOLD: u8 = 12;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualScenario {
    DesktopMain,
    DesktopContext,
    DesktopRoadmap,
    DesktopRelease,
    RemoteClient,
    RemotePwa,
}

impl VisualScenario {
    pub fn id(self) -> &'static str {
        match self {
            Self::DesktopMain => "desktop_main",
            Self::DesktopContext => "desktop_context",
            Self::DesktopRoadmap => "desktop_roadmap",
            Self::DesktopRelease => "desktop_release",
            Self::RemoteClient => "remote_client",
            Self::RemotePwa => "remote_pwa",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::DesktopMain => "Desktop: основной чат",
            Self::DesktopContext => "Desktop: контекст",
            Self::DesktopRoadmap => "Desktop: roadmap",
            Self::DesktopRelease => "Desktop: релиз",
            Self::RemoteClient => "Тонкий клиент",
            Self::RemotePwa => "Remote PWA",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct VisualSnapshotArgs {
    pub scenario: VisualScenario,
    pub path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VisualBaseline {
    pub scenario: VisualScenario,
    pub label: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
    pub created_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VisualComparison {
    pub scenario: VisualScenario,
    pub baseline_path: String,
    pub actual_path: String,
    pub passed: bool,
    pub dimensions_match: bool,
    pub mean_error: f64,
    pub changed_pixel_ratio: f64,
    pub mean_error_limit: f64,
    pub changed_pixel_limit: f64,
    pub created_at: u64,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VisualRegressionState {
    #[serde(default)]
    pub baselines: Vec<VisualBaseline>,
    #[serde(default)]
    pub comparisons: Vec<VisualComparison>,
}

pub fn load_visual_state(workspace: &Workspace) -> VisualRegressionState {
    workspace
        .read_text(VISUAL_STATE_PATH, 2_000_000)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn visual_regression_snapshot(workspace: &Workspace) -> ToolResult {
    ToolResult::ok(
        serde_json::to_string_pretty(&load_visual_state(workspace))
            .unwrap_or_else(|_| "visual regression state".to_string()),
    )
}

pub fn record_visual_baseline(
    workspace: &Workspace,
    args: &VisualSnapshotArgs,
) -> anyhow::Result<VisualBaseline> {
    let source = workspace.resolve_existing(args.path.trim())?;
    let image = image::open(&source)?;
    let (width, height) = image.dimensions();
    let relative = format!("{VISUAL_BASELINE_ROOT}/{}.png", args.scenario.id());
    let destination = workspace.resolve_for_write(&relative)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    image.save(&destination)?;
    let baseline = VisualBaseline {
        scenario: args.scenario,
        label: args.scenario.label().to_string(),
        path: relative,
        width,
        height,
        sha256: file_sha256(&destination)?,
        created_at: unix_timestamp(),
    };
    let mut state = load_visual_state(workspace);
    state
        .baselines
        .retain(|item| item.scenario != args.scenario);
    state.baselines.push(baseline.clone());
    state.baselines.sort_by_key(|item| item.scenario.id());
    save_visual_state(workspace, &state)?;
    Ok(baseline)
}

pub fn compare_visual_snapshot(
    workspace: &Workspace,
    args: &VisualSnapshotArgs,
) -> anyhow::Result<VisualComparison> {
    let mut state = load_visual_state(workspace);
    let baseline = state
        .baselines
        .iter()
        .find(|item| item.scenario == args.scenario)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("эталон для {} не сохранён", args.scenario.label()))?;
    let baseline_path = workspace.resolve_existing(&baseline.path)?;
    let actual_path = workspace.resolve_existing(args.path.trim())?;
    let expected = image::open(&baseline_path)?.to_rgba8();
    let actual = image::open(&actual_path)?.to_rgba8();
    let dimensions_match = expected.dimensions() == actual.dimensions();
    let (mean_error, changed_pixel_ratio) = if dimensions_match {
        compare_rgba(&expected, &actual)
    } else {
        (1.0, 1.0)
    };
    let passed = dimensions_match
        && mean_error <= DEFAULT_MEAN_ERROR_LIMIT
        && changed_pixel_ratio <= DEFAULT_CHANGED_PIXEL_LIMIT;
    let comparison = VisualComparison {
        scenario: args.scenario,
        baseline_path: baseline.path,
        actual_path: args.path.trim().to_string(),
        passed,
        dimensions_match,
        mean_error,
        changed_pixel_ratio,
        mean_error_limit: DEFAULT_MEAN_ERROR_LIMIT,
        changed_pixel_limit: DEFAULT_CHANGED_PIXEL_LIMIT,
        created_at: unix_timestamp(),
        detail: if !dimensions_match {
            format!(
                "размеры отличаются: эталон {}x{}, снимок {}x{}",
                expected.width(),
                expected.height(),
                actual.width(),
                actual.height()
            )
        } else if passed {
            "визуальный снимок укладывается в допуски".to_string()
        } else {
            format!(
                "визуальное отличие выше допуска: mean {:.3}, pixels {:.1}%",
                mean_error,
                changed_pixel_ratio * 100.0
            )
        },
    };
    state.comparisons.push(comparison.clone());
    if state.comparisons.len() > 200 {
        let overflow = state.comparisons.len() - 200;
        state.comparisons.drain(0..overflow);
    }
    save_visual_state(workspace, &state)?;
    Ok(comparison)
}

fn save_visual_state(workspace: &Workspace, state: &VisualRegressionState) -> anyhow::Result<()> {
    workspace.write_text(VISUAL_STATE_PATH, &serde_json::to_string_pretty(state)?)
}

fn compare_rgba(expected: &image::RgbaImage, actual: &image::RgbaImage) -> (f64, f64) {
    let mut absolute_error = 0_u64;
    let mut changed = 0_u64;
    let pixels = expected.width() as u64 * expected.height() as u64;
    for (left, right) in expected.pixels().zip(actual.pixels()) {
        let mut pixel_error = 0_u16;
        for channel in 0..4 {
            let delta = left[channel].abs_diff(right[channel]);
            absolute_error += u64::from(delta);
            pixel_error += u16::from(delta);
        }
        if pixel_error / 4 > u16::from(PIXEL_CHANGE_THRESHOLD) {
            changed += 1;
        }
    }
    let mean_error = absolute_error as f64 / (pixels.max(1) as f64 * 4.0 * 255.0);
    let changed_ratio = changed as f64 / pixels.max(1) as f64;
    (mean_error, changed_ratio)
}

fn file_sha256(path: &Path) -> anyhow::Result<String> {
    let bytes = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
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
    fn records_and_compares_named_visual_baseline() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = Workspace::new(temp.path().to_path_buf()).expect("workspace");
        let source = workspace
            .resolve_for_write("screens/current.png")
            .expect("source path");
        fs::create_dir_all(source.parent().expect("parent")).expect("screen dir");
        image::RgbaImage::from_pixel(4, 4, image::Rgba([20, 30, 40, 255]))
            .save(&source)
            .expect("source image");
        let args = VisualSnapshotArgs {
            scenario: VisualScenario::DesktopMain,
            path: "screens/current.png".to_string(),
        };
        let baseline = record_visual_baseline(&workspace, &args).expect("baseline");
        assert_eq!(baseline.width, 4);
        let comparison = compare_visual_snapshot(&workspace, &args).expect("comparison");
        assert!(comparison.passed);

        image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 255, 255, 255]))
            .save(&source)
            .expect("changed image");
        let changed = compare_visual_snapshot(&workspace, &args).expect("changed comparison");
        assert!(!changed.passed);
        assert!(changed.changed_pixel_ratio > 0.9);
    }
}
