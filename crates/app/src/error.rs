use std::{io, path::PathBuf};

use coldstart_core::{ManifestError, ProjectError, ProjectId};
use coldstart_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
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
        source: Box<AppError>,
        residual_paths: Vec<PathBuf>,
    },
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotADirectory(_) => "not_a_directory",
            Self::AlreadyInitialized(_) => "already_initialized",
            Self::StateAlreadyExists(_) => "state_already_exists",
            Self::ManifestMissing(_) => "manifest_missing",
            Self::DatabaseMissing(_) => "database_missing",
            Self::InvalidManifest(_) => "invalid_manifest",
            Self::InvalidProject(_) => "invalid_project",
            Self::ProjectMissingFromDatabase(_) => "project_missing_from_database",
            Self::ProjectMetadataMismatch(_) => "project_metadata_mismatch",
            Self::NonUtf8Path(_) => "non_utf8_path",
            Self::Json(_) => "json",
            Self::Storage(_) => "storage",
            Self::Io(_) => "io",
            Self::RollbackFailed { .. } => "rollback_failed",
        }
    }
}
