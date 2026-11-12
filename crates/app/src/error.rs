use std::{io, path::PathBuf};

use coldstart_adapters::{DocumentError, GitError, GitHubError, ModelError, SecretStoreError};
use coldstart_core::{
    BlueprintRevisionId, CredentialId, InfoRequestId, IntegrationValidationError, ManifestError,
    PlanCompileError, PlanRevisionId, ProjectError, ProjectId, RepositoryId, WorkItemId,
};
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
    #[error("publishing a plan requires a published blueprint")]
    PublishedBlueprintRequired,
    #[error("invalid plan: {0}")]
    InvalidPlan(String),
    #[error("plan revision {0} does not exist")]
    PlanNotFound(PlanRevisionId),
    #[error("plan revision {0} is not a draft")]
    PlanNotDraft(PlanRevisionId),
    #[error("a plan revision is already active")]
    PlanAlreadyActive,
    #[error("replacing the active plan requires explicit confirmation")]
    ReplanConfirmationRequired,
    #[error("work item {0} does not exist")]
    WorkItemNotFound(WorkItemId),
    #[error("work item {0} cannot be operated in its current state")]
    WorkNotClaimable(WorkItemId),
    #[error("lease token is invalid for work item {0}")]
    LeaseInvalid(WorkItemId),
    #[error("lease for work item {0} has expired")]
    LeaseExpired(WorkItemId),
    #[error("work item {0} is not a human task")]
    HumanWorkRequired(WorkItemId),
    #[error("work report was rejected: {0}")]
    ReportRejected(String),
    #[error("claim request was already used: {0}")]
    DuplicateClaimRequest(String),
    #[error("task reference not found: {0}")]
    ReferenceNotFound(String),
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
    #[error("invalid integration configuration: {0}")]
    InvalidIntegration(#[from] IntegrationValidationError),
    #[error("Git failure: {0}")]
    Git(#[from] GitError),
    #[error("GitHub failure: {0}")]
    GitHub(#[from] GitHubError),
    #[error("secret store failure: {0}")]
    SecretStore(#[from] SecretStoreError),
    #[error("Git preparation failed: {source}; lease release also failed: {release_error}")]
    GitPreparationFailed {
        source: Box<AppError>,
        release_error: String,
    },
    #[error("credential {0} does not exist")]
    CredentialNotFound(CredentialId),
    #[error("repository {0} does not exist")]
    RepositoryNotFound(String),
    #[error("repository {0} is configured for credential authentication without a credential")]
    RepositoryCredentialMissing(RepositoryId),
    #[error("capability {capability_id} is not configured for environment {environment}")]
    CapabilityNotFound {
        capability_id: String,
        environment: String,
    },
    #[error("GitHub client is not configured")]
    GitHubNotConfigured,
    #[error("GitHub device flow session does not exist: {0}")]
    DeviceFlowNotFound(String),
    #[error("integration operation failed: {0}")]
    Integration(String),
    #[error("work item {0} has no verification commands configured")]
    VerificationNotConfigured(WorkItemId),
    #[error("work item {0} is already running verification")]
    VerificationInProgress(WorkItemId),
    #[error("repository verification failed")]
    VerificationFailed { details: serde_json::Value },
    #[error("repository verification timed out")]
    VerificationTimeout { details: serde_json::Value },
    #[error("repository verification was interrupted")]
    VerificationInterrupted { details: serde_json::Value },
    #[error("invalid verification command: {0}")]
    VerificationCommandInvalid(String),
    #[error("AI Review configuration requires explicit privacy consent")]
    ReviewConsentRequired,
    #[error("AI Review credential is missing or unavailable")]
    ReviewCredentialMissing,
    #[error("work item {0} has no Review snapshot")]
    ReviewNotConfigured(WorkItemId),
    #[error("work item {0} is already running Review")]
    ReviewInProgress(WorkItemId),
    #[error("invalid Review standard: {0}")]
    ReviewStandardInvalid(String),
    #[error("Review input exceeds the {limit} character limit; found {actual}")]
    ReviewInputTooLarge { actual: usize, limit: usize },
    #[error("AI Review blocked completion")]
    ReviewBlocked { details: serde_json::Value },
    #[error("AI Review could not complete")]
    ReviewFailed { details: serde_json::Value },
    #[error("AI Review was interrupted")]
    ReviewInterrupted { details: serde_json::Value },
    #[error("AI Review failed with {code}")]
    ReviewModelFailure {
        code: &'static str,
        details: serde_json::Value,
    },
    #[error("invalid AI Review waiver: {0}")]
    ReviewWaiverInvalid(String),
    #[error("project memory page limit must be between {min} and {max}; found {limit}")]
    InvalidMemoryLimit { limit: u32, min: u32, max: u32 },
    #[error("project memory cursor must be greater than zero; found {0}")]
    InvalidMemoryCursor(i64),
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
            Self::PublishedBlueprintRequired => "published_blueprint_required",
            Self::InvalidPlan(_) => "invalid_plan",
            Self::PlanNotFound(_) => "plan_not_found",
            Self::PlanNotDraft(_) => "plan_not_draft",
            Self::PlanAlreadyActive => "plan_already_active",
            Self::ReplanConfirmationRequired => "replan_confirmation_required",
            Self::WorkItemNotFound(_) => "work_item_not_found",
            Self::WorkNotClaimable(_) => "work_not_claimable",
            Self::LeaseInvalid(_) => "lease_invalid",
            Self::LeaseExpired(_) => "lease_expired",
            Self::HumanWorkRequired(_) => "human_work_required",
            Self::ReportRejected(_) => "report_invalid",
            Self::DuplicateClaimRequest(_) => "claim_request_duplicate",
            Self::ReferenceNotFound(_) => "reference_not_found",
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
            Self::Storage(StorageError::PlanNotFound(_)) => "plan_not_found",
            Self::Storage(StorageError::PlanNotDraft(_)) => "plan_not_draft",
            Self::Storage(StorageError::PlanAlreadyActive) => "plan_already_active",
            Self::Storage(StorageError::PlanLeaseActive) => "plan_lease_active",
            Self::Storage(StorageError::PlanNoLongerActive(_)) => "plan_no_longer_active",
            Self::Storage(StorageError::ContractStale(_)) => "contract_stale",
            Self::Storage(StorageError::PublishedBlueprintRequired) => {
                "published_blueprint_required"
            }
            Self::Storage(StorageError::BlueprintContentMismatch) => "blueprint_content_mismatch",
            Self::Storage(StorageError::WorkItemNotFound(_)) => "work_item_not_found",
            Self::Storage(StorageError::WorkNotClaimable(_)) => "work_not_claimable",
            Self::Storage(StorageError::LeaseInvalid(_)) => "lease_invalid",
            Self::Storage(StorageError::LeaseExpired(_)) => "lease_expired",
            Self::Storage(StorageError::ContractMismatch) => "contract_stale",
            Self::Storage(StorageError::ReportRejected(_)) => "report_invalid",
            Self::Storage(StorageError::DuplicateClaimRequest(_)) => "claim_request_duplicate",
            Self::Storage(StorageError::HumanWorkRequired(_)) => "human_work_required",
            Self::Storage(StorageError::EmptyClaimRequest) => "invalid_claim_request",
            Self::Storage(StorageError::EmptyWorkReason) => "empty_work_reason",
            Self::Storage(StorageError::EmptyHumanWorkNote) => "empty_human_work_note",
            Self::Storage(StorageError::InvalidPlan(_)) => "invalid_plan",
            Self::Storage(StorageError::VerificationNotConfigured(_))
            | Self::VerificationNotConfigured(_) => "verification_not_configured",
            Self::Storage(StorageError::VerificationInProgress(_))
            | Self::VerificationInProgress(_) => "verification_in_progress",
            Self::Storage(StorageError::VerificationCommandInvalid(_))
            | Self::VerificationCommandInvalid(_) => "verification_command_invalid",
            Self::Storage(StorageError::ReviewNotConfigured(_)) => "review_not_configured",
            Self::Storage(StorageError::ReviewInProgress(_)) => "review_in_progress",
            Self::Storage(
                StorageError::ReviewStandardNotFound(_) | StorageError::ReviewStandardDeleted(_),
            ) => "review_standard_invalid",
            Self::Storage(StorageError::ReviewInvalid(_)) => "review_failed",
            Self::Storage(StorageError::EmptyReviewWaiverReason) => "review_waiver_invalid",
            Self::Storage(_) => "storage",
            Self::Io(_) => "io",
            Self::RollbackFailed { .. } => "rollback_failed",
            Self::InvalidIntegration(_) => "invalid_integration",
            Self::Git(error) => error.code(),
            Self::GitHub(error) => error.code(),
            Self::SecretStore(_) => "secret_store_failed",
            Self::GitPreparationFailed { .. } => "git_preparation_failed",
            Self::CredentialNotFound(_) => "credential_not_found",
            Self::RepositoryNotFound(_) => "repository_not_found",
            Self::RepositoryCredentialMissing(_) => "repository_credential_missing",
            Self::CapabilityNotFound { .. } => "capability_not_found",
            Self::GitHubNotConfigured => "github_not_configured",
            Self::DeviceFlowNotFound(_) => "device_flow_not_found",
            Self::Integration(_) => "integration_failed",
            Self::VerificationFailed { .. } => "verification_failed",
            Self::VerificationTimeout { .. } => "verification_timeout",
            Self::VerificationInterrupted { .. } => "verification_interrupted",
            Self::ReviewConsentRequired => "review_consent_required",
            Self::ReviewCredentialMissing => "review_credential_missing",
            Self::ReviewNotConfigured(_) => "review_not_configured",
            Self::ReviewInProgress(_) => "review_in_progress",
            Self::ReviewStandardInvalid(_) => "review_standard_invalid",
            Self::ReviewInputTooLarge { .. } => "review_input_too_large",
            Self::ReviewBlocked { .. } => "review_blocked",
            Self::ReviewFailed { .. } => "review_failed",
            Self::ReviewInterrupted { .. } => "review_interrupted",
            Self::ReviewModelFailure { code, .. } => code,
            Self::ReviewWaiverInvalid(_) => "review_waiver_invalid",
            Self::InvalidMemoryLimit { .. } => "invalid_memory_limit",
            Self::InvalidMemoryCursor(_) => "invalid_memory_cursor",
        }
    }

    pub fn verification_details(&self) -> Option<serde_json::Value> {
        match self {
            Self::VerificationFailed { details }
            | Self::VerificationTimeout { details }
            | Self::VerificationInterrupted { details } => Some(details.clone()),
            _ => None,
        }
    }

    pub fn review_details(&self) -> Option<serde_json::Value> {
        match self {
            Self::ReviewBlocked { details }
            | Self::ReviewFailed { details }
            | Self::ReviewInterrupted { details }
            | Self::ReviewModelFailure { details, .. } => Some(details.clone()),
            _ => None,
        }
    }
}

impl From<PlanCompileError> for AppError {
    fn from(error: PlanCompileError) -> Self {
        Self::InvalidPlan(error.to_string())
    }
}
