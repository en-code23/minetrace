use std::path::PathBuf;

use tauri::State;

use crate::{
    api::{
        dto::{DashboardData, DiscoveredLocationDto},
        error::ApiError,
    },
    application::{
        read_models::{
            BoundedCollection, InstanceSummary, ServerSummary, SessionPage, VersionSummary,
            WorldSummary,
        },
        scan_models::{ScanMode, ScanStatus},
    },
    state::AppState,
};

#[tauri::command]
pub async fn discover_installations(
    state: State<'_, AppState>,
) -> Result<Vec<DiscoveredLocationDto>, ApiError> {
    let service = state.discovery.clone();
    let installations = tauri::async_runtime::spawn_blocking(move || service.discover())
        .await
        .map_err(|error| ApiError::background_task(error.to_string()))?
        .map_err(ApiError::from)?;

    Ok(installations.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn add_custom_location(
    path: String,
    state: State<'_, AppState>,
) -> Result<DiscoveredLocationDto, ApiError> {
    let service = state.discovery.clone();
    let installation = tauri::async_runtime::spawn_blocking(move || {
        service.add_custom_location(PathBuf::from(path))
    })
    .await
    .map_err(|error| ApiError::background_task(error.to_string()))?
    .map_err(ApiError::from)?;

    Ok(installation.into())
}

#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> Result<DashboardData, ApiError> {
    let database = state.database.clone();
    let dashboard = state.dashboard;
    tauri::async_runtime::spawn_blocking(move || dashboard.load(&database))
        .await
        .map_err(|error| ApiError::background_task(error.to_string()))?
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_sessions(state: State<'_, AppState>) -> Result<SessionPage, ApiError> {
    let database = state.database.clone();
    let dashboard = state.dashboard;
    tauri::async_runtime::spawn_blocking(move || dashboard.session_page(&database))
        .await
        .map_err(|error| ApiError::background_task(error.to_string()))?
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_instances(
    state: State<'_, AppState>,
) -> Result<BoundedCollection<InstanceSummary>, ApiError> {
    let database = state.database.clone();
    let explorer = state.explorer;
    tauri::async_runtime::spawn_blocking(move || explorer.instance_collection(&database))
        .await
        .map_err(|error| ApiError::background_task(error.to_string()))?
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_worlds(
    state: State<'_, AppState>,
) -> Result<BoundedCollection<WorldSummary>, ApiError> {
    let database = state.database.clone();
    let explorer = state.explorer;
    tauri::async_runtime::spawn_blocking(move || explorer.world_collection(&database))
        .await
        .map_err(|error| ApiError::background_task(error.to_string()))?
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_servers(
    state: State<'_, AppState>,
) -> Result<BoundedCollection<ServerSummary>, ApiError> {
    let database = state.database.clone();
    let explorer = state.explorer;
    tauri::async_runtime::spawn_blocking(move || explorer.server_collection(&database))
        .await
        .map_err(|error| ApiError::background_task(error.to_string()))?
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_versions(
    state: State<'_, AppState>,
) -> Result<BoundedCollection<VersionSummary>, ApiError> {
    let database = state.database.clone();
    let explorer = state.explorer;
    tauri::async_runtime::spawn_blocking(move || explorer.version_collection(&database))
        .await
        .map_err(|error| ApiError::background_task(error.to_string()))?
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn start_scan(
    mode: ScanMode,
    state: State<'_, AppState>,
) -> Result<ScanStatus, ApiError> {
    let service = state.scan.clone();
    tauri::async_runtime::spawn_blocking(move || service.start(mode))
        .await
        .map_err(|error| ApiError::background_task(error.to_string()))?
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn get_scan_status(state: State<'_, AppState>) -> Result<ScanStatus, ApiError> {
    state.scan.status().map_err(ApiError::from)
}

#[tauri::command]
pub async fn cancel_scan(state: State<'_, AppState>) -> Result<ScanStatus, ApiError> {
    let service = state.scan.clone();
    tauri::async_runtime::spawn_blocking(move || service.cancel())
        .await
        .map_err(|error| ApiError::background_task(error.to_string()))?
        .map_err(ApiError::from)
}
