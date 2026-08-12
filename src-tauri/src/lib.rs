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
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let default_panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        log::error!("unhandled native panic: {panic_info}");
        default_panic_hook(panic_info);
    }));

    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(1_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            log::info!(
                "starting MineTrace {} on {} {}",
                app.package_info().version,
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            let state = match bootstrap::initialize(app.handle()) {
                Ok(state) => state,
                Err(error) => {
                    log::error!("native startup initialization failed: {error}");
                    let log_location = app
                        .path()
                        .app_log_dir()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|_| "the MineTrace application log folder".to_owned());
                    app.dialog()
                        .message(format!(
                            "MineTrace could not open its local archive. Your Minecraft files were not changed.\n\nDiagnostic logs: {log_location}\n\nError: {error}"
                        ))
                        .kind(MessageDialogKind::Error)
                        .title("MineTrace could not start")
                        .blocking_show();
                    return Err(Box::new(error));
                }
            };
            app.manage(state);
            log::info!("MineTrace native backend initialized");
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
            api::commands::record_frontend_issue,
            api::commands::save_share_image,
            api::commands::add_custom_location,
            api::commands::start_scan,
            api::commands::get_scan_status,
            api::commands::cancel_scan,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run MineTrace");
}
