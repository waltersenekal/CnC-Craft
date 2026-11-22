#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod cam;
mod canvas;
mod checks;
mod chrome;
mod design;
mod diagnostics;
mod effects;
mod design_ui;
mod geom;
mod guides;
mod icon;
mod inlay;
mod jobtools;
mod io;
mod layout;
mod machine;
mod machine_ui;
mod model;
mod modify;
mod modify_ui;
mod panels;
mod relief;
mod shape_tools;
mod sim;
mod text;
mod tiles;
mod toolcat;
mod view3d;
mod windows;
mod wp_tools;

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let mut screenshot: Option<std::path::PathBuf> = None;
    let mut open: Option<std::path::PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshot" && i + 1 < args.len() {
            screenshot = Some(args[i + 1].clone().into());
            i += 2;
            continue;
        }
        open = Some(args[i].clone().into());
        i += 1;
    }
    if let Ok(p) = std::env::var("CNCCRAFT_ICON_PNG") {
        // testing helper: write the icon as a picture and stop
        let _ = image::save_buffer(p, &icon::icon_rgba(256), 256, 256, image::ExtendedColorType::Rgba8);
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CncCraft")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_icon(std::sync::Arc::new(egui::IconData { rgba: icon::icon_rgba(128), width: 128, height: 128 })),
        ..Default::default()
    };
    eframe::run_native(
        "CncCraft",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, screenshot, open)))),
    )
}
