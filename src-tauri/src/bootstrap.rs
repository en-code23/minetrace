use std::fs;

use tauri::{AppHandle, Manager, Runtime};

use crate::{
    application::{DashboardService, DiscoveryService, ExplorerService, ScanService},
    discovery::AdapterRegistry,
    error::BackendError,
    platform::PlatformPaths,
    state::AppState,
    storage::Database,
};

pub fn initialize<R: Runtime>(app: &AppHandle<R>) -> Result<AppState, BackendError> {
    let data_dir = app
        .path()
        .app_local_data_dir()
        .map_err(|error| BackendError::BackgroundTask(error.to_string()))?;
    fs::create_dir_all(&data_dir)
        .map_err(|error| BackendError::io("create app data directory", &data_dir, error))?;

    if let Ok(log_dir) = app.path().app_log_dir() {
        fs::create_dir_all(&log_dir)
            .map_err(|error| BackendError::io("create app log directory", &log_dir, error))?;
    }

    let database = Database::open(data_dir.join("minetrace.sqlite3"))?;
    database.recover_interrupted_scans()?;

    let platform_paths = PlatformPaths::from_app(app);
    let discovery = std::sync::Arc::new(DiscoveryService::new(
        database.clone(),
        platform_paths,
        AdapterRegistry::standard(),
    ));
    let scan = std::sync::Arc::new(ScanService::new(database.clone(), discovery.clone()));

    Ok(AppState {
        database,
        dashboard: DashboardService,
        explorer: ExplorerService,
        discovery,
        scan,
    })
}
