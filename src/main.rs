#![recursion_limit = "256"]

mod agent;
mod agent_history;
mod app;
mod asset_3d;
mod asset_library;
mod assets;
mod config;
mod conversation;
mod crash;
mod diagnostics;
mod evals;
mod game_production;
mod game_task_builder;
mod game_workflows;
mod governance;
mod http;
mod mcp;
mod memory;
mod orchestration;
mod production_validation;
mod project;
mod project_graph;
mod project_semantics;
mod provider_health;
mod relay;
mod remote;
mod remote_timeline;
mod roadmap;
mod run_timeline;
mod self_improvement;
mod self_modification;
mod terminal;
mod tools;
mod unreal;
mod unreal_gameplay;
mod unreal_intelligence;
mod updater;
mod vertical_slice;
mod visual_regression;
mod workspace;

use app::LeetcodeApp;
use std::sync::Arc;

const APP_ICON_PNG: &[u8] = include_bytes!("../assets/app-icon.png");

fn main() -> eframe::Result<()> {
    if std::env::args().any(|argument| argument == "--production-smoke") {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "app": "leetcode",
                "version": env!("CARGO_PKG_VERSION"),
                "icon_bytes": APP_ICON_PNG.len(),
                "mode": "production_smoke"
            })
        );
        return Ok(());
    }
    crash::install_panic_hook();

    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([900.0, 620.0]);
    let viewport = if let Some(icon) = load_app_icon() {
        viewport.with_icon(icon)
    } else {
        viewport
    };

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Leetcode",
        options,
        Box::new(|cc| Ok(Box::new(LeetcodeApp::new(cc)))),
    )
}

fn load_app_icon() -> Option<Arc<egui::IconData>> {
    let image = image::load_from_memory(APP_ICON_PNG).ok()?.into_rgba8();
    let (width, height) = image.dimensions();

    Some(Arc::new(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }))
}
