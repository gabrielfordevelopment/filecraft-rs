#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod background;
mod bulk_search;
mod dialog;
mod export_footer;
mod file_content_export;
mod file_rename;
mod folder_content_export;
mod folder_tree_view;
mod icons;
mod ignored_comments;
mod numbers;
mod options;
mod platform;
mod preset_list;
mod preset_windows;
mod requests;
mod state;
mod strings;
mod theme;
mod toast;
mod tree_generator;
mod widgets;

use std::path::PathBuf;
use std::sync::Arc;

/// Command line option that stores settings in the given folder instead of the user's settings.
const SETTINGS_DIR_ARG: &str = "--settings-dir";

const APP_ICON: &[u8] = include_bytes!("../../../assets/icons/app_icon.ico");

fn app_icon() -> Option<Arc<egui::IconData>> {
    match image::load_from_memory_with_format(APP_ICON, image::ImageFormat::Ico) {
        Ok(image) => {
            let image = image.to_rgba8();
            Some(Arc::new(egui::IconData {
                width: image.width(),
                height: image.height(),
                rgba: image.into_raw(),
            }))
        }
        Err(error) => {
            eprintln!("FileCraft: the application icon could not be decoded: {error}");
            None
        }
    }
}

fn settings_dir_arg() -> Option<PathBuf> {
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        if arg == SETTINGS_DIR_ARG {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

fn main() -> eframe::Result {
    let settings_dir = settings_dir_arg();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(strings::APP_TITLE)
        .with_inner_size([834.0, 761.0])
        .with_min_inner_size([720.0, 560.0]);
    if let Some(icon) = app_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        ..Default::default()
    };
    eframe::run_native(
        strings::APP_TITLE,
        options,
        Box::new(move |cc| Ok(Box::new(app::FileCraftApp::new(cc, settings_dir)))),
    )
}
