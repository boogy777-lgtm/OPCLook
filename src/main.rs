#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use opcda_egui::app;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_maximized(true)
            .with_title("OPC DA/UA Test Client (Rust + egui)")
            .with_icon(egui::IconData {
                rgba: include_bytes!("../assets/icon_32.rgba").to_vec(),
                width: 32,
                height: 32,
            }),
        ..Default::default()
    };
    eframe::run_native(
        "opcda-egui",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
