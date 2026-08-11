use serde::Serialize;

use crate::error::BackendError;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ApiError {
    pub fn background_task(message: impl Into<String>) -> Self {
        Self {
            code: "background_task_failed",
            message: message.into(),
            retryable: true,
            details: None,
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_request",
            message: message.into(),
            retryable: false,
            details: None,
        }
    }
}

impl From<BackendError> for ApiError {
    fn from(error: BackendError) -> Self {
        match error {
            BackendError::Io {
                path,
                source,
                operation,
            } => {
                let (code, message, retryable) = match source.kind() {
                    std::io::ErrorKind::NotFound => (
                        "location_not_found",
                        "That folder no longer exists or is unavailable.".to_owned(),
                        false,
                    ),
                    std::io::ErrorKind::PermissionDenied => (
                        "permission_denied",
                        "MineTrace does not have permission to read that folder.".to_owned(),
                        true,
                    ),
                    _ => (
                        "filesystem_error",
                        "MineTrace could not inspect that folder.".to_owned(),
                        true,
                    ),
                };

                Self {
                    code,
                    message,
                    retryable,
                    details: Some(serde_json::json!({
                        "operation": operation,
                        "path": path,
                    })),
                }
            }
            BackendError::InvalidLocation {
                path,
                reason,
                score,
            } => Self {
                code: "invalid_minecraft_location",
                message: "Choose a Minecraft game directory, launcher root, or instance folder."
                    .to_owned(),
                retryable: false,
                details: Some(serde_json::json!({
                    "path": path,
                    "reason": reason,
                    "validationScore": score,
                })),
            },
            BackendError::Database(_) | BackendError::MigrationChecksum { .. } => Self {
                code: "database_error",
                message: "MineTrace could not open its local database.".to_owned(),
                retryable: true,
                details: None,
            },
            BackendError::BackgroundTask(message) => Self::background_task(message),
        }
    }
}
