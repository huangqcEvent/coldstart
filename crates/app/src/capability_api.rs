use std::{path::PathBuf, sync::Arc, time::Instant};

use chrono::Utc;
use coldstart_adapters::{
    CapabilityApiClient, CapabilityApiError as HttpError, CapabilityApiHttpRequest, SecretStore,
    SecretStoreError, UreqCapabilityApiClient, WindowsSecretStore,
};
use coldstart_core::{
    BlueprintEntityId, CapabilityApiEndpointId, CapabilityApiMethod, CapabilityApiValidationError,
    CapabilityStatus, CredentialEnvironment, MAX_CAPABILITY_API_REQUEST_BODY_BYTES, TaskKind,
    WorkItemId,
};
use serde::{Deserialize, Serialize};

use crate::{AppError, context::OpenProject};

const MAX_CONTENT_TYPE_CHARS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityApiQueryParameter {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CallCapabilityApiRequest {
    pub capability_id: BlueprintEntityId,
    pub environment: String,
    pub endpoint_id: CapabilityApiEndpointId,
    pub work_id: WorkItemId,
    pub lease_token: String,
    pub method: CapabilityApiMethod,
    pub path: String,
    #[serde(default)]
    pub query: Vec<CapabilityApiQueryParameter>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityApiCallView {
    pub endpoint_id: CapabilityApiEndpointId,
    pub capability_id: BlueprintEntityId,
    pub environment: String,
    pub method: CapabilityApiMethod,
    pub path: String,
    pub status: u16,
    pub content_type: Option<String>,
    pub body: String,
    pub truncated: bool,
    pub duration_ms: u64,
    pub redacted: bool,
}

#[derive(Clone)]
pub struct CapabilityApiService {
    secrets: Arc<dyn SecretStore>,
    client: Arc<dyn CapabilityApiClient>,
}

impl CapabilityApiService {
    pub fn new() -> Self {
        Self::with_dependencies(
            Arc::new(WindowsSecretStore::new()),
            Arc::new(UreqCapabilityApiClient::new()),
        )
    }

    pub fn with_dependencies(
        secrets: Arc<dyn SecretStore>,
        client: Arc<dyn CapabilityApiClient>,
    ) -> Self {
        Self { secrets, client }
    }

    pub fn call(
        &self,
        project_home: PathBuf,
        request: CallCapabilityApiRequest,
    ) -> Result<CapabilityApiCallView, AppError> {
        let CallCapabilityApiRequest {
            capability_id,
            environment,
            endpoint_id,
            work_id,
            lease_token,
            method,
            path,
            query: requested_query,
            body: requested_body,
            content_type: requested_content_type,
        } = request;
        let mut project = OpenProject::open(project_home)?;
        let authorization =
            project
                .database
                .authorize_work_readonly(&work_id, &lease_token, Utc::now())?;
        if authorization.task_spec.kind != TaskKind::AgentCoding {
            return Err(AppError::CapabilityApiNotAuthorized(
                "work item is not an agent coding task".to_owned(),
            ));
        }
        if !authorization
            .task_spec
            .context_refs
            .contains(&capability_id)
        {
            return Err(AppError::CapabilityApiNotAuthorized(
                "task contract does not authorize this capability".to_owned(),
            ));
        }

        let capability = project
            .database
            .list_capability_states(&project.project.id)?
            .into_iter()
            .find(|state| state.capability_id == capability_id && state.environment == environment)
            .ok_or_else(|| AppError::CapabilityApiUnavailable {
                capability_id: capability_id.to_string(),
                environment: environment.clone(),
            })?;
        if !matches!(
            capability.status,
            CapabilityStatus::Configured | CapabilityStatus::Verified
        ) {
            return Err(AppError::CapabilityApiUnavailable {
                capability_id: capability_id.to_string(),
                environment,
            });
        }

        let endpoint = project
            .database
            .capability_api_endpoint(&project.project.id, &endpoint_id)?
            .filter(|endpoint| {
                endpoint.capability_id == capability_id && endpoint.environment == environment
            })
            .ok_or(AppError::CapabilityApiEndpointNotFound(endpoint_id))?;
        endpoint
            .authorize_request(method, &path)
            .map_err(|error| match error {
                CapabilityApiValidationError::MethodNotAllowed(method) => {
                    AppError::CapabilityApiMethodNotAllowed(method.to_string())
                }
                CapabilityApiValidationError::PathNotAllowed(path)
                | CapabilityApiValidationError::InvalidPath(path) => {
                    AppError::CapabilityApiPathNotAllowed(path)
                }
                other => AppError::InvalidCapabilityApiEndpoint(other),
            })?;
        let query = requested_query
            .into_iter()
            .map(|item| (item.key, item.value))
            .collect::<Vec<_>>();
        let url = endpoint
            .resolve_url(&path, &query)
            .map_err(AppError::InvalidCapabilityApiEndpoint)?;

        let body = requested_body.map(String::into_bytes);
        if let Some(body) = body.as_ref()
            && body.len() > MAX_CAPABILITY_API_REQUEST_BODY_BYTES
        {
            return Err(AppError::CapabilityApiRequestTooLarge {
                actual: body.len(),
                limit: MAX_CAPABILITY_API_REQUEST_BODY_BYTES,
            });
        }

        let mut headers = Vec::new();
        let mut redaction_terms = Vec::new();
        if let Some(credential_id) = endpoint.credential_id {
            let credential = project
                .database
                .credential(&project.project.id, &credential_id)?
                .ok_or(AppError::CapabilityApiCredentialMissing)?;
            if credential.environment != CredentialEnvironment::Shared
                && credential.environment.as_str() != endpoint.environment
            {
                return Err(AppError::CapabilityApiCredentialMismatch);
            }
            let auth_header = endpoint
                .auth_header
                .as_deref()
                .ok_or(AppError::CapabilityApiCredentialMissing)?;
            let secret = self
                .secrets
                .get(&credential.secret_ref)
                .map_err(|error| match error {
                    SecretStoreError::NotFound(_) | SecretStoreError::EmptyReference => {
                        AppError::CapabilityApiCredentialMissing
                    }
                    SecretStoreError::EmptySecret | SecretStoreError::Backend(_) => {
                        AppError::CapabilityApiRequestFailed
                    }
                })?;
            if secret.is_empty()
                || secret
                    .chars()
                    .any(|character| matches!(character, '\r' | '\n' | '\0'))
            {
                return Err(AppError::CapabilityApiCredentialMissing);
            }
            let header_value = match endpoint.auth_scheme.as_deref() {
                Some(scheme) => format!("{scheme} {secret}"),
                None => secret.clone(),
            };
            redaction_terms.push(header_value.clone());
            redaction_terms.push(secret);
            headers.push((auth_header.to_owned(), header_value));
        }
        if let Some(content_type) = requested_content_type.as_deref() {
            let content_type = content_type.trim();
            if !content_type.is_empty()
                && content_type.is_ascii()
                && content_type.chars().count() <= MAX_CONTENT_TYPE_CHARS
                && !content_type.chars().any(char::is_control)
            {
                headers.push(("Content-Type".to_owned(), content_type.to_owned()));
            } else if !content_type.is_empty() {
                return Err(AppError::CapabilityApiRequestFailed);
            }
        }

        let started = Instant::now();
        let response = self
            .client
            .execute(&CapabilityApiHttpRequest {
                url: url.to_string(),
                method,
                headers,
                body,
            })
            .map_err(map_http_error)?;
        let duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let (body, mut redacted) = redact_text(&response.body, &redaction_terms);
        let content_type = match response.content_type {
            Some(value) => {
                let (value, content_type_redacted) =
                    redact_text(value.as_bytes(), &redaction_terms);
                redacted |= content_type_redacted;
                Some(value)
            }
            None => None,
        };
        Ok(CapabilityApiCallView {
            endpoint_id: endpoint.id,
            capability_id: endpoint.capability_id,
            environment: endpoint.environment,
            method,
            path,
            status: response.status,
            content_type,
            body,
            truncated: response.truncated,
            duration_ms,
            redacted,
        })
    }
}

impl Default for CapabilityApiService {
    fn default() -> Self {
        Self::new()
    }
}

fn map_http_error(error: HttpError) -> AppError {
    match error {
        HttpError::RedirectNotAllowed => AppError::CapabilityApiRedirectNotAllowed,
        HttpError::Timeout => AppError::CapabilityApiTimeout,
        HttpError::InvalidUrl
        | HttpError::HttpsRequired
        | HttpError::CredentialsInUrl
        | HttpError::FragmentNotAllowed
        | HttpError::Request(_) => AppError::CapabilityApiRequestFailed,
    }
}

fn redact_text(input: &[u8], terms: &[String]) -> (String, bool) {
    let mut text = String::from_utf8_lossy(input).into_owned();
    let mut redacted = false;
    for term in terms {
        if !term.is_empty() && text.contains(term) {
            text = text.replace(term, "[REDACTED]");
            redacted = true;
        }
    }
    (text, redacted)
}
