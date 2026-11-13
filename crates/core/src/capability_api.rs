use std::{collections::BTreeSet, fmt};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::{Host, Url};

use crate::{
    BlueprintEntityId, CapabilityApiEndpointId, CredentialId, ProjectId,
    validate_capability_environment,
};

pub const MAX_CAPABILITY_API_NAME_CHARS: usize = 120;
pub const MAX_CAPABILITY_API_BASE_URL_CHARS: usize = 2_048;
pub const MAX_CAPABILITY_API_PATH_CHARS: usize = 512;
pub const MAX_CAPABILITY_API_AUTH_HEADER_CHARS: usize = 64;
pub const MAX_CAPABILITY_API_AUTH_SCHEME_CHARS: usize = 40;
pub const MAX_CAPABILITY_API_QUERY_ITEMS: usize = 32;
pub const MAX_CAPABILITY_API_QUERY_KEY_CHARS: usize = 128;
pub const MAX_CAPABILITY_API_QUERY_VALUE_CHARS: usize = 1_024;
pub const MAX_CAPABILITY_API_REQUEST_BODY_BYTES: usize = 256 * 1024;
pub const MAX_CAPABILITY_API_RESPONSE_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum CapabilityApiMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl CapabilityApiMethod {
    pub const ALL: [Self; 5] = [Self::Get, Self::Post, Self::Put, Self::Patch, Self::Delete];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "GET" => Some(Self::Get),
            "POST" => Some(Self::Post),
            "PUT" => Some(Self::Put),
            "PATCH" => Some(Self::Patch),
            "DELETE" => Some(Self::Delete),
            _ => None,
        }
    }
}

impl fmt::Display for CapabilityApiMethod {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityApiEndpointInput {
    pub capability_id: BlueprintEntityId,
    pub environment: String,
    pub name: String,
    pub base_url: String,
    pub path_prefix: String,
    pub allowed_methods: Vec<CapabilityApiMethod>,
    pub credential_id: Option<CredentialId>,
    #[serde(default)]
    pub auth_header: Option<String>,
    #[serde(default)]
    pub auth_scheme: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityApiEndpoint {
    pub id: CapabilityApiEndpointId,
    pub project_id: ProjectId,
    pub capability_id: BlueprintEntityId,
    pub environment: String,
    pub name: String,
    pub base_url: String,
    pub path_prefix: String,
    pub allowed_methods: Vec<CapabilityApiMethod>,
    pub credential_id: Option<CredentialId>,
    pub auth_header: Option<String>,
    pub auth_scheme: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl CapabilityApiEndpoint {
    pub fn new(
        project_id: ProjectId,
        input: CapabilityApiEndpointInput,
    ) -> Result<Self, CapabilityApiValidationError> {
        let now = Utc::now();
        let mut endpoint = Self {
            id: CapabilityApiEndpointId::new(),
            project_id,
            capability_id: input.capability_id.clone(),
            environment: input.environment.clone(),
            name: input.name.clone(),
            base_url: input.base_url.clone(),
            path_prefix: input.path_prefix.clone(),
            allowed_methods: input.allowed_methods.clone(),
            credential_id: input.credential_id,
            auth_header: input.auth_header.clone(),
            auth_scheme: input.auth_scheme.clone(),
            created_at: now,
            updated_at: now,
        };
        endpoint.apply(input)?;
        Ok(endpoint)
    }

    pub fn update_from(
        &mut self,
        input: CapabilityApiEndpointInput,
    ) -> Result<(), CapabilityApiValidationError> {
        if input.capability_id != self.capability_id {
            return Err(CapabilityApiValidationError::CapabilityCannotChange);
        }
        self.apply(input)?;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn validate(&self) -> Result<(), CapabilityApiValidationError> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(CapabilityApiValidationError::EmptyName);
        }
        if name.chars().count() > MAX_CAPABILITY_API_NAME_CHARS {
            return Err(CapabilityApiValidationError::NameTooLong);
        }
        validate_capability_environment(&self.environment).map_err(|_| {
            CapabilityApiValidationError::UnsupportedEnvironment(self.environment.clone())
        })?;
        validate_base_url(&self.base_url)?;
        validate_path(&self.path_prefix, true)?;
        if self.allowed_methods.is_empty() {
            return Err(CapabilityApiValidationError::NoAllowedMethods);
        }
        let unique = self
            .allowed_methods
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if unique.len() != self.allowed_methods.len() {
            return Err(CapabilityApiValidationError::DuplicateAllowedMethod);
        }
        match (self.credential_id, self.auth_header.as_deref()) {
            (Some(_), Some(header)) => validate_auth_header(header)?,
            (Some(_), None) => return Err(CapabilityApiValidationError::AuthHeaderRequired),
            (None, Some(_)) => {
                return Err(CapabilityApiValidationError::CredentialRequiredForAuthHeader);
            }
            (None, None) => {}
        }
        if let Some(scheme) = self.auth_scheme.as_deref() {
            validate_auth_scheme(scheme)?;
        }
        Ok(())
    }

    pub fn authorize_request(
        &self,
        method: CapabilityApiMethod,
        path: &str,
    ) -> Result<(), CapabilityApiValidationError> {
        if !self.allowed_methods.contains(&method) {
            return Err(CapabilityApiValidationError::MethodNotAllowed(method));
        }
        self.validate_request_path(path)
    }

    pub fn resolve_url(
        &self,
        path: &str,
        query: &[(String, String)],
    ) -> Result<Url, CapabilityApiValidationError> {
        self.validate_request_path(path)?;
        validate_query(query)?;
        let base = self.base_url.trim().trim_end_matches('/');
        let mut url = Url::parse(&format!("{base}{path}"))
            .map_err(|_| CapabilityApiValidationError::InvalidBaseUrl)?;
        if url.query().is_some() || url.fragment().is_some() {
            return Err(CapabilityApiValidationError::BaseUrlMustNotContainQuery);
        }
        if !query.is_empty() {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
        }
        Ok(url)
    }

    fn validate_request_path(&self, path: &str) -> Result<(), CapabilityApiValidationError> {
        validate_path(path, true)?;
        if !path_is_within_prefix(&self.path_prefix, path) {
            return Err(CapabilityApiValidationError::PathNotAllowed(
                path.to_owned(),
            ));
        }
        Ok(())
    }

    fn apply(
        &mut self,
        input: CapabilityApiEndpointInput,
    ) -> Result<(), CapabilityApiValidationError> {
        let base_url = input.base_url.trim().trim_end_matches('/').to_owned();
        let path_prefix = normalize_path_prefix(&input.path_prefix)?;
        let auth_header = normalize_optional(input.auth_header);
        let auth_scheme = normalize_optional(input.auth_scheme);
        let allowed_methods = input.allowed_methods;
        self.capability_id = input.capability_id;
        self.environment = input.environment.trim().to_owned();
        self.name = input.name.trim().to_owned();
        self.base_url = base_url;
        self.path_prefix = path_prefix;
        self.allowed_methods = allowed_methods;
        self.credential_id = input.credential_id;
        self.auth_header = auth_header;
        self.auth_scheme = auth_scheme;
        self.validate()?;
        self.allowed_methods.sort();
        self.allowed_methods.dedup();
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapabilityApiValidationError {
    #[error("capability API name cannot be empty")]
    EmptyName,
    #[error("capability API name must be at most {MAX_CAPABILITY_API_NAME_CHARS} characters")]
    NameTooLong,
    #[error("unsupported capability API environment: {0}")]
    UnsupportedEnvironment(String),
    #[error("capability API base URL must be an absolute HTTPS URL")]
    InvalidBaseUrl,
    #[error("capability API base URL must not contain credentials")]
    BaseUrlContainsCredentials,
    #[error("capability API base URL must not contain a query or fragment")]
    BaseUrlMustNotContainQuery,
    #[error("capability API host is not allowed: {0}")]
    DisallowedHost(String),
    #[error("capability API path is invalid: {0}")]
    InvalidPath(String),
    #[error("capability API path is outside the configured prefix: {0}")]
    PathNotAllowed(String),
    #[error("capability API method is not allowed: {0}")]
    MethodNotAllowed(CapabilityApiMethod),
    #[error("at least one capability API method must be allowed")]
    NoAllowedMethods,
    #[error("capability API methods must be unique")]
    DuplicateAllowedMethod,
    #[error("a credential requires an authentication header")]
    AuthHeaderRequired,
    #[error("an authentication header requires a credential")]
    CredentialRequiredForAuthHeader,
    #[error("invalid authentication header name: {0}")]
    InvalidAuthHeader(String),
    #[error("authentication header is reserved: {0}")]
    ReservedAuthHeader(String),
    #[error("invalid authentication scheme: {0}")]
    InvalidAuthScheme(String),
    #[error("the capability of an endpoint cannot be changed")]
    CapabilityCannotChange,
    #[error("capability API query contains too many entries")]
    TooManyQueryEntries,
    #[error("capability API query key or value is too long")]
    QueryEntryTooLong,
    #[error("capability API query key cannot be empty")]
    EmptyQueryKey,
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn normalize_path_prefix(value: &str) -> Result<String, CapabilityApiValidationError> {
    let value = value.trim();
    let value = if value.is_empty() { "/" } else { value };
    validate_path(value, true)?;
    if value != "/" {
        Ok(value.trim_end_matches('/').to_owned())
    } else {
        Ok("/".to_owned())
    }
}

fn validate_base_url(value: &str) -> Result<(), CapabilityApiValidationError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_CAPABILITY_API_BASE_URL_CHARS {
        return Err(CapabilityApiValidationError::InvalidBaseUrl);
    }
    let url = Url::parse(value).map_err(|_| CapabilityApiValidationError::InvalidBaseUrl)?;
    if url.scheme() != "https" {
        return Err(CapabilityApiValidationError::InvalidBaseUrl);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(CapabilityApiValidationError::BaseUrlContainsCredentials);
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(CapabilityApiValidationError::BaseUrlMustNotContainQuery);
    }
    let host = url
        .host()
        .ok_or(CapabilityApiValidationError::InvalidBaseUrl)?;
    if !host_is_public(host) {
        return Err(CapabilityApiValidationError::DisallowedHost(
            url.host_str().unwrap_or_default().to_owned(),
        ));
    }
    validate_path(url.path(), true)
}

fn host_is_public(host: Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => {
            let domain = domain.trim_end_matches('.').to_ascii_lowercase();
            !domain.is_empty()
                && domain != "localhost"
                && !domain.ends_with(".localhost")
                && !domain.ends_with(".local")
        }
        Host::Ipv4(address) => {
            !(address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_broadcast()
                || address.is_documentation()
                || address.is_unspecified()
                || address.is_multicast())
        }
        Host::Ipv6(address) => {
            !(address.is_loopback()
                || address.is_unique_local()
                || address.is_unicast_link_local()
                || address.is_unspecified()
                || address.is_multicast())
        }
    }
}

fn validate_path(value: &str, allow_root: bool) -> Result<(), CapabilityApiValidationError> {
    if value.chars().count() > MAX_CAPABILITY_API_PATH_CHARS
        || !value.starts_with('/')
        || (!allow_root && value == "/")
        || value.contains("//")
        || value.contains('\\')
        || value.contains('?')
        || value.contains('#')
        || value
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(CapabilityApiValidationError::InvalidPath(value.to_owned()));
    }
    let lower = value.to_ascii_lowercase();
    if lower.contains("%2e") || lower.contains("%2f") || lower.contains("%5c") {
        return Err(CapabilityApiValidationError::InvalidPath(value.to_owned()));
    }
    for segment in value.split('/').skip(1) {
        if segment == "." || segment == ".." {
            return Err(CapabilityApiValidationError::InvalidPath(value.to_owned()));
        }
    }
    Ok(())
}

fn path_is_within_prefix(prefix: &str, path: &str) -> bool {
    prefix == "/"
        || path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn validate_auth_header(value: &str) -> Result<(), CapabilityApiValidationError> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_CAPABILITY_API_AUTH_HEADER_CHARS
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || "!#$%&'*+-.^_`|~".contains(character)
        })
    {
        return Err(CapabilityApiValidationError::InvalidAuthHeader(
            value.to_owned(),
        ));
    }
    if matches!(
        value.to_ascii_lowercase().as_str(),
        "host"
            | "content-length"
            | "transfer-encoding"
            | "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "upgrade"
            | "content-type"
            | "accept"
            | "user-agent"
            | "cookie"
            | "set-cookie"
    ) {
        return Err(CapabilityApiValidationError::ReservedAuthHeader(
            value.to_owned(),
        ));
    }
    Ok(())
}

fn validate_auth_scheme(value: &str) -> Result<(), CapabilityApiValidationError> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_CAPABILITY_API_AUTH_SCHEME_CHARS
        || value.chars().any(|character| character.is_control())
    {
        return Err(CapabilityApiValidationError::InvalidAuthScheme(
            value.to_owned(),
        ));
    }
    Ok(())
}

fn validate_query(query: &[(String, String)]) -> Result<(), CapabilityApiValidationError> {
    if query.len() > MAX_CAPABILITY_API_QUERY_ITEMS {
        return Err(CapabilityApiValidationError::TooManyQueryEntries);
    }
    for (key, value) in query {
        if key.is_empty() {
            return Err(CapabilityApiValidationError::EmptyQueryKey);
        }
        if key.chars().count() > MAX_CAPABILITY_API_QUERY_KEY_CHARS
            || value.chars().count() > MAX_CAPABILITY_API_QUERY_VALUE_CHARS
            || key
                .chars()
                .chain(value.chars())
                .any(|character| character.is_control())
        {
            return Err(CapabilityApiValidationError::QueryEntryTooLong);
        }
    }
    Ok(())
}
