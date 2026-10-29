//! Client configuration shared by both facades.

use std::time::Duration;

use crate::error::{Error, Result};
use crate::retry::{DEFAULT_RETRY, RetryPolicy};

/// Everything a client needs before the first request.
///
/// Build one with [`ClientConfig::new`] and the `with_*` setters, then hand it
/// to [`crate::blocking::Client::new`] or
/// [`crate::asynchronous::Client::new`].
#[derive(Debug, Clone)]
pub struct ClientConfig {
    base_url: String,
    token: Option<String>,
    token_expiry: Option<Duration>,
    api_key: Option<String>,
    host_credential: Option<String>,
    headers: Vec<(String, String)>,
    retry: RetryPolicy,
    timeout: Duration,
    user_agent: String,
}

impl ClientConfig {
    /// Validate and adopt a pool base URL.
    ///
    /// The URL must be absolute `http` or `https`; a trailing slash is removed
    /// so every request path joins cleanly.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        let base_url = normalise_base_url(&base_url.into())?;
        Ok(Self {
            base_url,
            token: None,
            token_expiry: None,
            api_key: None,
            host_credential: None,
            headers: Vec::new(),
            retry: DEFAULT_RETRY,
            timeout: Duration::from_secs(30),
            user_agent: format!("tidepool-client/{}", env!("CARGO_PKG_VERSION")),
        })
    }

    /// Start a builder for a base URL.
    pub fn builder(base_url: impl Into<String>) -> ClientConfigBuilder {
        ClientConfigBuilder::new(base_url)
    }

    /// Send an operator JWT as `Authorization: Bearer`.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// How long the configured token is usable, when the server said so.
    pub fn with_token_expiry(mut self, expiry: Duration) -> Self {
        self.token_expiry = Some(expiry);
        self
    }

    /// Send an API key as `x-api-key`.
    pub fn with_api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Send a host relationship credential as `X-Host-Credential`.
    pub fn with_host_credential(mut self, credential: impl Into<String>) -> Self {
        self.host_credential = Some(credential.into());
        self
    }

    /// Add a header sent with every request.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Replace the retry policy.
    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Replace the per-request timeout (default 30 seconds).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Replace the `User-Agent` header.
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// The absolute pool URL, without a trailing slash.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The configured operator token, if any.
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// The configured token lifetime, if any.
    pub fn token_expiry(&self) -> Option<Duration> {
        self.token_expiry
    }

    /// The configured API key, if any.
    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// The configured host credential, if any.
    pub fn host_credential(&self) -> Option<&str> {
        self.host_credential.as_deref()
    }

    /// Headers sent with every request.
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// The retry policy.
    pub fn retry(&self) -> RetryPolicy {
        self.retry
    }

    /// The per-request timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// The `User-Agent` header.
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }
}

/// A mutable builder for [`ClientConfig`].
///
/// The setters mirror [`ClientConfig`]'s; [`ClientConfigBuilder::build`]
/// validates the base URL captured at construction.
#[derive(Debug, Clone)]
pub struct ClientConfigBuilder {
    base_url: String,
    token: Option<String>,
    token_expiry: Option<Duration>,
    api_key: Option<String>,
    host_credential: Option<String>,
    headers: Vec<(String, String)>,
    retry: RetryPolicy,
    timeout: Duration,
    user_agent: String,
}

impl ClientConfigBuilder {
    /// Start with a base URL.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            token: None,
            token_expiry: None,
            api_key: None,
            host_credential: None,
            headers: Vec::new(),
            retry: DEFAULT_RETRY,
            timeout: Duration::from_secs(30),
            user_agent: format!("tidepool-client/{}", env!("CARGO_PKG_VERSION")),
        }
    }

    /// Send an operator JWT as `Authorization: Bearer`.
    pub fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// How long the configured token is usable.
    pub fn token_expiry(mut self, expiry: Duration) -> Self {
        self.token_expiry = Some(expiry);
        self
    }

    /// Send an API key as `x-api-key`.
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// Send a host credential as `X-Host-Credential`.
    pub fn host_credential(mut self, credential: impl Into<String>) -> Self {
        self.host_credential = Some(credential.into());
        self
    }

    /// Add a header sent with every request.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// Replace the retry policy.
    pub fn retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// Replace the per-request timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Replace the `User-Agent`.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Finish, validating the base URL captured at construction.
    pub fn build(self) -> Result<ClientConfig> {
        Ok(ClientConfig {
            base_url: normalise_base_url(&self.base_url)?,
            token: self.token,
            token_expiry: self.token_expiry,
            api_key: self.api_key,
            host_credential: self.host_credential,
            headers: self.headers,
            retry: self.retry,
            timeout: self.timeout,
            user_agent: self.user_agent,
        })
    }
}

/// Validate a base URL and remove trailing slashes.
fn normalise_base_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(Error::Config("base URL is required".to_owned()));
    }
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .ok_or_else(|| {
            Error::Config(format!(
                "base URL `{trimmed}` must be absolute http or https"
            ))
        })?;
    if rest.is_empty() || rest.starts_with('/') {
        return Err(Error::Config(format!("base URL `{trimmed}` has no host")));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(Error::Config(format!(
            "base URL `{trimmed}` contains whitespace"
        )));
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_trailing_slashes() {
        let config = ClientConfig::new("https://pool.example.com///").expect("config");
        assert_eq!(config.base_url(), "https://pool.example.com");
    }

    #[test]
    fn rejects_relative_and_hostless_urls() {
        assert!(ClientConfig::new("pool.example.com").is_err());
        assert!(ClientConfig::new("ftp://pool.example.com").is_err());
        assert!(ClientConfig::new("https://").is_err());
        assert!(ClientConfig::new("   ").is_err());
        assert!(ClientConfig::new("https://pool example.com").is_err());
    }

    #[test]
    fn default_user_agent_names_the_crate() {
        let config = ClientConfig::new("http://localhost:8080").expect("config");
        assert!(config.user_agent().starts_with("tidepool-client/"));
        assert_eq!(config.timeout(), Duration::from_secs(30));
        assert_eq!(config.retry(), DEFAULT_RETRY);
    }

    #[test]
    fn builder_round_trips_credentials_and_headers() {
        let config = ClientConfigBuilder::new("http://localhost:8080/")
            .token("jwt")
            .api_key("key")
            .host_credential("host-secret")
            .header("x-trace", "1")
            .timeout(Duration::from_secs(5))
            .build()
            .expect("config");
        assert_eq!(config.base_url(), "http://localhost:8080");
        assert_eq!(config.token(), Some("jwt"));
        assert_eq!(config.api_key(), Some("key"));
        assert_eq!(config.host_credential(), Some("host-secret"));
        assert_eq!(config.headers(), &[("x-trace".to_owned(), "1".to_owned())]);
        assert_eq!(config.timeout(), Duration::from_secs(5));
    }

    #[test]
    fn builder_rejects_an_invalid_url_even_after_new() {
        let error = ClientConfigBuilder::new("not-a-url")
            .build()
            .expect_err("must fail");
        assert!(matches!(error, Error::Config(_)));
    }
}
