//! 受限能力 API 的 HTTPS 客户端。
//!
//! 客户端禁止重定向、限制请求和响应大小、仅允许 HTTPS，并在测试中显式开启本地
//! HTTP；错误与调试输出不得泄露鉴权头或 Secret。

use std::{fmt, io::Read, time::Duration};

use coldstart_core::{
    CapabilityApiMethod, MAX_CAPABILITY_API_REQUEST_BODY_BYTES, MAX_CAPABILITY_API_RESPONSE_BYTES,
};
use thiserror::Error;
use url::{Host, Url};

const CAPABILITY_API_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct CapabilityApiHttpRequest {
    pub url: String,
    pub method: CapabilityApiMethod,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

impl fmt::Debug for CapabilityApiHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapabilityApiHttpRequest")
            .field("url", &self.url)
            .field("method", &self.method)
            .field(
                "headers",
                &self
                    .headers
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>(),
            )
            .field("body_bytes", &self.body.as_ref().map_or(0, Vec::len))
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityApiHttpResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
    pub truncated: bool,
}

pub trait CapabilityApiClient: Send + Sync {
    fn execute(
        &self,
        request: &CapabilityApiHttpRequest,
    ) -> Result<CapabilityApiHttpResponse, CapabilityApiError>;
}

#[derive(Debug, Error)]
pub enum CapabilityApiError {
    #[error("capability API URL is invalid")]
    InvalidUrl,
    #[error("capability API URL must use HTTPS")]
    HttpsRequired,
    #[error("capability API URL must not contain embedded credentials")]
    CredentialsInUrl,
    #[error("capability API URL must not contain a fragment")]
    FragmentNotAllowed,
    #[error("capability API redirects are not allowed")]
    RedirectNotAllowed,
    #[error("capability API request timed out")]
    Timeout,
    #[error("capability API request failed: {0}")]
    Request(String),
}

#[derive(Clone)]
pub struct UreqCapabilityApiClient {
    agent: ureq::Agent,
    allow_insecure_localhost: bool,
}

impl fmt::Debug for UreqCapabilityApiClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UreqCapabilityApiClient")
            .field("allow_insecure_localhost", &self.allow_insecure_localhost)
            .finish_non_exhaustive()
    }
}

impl UreqCapabilityApiClient {
    pub fn new() -> Self {
        Self::with_test_network_access(false)
    }

    pub fn for_testing() -> Self {
        Self::with_test_network_access(true)
    }

    fn with_test_network_access(allow_insecure_localhost: bool) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(CAPABILITY_API_TIMEOUT))
            .max_redirects(0)
            .max_redirects_will_error(true)
            .http_status_as_error(false)
            .https_only(!allow_insecure_localhost)
            .build();
        Self {
            agent: config.into(),
            allow_insecure_localhost,
        }
    }

    fn validate_url(&self, value: &str) -> Result<Url, CapabilityApiError> {
        let url = Url::parse(value).map_err(|_| CapabilityApiError::InvalidUrl)?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err(CapabilityApiError::CredentialsInUrl);
        }
        if url.fragment().is_some() {
            return Err(CapabilityApiError::FragmentNotAllowed);
        }
        let host = url.host().ok_or(CapabilityApiError::InvalidUrl)?;
        match url.scheme() {
            "https" => {
                if !host_is_public(host) {
                    return Err(CapabilityApiError::InvalidUrl);
                }
            }
            "http" if self.allow_insecure_localhost && host_is_loopback(host) => {}
            "http" => return Err(CapabilityApiError::HttpsRequired),
            _ => return Err(CapabilityApiError::HttpsRequired),
        }
        Ok(url)
    }
}

impl Default for UreqCapabilityApiClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CapabilityApiClient for UreqCapabilityApiClient {
    fn execute(
        &self,
        request: &CapabilityApiHttpRequest,
    ) -> Result<CapabilityApiHttpResponse, CapabilityApiError> {
        if request
            .body
            .as_ref()
            .is_some_and(|body| body.len() > MAX_CAPABILITY_API_REQUEST_BODY_BYTES)
        {
            return Err(CapabilityApiError::Request(
                "request body exceeds the limit".to_owned(),
            ));
        }
        let url = self.validate_url(&request.url)?;
        let method = ureq::http::Method::from_bytes(request.method.as_str().as_bytes())
            .map_err(|_| CapabilityApiError::InvalidUrl)?;
        let mut builder = ureq::http::Request::builder()
            .method(method)
            .uri(url.as_str());
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        let http_request = builder
            .body(())
            .map_err(|_| CapabilityApiError::Request("invalid request headers".to_owned()))?;
        let (parts, _) = http_request.into_parts();
        let response = if let Some(body) = request.body.as_deref() {
            self.agent
                .run(ureq::http::Request::from_parts(parts, body.to_vec()))
        } else {
            self.agent.run(ureq::http::Request::from_parts(parts, ()))
        }
        .map_err(map_transport_error)?;
        finish_response(response)
    }
}

fn finish_response(
    mut response: ureq::http::Response<ureq::Body>,
) -> Result<CapabilityApiHttpResponse, CapabilityApiError> {
    let status = response.status().as_u16();
    if (300..400).contains(&status) {
        return Err(CapabilityApiError::RedirectNotAllowed);
    }
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let read_limit = u64::try_from(MAX_CAPABILITY_API_RESPONSE_BYTES + 1).unwrap_or(u64::MAX);
    let mut body = Vec::with_capacity(MAX_CAPABILITY_API_RESPONSE_BYTES.min(16 * 1024));
    response
        .body_mut()
        .with_config()
        .limit(read_limit)
        .reader()
        .take(read_limit)
        .read_to_end(&mut body)
        .map_err(|error| map_transport_error(error.into()))?;
    let truncated = body.len() > MAX_CAPABILITY_API_RESPONSE_BYTES;
    body.truncate(MAX_CAPABILITY_API_RESPONSE_BYTES);
    Ok(CapabilityApiHttpResponse {
        status,
        content_type,
        body,
        truncated,
    })
}

fn map_transport_error(error: ureq::Error) -> CapabilityApiError {
    match error {
        ureq::Error::Timeout(_) => CapabilityApiError::Timeout,
        ureq::Error::TooManyRedirects | ureq::Error::RedirectFailed => {
            CapabilityApiError::RedirectNotAllowed
        }
        ureq::Error::RequireHttpsOnly(_) => CapabilityApiError::HttpsRequired,
        ureq::Error::BadUri(_) | ureq::Error::HostNotFound => CapabilityApiError::InvalidUrl,
        other => CapabilityApiError::Request(other.to_string()),
    }
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

fn host_is_loopback(host: Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(address) => address.is_loopback(),
        Host::Ipv6(address) => address.is_loopback(),
    }
}
