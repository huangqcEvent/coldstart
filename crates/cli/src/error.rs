use std::{io, path::PathBuf};

use coldstart_core::{ManifestError, ProjectError, ProjectId};
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
    #[error("project manifest is missing: {0}")]
    ManifestMissing(PathBuf),
    #[error("project database is missing: {0}")]
    DatabaseMissing(PathBuf),
    #[error("invalid project manifest: {0}")]
    InvalidManifest(#[from] ManifestError),
    #[error("invalid project name: {0}")]
    InvalidProject(#[from] ProjectError),
    #[error("project {0} is missing from the database")]
    ProjectMissingFromDatabase(ProjectId),
    #[error("project manifest and database metadata disagree for project {0}")]
    ProjectMetadataMismatch(ProjectId),
    #[error("project path is not valid UTF-8: {0}")]
    NonUtf8Path(PathBuf),
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
