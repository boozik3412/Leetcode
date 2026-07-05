#![recursion_limit = "256"]

mod agent;
mod agent_history;
mod app;
mod asset_library;
mod assets;
mod config;
mod conversation;
mod crash;
mod diagnostics;
mod evals;
mod game_workflows;
mod governance;
mod http;
mod memory;
mod orchestration;
mod project;
mod provider_health;
mod relay;
mod remote;
mod roadmap;
mod run_timeline;
mod self_modification;
mod terminal;
mod tools;
mod updater;
mod workspace;

use app::LeetcodeApp;
use std::sync::Arc;

const APP_ICON_PNG: &[u8] = include_bytes!("../assets/app-icon.png");

fn main() -> eframe::Result<()> {
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
