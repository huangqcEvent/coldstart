use std::{fmt, process::Command, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

const DEFAULT_API_BASE: &str = "https://api.github.com";
const DEFAULT_OAUTH_BASE: &str = "https://github.com";
const GITHUB_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ERROR_BODY_CHARS: usize = 600;

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
            Self::Request(_) | Self::HttpStatus { .. } => "github_request_failed",
            Self::InvalidResponse(_) => "github_response_invalid",
            Self::DeviceFlow { .. } => "github_device_flow_failed",
        }
    }
}
