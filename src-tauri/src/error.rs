use std::{io, path::Path};

#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("database operation failed: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("{operation} failed for {path}: {source}")]
    Io {
        operation: &'static str,
        path: String,
        #[source]
        source: io::Error,
    },

    #[error("location is not a supported Minecraft directory: {path}")]
    InvalidLocation {
        path: String,
        reason: String,
        score: u8,
    },

    #[error("database migration {version} ({name}) does not match the recorded checksum")]
    MigrationChecksum { version: i64, name: &'static str },

    #[error("background task failed: {0}")]
    BackgroundTask(String),
}

impl BackendError {
    pub fn io(operation: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.to_string_lossy().into_owned(),
            source,
        }
    }
}
