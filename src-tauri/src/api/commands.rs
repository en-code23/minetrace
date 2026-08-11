use std::path::PathBuf;

use tauri::State;

use crate::{
    api::{
        dto::{DashboardData, DiscoveredLocationDto},
        error::ApiError,
    },
    application::{
        ProfileData,
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
pub async fn get_profile(state: State<'_, AppState>) -> Result<ProfileData, ApiError> {
    let database = state.database.clone();
    let discovery = state.discovery.clone();
    let profile = state.profile;
    tauri::async_runtime::spawn_blocking(move || profile.load(&database, &discovery))
        .await
        .map_err(|error| ApiError::background_task(error.to_string()))?
        .map_err(ApiError::from)
}

#[tauri::command]
pub async fn save_share_image(path: String, bytes: Vec<u8>) -> Result<(), ApiError> {
    const MAX_SHARE_IMAGE_BYTES: usize = 10 * 1024 * 1024;
    if bytes.len() > MAX_SHARE_IMAGE_BYTES || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(ApiError::invalid_request(
            "The share card must be a PNG smaller than 10 MiB.",
        ));
    }
    let path = PathBuf::from(path);
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
        != Some("png")
    {
        return Err(ApiError::invalid_request(
            "Choose a destination ending in .png.",
        ));
    }
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::write(&path, bytes)
            .map_err(|error| crate::error::BackendError::io("save share image", &path, error))
    })
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
