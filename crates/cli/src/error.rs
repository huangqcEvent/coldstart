use std::{io, path::PathBuf};

use coldstart_core::ProjectError;
use coldstart_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("project home path is not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("project is already initialized: {0}")]
    AlreadyInitialized(PathBuf),
    #[error("state database exists without a project manifest: {0}")]
    StateAlreadyExists(PathBuf),
    #[error("invalid project name: {0}")]
    InvalidProject(#[from] ProjectError),
    #[error("JSON failure: {0}")]
    Json(#[from] serde_json::Error),
    #[error("storage failure: {0}")]
    Storage(#[from] StorageError),
    #[error("filesystem failure: {0}")]
    Io(#[from] io::Error),
    #[error("initialization failed: {source}; cleanup left residual paths: {residual_paths:?}")]
    RollbackFailed {
        #[source]
        source: Box<CliError>,
        residual_paths: Vec<PathBuf>,
    },
}
