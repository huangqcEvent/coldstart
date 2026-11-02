mod document;
mod git;
mod github;
mod model;
mod review;
mod secret;
mod verification;

pub use document::{
    DocumentError, ExtractedDocument, MAX_DOCUMENT_BYTES, MAX_EXTRACTED_CHARACTERS,
    extract_document,
};
pub use git::{GitAdapter, GitAuth, GitCommitVerification, GitError, GitRepositoryStatus};
pub use github::{
    CreatePrivateRepositoryRequest, DeviceFlowPoll, DeviceFlowStart, GitHubClient,
    GitHubConnection, GitHubError, GitHubRepository, GitHubToken, import_gh_token,
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
