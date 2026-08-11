mod api;
mod application;
mod bootstrap;
mod discovery;
mod domain;
mod error;
mod export;
mod parser;
mod platform;
mod privacy;
mod scan;
mod state;
mod storage;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let state = bootstrap::initialize(app.handle())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api::commands::discover_installations,
            api::commands::get_dashboard,
            api::commands::get_sessions,
            api::commands::get_instances,
            api::commands::get_worlds,
            api::commands::get_servers,
            api::commands::get_versions,
            api::commands::get_profile,
            api::commands::save_share_image,
            api::commands::add_custom_location,
            api::commands::start_scan,
            api::commands::get_scan_status,
            api::commands::cancel_scan,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run MineTrace");
}
