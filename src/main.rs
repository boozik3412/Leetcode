mod agent;
mod app;
mod assets;
mod config;
mod tools;
mod workspace;

use app::LeetcodeApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([900.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Leetcode",
        options,
        Box::new(|cc| Ok(Box::new(LeetcodeApp::new(cc)))),
    )
}
