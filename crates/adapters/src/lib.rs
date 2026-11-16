//! ColdStart 外部 I/O 适配器。
//!
//! 适配器只负责 Git、GitHub、模型服务、文档解析、系统凭据和验证探针等边界；
//! 领域规则和状态机仍留在 core/app，避免 I/O 细节渗入控制面。

mod capability_api;
mod document;
mod git;
mod github;
mod model;
mod review;
mod secret;
mod verification;

pub use capability_api::{
    CapabilityApiClient, CapabilityApiError, CapabilityApiHttpRequest, CapabilityApiHttpResponse,
    UreqCapabilityApiClient,
};
pub use document::{
    DocumentError, ExtractedDocument, ExtractedImage, MAX_DOCUMENT_BYTES, MAX_EXTRACTED_CHARACTERS,
    MAX_IMAGE_BYTES, MAX_IMAGES_PER_DOCUMENT, extract_document,
};
pub use git::{
    GitAdapter, GitAuth, GitCommitVerification, GitError, GitMergeConflictReport,
    GitRepositoryStatus,
};
pub use github::{
    CreateDraftPullRequestRequest, CreatePrivateRepositoryRequest, DeviceFlowPoll, DeviceFlowStart,
    GitHubActor, GitHubCheckApp, GitHubCheckRun, GitHubClient, GitHubConnection, GitHubDeployment,
    GitHubDeploymentSnapshot, GitHubDeploymentStatus, GitHubError, GitHubIssue, GitHubIssueList,
    GitHubIssueListState, GitHubIssueState, GitHubProject, GitHubProjectField, GitHubProjectItem,
    GitHubProjectItemKind, GitHubProjectItemState, GitHubProjectOverview, GitHubPullRequest,
    GitHubPullRequestBranch, GitHubPullRequestReview, GitHubPullRequestReviewComment,
    GitHubRepository, GitHubRepositoryCoordinates, GitHubToken, MAX_GITHUB_ISSUE_BODY_CHARS,
    MAX_GITHUB_ISSUE_ITEMS, MAX_GITHUB_ISSUE_LABEL_CHARS, MAX_GITHUB_ISSUE_LABELS,
    MAX_GITHUB_ISSUE_TITLE_CHARS, MAX_GITHUB_PROJECT_FIELD_CHARS, MAX_GITHUB_PROJECT_FIELDS,
    MAX_GITHUB_PROJECT_ITEM_TITLE_CHARS, MAX_GITHUB_PROJECT_ITEMS, MAX_GITHUB_PROJECT_TITLE_CHARS,
    MAX_GITHUB_PROJECTS, import_gh_token,
};
pub use model::{
    BlueprintModel, ModelAnchor, ModelConfig, ModelError, ModelExtractionRequest, ModelSource,
    OpenAiCompatibleModel,
};
pub use review::{
    OpenAiCompatibleReviewModel, ReviewDiff, ReviewDiffFile, ReviewModel, ReviewModelError,
    ReviewModelOutput, ReviewModelRequest, ReviewStandardPrompt,
};
pub use secret::{MemorySecretStore, SecretStore, SecretStoreError, WindowsSecretStore};
pub use verification::{
    VerificationCommandExecution, VerificationCommandExecutor, WindowsVerificationExecutor,
    sanitize_verification_output,
};
