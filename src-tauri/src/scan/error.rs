use std::{io, path::Path};

#[derive(Debug, thiserror::Error)]
pub(crate) enum ScanError {
    #[error("scan cancelled while reading a source file")]
    Cancelled,

    #[error("scan root is not a directory: {0}")]
    RootNotDirectory(String),

    #[error("scan root must not be a symbolic link: {0}")]
    RootIsSymlink(String),

    #[error("inventory entry limit of {limit} was exceeded under {root}")]
    EntryLimitExceeded { root: String, limit: usize },

    #[error("file changed while it was being fingerprinted: {0}")]
    FileChanged(String),

    #[error("candidate is not a regular file: {0}")]
    NotRegularFile(String),

    #[error("{operation} failed for {path}: {source}")]
    Io {
        operation: &'static str,
        path: String,
        #[source]
        source: io::Error,
    },
}

impl ScanError {
    pub(crate) fn io(operation: &'static str, path: &Path, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.to_string_lossy().into_owned(),
            source,
        }
    }
}
