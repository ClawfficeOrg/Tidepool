//! The chatroom surface over the pool's HTTP API.
//!
//! All reads and the member's own posts use the documented routes:
//!
//! ```text
//! GET  /v1/chatrooms
//! GET  /v1/chatrooms/{room}/members
//! GET  /v1/chatrooms/{room}/messages?after=&limit=
//! POST /v1/chatrooms/{room}/messages
//! ```
//!
//! # A session's words are never sent as a person's
//!
//! The pool's HTTP route authenticates an operator and always authors the
//! message as that person. There is no session-authenticated send route in
//! v1, so [`HttpChat::send_as_session`] refuses with
//! [`ChatError::SessionPostingUnavailable`] instead of silently posting the
//! agent's reply under the member's name. The client's session loop can still
//! show a session's replies in a room when the session itself (or a host that
//! can act for it) posts them; what this client will not do is misattribute
//! them.
//!
//! The operator token lives in [`Session`] and is never formatted into an
//! error or a log line. A room id is validated before it is placed in a path.

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::host::client::Session;

use super::model::{ChatMember, ChatMessage, ChatPage, ChatRoom};
use super::{ChatError, ChatTransport, MAX_BODY_BYTES};

/// How long a connect to the pool may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// How long a read from the pool may take.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// The pool's chatroom routes, over `ureq`.
pub struct HttpChat {
    agent: ureq::Agent,
    session: Session,
}

impl std::fmt::Debug for HttpChat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpChat")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl HttpChat {
    /// Build the chat transport for a signed-in operator session.
    pub fn new(session: Session) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(CONNECT_TIMEOUT)
            .timeout_read(READ_TIMEOUT)
            .timeout_write(READ_TIMEOUT)
            .build();
        Self { agent, session }
    }

    fn authorized(&self, method: &str, path: &str) -> ureq::Request {
        self.agent
            .request(method, &format!("{}{path}", self.session.pool_url()))
            .set("Authorization", &format!("Bearer {}", self.session.token()))
            .set("Accept", "application/json")
    }
}

impl ChatTransport for HttpChat {
    fn rooms(&mut self) -> Result<Vec<ChatRoom>, ChatError> {
        let response = self.authorized("GET", "/v1/chatrooms").call();
        decode(response)
    }

    fn members(&mut self, room: &str) -> Result<Vec<ChatMember>, ChatError> {
        let room = safe_id(room)?;
        let response = self
            .authorized("GET", &format!("/v1/chatrooms/{room}/members"))
            .call();
        decode(response)
    }

    fn history(
        &mut self,
        room: &str,
        after: Option<u64>,
        limit: u32,
    ) -> Result<ChatPage, ChatError> {
        let room = safe_id(room)?;
        let mut path = format!("/v1/chatrooms/{room}/messages?limit={limit}");
        if let Some(after) = after {
            path.push_str(&format!("&after={after}"));
        }
        let response = self.authorized("GET", &path).call();
        decode(response)
    }

    fn send(
        &mut self,
        room: &str,
        body: &str,
        reply_to: Option<&str>,
    ) -> Result<ChatMessage, ChatError> {
        let room = safe_id(room)?;
        if body.trim().is_empty() {
            return Err(ChatError::InvalidBody {
                detail: "a message needs some words".to_owned(),
            });
        }
        if body.len() > MAX_BODY_BYTES {
            return Err(ChatError::InvalidBody {
                detail: format!("a message must be at most {MAX_BODY_BYTES} bytes"),
            });
        }
        let response = self
            .authorized("POST", &format!("/v1/chatrooms/{room}/messages"))
            .send_json(serde_json::json!({
                "body": body,
                "replyTo": reply_to,
            }));
        decode(response)
    }

    fn send_as_session(
        &mut self,
        _room: &str,
        session: &str,
        _body: &str,
        _reply_to: Option<&str>,
    ) -> Result<ChatMessage, ChatError> {
        // The v1 member route has no way to act as a session. Refusing is the
        // only authorship-safe answer: posting here would hand the agent's
        // words to the person's name.
        Err(ChatError::SessionPostingUnavailable {
            session: session.to_owned(),
        })
    }
}

/// Validate an id before it is placed in a path.
fn safe_id(id: &str) -> Result<&str, ChatError> {
    let valid = !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(id)
    } else {
        Err(ChatError::InvalidRoom {
            room: id.to_owned(),
        })
    }
}

/// One problem document, when the pool sent one.
#[derive(Debug, Default, serde::Deserialize)]
struct ProblemBody {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

/// Turn one HTTP result into a typed body, mapping the pool's errors.
fn decode<T: DeserializeOwned>(
    result: Result<ureq::Response, ureq::Error>,
) -> Result<T, ChatError> {
    let response = match result {
        Ok(response) => response,
        Err(ureq::Error::Transport(transport)) => {
            return Err(ChatError::Unreachable {
                detail: transport.to_string(),
            });
        }
        Err(ureq::Error::Status(status, response)) => {
            return Err(status_error(status, response));
        }
    };
    let body = response
        .into_string()
        .map_err(|source| ChatError::Protocol {
            detail: source.to_string(),
        })?;
    serde_json::from_str(&body).map_err(|source| ChatError::Protocol {
        detail: source.to_string(),
    })
}

/// Map a non-2xx answer to a [`ChatError`].
fn status_error(status: u16, response: ureq::Response) -> ChatError {
    let body = response.into_string().unwrap_or_default();
    let problem: Option<ProblemBody> = serde_json::from_str(&body).ok();
    let detail = problem
        .as_ref()
        .and_then(|problem| problem.detail.clone().or_else(|| problem.title.clone()))
        .unwrap_or_default();
    match status {
        401 => ChatError::Unauthorized,
        403 => ChatError::Forbidden { detail },
        other => ChatError::Problem {
            status: other,
            code: problem
                .and_then(|problem| problem.code)
                .unwrap_or_else(|| "unknown".to_owned()),
            detail,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat() -> HttpChat {
        HttpChat::new(Session::for_test(
            "http://127.0.0.1:9",
            "person@example.com",
            "operator-token",
        ))
    }

    #[test]
    fn a_path_unsafe_room_is_refused_before_a_request() {
        let mut chat = chat();
        let error = chat.history("../admin", None, 10).expect_err("refused");
        assert!(matches!(error, ChatError::InvalidRoom { .. }));
    }

    #[test]
    fn a_session_post_is_refused_rather_than_attributed_to_the_person() {
        let mut chat = chat();
        let error = chat
            .send_as_session("room-1", "sess-1", "I will take it", None)
            .expect_err("refused");
        match error {
            ChatError::SessionPostingUnavailable { session } => assert_eq!(session, "sess-1"),
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn an_empty_body_is_refused_before_a_request() {
        let mut chat = chat();
        let error = chat.send("room-1", "  ", None).expect_err("refused");
        assert!(matches!(error, ChatError::InvalidBody { .. }));
    }
}
