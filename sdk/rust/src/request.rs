//! Request assembly shared by the blocking and async facades.

use std::collections::HashMap;
use std::time::Duration;

use crate::auth::AuthState;
use crate::config::ClientConfig;
use crate::error::Result;
use crate::problem::ProblemError;
use crate::retry::RetryPolicy;
use crate::route::{Endpoint, RouteInfo, encode_component, path_with};

/// Per-call overrides for one request.
#[derive(Debug, Clone, Default)]
pub struct CallOptions {
    /// Correlation id sent as `x-request-id`.
    pub request_id: Option<String>,
    /// Extra headers, applied after the client's and the credentials'.
    pub headers: Vec<(String, String)>,
    /// Per-call timeout; defaults to the client's timeout.
    pub timeout: Option<Duration>,
    /// Per-call retry policy; defaults to the client's policy.
    pub retry: Option<RetryPolicy>,
}

impl CallOptions {
    /// No overrides.
    pub fn new() -> Self {
        Self::default()
    }

    /// Send this correlation id.
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }

    /// Add one header.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Override the timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Override the retry policy.
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = Some(retry);
        self
    }

    /// Send this request once, with no retry.
    pub fn without_retry(mut self) -> Self {
        self.retry = Some(RetryPolicy::none());
        self
    }

    /// The effective timeout.
    pub(crate) fn timeout(&self, config: &ClientConfig) -> Duration {
        self.timeout.unwrap_or_else(|| config.timeout())
    }

    /// The effective retry policy.
    pub(crate) fn retry(&self, config: &ClientConfig) -> RetryPolicy {
        self.retry.unwrap_or_else(|| config.retry())
    }
}

/// A request ready for one transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedRequest {
    pub(crate) method: &'static str,
    pub(crate) url: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Option<Vec<u8>>,
}

/// One HTTP response, before the SDK's problem/JSON handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawResponse {
    pub(crate) status: u16,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) body: Vec<u8>,
}

/// Assemble a request from a typed endpoint.
pub(crate) fn prepare<E: Endpoint>(
    config: &ClientConfig,
    auth: &AuthState,
    request: &E,
    options: &CallOptions,
) -> Result<PreparedRequest> {
    let query = request.query();
    let body = request.body()?;
    Ok(prepare_parts(
        config,
        auth,
        E::ROUTE,
        request.path(),
        &query,
        body,
        options,
    ))
}

/// Assemble a request from raw path and query pairs, for generic dispatch.
#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_raw(
    config: &ClientConfig,
    auth: &AuthState,
    route: &'static RouteInfo,
    path_params: &[(String, String)],
    query: &[(String, String)],
    body: Option<Vec<u8>>,
    options: &CallOptions,
) -> PreparedRequest {
    let pairs: Vec<(&str, &str)> = path_params
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    let path = path_with(route.path, &pairs);
    let query: Vec<(&str, String)> = query
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    prepare_parts(config, auth, route, path, &query, body, options)
}

/// Assemble a request from already-rendered parts.
fn prepare_parts(
    config: &ClientConfig,
    auth: &AuthState,
    route: &RouteInfo,
    path: String,
    query: &[(&str, String)],
    body: Option<Vec<u8>>,
    options: &CallOptions,
) -> PreparedRequest {
    let url = build_url(config.base_url(), &path, query);
    let mut headers: Vec<(String, String)> = Vec::new();
    set_header(&mut headers, "accept", "application/json");
    set_header(&mut headers, "user-agent", config.user_agent());
    for (name, value) in config.headers() {
        set_header(&mut headers, name, value);
    }
    for (name, value) in auth.headers() {
        set_header(&mut headers, &name, &value);
    }
    for (name, value) in &options.headers {
        set_header(&mut headers, name, value);
    }
    if let Some(request_id) = &options.request_id {
        set_header(&mut headers, "x-request-id", request_id);
    }
    if body.is_some() && !headers.iter().any(|(name, _)| name == "content-type") {
        set_header(&mut headers, "content-type", "application/json");
    }
    PreparedRequest {
        method: route.method.as_str(),
        url,
        headers,
        body,
    }
}

/// Set a header, replacing any previous value with the same lowercased name.
fn set_header(headers: &mut Vec<(String, String)>, name: &str, value: &str) {
    let lower = name.to_ascii_lowercase();
    if let Some(slot) = headers
        .iter_mut()
        .find(|(existing, _)| existing.eq_ignore_ascii_case(&lower))
    {
        slot.1 = value.to_owned();
    } else {
        headers.push((lower, value.to_owned()));
    }
}

/// Join the base URL, path and percent-encoded query pairs.
pub(crate) fn build_url(base_url: &str, path: &str, query: &[(&str, String)]) -> String {
    if query.is_empty() {
        return format!("{base_url}{path}");
    }
    let encoded = query
        .iter()
        .map(|(name, value)| format!("{}={}", encode_component(name), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{base_url}{path}?{encoded}")
}

/// True for a 2xx status.
pub(crate) fn is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

/// Parse the problem document of a failed response.
pub(crate) fn problem_from(
    route: &RouteInfo,
    response: &RawResponse,
    attempts: u32,
) -> ProblemError {
    ProblemError::from_parts(
        route.operation_id,
        response.status,
        &response.headers,
        &response.body,
        attempts,
    )
}

/// Decode a success response into the endpoint's typed body.
pub(crate) fn decode<E: Endpoint>(response: &RawResponse) -> Result<E::Response>
where
    E::Response: serde::de::DeserializeOwned,
{
    E::decode_response(&response.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::ROUTE_GET_V1_HOSTS_ID;

    fn config() -> ClientConfig {
        ClientConfig::new("https://pool.example.com")
            .expect("config")
            .with_token("jwt")
            .with_api_key("key")
            .with_header("X-Trace", "config")
    }

    fn auth() -> AuthState {
        AuthState::from_config(&config())
    }

    #[test]
    fn builds_an_encoded_url() {
        assert_eq!(
            build_url(
                "https://pool.example.com",
                "/v1/chatrooms/a/messages",
                &[("after", "1 2".to_owned()), ("limit", "5".to_owned())]
            ),
            "https://pool.example.com/v1/chatrooms/a/messages?after=1%202&limit=5"
        );
        assert_eq!(
            build_url("https://pool.example.com", "/v1/hosts", &[]),
            "https://pool.example.com/v1/hosts"
        );
    }

    #[test]
    fn call_options_override_the_headers_last() {
        let options = CallOptions::new()
            .with_header("x-trace", "call")
            .with_request_id("req_1");
        let request = prepare_raw(
            &config(),
            &auth(),
            &ROUTE_GET_V1_HOSTS_ID,
            &[("id".to_owned(), "a b".to_owned())],
            &[],
            None,
            &options,
        );
        assert_eq!(request.method, "GET");
        assert_eq!(request.url, "https://pool.example.com/v1/hosts/a%20b");
        assert!(
            request
                .headers
                .contains(&("authorization".to_owned(), "Bearer jwt".to_owned()))
        );
        assert!(
            request
                .headers
                .contains(&("x-api-key".to_owned(), "key".to_owned()))
        );
        assert!(
            request
                .headers
                .contains(&("x-trace".to_owned(), "call".to_owned()))
        );
        assert!(
            request
                .headers
                .contains(&("x-request-id".to_owned(), "req_1".to_owned()))
        );
        assert!(
            request
                .headers
                .contains(&("accept".to_owned(), "application/json".to_owned()))
        );
        assert_eq!(request.body, None);
    }

    #[test]
    fn a_body_adds_a_json_content_type_once() {
        let options = CallOptions::new();
        let request = prepare_raw(
            &config(),
            &auth(),
            &crate::routes::ROUTE_POST_V1_HOSTS,
            &[],
            &[],
            Some(br#"{"display_name":"box"}"#.to_vec()),
            &options,
        );
        let content_types = request
            .headers
            .iter()
            .filter(|(name, _)| name == "content-type")
            .count();
        assert_eq!(content_types, 1);
        assert!(
            request
                .headers
                .contains(&("content-type".to_owned(), "application/json".to_owned()))
        );
    }

    #[test]
    fn a_caller_content_type_wins() {
        let options = CallOptions::new().with_header("Content-Type", "application/problem+json");
        let request = prepare_raw(
            &config(),
            &auth(),
            &crate::routes::ROUTE_POST_V1_HOSTS,
            &[],
            &[],
            Some(b"{}".to_vec()),
            &options,
        );
        assert!(request.headers.contains(&(
            "content-type".to_owned(),
            "application/problem+json".to_owned()
        )));
    }

    #[test]
    fn retry_and_timeout_defaults_come_from_the_config() {
        let config = config();
        let options = CallOptions::new();
        assert_eq!(options.timeout(&config), Duration::from_secs(30));
        assert_eq!(options.retry(&config), config.retry());
        assert_eq!(
            options
                .clone()
                .with_timeout(Duration::from_secs(2))
                .timeout(&config),
            Duration::from_secs(2)
        );
        assert_eq!(
            options.clone().without_retry().retry(&config).max_retries,
            0
        );
    }
}
