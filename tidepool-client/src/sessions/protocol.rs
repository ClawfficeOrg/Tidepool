//! The host session protocol, version 1.
//!
//! A session runs on this machine, not in the pool: the local host agent owns
//! the process, its working directory and its transcript. This module is the
//! client's whole view of that agent, one method per request, so a scripted
//! implementation in the tests has the same shape as [`HttpHostSessions`] and
//! cannot accidentally share logic with it.
//!
//! # Routes
//!
//! | Method | Path | Meaning |
//! |---|---|---|
//! | `GET` | `/v1/sessions` | every session the host is running or remembers |
//! | `POST` | `/v1/sessions` | start one in a project directory |
//! | `GET` | `/v1/sessions/{id}/events?after=&limit=` | the timeline after an exclusive cursor |
//! | `POST` | `/v1/sessions/{id}/input` | prompt, steer or queue one input |
//! | `POST` | `/v1/sessions/{id}/abort` | stop the session |
//!
//! The vocabulary is the pool's, not a new one: a session id is the host
//! execution's public id, event kinds are the relay's (`started`, `status`,
//! `output`, `tool`, `message`, `stopped`), and a status is one of `queued`,
//! `running`, `waiting`, `succeeded`, `failed` or `interrupted`. The host
//! protocol's `interrupt_execution` is what an abort becomes, and its
//! `execution_output` is what an `output` event carries.
//!
//! # The credential
//!
//! The host agent listens on loopback and is addressed with this machine's
//! relationship credential in `X-Host-Credential`. [`HttpHostSessions`]'s
//! `Debug` redacts it, no error variant carries it and nothing here logs a
//! request body.
//!
//! # Status is never guessed
//!
//! [`SessionStatus::parse`] maps a wire value it does not know to
//! [`SessionStatus::Unknown`], and only `succeeded` answers
//! [`SessionStatus::is_complete`]. An `interrupted` session is terminal but
//! deliberately not complete, which is what keeps it from being drawn as a
//! finished one.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// How long a connect to the local host may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// How long a read from the local host may take.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// The largest page a timeline read asks for.
pub const MAX_EVENT_PAGE: u32 = 500;
/// The longest input the host accepts, in characters.
pub const MAX_INPUT_CHARS: usize = 32_000;

/// What could not be done with the local host.
///
/// No variant carries the host credential or a request body.
#[derive(Debug, Error)]
pub enum SessionError {
    /// The host URL is not an `http`/`https` URL with a host.
    #[error("`{url}` is not a usable host URL (expected http:// or https:// and a host)")]
    InvalidHostUrl {
        /// The rejected input.
        url: String,
    },
    /// The host agent could not be reached.
    #[error("the host agent could not be reached: {detail}")]
    Unreachable {
        /// The transport's description.
        detail: String,
    },
    /// The host answered `401`: this machine's credential is not accepted.
    #[error("the host agent did not accept this machine's credential")]
    Unauthorized,
    /// The host answered `403`: this action is not allowed.
    #[error("the host agent refused this action: {detail}")]
    Forbidden {
        /// The problem detail, when the host sent one.
        detail: String,
    },
    /// The host answered with an error problem document.
    #[error("the host agent answered {status} ({code}){detail}")]
    Problem {
        /// HTTP status.
        status: u16,
        /// The problem code, or `unknown` when absent.
        code: String,
        /// The problem detail, empty when absent.
        detail: String,
    },
    /// A response was not the documented shape, or a value was unusable.
    #[error("the host agent's answer was not understood: {detail}")]
    Protocol {
        /// What was wrong.
        detail: String,
    },
    /// The request was refused before it was sent.
    #[error("{detail}")]
    InvalidRequest {
        /// What the caller has to change.
        detail: String,
    },
    /// The client has no session by that id.
    #[error("no session `{id}` is known to this client")]
    UnknownSession {
        /// The id that was asked for.
        id: String,
    },
    /// The client has not opened a session.
    #[error("no session is open")]
    NoSessionSelected,
}

/// A session's lifecycle, as the host reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    /// Accepted and waiting for the agent.
    Queued,
    /// The agent is working.
    Running,
    /// The agent is idle and the session can take a follow-up.
    Waiting,
    /// The agent finished normally.
    Succeeded,
    /// The agent stopped with an error.
    Failed,
    /// The session was stopped, by a person or by the host, before it finished.
    Interrupted,
    /// A status this build does not know; never read as complete.
    Unknown(String),
}

impl Default for SessionStatus {
    fn default() -> Self {
        Self::Unknown(String::new())
    }
}

impl SessionStatus {
    /// Parse the host's wire value.
    pub fn parse(value: &str) -> Self {
        match value.trim() {
            "queued" => Self::Queued,
            "running" => Self::Running,
            "waiting" | "waiting_for_input" | "idle" => Self::Waiting,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "interrupted" | "stopped" => Self::Interrupted,
            other => Self::Unknown(other.to_owned()),
        }
    }

    /// The host's wire value.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Unknown(value) => value,
        }
    }

    /// The one-word label the client shows.
    pub fn label(&self) -> &str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Unknown(value) if value.is_empty() => "unknown",
            Self::Unknown(value) => value,
        }
    }

    /// The sentence the status line carries.
    ///
    /// An interrupted session says so in words; it never borrows the wording
    /// of a finished one.
    pub fn note(&self) -> &str {
        match self {
            Self::Queued => "queued — waiting for the agent",
            Self::Running => "running",
            Self::Waiting => "waiting for you",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed — the agent stopped with an error",
            Self::Interrupted => "interrupted — stopped before it could finish",
            Self::Unknown(value) if value.is_empty() => "unknown — the host has not said",
            Self::Unknown(value) => value,
        }
    }

    /// Whether the agent is working, which is when an input steers it.
    pub fn is_busy(&self) -> bool {
        matches!(self, Self::Running)
    }

    /// Whether the session has stopped for good.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Interrupted)
    }

    /// Whether the session finished successfully.
    ///
    /// Only `succeeded` answers `true`; `interrupted` is terminal but not
    /// complete, and `unknown` claims nothing.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Succeeded)
    }

    /// What an abort request leaves behind.
    ///
    /// A host that answers an abort with a success is not believed: the person
    /// asked for the session to stop, so the session is labelled interrupted
    /// rather than complete.
    pub fn after_abort(self) -> Self {
        if self.is_complete() {
            Self::Interrupted
        } else {
            self
        }
    }
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

impl Serialize for SessionStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SessionStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(Self::parse(&value))
    }
}

/// One session, as the host lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    /// The session's public id (the host execution id).
    pub id: String,
    /// The project directory it runs in.
    #[serde(default)]
    pub project: String,
    /// The first line of its prompt, for the list.
    #[serde(default)]
    pub title: String,
    /// Lifecycle.
    #[serde(default)]
    pub status: SessionStatus,
    /// How many inputs are waiting behind the current turn.
    #[serde(default)]
    pub queued: u32,
    /// The highest timeline sequence the host has written.
    #[serde(default)]
    pub last_sequence: u64,
    /// When it started, as the host reports it.
    #[serde(default)]
    pub started_at: String,
}

impl SessionSummary {
    /// Whether the agent is working.
    pub fn is_busy(&self) -> bool {
        self.status.is_busy()
    }

    /// Whether the session finished successfully.
    pub fn is_complete(&self) -> bool {
        self.status.is_complete()
    }

    /// Whether the session has stopped for good.
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    /// The status sentence for the list and the header.
    pub fn status_note(&self) -> &str {
        self.status.note()
    }
}

/// What starting a session asks the host for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSession {
    /// The project directory the agent works in.
    pub project: String,
    /// The first prompt.
    pub prompt: String,
    /// An optional agent selection the host may honour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

impl StartSession {
    /// A session in `project` with `prompt`.
    pub fn new(project: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            project: project.into(),
            prompt: prompt.into(),
            agent: None,
        }
    }

    /// Ask for a particular agent.
    pub fn with_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }

    /// Refuse a request the host could only reject.
    pub fn validate(&self) -> Result<(), SessionError> {
        if self.project.trim().is_empty() {
            return Err(SessionError::InvalidRequest {
                detail: "a session needs a project directory".to_owned(),
            });
        }
        if self.project.chars().any(|character| character == '\0') {
            return Err(SessionError::InvalidRequest {
                detail: "the project directory contains a NUL character".to_owned(),
            });
        }
        validate_input(&self.prompt)?;
        Ok(())
    }
}

/// One thing that happened in a session.
///
/// `kind` uses the relay's vocabulary. `author` is `user` or `session`; a
/// missing author is never read as the person's words.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostEvent {
    /// Monotonic sequence within the session.
    pub sequence: u64,
    /// `started`, `status`, `output`, `tool`, `message` or `stopped`.
    pub kind: String,
    /// Who said it, for a `message`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// The text of an output or a message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The lifecycle value of a `status` or `stopped` event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// The tool's name, for a `tool` event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// A short result or progress line for a `tool` event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// When the host wrote it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
}

impl HostEvent {
    /// The lifecycle this event carries, when it carries one.
    pub fn status_value(&self) -> Option<SessionStatus> {
        match self.kind.as_str() {
            "started" => Some(SessionStatus::Running),
            "status" | "stopped" => self.status.as_deref().map(SessionStatus::parse),
            _ => None,
        }
    }
}

/// One page of a session's timeline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPage {
    /// Events in sequence order.
    #[serde(default)]
    pub events: Vec<HostEvent>,
    /// The host's next cursor, when it has one.
    #[serde(default)]
    pub next: Option<u64>,
}

impl EventPage {
    /// The cursor after this page: the host's `next` when it sent one, else the
    /// highest sequence in the page.
    pub fn cursor(&self) -> Option<u64> {
        self.next
            .or_else(|| self.events.iter().map(|event| event.sequence).max())
    }
}

/// How one input is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputMode {
    /// A follow-up to an idle session.
    Prompt,
    /// A correction to a running session.
    Steer,
    /// Work held until the current turn ends.
    Queue,
}

impl InputMode {
    /// The wire value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Steer => "steer",
            Self::Queue => "queue",
        }
    }

    /// The words the control is offered with.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Prompt => "Send follow-up",
            Self::Steer => "Steer this session",
            Self::Queue => "Queue while busy",
        }
    }

    /// What the control does, in one sentence.
    pub fn hint(&self) -> &'static str {
        match self {
            Self::Prompt => "The agent is idle, so this starts its next turn.",
            Self::Steer => "The agent is working; this redirects the turn in progress.",
            Self::Queue => "The agent is working; this is held until the current turn ends.",
        }
    }

    /// The mode an input takes for a session in `status`.
    ///
    /// A busy session is steered; it is never offered as a second prompt.
    pub fn for_status(status: &SessionStatus) -> Self {
        match status {
            SessionStatus::Running => Self::Steer,
            SessionStatus::Queued => Self::Queue,
            _ => Self::Prompt,
        }
    }
}

/// What the host did with an input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputReceipt {
    /// How the host delivered it, which must be the mode that was asked for.
    pub mode: InputMode,
    /// The timeline entry the host recorded for it.
    pub event: HostEvent,
}

/// Everything this client asks the local host agent.
pub trait HostSessions {
    /// Every session the host is running or remembers.
    fn list(&mut self) -> Result<Vec<SessionSummary>, SessionError>;

    /// Start a session in a project directory.
    fn start(&mut self, request: &StartSession) -> Result<SessionSummary, SessionError>;

    /// Read the timeline after an exclusive cursor.
    fn events(&mut self, session: &str, after: u64, limit: u32) -> Result<EventPage, SessionError>;

    /// Deliver one input as a prompt, a steer or a queued turn.
    fn input(
        &mut self,
        session: &str,
        text: &str,
        mode: InputMode,
    ) -> Result<InputReceipt, SessionError>;

    /// Stop a session. The host answers with its interrupted state.
    fn abort(&mut self, session: &str) -> Result<SessionSummary, SessionError>;
}

/// The local host agent's HTTP API, over `ureq`.
pub struct HttpHostSessions {
    agent: ureq::Agent,
    base: String,
    credential: String,
}

impl std::fmt::Debug for HttpHostSessions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpHostSessions")
            .field("base", &self.base)
            .field("credential", &crate::settings::redact(&self.credential))
            .finish_non_exhaustive()
    }
}

impl HttpHostSessions {
    /// Build the client for the host at `base_url`.
    ///
    /// `credential` is this machine's host relationship credential; it is sent
    /// as `X-Host-Credential` and is never rendered.
    pub fn new(base_url: &str, credential: &str) -> Result<Self, SessionError> {
        let base = normalize_host_url(base_url)?;
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout_read(READ_TIMEOUT)
            .timeout_write(READ_TIMEOUT)
            .build();
        Ok(Self {
            agent,
            base,
            credential: credential.to_owned(),
        })
    }

    /// The normalised base URL.
    pub fn base_url(&self) -> &str {
        &self.base
    }

    fn authorized(&self, method: &str, path: &str) -> ureq::Request {
        self.agent
            .request(method, &format!("{}{path}", self.base))
            .set("X-Host-Credential", &self.credential)
            .set("Accept", "application/json")
    }
}

impl HostSessions for HttpHostSessions {
    fn list(&mut self) -> Result<Vec<SessionSummary>, SessionError> {
        let response = self.authorized("GET", "/v1/sessions").call();
        let body: SessionsResponse = decode(response)?;
        Ok(body.sessions)
    }

    fn start(&mut self, request: &StartSession) -> Result<SessionSummary, SessionError> {
        request.validate()?;
        let response = self
            .authorized("POST", "/v1/sessions")
            .send_json(serde_json::to_value(request).map_err(protocol)?);
        decode(response)
    }

    fn events(&mut self, session: &str, after: u64, limit: u32) -> Result<EventPage, SessionError> {
        let session = safe_id(session)?;
        let limit = limit.clamp(1, MAX_EVENT_PAGE);
        let response = self
            .authorized(
                "GET",
                &format!("/v1/sessions/{session}/events?after={after}&limit={limit}"),
            )
            .call();
        decode(response)
    }

    fn input(
        &mut self,
        session: &str,
        text: &str,
        mode: InputMode,
    ) -> Result<InputReceipt, SessionError> {
        let session = safe_id(session)?;
        validate_input(text)?;
        let response = self
            .authorized("POST", &format!("/v1/sessions/{session}/input"))
            .send_json(serde_json::json!({ "text": text, "mode": mode }));
        decode(response)
    }

    fn abort(&mut self, session: &str) -> Result<SessionSummary, SessionError> {
        let session = safe_id(session)?;
        let response = self
            .authorized("POST", &format!("/v1/sessions/{session}/abort"))
            .call();
        decode(response)
    }
}

#[derive(Debug, Deserialize)]
struct SessionsResponse {
    #[serde(default)]
    sessions: Vec<SessionSummary>,
}

/// Refuse an input the host could only reject.
fn validate_input(text: &str) -> Result<(), SessionError> {
    if text.trim().is_empty() {
        return Err(SessionError::InvalidRequest {
            detail: "an input needs some text".to_owned(),
        });
    }
    if text.chars().count() > MAX_INPUT_CHARS {
        return Err(SessionError::InvalidRequest {
            detail: format!("an input may be at most {MAX_INPUT_CHARS} characters"),
        });
    }
    if text.chars().any(|character| character == '\0') {
        return Err(SessionError::InvalidRequest {
            detail: "the input contains a NUL character".to_owned(),
        });
    }
    Ok(())
}

/// Normalise and validate the host agent's base URL.
///
/// The host is local, so plain `http` is expected, but a TLS proxy is allowed.
/// The returned value has no trailing slash.
pub fn normalize_host_url(input: &str) -> Result<String, SessionError> {
    let trimmed = input.trim();
    let invalid = || SessionError::InvalidHostUrl {
        url: trimmed.to_owned(),
    };
    if trimmed.is_empty() || !trimmed.is_ascii() {
        return Err(invalid());
    }
    if trimmed.contains('?') || trimmed.contains('#') || trimmed.chars().any(char::is_whitespace) {
        return Err(invalid());
    }
    let lower = trimmed.to_ascii_lowercase();
    let rest = if lower.starts_with("http://") {
        &trimmed["http://".len()..]
    } else if lower.starts_with("https://") {
        &trimmed["https://".len()..]
    } else {
        return Err(invalid());
    };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() || authority.contains('@') {
        return Err(invalid());
    }
    let base = trimmed.trim_end_matches('/');
    if base.is_empty() {
        return Err(invalid());
    }
    Ok(base.to_owned())
}

/// Validate a session id before it is put into a path.
fn safe_id(id: &str) -> Result<&str, SessionError> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(id)
    } else {
        Err(SessionError::Protocol {
            detail: "the session id is not path-safe".to_owned(),
        })
    }
}

fn protocol(source: serde_json::Error) -> SessionError {
    SessionError::Protocol {
        detail: source.to_string(),
    }
}

/// Turn one HTTP result into a typed body, mapping the host's errors.
fn decode<T: DeserializeOwned>(
    result: Result<ureq::Response, ureq::Error>,
) -> Result<T, SessionError> {
    let response = match result {
        Ok(response) => response,
        Err(ureq::Error::Transport(transport)) => {
            return Err(SessionError::Unreachable {
                detail: transport.to_string(),
            });
        }
        Err(ureq::Error::Status(status, response)) => {
            return Err(status_error(status, response));
        }
    };
    let body = response
        .into_string()
        .map_err(|source| SessionError::Protocol {
            detail: source.to_string(),
        })?;
    serde_json::from_str(&body).map_err(protocol)
}

/// Map a non-2xx answer to a [`SessionError`].
fn status_error(status: u16, response: ureq::Response) -> SessionError {
    let body = response.into_string().unwrap_or_default();
    let problem: Option<ProblemBody> = serde_json::from_str(&body).ok();
    let detail = problem
        .as_ref()
        .and_then(|problem| problem.detail.clone().or_else(|| problem.title.clone()))
        .unwrap_or_default();
    match status {
        401 => SessionError::Unauthorized,
        403 => SessionError::Forbidden { detail },
        other => SessionError::Problem {
            status: other,
            code: problem
                .and_then(|problem| problem.code)
                .unwrap_or_else(|| "unknown".to_owned()),
            detail: if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            },
        },
    }
}

#[derive(Debug, Deserialize)]
struct ProblemBody {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[cfg(test)]
pub(crate) mod testing {
    //! A host agent that speaks the documented routes on a loopback socket.
    //!
    //! Routes are consumed in order, so the same method and path can answer
    //! twice with different bodies: that is how steer and queue are told apart
    //! without a second route. The server never leaves the test process.

    use std::collections::BTreeMap;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::{self, JoinHandle};
    use std::time::{Duration, Instant};

    /// One request the server received.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Captured {
        /// Method, as sent.
        pub method: String,
        /// Path, as sent, including the query.
        pub path: String,
        /// The `X-Host-Credential` header, when present.
        pub credential: Option<String>,
        /// The `Authorization` header, when present.
        pub authorization: Option<String>,
        /// The request body.
        pub body: String,
    }

    impl Captured {
        /// The request body as JSON, when it parses.
        pub fn json(&self) -> serde_json::Value {
            serde_json::from_str(&self.body).unwrap_or(serde_json::Value::Null)
        }

        /// The path without its query string.
        pub fn path_only(&self) -> &str {
            self.path.split('?').next().unwrap_or(&self.path)
        }

        /// One query parameter's value, when present.
        pub fn query(&self, name: &str) -> Option<&str> {
            let query = self.path.split_once('?')?.1;
            query.split('&').find_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                (key == name).then_some(value)
            })
        }
    }

    struct Route {
        method: String,
        path: String,
        status: u16,
        body: String,
    }

    /// The loopback host. Dropping it stops the accept loop.
    pub struct ScriptedHost {
        url: String,
        routes: Arc<Mutex<Vec<Route>>>,
        requests: Arc<Mutex<Vec<Captured>>>,
        stop: Arc<AtomicBool>,
        handle: Option<JoinHandle<()>>,
    }

    impl ScriptedHost {
        /// Start a host with `(method, path, status, body)` routes, in order.
        pub fn start(routes: &[(&str, &str, u16, &str)]) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let address = listener.local_addr().expect("local address");
            listener
                .set_nonblocking(true)
                .expect("non-blocking listener");
            let url = format!("http://{address}");
            let routes: Vec<Route> = routes
                .iter()
                .map(|(method, path, status, body)| Route {
                    method: (*method).to_owned(),
                    path: (*path).to_owned(),
                    status: *status,
                    body: (*body).to_owned(),
                })
                .collect();
            let routes = Arc::new(Mutex::new(routes));
            let requests = Arc::new(Mutex::new(Vec::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let thread_routes = Arc::clone(&routes);
            let thread_requests = Arc::clone(&requests);
            let thread_stop = Arc::clone(&stop);
            let handle = thread::spawn(move || {
                while !thread_stop.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream
                                .set_read_timeout(Some(Duration::from_secs(2)))
                                .expect("read timeout");
                            if let Some(captured) = read_request(&mut stream) {
                                let path_only = captured
                                    .path
                                    .split('?')
                                    .next()
                                    .unwrap_or(&captured.path)
                                    .to_owned();
                                let route = {
                                    let mut routes = thread_routes.lock().expect("route table");
                                    routes
                                        .iter()
                                        .position(|route| {
                                            route.method == captured.method
                                                && route.path == path_only
                                        })
                                        .map(|index| routes.remove(index))
                                };
                                thread_requests.lock().expect("request log").push(captured);
                                let (status, body) = route.map_or_else(
                                    || (404, "{\"code\":\"not_found\"}".to_owned()),
                                    |route| (route.status, route.body),
                                );
                                let _ = respond(&mut stream, status, &body);
                            }
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            });
            Self {
                url,
                routes,
                requests,
                stop,
                handle: Some(handle),
            }
        }

        /// The base URL to give the client.
        pub fn url(&self) -> &str {
            &self.url
        }

        /// How many routes are still unanswered, for a clear failure.
        pub fn unanswered(&self) -> usize {
            self.routes.lock().expect("route table").len()
        }

        /// Stop the server and return every request it handled.
        pub fn finish(mut self) -> Vec<Captured> {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
            self.requests.lock().expect("request log").clone()
        }
    }

    impl Drop for ScriptedHost {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    /// Read one HTTP/1.1 request with a `Content-Length` body.
    fn read_request(stream: &mut std::net::TcpStream) -> Option<Captured> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];
        loop {
            if let Some(head_end) = find(&buffer, b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
                let content_length = head
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                    .unwrap_or(0);
                let body_start = head_end + 4;
                while buffer.len() < body_start + content_length {
                    let read = stream.read(&mut chunk).ok()?;
                    if read == 0 {
                        break;
                    }
                    buffer.extend_from_slice(&chunk[..read]);
                }
                let mut lines = head.lines();
                let request_line = lines.next()?;
                let mut parts = request_line.split_whitespace();
                let method = parts.next()?.to_owned();
                let path = parts.next()?.to_owned();
                let mut headers = BTreeMap::new();
                for line in lines {
                    if let Some((name, value)) = line.split_once(':') {
                        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
                    }
                }
                let credential = headers.get("x-host-credential").cloned();
                let authorization = headers.get("authorization").cloned();
                let body = String::from_utf8_lossy(
                    &buffer[body_start..(body_start + content_length).min(buffer.len())],
                )
                .into_owned();
                return Some(Captured {
                    method,
                    path,
                    credential,
                    authorization,
                    body,
                });
            }
            if Instant::now() > deadline {
                return None;
            }
            match stream.read(&mut chunk) {
                Ok(0) => return None,
                Ok(read) => buffer.extend_from_slice(&chunk[..read]),
                Err(_) => return None,
            }
        }
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    fn respond(stream: &mut std::net::TcpStream, status: u16, body: &str) -> std::io::Result<()> {
        let reason = match status {
            200 => "OK",
            201 => "Created",
            202 => "Accepted",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            409 => "Conflict",
            422 => "Unprocessable Entity",
            _ => "Response",
        };
        let head = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(body.as_bytes())?;
        stream.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::ScriptedHost;
    use super::*;

    const SESSION: &str = r#"{"id":"ses_1","project":"/Users/operator/work/tidepool","title":"Add the sessions pane","status":"running","queued":0,"lastSequence":7,"startedAt":"2026-10-29T09:00:00Z"}"#;
    const INTERRUPTED: &str = r#"{"id":"ses_1","project":"/Users/operator/work/tidepool","title":"Add the sessions pane","status":"interrupted","queued":0,"lastSequence":9,"startedAt":"2026-10-29T09:00:00Z"}"#;
    const EVENT: &str =
        r#"{"sequence":8,"kind":"message","author":"user","text":"tighten the wording"}"#;

    fn client(url: &str) -> HttpHostSessions {
        HttpHostSessions::new(url, "host-credential-secret").expect("host client")
    }

    #[test]
    fn a_session_starts_in_the_chosen_project_with_the_first_prompt() {
        let host = ScriptedHost::start(&[("POST", "/v1/sessions", 201, SESSION)]);
        let mut client = client(host.url());
        let request = StartSession::new("/Users/operator/work/tidepool", "Add the sessions pane");
        let session = client.start(&request).expect("start");
        assert_eq!(session.id, "ses_1");
        assert_eq!(session.status, SessionStatus::Running);
        assert!(session.is_busy());
        assert_eq!(session.status_note(), "running");

        let captured = host.finish();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].method, "POST");
        assert_eq!(captured[0].path, "/v1/sessions");
        assert_eq!(
            captured[0].json()["project"],
            serde_json::json!("/Users/operator/work/tidepool")
        );
        assert_eq!(
            captured[0].json()["prompt"],
            serde_json::json!("Add the sessions pane")
        );
        assert_eq!(
            captured[0].credential.as_deref(),
            Some("host-credential-secret")
        );
    }

    #[test]
    fn steer_and_queue_are_distinct_modes_on_the_same_input_route() {
        let steered = format!(r#"{{"mode":"steer","event":{EVENT}}}"#);
        let queued = r#"{"mode":"queue","event":{"sequence":9,"kind":"message","author":"user","text":"and add a test"}}"#.to_owned();
        let host = ScriptedHost::start(&[
            ("POST", "/v1/sessions/ses_1/input", 201, &steered),
            ("POST", "/v1/sessions/ses_1/input", 201, &queued),
        ]);
        let mut client = client(host.url());
        let receipt = client
            .input("ses_1", "tighten the wording", InputMode::Steer)
            .expect("steer");
        assert_eq!(receipt.mode, InputMode::Steer);
        assert_eq!(receipt.event.sequence, 8);
        let receipt = client
            .input("ses_1", "and add a test", InputMode::Queue)
            .expect("queue");
        assert_eq!(receipt.mode, InputMode::Queue);

        let captured = host.finish();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].json()["mode"], serde_json::json!("steer"));
        assert_eq!(
            captured[0].json()["text"],
            serde_json::json!("tighten the wording")
        );
        assert_eq!(captured[1].json()["mode"], serde_json::json!("queue"));
    }

    #[test]
    fn abort_reads_the_interrupted_session_back() {
        let host = ScriptedHost::start(&[("POST", "/v1/sessions/ses_1/abort", 200, INTERRUPTED)]);
        let mut client = client(host.url());
        let session = client.abort("ses_1").expect("abort");
        assert_eq!(session.status, SessionStatus::Interrupted);
        assert_eq!(session.status.label(), "interrupted");
        assert!(session.is_terminal());
        assert!(!session.is_complete());
        let captured = host.finish();
        assert_eq!(captured[0].path, "/v1/sessions/ses_1/abort");
    }

    #[test]
    fn the_timeline_is_read_after_the_cursor() {
        let page = format!(r#"{{"events":[{EVENT}],"next":8}}"#);
        let host = ScriptedHost::start(&[("GET", "/v1/sessions/ses_1/events", 200, &page)]);
        let mut client = client(host.url());
        let page = client.events("ses_1", 7, 100).expect("events");
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.cursor(), Some(8));
        let captured = host.finish();
        assert_eq!(captured[0].query("after"), Some("7"));
        assert_eq!(captured[0].query("limit"), Some("100"));
    }

    #[test]
    fn a_list_read_returns_every_session() {
        let body = format!(r#"{{"sessions":[{SESSION}]}}"#);
        let host = ScriptedHost::start(&[("GET", "/v1/sessions", 200, &body)]);
        let mut client = client(host.url());
        let sessions = client.list().expect("list");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].project, "/Users/operator/work/tidepool");
        let _ = host.finish();
    }

    #[test]
    fn a_session_id_that_is_not_path_safe_is_refused_before_the_request() {
        let mut client = client("http://127.0.0.1:1");
        for id in ["", "../v1/sessions", "ses/1", "ses 1"] {
            assert!(
                matches!(client.abort(id), Err(SessionError::Protocol { .. })),
                "{id} should be refused"
            );
        }
    }

    #[test]
    fn an_empty_project_or_prompt_is_refused_before_the_host() {
        let mut client = client("http://127.0.0.1:1");
        for request in [
            StartSession::new("   ", "Add the sessions pane"),
            StartSession::new("/Users/operator/work/tidepool", " \n "),
        ] {
            assert!(matches!(
                client.start(&request),
                Err(SessionError::InvalidRequest { .. })
            ));
        }
        assert!(matches!(
            client.input("ses_1", "  ", InputMode::Prompt),
            Err(SessionError::InvalidRequest { .. })
        ));
        assert!(matches!(
            client.input("ses_1", &"x".repeat(MAX_INPUT_CHARS + 1), InputMode::Prompt),
            Err(SessionError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn an_unreachable_host_is_a_transport_error_not_a_panic() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        drop(listener);
        let mut client = client(&format!("http://{address}"));
        let error = client.list().expect_err("closed port");
        assert!(matches!(error, SessionError::Unreachable { .. }));
    }

    #[test]
    fn a_problem_document_becomes_a_named_error() {
        let host = ScriptedHost::start(&[(
            "POST",
            "/v1/sessions/ses_1/abort",
            409,
            r#"{"code":"session_not_running","detail":"the session already finished"}"#,
        )]);
        let mut client = client(host.url());
        match client.abort("ses_1") {
            Err(SessionError::Problem {
                status,
                code,
                detail,
            }) => {
                assert_eq!(status, 409);
                assert_eq!(code, "session_not_running");
                assert!(detail.contains("already finished"));
            }
            other => panic!("expected a problem, got {other:?}"),
        }
        let _ = host.finish();
    }

    #[test]
    fn a_401_is_unauthorized_and_a_403_is_forbidden() {
        let host = ScriptedHost::start(&[
            ("GET", "/v1/sessions", 401, r#"{"code":"unauthorized"}"#),
            (
                "POST",
                "/v1/sessions/ses_1/abort",
                403,
                r#"{"detail":"a viewer may not stop a session"}"#,
            ),
        ]);
        let mut client = client(host.url());
        assert!(matches!(client.list(), Err(SessionError::Unauthorized)));
        match client.abort("ses_1") {
            Err(SessionError::Forbidden { detail }) => assert!(detail.contains("viewer")),
            other => panic!("expected forbidden, got {other:?}"),
        }
        let _ = host.finish();
    }

    #[test]
    fn the_credential_is_never_rendered() {
        let client = client("http://127.0.0.1:4000");
        let rendered = format!("{client:?}");
        assert!(rendered.contains("127.0.0.1:4000"));
        assert!(!rendered.contains("host-credential-secret"));
    }

    #[test]
    fn the_host_url_is_normalised_and_the_rest_refused() {
        assert_eq!(
            normalize_host_url(" http://127.0.0.1:4000/ ").expect("valid"),
            "http://127.0.0.1:4000"
        );
        assert_eq!(
            normalize_host_url("https://host.example.test/tidepool").expect("path prefix"),
            "https://host.example.test/tidepool"
        );
        for bad in [
            "",
            "127.0.0.1:4000",
            "ftp://127.0.0.1",
            "http://",
            "http://user:pass@127.0.0.1",
            "http://127.0.0.1/?a=1",
            "http://127.0.0.1/#frag",
            "http://127.0.0.1/a b",
        ] {
            assert!(
                matches!(
                    normalize_host_url(bad),
                    Err(SessionError::InvalidHostUrl { .. })
                ),
                "{bad} should be refused"
            );
        }
    }

    #[test]
    fn an_interrupted_session_is_terminal_but_never_complete() {
        let interrupted = SessionStatus::parse("interrupted");
        assert_eq!(interrupted.label(), "interrupted");
        assert!(interrupted.is_terminal());
        assert!(!interrupted.is_complete());
        assert!(interrupted.note().contains("interrupted"));
        assert!(!interrupted.note().contains("complete"));
        assert!(!interrupted.note().contains("done"));

        // The host's `stopped` value is an interruption, not a success.
        assert_eq!(SessionStatus::parse("stopped"), SessionStatus::Interrupted);
        assert!(SessionStatus::parse("succeeded").is_complete());
        assert!(!SessionStatus::parse("maintenance").is_complete());
    }

    #[test]
    fn a_failed_session_says_it_failed_and_is_not_complete() {
        let failed = SessionStatus::parse("failed");
        assert_eq!(failed.label(), "failed");
        assert!(failed.note().contains("failed"));
        assert!(failed.is_terminal());
        assert!(!failed.is_complete());
    }

    #[test]
    fn an_abort_never_leaves_a_success_behind() {
        assert_eq!(
            SessionStatus::Succeeded.after_abort(),
            SessionStatus::Interrupted
        );
        assert_eq!(SessionStatus::Running.after_abort(), SessionStatus::Running);
        assert!(!SessionStatus::Running.after_abort().is_complete());
        assert_eq!(
            SessionStatus::Interrupted.after_abort(),
            SessionStatus::Interrupted
        );
        assert_eq!(SessionStatus::Waiting.after_abort(), SessionStatus::Waiting);
    }

    #[test]
    fn a_busy_session_is_steered_and_a_queued_one_queues() {
        assert_eq!(
            InputMode::for_status(&SessionStatus::Running),
            InputMode::Steer
        );
        assert_eq!(
            InputMode::for_status(&SessionStatus::Queued),
            InputMode::Queue
        );
        assert_eq!(
            InputMode::for_status(&SessionStatus::Waiting),
            InputMode::Prompt
        );
        assert!(InputMode::Steer.label().contains("Steer"));
        assert!(!InputMode::Steer.label().to_lowercase().contains("prompt"));
    }
}
