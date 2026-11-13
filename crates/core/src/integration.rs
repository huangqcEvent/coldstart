//! 仓库、凭据引用和三方能力配置模型。
//!
//! 该模块定义本地仓库键、远程 URL、分支、同步策略和能力环境等不变量；凭据只
//! 保存引用与元数据，真实 Secret 始终由系统凭据库持有。

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{BlueprintEntityId, CredentialId, GitSyncId, ProjectId, RepositoryId, WorkItemId};

pub const REPOSITORY_KEYS: [&str; 5] = ["server", "admin", "web", "app", "mini_program"];
pub const CAPABILITY_ENVIRONMENTS: [&str; 3] = ["development", "staging", "production"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryAuthMode {
    None,
    Credential,
    SshAgent,
}

impl RepositoryAuthMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Credential => "credential",
            Self::SshAgent => "ssh_agent",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "credential" => Some(Self::Credential),
            "ssh_agent" => Some(Self::SshAgent),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositorySyncPolicy {
    Manual,
    FetchBeforeClaim,
}

impl RepositorySyncPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::FetchBeforeClaim => "fetch_before_claim",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "manual" => Some(Self::Manual),
            "fetch_before_claim" => Some(Self::FetchBeforeClaim),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub id: RepositoryId,
    pub project_id: ProjectId,
    pub key: String,
    pub name: String,
    pub local_path: PathBuf,
    pub remote_url: String,
    pub default_branch: String,
    pub protected_branches: Vec<String>,
    pub auth_mode: RepositoryAuthMode,
    pub credential_id: Option<CredentialId>,
    pub sync_policy: RepositorySyncPolicy,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Repository {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project_id: ProjectId,
        key: String,
        name: String,
        local_path: PathBuf,
        remote_url: String,
        default_branch: String,
        protected_branches: Vec<String>,
        auth_mode: RepositoryAuthMode,
        credential_id: Option<CredentialId>,
        sync_policy: RepositorySyncPolicy,
    ) -> Result<Self, IntegrationValidationError> {
        let now = Utc::now();
        let repository = Self {
            id: RepositoryId::new(),
            project_id,
            key,
            name,
            local_path,
            remote_url,
            default_branch,
            protected_branches,
            auth_mode,
            credential_id,
            sync_policy,
            created_at: now,
            updated_at: now,
        };
        repository.validate()?;
        Ok(repository)
    }

    /// 校验仓库键、路径、远程地址、分支和同步策略，返回首个不稳定字段。
    pub fn validate(&self) -> Result<(), IntegrationValidationError> {
        validate_repository_key(&self.key)?;
        if self.name.trim().is_empty() {
            return Err(IntegrationValidationError::EmptyRepositoryName);
        }
        if self.name.chars().count() > 120 {
            return Err(IntegrationValidationError::RepositoryNameTooLong);
        }
        if !self.local_path.is_absolute() {
            return Err(IntegrationValidationError::RepositoryPathNotAbsolute(
                self.local_path.clone(),
            ));
        }
        validate_remote_url(&self.remote_url)?;
        validate_branch_name(&self.default_branch)?;
        if self.protected_branches.is_empty() {
            return Err(IntegrationValidationError::NoProtectedBranches);
        }
        for branch in &self.protected_branches {
            validate_branch_name(branch)?;
        }
        if self.auth_mode == RepositoryAuthMode::Credential && self.credential_id.is_none() {
            return Err(IntegrationValidationError::CredentialRequired);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialEnvironment {
    Shared,
    Development,
    Staging,
    Production,
}

impl CredentialEnvironment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Development => "development",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "shared" => Some(Self::Shared),
            "development" => Some(Self::Development),
            "staging" => Some(Self::Staging),
            "production" => Some(Self::Production),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    ApiKey,
    OAuthToken,
    PersonalAccessToken,
    UsernamePassword,
    SigningCertificate,
    Other,
}

impl CredentialKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApiKey => "api_key",
            Self::OAuthToken => "oauth_token",
            Self::PersonalAccessToken => "personal_access_token",
            Self::UsernamePassword => "username_password",
            Self::SigningCertificate => "signing_certificate",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "api_key" => Some(Self::ApiKey),
            "oauth_token" => Some(Self::OAuthToken),
            "personal_access_token" => Some(Self::PersonalAccessToken),
            "username_password" => Some(Self::UsernamePassword),
            "signing_certificate" => Some(Self::SigningCertificate),
            "other" => Some(Self::Other),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialMetadata {
    pub id: CredentialId,
    pub project_id: ProjectId,
    pub name: String,
    pub provider: String,
    pub kind: CredentialKind,
    pub environment: CredentialEnvironment,
    pub secret_ref: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CredentialMetadata {
    pub fn new(
        project_id: ProjectId,
        name: String,
        provider: String,
        kind: CredentialKind,
        environment: CredentialEnvironment,
        secret_ref: String,
    ) -> Result<Self, IntegrationValidationError> {
        let now = Utc::now();
        let credential = Self {
            id: CredentialId::new(),
            project_id,
            name,
            provider,
            kind,
            environment,
            secret_ref,
            created_at: now,
            updated_at: now,
        };
        credential.validate()?;
        Ok(credential)
    }

    /// 校验凭据元数据的环境、类型和引用格式；Secret 不参与领域校验。
    pub fn validate(&self) -> Result<(), IntegrationValidationError> {
        if self.name.trim().is_empty() {
            return Err(IntegrationValidationError::EmptyCredentialName);
        }
        if self.provider.trim().is_empty() {
            return Err(IntegrationValidationError::EmptyCredentialProvider);
        }
        if self.secret_ref.trim().is_empty() {
            return Err(IntegrationValidationError::EmptySecretRef);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Missing,
    Configured,
    Verified,
    AuthRequired,
    Disabled,
}

impl CapabilityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Configured => "configured",
            Self::Verified => "verified",
            Self::AuthRequired => "auth_required",
            Self::Disabled => "disabled",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "missing" => Some(Self::Missing),
            "configured" => Some(Self::Configured),
            "verified" => Some(Self::Verified),
            "auth_required" => Some(Self::AuthRequired),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityState {
    pub project_id: ProjectId,
    pub capability_id: BlueprintEntityId,
    pub name: String,
    pub provider: String,
    pub environment: String,
    pub status: CapabilityStatus,
    pub credential_id: Option<CredentialId>,
    pub note: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDescriptor {
    pub project_id: ProjectId,
    pub capability_id: BlueprintEntityId,
    pub name: String,
    pub provider: String,
    pub environment: String,
}

impl CapabilityState {
    pub fn new(
        descriptor: CapabilityDescriptor,
        status: CapabilityStatus,
        credential_id: Option<CredentialId>,
        note: Option<String>,
    ) -> Result<Self, IntegrationValidationError> {
        validate_capability_environment(&descriptor.environment)?;
        let state = Self {
            project_id: descriptor.project_id,
            capability_id: descriptor.capability_id,
            name: descriptor.name,
            provider: descriptor.provider,
            environment: descriptor.environment,
            status,
            credential_id,
            note,
            updated_at: Utc::now(),
        };
        state.validate()?;
        Ok(state)
    }

    pub fn validate(&self) -> Result<(), IntegrationValidationError> {
        if self.name.trim().is_empty() {
            return Err(IntegrationValidationError::EmptyCapabilityName);
        }
        if self.provider.trim().is_empty() {
            return Err(IntegrationValidationError::EmptyCapabilityProvider);
        }
        validate_capability_environment(&self.environment)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GitSyncStatus {
    Pushed,
    Failed,
}

impl GitSyncStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pushed => "pushed",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pushed" => Some(Self::Pushed),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSyncRecord {
    pub id: GitSyncId,
    pub project_id: ProjectId,
    pub work_item_id: WorkItemId,
    pub repository_id: RepositoryId,
    pub branch: String,
    pub commit_sha: String,
    pub status: GitSyncStatus,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl GitSyncRecord {
    pub fn new(
        project_id: ProjectId,
        work_item_id: WorkItemId,
        repository_id: RepositoryId,
        branch: String,
        commit_sha: String,
        status: GitSyncStatus,
        error_message: Option<String>,
    ) -> Self {
        Self {
            id: GitSyncId::new(),
            project_id,
            work_item_id,
            repository_id,
            branch,
            commit_sha,
            status,
            error_message,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IntegrationValidationError {
    #[error("unsupported repository key: {0}")]
    UnsupportedRepositoryKey(String),
    #[error("repository name cannot be empty")]
    EmptyRepositoryName,
    #[error("repository name must be at most 120 characters")]
    RepositoryNameTooLong,
    #[error("repository path must be absolute: {0}")]
    RepositoryPathNotAbsolute(PathBuf),
    #[error("remote URL cannot be empty")]
    EmptyRemoteUrl,
    #[error("remote URL must not contain embedded credentials")]
    RemoteUrlContainsCredentials,
    #[error("invalid branch name: {0}")]
    InvalidBranchName(String),
    #[error("at least one protected branch is required")]
    NoProtectedBranches,
    #[error("credential authentication requires a credential reference")]
    CredentialRequired,
    #[error("credential name cannot be empty")]
    EmptyCredentialName,
    #[error("credential provider cannot be empty")]
    EmptyCredentialProvider,
    #[error("secret reference cannot be empty")]
    EmptySecretRef,
    #[error("capability name cannot be empty")]
    EmptyCapabilityName,
    #[error("capability provider cannot be empty")]
    EmptyCapabilityProvider,
    #[error("unsupported capability environment: {0}")]
    UnsupportedCapabilityEnvironment(String),
}

pub fn validate_repository_key(value: &str) -> Result<(), IntegrationValidationError> {
    if REPOSITORY_KEYS.contains(&value.trim()) {
        Ok(())
    } else {
        Err(IntegrationValidationError::UnsupportedRepositoryKey(
            value.to_owned(),
        ))
    }
}

/// 接受受支持的 HTTPS 或 SSH Git 远程格式，拒绝不安全协议和凭据内嵌。
pub fn validate_remote_url(value: &str) -> Result<(), IntegrationValidationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(IntegrationValidationError::EmptyRemoteUrl);
    }
    if let Some(scheme_end) = value.find("://") {
        let authority_end = value[scheme_end + 3..]
            .find('/')
            .map(|offset| scheme_end + 3 + offset)
            .unwrap_or(value.len());
        if value[scheme_end + 3..authority_end].contains('@') {
            return Err(IntegrationValidationError::RemoteUrlContainsCredentials);
        }
    }
    Ok(())
}

/// 校验 Git 分支名长度、字符集、前导符号和保留标记。
pub fn validate_branch_name(value: &str) -> Result<(), IntegrationValidationError> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 200
        && !value.starts_with('-')
        && !value.ends_with('.')
        && !value.ends_with('/')
        && !value.contains("..")
        && !value.contains("@{")
        && !value
            .chars()
            .any(|character| matches!(character, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
        && value
            .split('/')
            .all(|segment| !segment.is_empty() && !segment.starts_with('.'));
    if valid {
        Ok(())
    } else {
        Err(IntegrationValidationError::InvalidBranchName(
            value.to_owned(),
        ))
    }
}

pub fn validate_capability_environment(value: &str) -> Result<(), IntegrationValidationError> {
    if CAPABILITY_ENVIRONMENTS.contains(&value.trim()) {
        Ok(())
    } else {
        Err(IntegrationValidationError::UnsupportedCapabilityEnvironment(value.to_owned()))
    }
}
