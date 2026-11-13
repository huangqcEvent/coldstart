use std::{fmt, process::Command, time::Duration};

use coldstart_core::{
    MAX_PULL_REQUEST_CHECK_NAME_CHARS, MAX_PULL_REQUEST_INSIGHT_ITEMS,
    MAX_PULL_REQUEST_TITLE_CHARS, MAX_PULL_REQUEST_URL_CHARS, validate_branch_name,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

const DEFAULT_API_BASE: &str = "https://api.github.com";
const DEFAULT_OAUTH_BASE: &str = "https://github.com";
const GITHUB_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ERROR_BODY_CHARS: usize = 600;
const MAX_PULL_REQUEST_BODY_CHARS: usize = 60_000;

#[derive(Clone, PartialEq, Eq)]
pub struct GitHubToken(String);

impl GitHubToken {
    pub fn new(value: impl Into<String>) -> Result<Self, GitHubError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
            return Err(GitHubError::InvalidToken);
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for GitHubToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("GitHubToken(***)")
    }
}

pub struct DeviceFlowStart {
    device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

impl DeviceFlowStart {
    pub fn device_code(&self) -> &str {
        &self.device_code
    }
}

impl fmt::Debug for DeviceFlowStart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeviceFlowStart")
            .field("device_code", &"***")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceFlowPoll {
    Pending {
        interval: u64,
    },
    SlowDown {
        interval: u64,
    },
    Authorized {
        token: GitHubToken,
        token_type: String,
        scopes: Vec<String>,
    },
    AccessDenied,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitHubConnection {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    #[serde(skip)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePrivateRepositoryRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "snake_case"))]
pub struct GitHubRepository {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub clone_url: String,
    pub ssh_url: String,
    pub html_url: String,
    pub default_branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubRepositoryCoordinates {
    pub owner: String,
    pub repository: String,
}

impl GitHubRepositoryCoordinates {
    pub fn from_remote_url(value: &str) -> Result<Self, GitHubError> {
        let value = value.trim();
        let (host, path) = if let Some(rest) = value.strip_prefix("git@") {
            rest.split_once(':').ok_or_else(|| {
                GitHubError::UnsupportedRemote("SSH remote is missing a repository path".to_owned())
            })?
        } else if let Some(rest) = value.strip_prefix("ssh://") {
            let rest = rest.strip_prefix("git@").unwrap_or(rest);
            rest.split_once('/').ok_or_else(|| {
                GitHubError::UnsupportedRemote("SSH remote is missing a repository path".to_owned())
            })?
        } else if let Some(rest) = value.strip_prefix("https://") {
            rest.split_once('/').ok_or_else(|| {
                GitHubError::UnsupportedRemote(
                    "HTTPS remote is missing a repository path".to_owned(),
                )
            })?
        } else {
            return Err(GitHubError::UnsupportedRemote(
                "remote must use a GitHub HTTPS or SSH URL".to_owned(),
            ));
        };
        if !host.eq_ignore_ascii_case("github.com") {
            return Err(GitHubError::UnsupportedRemote(format!(
                "unsupported GitHub host: {host}"
            )));
        }
        let path = path.trim_end_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let (owner, repository) = path.split_once('/').ok_or_else(|| {
            GitHubError::UnsupportedRemote("remote is missing an owner or repository".to_owned())
        })?;
        if repository.contains('/') {
            return Err(GitHubError::UnsupportedRemote(
                "remote contains extra path segments".to_owned(),
            ));
        }
        validate_owner(owner)?;
        validate_repository_name(repository)?;
        Ok(Self {
            owner: owner.to_owned(),
            repository: repository.to_owned(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDraftPullRequestRequest {
    pub owner: String,
    pub repository: String,
    pub title: String,
    pub body: String,
    pub head: String,
    pub base: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitHubPullRequestBranch {
    #[serde(rename = "ref")]
    pub reference: String,
    pub sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitHubPullRequest {
    pub number: u64,
    pub html_url: String,
    pub state: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub merged: bool,
    pub title: String,
    pub head: GitHubPullRequestBranch,
    pub base: GitHubPullRequestBranch,
    #[serde(default)]
    pub mergeable: Option<bool>,
    #[serde(default)]
    pub mergeable_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitHubActor {
    pub login: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitHubCheckApp {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitHubCheckRun {
    pub id: u64,
    pub name: String,
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub details_url: Option<String>,
    #[serde(default)]
    pub app: Option<GitHubCheckApp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitHubPullRequestReview {
    pub id: u64,
    pub state: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub user: Option<GitHubActor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GitHubPullRequestReviewComment {
    pub id: u64,
    pub path: String,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub original_line: Option<u32>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub html_url: Option<String>,
    #[serde(default)]
    pub user: Option<GitHubActor>,
}

pub struct GitHubClient {
    api_base: String,
    oauth_base: String,
    agent: ureq::Agent,
}

impl fmt::Debug for GitHubClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GitHubClient")
            .field("api_base", &self.api_base)
            .field("oauth_base", &self.oauth_base)
            .finish()
    }
}

impl GitHubClient {
    pub fn new() -> Result<Self, GitHubError> {
        Self::with_endpoints(DEFAULT_API_BASE, DEFAULT_OAUTH_BASE)
    }

    pub fn with_endpoints(api_base: &str, oauth_base: &str) -> Result<Self, GitHubError> {
        let api_base = normalize_base_url(api_base)?;
        let oauth_base = normalize_base_url(oauth_base)?;
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(GITHUB_TIMEOUT))
            .http_status_as_error(false)
            .build();
        Ok(Self {
            api_base,
            oauth_base,
            agent: config.into(),
        })
    }

    pub fn start_device_flow(
        &self,
        client_id: &str,
        scopes: &[&str],
    ) -> Result<DeviceFlowStart, GitHubError> {
        let client_id = require_client_id(client_id)?;
        let scope = scopes
            .iter()
            .map(|scope| scope.trim())
            .filter(|scope| !scope.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let body = serde_json::json!({
            "client_id": client_id,
            "scope": scope,
        });
        let response = self.post_json(&self.oauth_base, "/login/device/code", &body, None)?;
        if !(200..300).contains(&response.status) {
            return Err(GitHubError::HttpStatus {
                status: response.status,
                body: truncate_error(&response.body),
            });
        }
        let start: RawDeviceFlowStart = serde_json::from_str(&response.body)
            .map_err(|error| GitHubError::InvalidResponse(error.to_string()))?;
        if start.device_code.trim().is_empty()
            || start.user_code.trim().is_empty()
            || start.verification_uri.trim().is_empty()
        {
            return Err(GitHubError::InvalidResponse(
                "device flow response omitted required fields".to_owned(),
            ));
        }
        Ok(DeviceFlowStart {
            device_code: start.device_code,
            user_code: start.user_code,
            verification_uri: start.verification_uri,
            expires_in: start.expires_in,
            interval: start.interval.unwrap_or(5).max(1),
        })
    }

    pub fn poll_device_flow(
        &self,
        client_id: &str,
        device_code: &str,
    ) -> Result<DeviceFlowPoll, GitHubError> {
        let client_id = require_client_id(client_id)?;
        if device_code.trim().is_empty() {
            return Err(GitHubError::InvalidConfiguration(
                "device code cannot be empty".to_owned(),
            ));
        }
        let body = serde_json::json!({
            "client_id": client_id,
            "device_code": device_code,
            "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        });
        let response =
            self.post_json(&self.oauth_base, "/login/oauth/access_token", &body, None)?;
        let poll: RawDeviceFlowPoll = serde_json::from_str(&response.body)
            .map_err(|error| GitHubError::InvalidResponse(error.to_string()))?;
        if let Some(token) = poll.access_token.filter(|token| !token.trim().is_empty()) {
            return Ok(DeviceFlowPoll::Authorized {
                token: GitHubToken::new(token)?,
                token_type: poll.token_type.unwrap_or_else(|| "bearer".to_owned()),
                scopes: split_scopes(poll.scope.as_deref()),
            });
        }
        match poll.error.as_deref() {
            Some("authorization_pending") => Ok(DeviceFlowPoll::Pending {
                interval: poll.interval.unwrap_or(5).max(1),
            }),
            Some("slow_down") => Ok(DeviceFlowPoll::SlowDown {
                interval: poll.interval.unwrap_or(10).max(1),
            }),
            Some("access_denied") => Ok(DeviceFlowPoll::AccessDenied),
            Some("expired_token") => Ok(DeviceFlowPoll::Expired),
            Some(error) => Err(GitHubError::DeviceFlow {
                code: error.to_owned(),
                description: poll.error_description.unwrap_or_default(),
            }),
            None if !(200..300).contains(&response.status) => Err(GitHubError::HttpStatus {
                status: response.status,
                body: truncate_error(&response.body),
            }),
            None => Err(GitHubError::InvalidResponse(
                "device flow response omitted access token and error".to_owned(),
            )),
        }
    }

    pub fn authenticated_user(&self, token: &GitHubToken) -> Result<GitHubConnection, GitHubError> {
        let response = self.get_json("/user", Some(token))?;
        if !(200..300).contains(&response.status) {
            return Err(GitHubError::HttpStatus {
                status: response.status,
                body: truncate_error(&redact_secret(&response.body, Some(token))),
            });
        }
        let mut connection: GitHubConnection = serde_json::from_str(&response.body)
            .map_err(|error| GitHubError::InvalidResponse(error.to_string()))?;
        connection.scopes = split_scopes(response.scopes.as_deref());
        Ok(connection)
    }

    pub fn create_private_repository(
        &self,
        token: &GitHubToken,
        request: &CreatePrivateRepositoryRequest,
    ) -> Result<GitHubRepository, GitHubError> {
        validate_repository_name(&request.name)?;
        let body = serde_json::json!({
            "name": request.name.trim(),
            "description": request.description.as_deref().map(str::trim).filter(|value| !value.is_empty()),
            "private": true,
            "auto_init": false,
        });
        let response = self.post_json(&self.api_base, "/user/repos", &body, Some(token))?;
        if !(200..300).contains(&response.status) {
            return Err(GitHubError::HttpStatus {
                status: response.status,
                body: truncate_error(&redact_secret(&response.body, Some(token))),
            });
        }
        let repository: GitHubRepository = serde_json::from_str(&response.body)
            .map_err(|error| GitHubError::InvalidResponse(error.to_string()))?;
        if !repository.private {
            return Err(GitHubError::InvalidResponse(
                "GitHub created a public repository".to_owned(),
            ));
        }
        Ok(repository)
    }

    pub fn create_draft_pull_request(
        &self,
        token: &GitHubToken,
        request: &CreateDraftPullRequestRequest,
    ) -> Result<GitHubPullRequest, GitHubError> {
        validate_owner(&request.owner)?;
        validate_repository_name(&request.repository)?;
        validate_pull_request_fields(&request.title, &request.body, &request.head, &request.base)?;
        let path = format!(
            "/repos/{}/{}/pulls",
            request.owner.trim(),
            request.repository.trim()
        );
        let body = serde_json::json!({
            "title": request.title.trim(),
            "body": request.body.trim(),
            "head": request.head.trim(),
            "base": request.base.trim(),
            "draft": true,
        });
        let response = self.post_json(&self.api_base, &path, &body, Some(token))?;
        if !(200..300).contains(&response.status) {
            return Err(GitHubError::HttpStatus {
                status: response.status,
                body: truncate_error(&redact_secret(&response.body, Some(token))),
            });
        }
        parse_pull_request_response(&response.body)
    }

    pub fn pull_request(
        &self,
        token: &GitHubToken,
        owner: &str,
        repository: &str,
        number: u64,
    ) -> Result<GitHubPullRequest, GitHubError> {
        validate_owner(owner)?;
        validate_repository_name(repository)?;
        if number == 0 {
            return Err(GitHubError::InvalidPullRequest(
                "pull request number must be greater than zero".to_owned(),
            ));
        }
        let path = format!(
            "/repos/{owner}/{repository}/pulls/{number}",
            owner = owner.trim(),
            repository = repository.trim(),
        );
        let response = self.get_json(&path, Some(token))?;
        if !(200..300).contains(&response.status) {
            return Err(GitHubError::HttpStatus {
                status: response.status,
                body: truncate_error(&redact_secret(&response.body, Some(token))),
            });
        }
        parse_pull_request_response(&response.body)
    }

    pub fn pull_request_checks(
        &self,
        token: &GitHubToken,
        owner: &str,
        repository: &str,
        head_sha: &str,
    ) -> Result<Vec<GitHubCheckRun>, GitHubError> {
        validate_owner(owner)?;
        validate_repository_name(repository)?;
        validate_commit_sha(head_sha)?;
        let path = format!(
            "/repos/{owner}/{repository}/commits/{head_sha}/check-runs?filter=all&per_page=100",
            owner = owner.trim(),
            repository = repository.trim(),
            head_sha = head_sha.trim(),
        );
        let response = self.get_json(&path, Some(token))?;
        if !(200..300).contains(&response.status) {
            return Err(GitHubError::HttpStatus {
                status: response.status,
                body: truncate_error(&redact_secret(&response.body, Some(token))),
            });
        }
        parse_check_runs_response(&response.body)
    }

    pub fn pull_request_reviews(
        &self,
        token: &GitHubToken,
        owner: &str,
        repository: &str,
        number: u64,
    ) -> Result<Vec<GitHubPullRequestReview>, GitHubError> {
        validate_pull_request_number(number)?;
        let path = format!(
            "/repos/{owner}/{repository}/pulls/{number}/reviews?per_page=100",
            owner = owner.trim(),
            repository = repository.trim(),
        );
        let response = self.get_json(&path, Some(token))?;
        if !(200..300).contains(&response.status) {
            return Err(GitHubError::HttpStatus {
                status: response.status,
                body: truncate_error(&redact_secret(&response.body, Some(token))),
            });
        }
        parse_pull_request_reviews_response(&response.body)
    }

    pub fn pull_request_review_comments(
        &self,
        token: &GitHubToken,
        owner: &str,
        repository: &str,
        number: u64,
    ) -> Result<Vec<GitHubPullRequestReviewComment>, GitHubError> {
        validate_pull_request_number(number)?;
        let path = format!(
            "/repos/{owner}/{repository}/pulls/{number}/comments?per_page=100",
            owner = owner.trim(),
            repository = repository.trim(),
        );
        let response = self.get_json(&path, Some(token))?;
        if !(200..300).contains(&response.status) {
            return Err(GitHubError::HttpStatus {
                status: response.status,
                body: truncate_error(&redact_secret(&response.body, Some(token))),
            });
        }
        parse_pull_request_review_comments_response(&response.body)
    }

    fn post_json(
        &self,
        base: &str,
        path: &str,
        body: &Value,
        token: Option<&GitHubToken>,
    ) -> Result<GitHubHttpResponse, GitHubError> {
        let url = join_url(base, path);
        let mut request = self
            .agent
            .post(&url)
            .header("Accept", "application/json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {}", token.expose_secret()));
        }
        let mut response = request
            .send_json(body)
            .map_err(|error| GitHubError::Request(redact_secret(&error.to_string(), token)))?;
        read_response(&mut response, token)
    }

    fn get_json(
        &self,
        path: &str,
        token: Option<&GitHubToken>,
    ) -> Result<GitHubHttpResponse, GitHubError> {
        let url = join_url(&self.api_base, path);
        let mut request = self
            .agent
            .get(&url)
            .header("Accept", "application/json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {}", token.expose_secret()));
        }
        let mut response = request
            .call()
            .map_err(|error| GitHubError::Request(redact_secret(&error.to_string(), token)))?;
        read_response(&mut response, token)
    }
}

pub fn import_gh_token() -> Result<GitHubToken, GitHubError> {
    let mut command = Command::new("gh");
    command.args(["auth", "token", "--hostname", "github.com"]);
    hide_console_window(&mut command);
    let output = command.output().map_err(|_| GitHubError::GhUnavailable)?;
    if !output.status.success() {
        return Err(GitHubError::GhNotAuthenticated);
    }
    let token = String::from_utf8(output.stdout).map_err(|_| GitHubError::InvalidGhToken)?;
    GitHubToken::new(token).map_err(|_| GitHubError::InvalidGhToken)
}

#[derive(Debug)]
struct GitHubHttpResponse {
    status: u16,
    body: String,
    scopes: Option<String>,
}

fn read_response(
    response: &mut ureq::http::Response<ureq::Body>,
    token: Option<&GitHubToken>,
) -> Result<GitHubHttpResponse, GitHubError> {
    let status = response.status().as_u16();
    let scopes = response
        .headers()
        .get("x-oauth-scopes")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| GitHubError::Request(redact_secret(&error.to_string(), token)))?;
    Ok(GitHubHttpResponse {
        status,
        body,
        scopes,
    })
}

#[derive(Debug, Deserialize)]
struct RawDeviceFlowStart {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawDeviceFlowPoll {
    access_token: Option<String>,
    token_type: Option<String>,
    scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

fn normalize_base_url(value: &str) -> Result<String, GitHubError> {
    let value = value.trim().trim_end_matches('/');
    if !(value.starts_with("http://") || value.starts_with("https://")) {
        return Err(GitHubError::InvalidConfiguration(
            "endpoint must start with http:// or https://".to_owned(),
        ));
    }
    Ok(value.to_owned())
}

fn join_url(base: &str, path: &str) -> String {
    format!("{base}/{}", path.trim_start_matches('/'))
}

fn require_client_id(client_id: &str) -> Result<&str, GitHubError> {
    let client_id = client_id.trim();
    if client_id.is_empty() {
        Err(GitHubError::InvalidConfiguration(
            "OAuth App Client ID cannot be empty".to_owned(),
        ))
    } else {
        Ok(client_id)
    }
}

fn validate_repository_name(name: &str) -> Result<(), GitHubError> {
    let name = name.trim();
    let valid = !name.is_empty()
        && name.len() <= 100
        && name != "."
        && name != ".."
        && !name.starts_with('.')
        && !name.ends_with('.')
        && name.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        });
    if valid {
        Ok(())
    } else {
        Err(GitHubError::InvalidRepositoryName(name.to_owned()))
    }
}

fn validate_owner(owner: &str) -> Result<(), GitHubError> {
    let owner = owner.trim();
    let valid = !owner.is_empty()
        && owner.len() <= 100
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && owner
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-');
    if valid {
        Ok(())
    } else {
        Err(GitHubError::InvalidRepositoryName(owner.to_owned()))
    }
}

fn validate_pull_request_fields(
    title: &str,
    body: &str,
    head: &str,
    base: &str,
) -> Result<(), GitHubError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > MAX_PULL_REQUEST_TITLE_CHARS {
        return Err(GitHubError::InvalidPullRequest(format!(
            "title must contain between 1 and {MAX_PULL_REQUEST_TITLE_CHARS} characters"
        )));
    }
    if body.chars().count() > MAX_PULL_REQUEST_BODY_CHARS {
        return Err(GitHubError::InvalidPullRequest(format!(
            "body must be at most {MAX_PULL_REQUEST_BODY_CHARS} characters"
        )));
    }
    validate_branch_name(head.trim())
        .map_err(|_| GitHubError::InvalidPullRequest("head branch is invalid".to_owned()))?;
    validate_branch_name(base.trim())
        .map_err(|_| GitHubError::InvalidPullRequest("base branch is invalid".to_owned()))?;
    if head.trim() == base.trim() {
        return Err(GitHubError::InvalidPullRequest(
            "head and base branches must differ".to_owned(),
        ));
    }
    Ok(())
}

fn validate_pull_request_number(number: u64) -> Result<(), GitHubError> {
    if number == 0 {
        Err(GitHubError::InvalidPullRequest(
            "pull request number must be greater than zero".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn validate_commit_sha(value: &str) -> Result<(), GitHubError> {
    let value = value.trim();
    if (7..=64).contains(&value.len())
        && value.chars().all(|character| character.is_ascii_hexdigit())
    {
        Ok(())
    } else {
        Err(GitHubError::InvalidResponse(
            "commit SHA must contain 7 to 64 hexadecimal characters".to_owned(),
        ))
    }
}

fn validate_optional_https_url(field: &str, value: Option<&str>) -> Result<(), GitHubError> {
    let Some(value) = value else {
        return Ok(());
    };
    let value = value.trim();
    if value.is_empty()
        || !value.starts_with("https://")
        || value.chars().count() > MAX_PULL_REQUEST_URL_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(GitHubError::InvalidResponse(format!(
            "{field} must be a bounded HTTPS URL"
        )));
    }
    Ok(())
}

fn parse_pull_request_response(body: &str) -> Result<GitHubPullRequest, GitHubError> {
    let pull_request: GitHubPullRequest = serde_json::from_str(body)
        .map_err(|error| GitHubError::InvalidResponse(error.to_string()))?;
    if pull_request.number == 0
        || !pull_request.html_url.starts_with("https://")
        || pull_request.title.trim().is_empty()
        || pull_request.head.reference.trim().is_empty()
        || pull_request.base.reference.trim().is_empty()
        || !matches!(pull_request.state.as_str(), "open" | "closed")
    {
        return Err(GitHubError::InvalidResponse(
            "pull request response omitted required fields".to_owned(),
        ));
    }
    validate_commit_sha(&pull_request.head.sha)?;
    if let Some(state) = pull_request.mergeable_state.as_deref()
        && (state.chars().count() > 40 || state.chars().any(char::is_control))
    {
        return Err(GitHubError::InvalidResponse(
            "pull request mergeable state is invalid".to_owned(),
        ));
    }
    Ok(pull_request)
}

fn parse_check_runs_response(body: &str) -> Result<Vec<GitHubCheckRun>, GitHubError> {
    #[derive(Deserialize)]
    struct CheckRunsResponse {
        check_runs: Vec<GitHubCheckRun>,
    }

    let response: CheckRunsResponse = serde_json::from_str(body)
        .map_err(|error| GitHubError::InvalidResponse(error.to_string()))?;
    if response.check_runs.len() > MAX_PULL_REQUEST_INSIGHT_ITEMS {
        return Err(GitHubError::InvalidResponse(format!(
            "check run list exceeds {MAX_PULL_REQUEST_INSIGHT_ITEMS} items"
        )));
    }
    for check in &response.check_runs {
        if check.id == 0
            || check.name.trim().is_empty()
            || check.name.chars().count() > MAX_PULL_REQUEST_CHECK_NAME_CHARS
            || check.status.trim().is_empty()
        {
            return Err(GitHubError::InvalidResponse(
                "check run response omitted required fields".to_owned(),
            ));
        }
        validate_optional_https_url("check details URL", check.details_url.as_deref())?;
    }
    Ok(response.check_runs)
}

fn parse_pull_request_reviews_response(
    body: &str,
) -> Result<Vec<GitHubPullRequestReview>, GitHubError> {
    let reviews: Vec<GitHubPullRequestReview> = serde_json::from_str(body)
        .map_err(|error| GitHubError::InvalidResponse(error.to_string()))?;
    if reviews.len() > MAX_PULL_REQUEST_INSIGHT_ITEMS {
        return Err(GitHubError::InvalidResponse(format!(
            "review list exceeds {MAX_PULL_REQUEST_INSIGHT_ITEMS} items"
        )));
    }
    for review in &reviews {
        if review.id == 0 || review.state.trim().is_empty() {
            return Err(GitHubError::InvalidResponse(
                "review response omitted required fields".to_owned(),
            ));
        }
        validate_optional_https_url("review URL", review.html_url.as_deref())?;
    }
    Ok(reviews)
}

fn parse_pull_request_review_comments_response(
    body: &str,
) -> Result<Vec<GitHubPullRequestReviewComment>, GitHubError> {
    let comments: Vec<GitHubPullRequestReviewComment> = serde_json::from_str(body)
        .map_err(|error| GitHubError::InvalidResponse(error.to_string()))?;
    if comments.len() > MAX_PULL_REQUEST_INSIGHT_ITEMS {
        return Err(GitHubError::InvalidResponse(format!(
            "review comment list exceeds {MAX_PULL_REQUEST_INSIGHT_ITEMS} items"
        )));
    }
    for comment in &comments {
        if comment.id == 0 || comment.path.trim().is_empty() || comment.line == Some(0) {
            return Err(GitHubError::InvalidResponse(
                "review comment response omitted required fields".to_owned(),
            ));
        }
        validate_optional_https_url("review comment URL", comment.html_url.as_deref())?;
    }
    Ok(comments)
}

fn split_scopes(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_owned)
        .collect()
}

fn redact_secret(value: &str, token: Option<&GitHubToken>) -> String {
    match token {
        Some(token) => value.replace(token.expose_secret(), "***"),
        None => value.to_owned(),
    }
}

fn truncate_error(value: &str) -> String {
    value.chars().take(MAX_ERROR_BODY_CHARS).collect()
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

#[derive(Debug, Error)]
pub enum GitHubError {
    #[error("invalid GitHub configuration: {0}")]
    InvalidConfiguration(String),
    #[error("GitHub token is invalid")]
    InvalidToken,
    #[error("GitHub CLI is unavailable")]
    GhUnavailable,
    #[error("GitHub CLI is not authenticated")]
    GhNotAuthenticated,
    #[error("GitHub CLI returned an invalid token")]
    InvalidGhToken,
    #[error("invalid GitHub repository name: {0}")]
    InvalidRepositoryName(String),
    #[error("unsupported GitHub remote: {0}")]
    UnsupportedRemote(String),
    #[error("invalid pull request: {0}")]
    InvalidPullRequest(String),
    #[error("GitHub request failed: {0}")]
    Request(String),
    #[error("GitHub returned HTTP {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("GitHub response was invalid: {0}")]
    InvalidResponse(String),
    #[error("GitHub device authorization failed ({code}): {description}")]
    DeviceFlow { code: String, description: String },
}

impl GitHubError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfiguration(_) => "github_config_invalid",
            Self::InvalidToken | Self::InvalidGhToken => "github_token_invalid",
            Self::GhUnavailable => "github_cli_unavailable",
            Self::GhNotAuthenticated => "github_cli_not_authenticated",
            Self::InvalidRepositoryName(_) => "github_repository_name_invalid",
            Self::UnsupportedRemote(_) => "github_remote_unsupported",
            Self::InvalidPullRequest(_) => "pull_request_invalid",
            Self::Request(_) | Self::HttpStatus { .. } => "github_request_failed",
            Self::InvalidResponse(_) => "github_response_invalid",
            Self::DeviceFlow { .. } => "github_device_flow_failed",
        }
    }
}
