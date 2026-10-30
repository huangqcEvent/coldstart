use std::{io, path::PathBuf};

use coldstart_adapters::{DocumentError, ModelError};
use coldstart_core::{BlueprintRevisionId, InfoRequestId, ManifestError, ProjectError, ProjectId};
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
    #[error("document failure: {0}")]
    Document(#[from] DocumentError),
    #[error("model failure: {0}")]
    Model(#[from] ModelError),
    #[error("invalid blueprint: {0}")]
    InvalidBlueprint(String),
    #[error("invalid source reference: {0}")]
    InvalidSourceReference(String),
    #[error("requirement source context exceeds {limit} characters; found {actual}")]
    SourceContextTooLarge { actual: usize, limit: usize },
    #[error("project has no imported requirement sources")]
    NoRequirementSources,
    #[error("blueprint revision {0} does not exist")]
    BlueprintNotFound(BlueprintRevisionId),
    #[error("blueprint revision {0} is not a draft")]
    BlueprintNotDraft(BlueprintRevisionId),
    #[error("info request {0} does not exist")]
    InfoRequestNotFound(InfoRequestId),
    #[error("info request {0} is not open")]
    InfoRequestNotOpen(InfoRequestId),
    #[error("info request answer cannot be empty")]
    InfoRequestAnswerEmpty,
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
    #[error("operation failed: {source}; cleanup left residual paths: {residual_paths:?}")]
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
            Self::Document(error) => error.code(),
            Self::Model(error) => error.code(),
            Self::InvalidBlueprint(_) => "invalid_blueprint",
            Self::InvalidSourceReference(_) => "invalid_source_reference",
            Self::SourceContextTooLarge { .. } => "source_context_too_large",
            Self::NoRequirementSources => "no_requirement_sources",
            Self::BlueprintNotFound(_) => "blueprint_not_found",
            Self::BlueprintNotDraft(_) => "blueprint_not_draft",
            Self::InfoRequestNotFound(_) => "info_request_not_found",
            Self::InfoRequestNotOpen(_) => "info_request_not_open",
            Self::InfoRequestAnswerEmpty => "info_request_answer_empty",
            Self::ProjectMissingFromDatabase(_) => "project_missing_from_database",
            Self::ProjectMetadataMismatch(_) => "project_metadata_mismatch",
            Self::NonUtf8Path(_) => "non_utf8_path",
            Self::Json(_) => "json",
            Self::Storage(StorageError::BlueprintNotFound(_)) => "blueprint_not_found",
            Self::Storage(StorageError::InfoRequestNotFound(_)) => "info_request_not_found",
            Self::Storage(StorageError::InfoRequestNotOpen(_)) => "info_request_not_open",
            Self::Storage(StorageError::EmptyInfoRequestAnswer) => "info_request_answer_empty",
            Self::Storage(StorageError::InvalidBlueprint(_)) => "invalid_blueprint",
            Self::Storage(_) => "storage",
            Self::Io(_) => "io",
            Self::RollbackFailed { .. } => "rollback_failed",
        }
    }
}
