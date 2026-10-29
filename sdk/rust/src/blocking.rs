//! The blocking facade: no async runtime, no caller-written executor.
//!
//! [`Client`] is a thin transport around the generated request types. It uses
//! `ureq`, keeps one `Agent` for connection pooling, and exposes the same
//! retry, problem-details and refresh behaviour as the async facade.

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::auth::{AuthState, RefreshHook, lock};
use crate::config::ClientConfig;
use crate::error::{Error, Result};
use crate::generated::endpoints::PostV1PoolLogin;
use crate::generated::types::{JsonValue, PoolLoginRequest, PoolLoginResponse};
use crate::request::{self, CallOptions, PreparedRequest, RawResponse, is_success};
use crate::retry::RetryPolicy;
use crate::route::{Endpoint, RouteInfo};
use crate::stream::{ChatFrame, ChatStream};

/// A blocking Tidepool client.
///
/// The client is `Send + Sync`; credentials can be changed between calls from
/// any thread. It never asks the caller to create a runtime.
pub struct Client {
    config: ClientConfig,
    agent: ureq::Agent,
    auth: std::sync::Mutex<AuthState>,
    refresh: Option<RefreshHook>,
}

impl Client {
    /// Build a client from a validated configuration.
    pub fn new(config: ClientConfig) -> Result<Self> {
        let agent = ureq::AgentBuilder::new().timeout(config.timeout()).build();
        let auth = AuthState::from_config(&config);
        Ok(Self {
            config,
            agent,
            auth: std::sync::Mutex::new(auth),
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

    /// Install a refresh hook, called at most once per request.
    ///
    /// The hook is consulted proactively when the known token lifetime has
    /// passed and reactively on a `401`. Returning `None` lets the `401`
    /// surface, so a stale credential cannot loop.
    #[must_use]
    pub fn with_refresh(mut self, hook: RefreshHook) -> Self {
        self.refresh = Some(hook);
        self
    }

    /// Call one typed endpoint, with the client's default options.
    pub fn call<E>(&self, request: &E) -> Result<E::Response>
    where
        E: Endpoint,
        E::Response: DeserializeOwned,
    {
        self.call_with(request, &CallOptions::new())
    }

    /// Call one typed endpoint with per-call overrides.
    pub fn call_with<E>(&self, request: &E, options: &CallOptions) -> Result<E::Response>
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
    }

    /// Call one operation by id, returning the raw JSON body.
    ///
    /// This is the generic escape hatch for callers that prefer the route table
    /// to the typed request types; every contract route is reachable this way.
    pub fn request_json(
        &self,
        operation_id: &str,
        path_params: &[(&str, &str)],
        query: &[(&str, &str)],
        body: Option<&JsonValue>,
    ) -> Result<JsonValue> {
        self.request_json_with(operation_id, path_params, query, body, &CallOptions::new())
    }

    /// [`Self::request_json`] with per-call overrides.
    pub fn request_json_with(
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
    }

    /// Sign in and adopt the returned token and lifetime.
    pub fn login(
        &self,
        email: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<PoolLoginResponse> {
        let response = self.call(&PostV1PoolLogin::new(PoolLoginRequest {
            email: email.into(),
            password: password.into(),
        }))?;
        let expires_in = u64::try_from(response.expires_in)
            .ok()
            .filter(|seconds| *seconds > 0)
            .map(Duration::from_secs);
        lock(&self.auth).set_token(Some(response.token.clone()), expires_in);
        Ok(response)
    }

    /// Build the typed live-stream handle for one room.
    ///
    /// No socket is opened. The handle carries the token in its URL because the
    /// pool's socket route expects it there; treat the whole URL as a secret.
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
    fn perform<T>(
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
                self.refresh();
            }
            let prepared = {
                let auth = lock(&self.auth);
                prepare(&auth)?
            };
            let response = self.send(&prepared, timeout)?;
            if is_success(response.status) {
                return decode(&response);
            }
            let retry_after = crate::problem::retry_after_ms(
                response.headers.get("retry-after").map(String::as_str),
            );
            if response.status == 401 && !refresh_attempted {
                refresh_attempted = true;
                if self.refresh() {
                    continue;
                }
            }
            if response.status == 429 && attempt < retry.max_retries {
                if let Ok(delay) = retry.delay(attempt, retry_after) {
                    std::thread::sleep(delay);
                    attempt += 1;
                    continue;
                }
            }
            return Err(request::problem_from(route, &response, attempt + 1).into());
        }
    }

    /// Apply the refresh hook, at most once per request.
    fn refresh(&self) -> bool {
        let Some(hook) = &self.refresh else {
            return false;
        };
        match hook() {
            Ok(Some(update)) => {
                lock(&self.auth).apply(update);
                true
            }
            Ok(None) | Err(_) => false,
        }
    }

    /// One transport attempt.
    fn send(&self, prepared: &PreparedRequest, timeout: Duration) -> Result<RawResponse> {
        let mut request = self
            .agent
            .request(prepared.method, &prepared.url)
            .timeout(timeout);
        for (name, value) in &prepared.headers {
            request = request.set(name, value);
        }
        let result = match &prepared.body {
            Some(body) => request.send_bytes(body),
            None => request.call(),
        };
        match result {
            Ok(response) | Err(ureq::Error::Status(_, response)) => raw_response(response),
            Err(error) => Err(Error::Transport(Box::new(error))),
        }
    }
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

impl Clone for Client {
    fn clone(&self) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(self.config.timeout())
            .build();
        Self {
            config: self.config.clone(),
            agent,
            auth: std::sync::Mutex::new(lock(&self.auth).clone()),
            refresh: self.refresh.clone(),
        }
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

/// Convert a `ureq` response into the transport-neutral shape.
fn raw_response(response: ureq::Response) -> Result<RawResponse> {
    let status = response.status();
    let mut headers = std::collections::HashMap::new();
    for name in response.headers_names() {
        let values = response.all(&name);
        if !values.is_empty() {
            headers.insert(name.to_ascii_lowercase(), values.join(", "));
        }
    }
    let body = response
        .into_string()
        .map_err(|error| Error::Transport(Box::new(error)))?;
    Ok(RawResponse {
        status,
        headers,
        body: body.into_bytes(),
    })
}

impl ChatStream {
    /// Open the socket and iterate typed frames until it closes.
    ///
    /// The iterator yields [`ChatFrame`] values, tracks the resume cursor, and
    /// ends after `chat.error` or a close frame. A transport failure is yielded
    /// once as an [`Error::Transport`] and then ends.
    pub fn connect(&self) -> Result<FrameIter> {
        let (socket, _response) =
            tungstenite::connect(self.url()).map_err(|error| Error::Transport(Box::new(error)))?;
        Ok(FrameIter {
            socket,
            cursor: self.after(),
            finished: false,
        })
    }
}

/// An open chatroom socket, yielding typed frames.
#[derive(Debug)]
pub struct FrameIter {
    socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    cursor: Option<u64>,
    finished: bool,
}

impl FrameIter {
    /// The highest sequence delivered so far.
    pub fn cursor(&self) -> Option<u64> {
        self.cursor
    }
}

impl Iterator for FrameIter {
    type Item = Result<ChatFrame>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        loop {
            match self.socket.read() {
                Ok(tungstenite::Message::Text(text)) => {
                    let frame = ChatFrame::parse(&text);
                    if let Some(cursor) = frame.cursor() {
                        self.cursor = Some(cursor);
                    }
                    if matches!(frame, ChatFrame::Error { .. }) {
                        self.finished = true;
                    }
                    return Some(Ok(frame));
                }
                Ok(tungstenite::Message::Binary(data)) => {
                    return Some(Ok(ChatFrame::Binary(data)));
                }
                Ok(tungstenite::Message::Close(_)) => {
                    self.finished = true;
                    return None;
                }
                Ok(tungstenite::Message::Ping(payload)) => {
                    if let Err(error) = self.socket.send(tungstenite::Message::Pong(payload)) {
                        self.finished = true;
                        return Some(Err(Error::Transport(Box::new(error))));
                    }
                }
                Ok(tungstenite::Message::Pong(_)) | Ok(tungstenite::Message::Frame(_)) => {}
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    self.finished = true;
                    return None;
                }
                Err(error) => {
                    self.finished = true;
                    return Some(Err(Error::Transport(Box::new(error))));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::TokenUpdate;
    use crate::generated::endpoints::GetV1ChatroomsRoomStream;

    #[test]
    fn stream_url_requires_and_carries_a_token() {
        let client = Client::from_base_url("https://pool.example.com").expect("client");
        let error = client
            .stream(&GetV1ChatroomsRoomStream::new("room_1"))
            .expect_err("no token");
        assert!(matches!(error, Error::Usage(_)));

        client.set_token("jwt");
        let stream = client
            .stream(&GetV1ChatroomsRoomStream::new("room_1"))
            .expect("stream");
        assert_eq!(
            stream.url(),
            "wss://pool.example.com/v1/chatrooms/room_1/stream?token=jwt"
        );
        assert_eq!(stream.after(), None);
    }

    #[test]
    fn stream_after_is_passed_through() {
        let client = Client::from_base_url("http://localhost:8080")
            .expect("client")
            .with_token("jwt");
        let request = GetV1ChatroomsRoomStream::new("a b").with_query(
            crate::generated::endpoints::GetV1ChatroomsRoomStreamQuery {
                token: None,
                after: Some(9),
            },
        );
        let stream = client.stream(&request).expect("stream");
        assert_eq!(
            stream.url(),
            "ws://localhost:8080/v1/chatrooms/a%20b/stream?after=9&token=jwt"
        );
        assert_eq!(stream.after(), Some(9));
    }

    #[test]
    fn only_http_urls_can_become_websockets() {
        assert_eq!(
            websocket_url("https://pool.example.com/x").expect("url"),
            "wss://pool.example.com/x"
        );
        assert_eq!(
            websocket_url("http://localhost/x").expect("url"),
            "ws://localhost/x"
        );
        assert!(websocket_url("ftp://pool/x").is_err());
    }

    #[test]
    fn credentials_can_be_changed_between_calls() {
        let client = Client::from_base_url("http://localhost:8080").expect("client");
        assert_eq!(client.token(), None);
        client.set_token("a");
        assert_eq!(client.token().as_deref(), Some("a"));
        client.set_token("b");
        assert_eq!(client.token().as_deref(), Some("b"));
        client.clear_token();
        assert_eq!(client.token(), None);
    }

    #[test]
    fn clone_keeps_credentials_but_not_a_secret_in_debug() {
        let client = Client::from_base_url("http://localhost:8080")
            .expect("client")
            .with_token("top-secret");
        let cloned = client.clone();
        assert_eq!(cloned.token().as_deref(), Some("top-secret"));
        let rendered = format!("{client:?}");
        assert!(rendered.contains("authenticated: true"));
        assert!(!rendered.contains("top-secret"));
    }

    #[test]
    fn proactive_refresh_runs_at_most_once() {
        let client = Client::from_base_url("http://localhost:8080")
            .expect("client")
            .with_token("old")
            .with_refresh(std::sync::Arc::new(|| {
                Ok(Some(TokenUpdate::new("new").with_expiry(Duration::ZERO)))
            }));
        assert!(client.refresh());
        assert_eq!(client.token().as_deref(), Some("new"));
        assert!(lock(&client.auth).expired());
    }
}
