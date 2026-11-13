use coldstart_adapters::{GitHubClient, GitHubRepositoryCoordinates, GitHubToken, SecretStore};
use coldstart_core::{CredentialKind, Repository, RepositoryAuthMode};

use crate::{AppError, context::OpenProject};

const GITHUB_PROVIDER: &str = "github";

pub(crate) struct GitHubRepositoryAccess {
    pub(crate) repository: Repository,
    pub(crate) coordinates: GitHubRepositoryCoordinates,
    pub(crate) token: GitHubToken,
}

pub(crate) fn resolve_github_repository_access(
    project: &OpenProject,
    repository_key: &str,
    secrets: &dyn SecretStore,
    github: Option<&GitHubClient>,
    ineligible: impl Fn(String, String) -> AppError,
) -> Result<GitHubRepositoryAccess, AppError> {
    let repository = project
        .database
        .repository_by_key(&project.project.id, repository_key)?
        .ok_or_else(|| AppError::RepositoryNotFound(repository_key.to_owned()))?;
    if repository.auth_mode != RepositoryAuthMode::Credential {
        return Err(ineligible(
            repository.key.clone(),
            "仓库必须使用 GitHub OAuth Token 或 Personal Access Token".to_owned(),
        ));
    }
    let credential_id = repository
        .credential_id
        .ok_or(AppError::RepositoryCredentialMissing(repository.id))?;
    let credential = project
        .database
        .credential(&project.project.id, &credential_id)?
        .ok_or(AppError::CredentialNotFound(credential_id))?;
    if !credential
        .provider
        .trim()
        .eq_ignore_ascii_case(GITHUB_PROVIDER)
        || !matches!(
            credential.kind,
            CredentialKind::OAuthToken | CredentialKind::PersonalAccessToken
        )
    {
        return Err(ineligible(
            repository.key.clone(),
            "仓库凭据不是 GitHub OAuth Token 或 Personal Access Token".to_owned(),
        ));
    }
    let coordinates = GitHubRepositoryCoordinates::from_remote_url(&repository.remote_url)
        .map_err(|error| ineligible(repository.key.clone(), error.to_string()))?;
    github.ok_or(AppError::GitHubNotConfigured)?;
    let token = GitHubToken::new(secrets.get(&credential.secret_ref)?)?;
    Ok(GitHubRepositoryAccess {
        repository,
        coordinates,
        token,
    })
}
