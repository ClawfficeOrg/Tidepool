//! The async facade: the same contract over `reqwest`.
//!
//! This feature is opt-in. It needs a Tokio runtime because `reqwest` needs
//! one; the blocking facade does not. No runtime is created here, so a caller
//! chooses the executor (`#[tokio::main]`, `Runtime::block_on`, …).

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::auth::{AsyncRefreshHook, AuthState, lock};
use crate::config::ClientConfig;
use crate::error::{Error, Result};
use crate::generated::endpoints::PostV1PoolLogin;
use crate::generated::types::{JsonValue, PoolLoginRequest, PoolLoginResponse};
use crate::request::{self, CallOptions, PreparedRequest, RawResponse, is_success};
use crate::retry::RetryPolicy;
use crate::route::{Endpoint, RouteInfo};
use crate::stream::ChatStream;

/// An async Tidepool client.
///
/// The client is `Send + Sync` and cheap to clone; every call is one
/// `reqwest::RequestBuilder`. It does not create a runtime and does not require
/// one to be constructed, only to be driven.
#[derive(Clone)]
pub struct Client {
    config: ClientConfig,
    http: reqwest::Client,
    auth: std::sync::Arc<std::sync::Mutex<AuthState>>,
    refresh: Option<AsyncRefreshHook>,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Client")
            .field("base_url", &self.config.base_url())
            .field("authenticated", &lock(&self.auth).token().is_some())
            .finish()
    }
}

impl Client {
    /// Build a client from a validated configuration.
    pub fn new(config: ClientConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(config.timeout())
            .build()
            .map_err(|error| Error::Transport(Box::new(error)))?;
        let auth = AuthState::from_config(&config);
        Ok(Self {
            config,
            http,
            auth: std::sync::Arc::new(std::sync::Mutex::new(auth)),
            refresh: None,
        })
    }

    /// Build a client for a base URL with the default configuration.
    pub fn from_base_url(base_url: impl Into<String>) -> Result<Self> {
        Self::new(ClientConfig::new(base_url)?)
    }

    /// The configuration this client was built from.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// The pool's base URL.
    pub fn base_url(&self) -> &str {
        self.config.base_url()
    }

    /// Send an operator token with every subsequent request.
    pub fn set_token(&self, token: impl Into<String>) {
        lock(&self.auth).set_token(Some(token.into()), None);
    }

    /// Send an operator token and remember how long it is usable.
    pub fn set_token_with_expiry(&self, token: impl Into<String>, expires_in: Duration) {
        lock(&self.auth).set_token(Some(token.into()), Some(expires_in));
    }

    /// Forget the operator token and its expiry.
    pub fn clear_token(&self) {
        lock(&self.auth).set_token(None, None);
    }

    /// The current operator token, if one is set.
    pub fn token(&self) -> Option<String> {
        lock(&self.auth).token().map(str::to_owned)
    }

    /// Replace the API key sent as `x-api-key`.
    pub fn set_api_key(&self, api_key: impl Into<String>) {
        lock(&self.auth).set_api_key(Some(api_key.into()));
    }

    /// Replace the host credential sent as `X-Host-Credential`.
    pub fn set_host_credential(&self, credential: impl Into<String>) {
        lock(&self.auth).set_host_credential(Some(credential.into()));
    }

    /// Adopt a token now, for use during construction.
    #[must_use]
    pub fn with_token(self, token: impl Into<String>) -> Self {
        self.set_token(token);
        self
    }

    /// Install an async refresh hook, called at most once per request.
    #[must_use]
    pub fn with_refresh(mut self, hook: AsyncRefreshHook) -> Self {
        self.refresh = Some(hook);
        self
    }

    /// Call one typed endpoint.
    pub async fn call<E>(&self, request: &E) -> Result<E::Response>
    where
        E: Endpoint,
        E::Response: DeserializeOwned,
    {
        self.call_with(request, &CallOptions::new()).await
    }

    /// Call one typed endpoint with per-call overrides.
    pub async fn call_with<E>(&self, request: &E, options: &CallOptions) -> Result<E::Response>
    where
        E: Endpoint,
        E::Response: DeserializeOwned,
    {
        let route = E::ROUTE;
        self.perform(
            route,
            options,
            |auth| request::prepare(&self.config, auth, request, options),
            |response| request::decode::<E>(response),
        )
        .await
    }

    /// Call one operation by id, returning the raw JSON body.
    pub async fn request_json(
        &self,
        operation_id: &str,
        path_params: &[(&str, &str)],
        query: &[(&str, &str)],
        body: Option<&JsonValue>,
    ) -> Result<JsonValue> {
        self.request_json_with(operation_id, path_params, query, body, &CallOptions::new())
            .await
    }

    /// [`Self::request_json`] with per-call overrides.
    pub async fn request_json_with(
        &self,
        operation_id: &str,
        path_params: &[(&str, &str)],
        query: &[(&str, &str)],
        body: Option<&JsonValue>,
        options: &CallOptions,
    ) -> Result<JsonValue> {
        let route = crate::describe(operation_id)?;
        let path_params: Vec<(String, String)> = path_params
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        let query: Vec<(String, String)> = query
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        let body = body.map(serde_json::to_vec).transpose()?;
        self.perform(
            route,
            options,
            |auth| {
                Ok(request::prepare_raw(
                    &self.config,
                    auth,
                    route,
                    &path_params,
                    &query,
                    body.clone(),
                    options,
                ))
            },
            |response| {
                let payload: &[u8] = if response.body.is_empty() {
                    b"null"
                } else {
                    &response.body
                };
                serde_json::from_slice(payload).map_err(|source| Error::Decode {
                    operation_id: route.operation_id,
                    source,
                })
            },
        )
        .await
    }

    /// Sign in and adopt the returned token and lifetime.
    pub async fn login(
        &self,
        email: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<PoolLoginResponse> {
        let response = self
            .call(&PostV1PoolLogin::new(PoolLoginRequest {
                email: email.into(),
                password: password.into(),
            }))
            .await?;
        let expires_in = u64::try_from(response.expires_in)
            .ok()
            .filter(|seconds| *seconds > 0)
            .map(Duration::from_secs);
        lock(&self.auth).set_token(Some(response.token.clone()), expires_in);
        Ok(response)
    }

    /// Build the typed live-stream handle for one room.
    ///
    /// No socket is opened; the async facade leaves the socket to the caller's
    /// WebSocket library so it does not force another dependency. The URL
    /// carries the token because the pool's socket route expects it there.
    pub fn stream<E: Endpoint>(&self, request: &E) -> Result<ChatStream> {
        let query = request.query();
        let token = query
            .iter()
            .find(|(name, _)| *name == "token")
            .map(|(_, value)| value.clone())
            .or_else(|| lock(&self.auth).token().map(str::to_owned))
            .ok_or_else(|| {
                Error::Usage(format!(
                    "{}: a token is required for the live stream",
                    E::ROUTE.operation_id
                ))
            })?;
        let mut query = query;
        if !query.iter().any(|(name, _)| *name == "token") {
            query.push(("token", token.clone()));
        }
        let after = query
            .iter()
            .find(|(name, _)| *name == "after")
            .and_then(|(_, value)| value.parse::<u64>().ok());
        let http_url = request::build_url(self.config.base_url(), &request.path(), &query);
        let url = websocket_url(&http_url)?;
        Ok(ChatStream::new(url, token, after))
    }

    /// Call one route with bounded retries, refresh-once and problem mapping.
    async fn perform<T>(
        &self,
        route: &'static RouteInfo,
        options: &CallOptions,
        mut prepare: impl FnMut(&AuthState) -> Result<PreparedRequest>,
        mut decode: impl FnMut(&RawResponse) -> Result<T>,
    ) -> Result<T> {
        let retry: RetryPolicy = options.retry(&self.config);
        let timeout = options.timeout(&self.config);
        let mut refresh_attempted = false;
        let mut attempt: u32 = 0;
        loop {
            if !refresh_attempted && lock(&self.auth).expired() {
                refresh_attempted = true;
                self.refresh().await;
            }
            let prepared = {
                let auth = lock(&self.auth);
                prepare(&auth)?
            };
            let response = self.send(&prepared, timeout).await?;
            if is_success(response.status) {
                return decode(&response);
            }
            let retry_after = crate::problem::retry_after_ms(
                response.headers.get("retry-after").map(String::as_str),
            );
            if response.status == 401 && !refresh_attempted {
                refresh_attempted = true;
                if self.refresh().await {
                    continue;
                }
            }
            if response.status == 429 && attempt < retry.max_retries {
                if let Ok(delay) = retry.delay(attempt, retry_after) {
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                    continue;
                }
            }
            return Err(request::problem_from(route, &response, attempt + 1).into());
        }
    }

    /// Apply the refresh hook, at most once per request.
    async fn refresh(&self) -> bool {
        let Some(hook) = &self.refresh else {
            return false;
        };
        match hook().await {
            Ok(Some(update)) => {
                lock(&self.auth).apply(update);
                true
            }
            Ok(None) | Err(_) => false,
        }
    }

    /// One transport attempt.
    async fn send(&self, prepared: &PreparedRequest, timeout: Duration) -> Result<RawResponse> {
        let method = reqwest::Method::from_bytes(prepared.method.as_bytes()).map_err(|error| {
            Error::Usage(format!("invalid method `{}`: {error}", prepared.method))
        })?;
        let mut builder = self.http.request(method, &prepared.url).timeout(timeout);
        for (name, value) in &prepared.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = &prepared.body {
            builder = builder.body(body.clone());
        }
        let response = builder
            .send()
            .await
            .map_err(|error| Error::Transport(Box::new(error)))?;
        let status = response.status().as_u16();
        let mut headers = std::collections::HashMap::new();
        for (name, value) in response.headers() {
            let value = value.to_str().unwrap_or_default().to_owned();
            headers.insert(name.as_str().to_ascii_lowercase(), value);
        }
        let body = response
            .bytes()
            .await
            .map_err(|error| Error::Transport(Box::new(error)))?;
        Ok(RawResponse {
            status,
            headers,
            body: body.to_vec(),
        })
    }
}

/// Turn an absolute `http(s)` URL into its `ws(s)` twin.
fn websocket_url(url: &str) -> Result<String> {
    if let Some(rest) = url.strip_prefix("https://") {
        return Ok(format!("wss://{rest}"));
    }
    if let Some(rest) = url.strip_prefix("http://") {
        return Ok(format!("ws://{rest}"));
    }
    Err(Error::Usage(format!(
        "cannot open a WebSocket for non-http URL `{url}`"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenUpdate;
    use crate::generated::endpoints::{GetV1ChatroomsRoomStream, GetV1ChatroomsRoomStreamQuery};

    #[test]
    fn stream_url_requires_and_carries_a_token() {
        let client = Client::from_base_url("https://pool.example.com").expect("client");
        assert!(
            client
                .stream(&GetV1ChatroomsRoomStream::new("room_1"))
                .is_err()
        );
        client.set_token("jwt");
        let stream = client
            .stream(&GetV1ChatroomsRoomStream::new("room_1"))
            .expect("stream");
        assert_eq!(
            stream.url(),
            "wss://pool.example.com/v1/chatrooms/room_1/stream?token=jwt"
        );
    }

    #[test]
    fn stream_after_is_typed() {
        let client = Client::from_base_url("http://localhost:8080")
            .expect("client")
            .with_token("jwt");
        let request =
            GetV1ChatroomsRoomStream::new("room").with_query(GetV1ChatroomsRoomStreamQuery {
                token: None,
                after: Some(4),
            });
        let stream = client.stream(&request).expect("stream");
        assert_eq!(stream.after(), Some(4));
        assert_eq!(
            stream.url(),
            "ws://localhost:8080/v1/chatrooms/room/stream?after=4&token=jwt"
        );
    }

    #[test]
    fn only_http_urls_can_become_websockets() {
        assert!(websocket_url("ftp://pool/x").is_err());
    }

    #[tokio::test]
    async fn async_refresh_hook_is_awaited_once() {
        let client = Client::from_base_url("http://localhost:8080")
            .expect("client")
            .with_token("old")
            .with_refresh(std::sync::Arc::new(|| {
                Box::pin(async { Ok(Some(TokenUpdate::new("new").with_expiry(Duration::ZERO))) })
            }));
        assert!(client.refresh().await);
        assert_eq!(client.token().as_deref(), Some("new"));
    }
}
