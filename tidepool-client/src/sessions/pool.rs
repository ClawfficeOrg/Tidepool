//! Publishing a client-started session's lifecycle to the pool.
//!
//! A session runs on this machine, so the pool only learns about it when the
//! client says so. This module is that one-way seam, over the documented relay
//! routes:
//!
//! ```text
//! PUT  /v1/sessions/{session}/relay            opt in
//! POST /v1/sessions/{session}/relay/events     publish one event
//! ```
//!
//! # Lifecycle by default, transcript only on request
//!
//! Starting a session from the pool-connected client announces that it exists
//! and how it ends: `started`, `status` and `stopped`. Output, messages and
//! tool results stay on this machine unless the person explicitly enables
//! content publication, because the relay is a publication channel and a
//! transcript is not something a session should leak by existing.
//! [`is_lifecycle`] is the rule the controller applies; this module publishes
//! whatever it is handed.
//!
//! # Publication is best effort
//!
//! A pool that is away must not stop the local session. Every method returns a
//! [`PoolError`] and the caller records it as a notice; nothing here panics and
//! nothing here carries the operator's token into a message.
//!
//! The relay only accepts a session the pool already knows as a host execution
//! owned by one of the organization's hosts. A session a person starts directly
//! on this machine has no such row in v1, so the opt-in is answered `404`; the
//! client reports that as a notice and keeps the local session. A pool route
//! that creates a standalone session is the follow-up this seam waits for.

use serde_json::json;

use crate::host::client::{PoolError, Session};
use crate::sessions::protocol::HostEvent;

/// Whether an event is lifecycle only.
///
/// Lifecycle events say that a session exists or changed state; they never
/// carry what the agent said or what a tool returned.
pub fn is_lifecycle(event: &HostEvent) -> bool {
    matches!(event.kind.as_str(), "started" | "status" | "stopped")
}

/// What the client publishes to the pool about a session.
pub trait PoolRelay {
    /// Opt the session into publication. Safe to call again.
    fn opt_in(&mut self, session: &str) -> Result<(), PoolError>;

    /// Publish one event. The relay assigns its own sequence.
    fn publish(&mut self, session: &str, event: &HostEvent) -> Result<(), PoolError>;
}

/// The pool's relay routes, over `ureq`.
pub struct HttpPoolRelay {
    agent: ureq::Agent,
    session: Session,
}

impl std::fmt::Debug for HttpPoolRelay {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpPoolRelay")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl HttpPoolRelay {
    /// Build the relay client for a signed-in operator session.
    pub fn new(session: Session) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(3))
            .timeout_read(std::time::Duration::from_secs(10))
            .timeout_write(std::time::Duration::from_secs(10))
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

impl PoolRelay for HttpPoolRelay {
    fn opt_in(&mut self, session: &str) -> Result<(), PoolError> {
        let session = safe_id(session)?;
        let response = self
            .authorized("PUT", &format!("/v1/sessions/{session}/relay"))
            .call();
        let state: RelayState = decode(response)?;
        if !state.enabled {
            return Err(PoolError::Protocol {
                detail: "the pool did not enable the session's relay".to_owned(),
            });
        }
        Ok(())
    }

    fn publish(&mut self, session: &str, event: &HostEvent) -> Result<(), PoolError> {
        let session = safe_id(session)?;
        let body = json!({
            "kind": event.kind,
            "payload": relay_payload(event),
        });
        let response = self
            .authorized("POST", &format!("/v1/sessions/{session}/relay/events"))
            .send_json(body);
        let _: serde_json::Value = decode(response)?;
        Ok(())
    }
}

/// The payload one event is published with.
///
/// Only the fields the relay needs: a lifecycle value, a message's words, or a
/// tool's name and summary. No credential-like key is ever produced.
pub fn relay_payload(event: &HostEvent) -> serde_json::Value {
    let status = event
        .status
        .as_deref()
        .filter(|status| !status.trim().is_empty());
    match event.kind.as_str() {
        "started" => json!({ "status": status.unwrap_or("running") }),
        "status" | "stopped" => json!({ "status": status.unwrap_or("interrupted") }),
        "output" => json!({ "text": event.text.as_deref().unwrap_or_default() }),
        "message" => json!({
            "author": event.author.as_deref().unwrap_or("session"),
            "text": event.text.as_deref().unwrap_or_default(),
        }),
        "tool" => {
            let mut payload = json!({ "tool": event.tool.as_deref().unwrap_or("tool") });
            if let Some(summary) = event.summary.as_deref() {
                payload["summary"] = json!(summary);
            }
            payload
        }
        other => json!({ "kind": other }),
    }
}

#[derive(Debug, serde::Deserialize)]
struct RelayState {
    #[serde(default)]
    enabled: bool,
}

/// Validate a session id before it is put into a path.
fn safe_id(id: &str) -> Result<&str, PoolError> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(id)
    } else {
        Err(PoolError::Protocol {
            detail: "the session id is not path-safe".to_owned(),
        })
    }
}

/// Turn one HTTP result into a typed body, mapping the pool's errors.
fn decode<T: serde::de::DeserializeOwned>(
    result: Result<ureq::Response, ureq::Error>,
) -> Result<T, PoolError> {
    let response = match result {
        Ok(response) => response,
        Err(ureq::Error::Transport(transport)) => {
            return Err(PoolError::Unreachable {
                detail: transport.to_string(),
            });
        }
        Err(ureq::Error::Status(status, response)) => {
            return Err(status_error(status, response));
        }
    };
    let body = response
        .into_string()
        .map_err(|source| PoolError::Protocol {
            detail: source.to_string(),
        })?;
    serde_json::from_str(&body).map_err(|source| PoolError::Protocol {
        detail: source.to_string(),
    })
}

/// Map a non-2xx answer to a [`PoolError`].
fn status_error(status: u16, response: ureq::Response) -> PoolError {
    let body = response.into_string().unwrap_or_default();
    let problem: Option<ProblemBody> = serde_json::from_str(&body).ok();
    let detail = problem
        .as_ref()
        .and_then(|problem| problem.detail.clone().or_else(|| problem.title.clone()))
        .unwrap_or_default();
    match status {
        401 => PoolError::Unauthorized,
        403 => PoolError::Forbidden { detail },
        other => PoolError::Problem {
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

#[derive(Debug, serde::Deserialize)]
struct ProblemBody {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    detail: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::protocol::testing::ScriptedHost;

    const STATE_ON: &str =
        r#"{"session":"ses_1","enabled":true,"startedAt":"2026-10-29T09:00:00Z","stoppedAt":null}"#;
    const STATE_OFF: &str =
        r#"{"session":"ses_1","enabled":false,"startedAt":null,"stoppedAt":null}"#;

    fn relay(url: &str) -> HttpPoolRelay {
        HttpPoolRelay::new(Session::for_test(
            url,
            "operator@example.test",
            "operator-token",
        ))
    }

    fn started() -> HostEvent {
        HostEvent {
            sequence: 1,
            kind: "started".to_owned(),
            status: Some("running".to_owned()),
            ..HostEvent::default()
        }
    }

    #[test]
    fn opt_in_and_publish_use_the_documented_relay_routes() {
        let host = ScriptedHost::start(&[
            ("PUT", "/v1/sessions/ses_1/relay", 200, STATE_ON),
            (
                "POST",
                "/v1/sessions/ses_1/relay/events",
                201,
                r#"{"id":"evt_1","session":"ses_1","sequence":1,"kind":"started","payload":{"status":"running"},"createdAt":"2026-10-29T09:00:00Z"}"#,
            ),
        ]);
        let mut relay = relay(host.url());
        relay.opt_in("ses_1").expect("opt in");
        relay.publish("ses_1", &started()).expect("publish");

        let captured = host.finish();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].method, "PUT");
        assert_eq!(captured[0].path, "/v1/sessions/ses_1/relay");
        assert_eq!(
            captured[0].authorization.as_deref(),
            Some("Bearer operator-token")
        );
        assert_eq!(captured[1].method, "POST");
        assert_eq!(captured[1].path, "/v1/sessions/ses_1/relay/events");
        assert_eq!(captured[1].json()["kind"], serde_json::json!("started"));
        assert_eq!(
            captured[1].json()["payload"]["status"],
            serde_json::json!("running")
        );
    }

    #[test]
    fn a_relay_that_stays_off_is_a_protocol_error() {
        let host = ScriptedHost::start(&[("PUT", "/v1/sessions/ses_1/relay", 200, STATE_OFF)]);
        let mut relay = relay(host.url());
        assert!(matches!(
            relay.opt_in("ses_1"),
            Err(PoolError::Protocol { .. })
        ));
        let _ = host.finish();
    }

    #[test]
    fn a_refused_publication_is_a_named_problem() {
        let host = ScriptedHost::start(&[(
            "POST",
            "/v1/sessions/ses_1/relay/events",
            409,
            r#"{"code":"relay_not_enabled","detail":"relay is off"}"#,
        )]);
        let mut relay = relay(host.url());
        match relay.publish("ses_1", &started()) {
            Err(PoolError::Problem { status, code, .. }) => {
                assert_eq!(status, 409);
                assert_eq!(code, "relay_not_enabled");
            }
            other => panic!("expected a problem, got {other:?}"),
        }
        let _ = host.finish();
    }

    #[test]
    fn a_session_id_that_is_not_path_safe_is_refused_before_the_request() {
        let mut relay = relay("http://127.0.0.1:1");
        assert!(matches!(
            relay.opt_in("../v1/pool/members"),
            Err(PoolError::Protocol { .. })
        ));
        assert!(matches!(
            relay.publish("ses/1", &started()),
            Err(PoolError::Protocol { .. })
        ));
    }

    #[test]
    fn the_operator_token_is_never_rendered() {
        let relay = relay("http://127.0.0.1:4000");
        let rendered = format!("{relay:?}");
        assert!(rendered.contains("127.0.0.1:4000"));
        assert!(!rendered.contains("operator-token"));
    }

    #[test]
    fn only_lifecycle_events_are_announced_by_default() {
        assert!(is_lifecycle(&started()));
        let status = HostEvent {
            kind: "status".to_owned(),
            status: Some("interrupted".to_owned()),
            ..HostEvent::default()
        };
        assert!(is_lifecycle(&status));
        for kind in ["output", "tool", "message"] {
            let event = HostEvent {
                kind: kind.to_owned(),
                ..HostEvent::default()
            };
            assert!(!is_lifecycle(&event), "{kind} is content, not lifecycle");
        }
    }

    #[test]
    fn a_tool_payload_names_the_tool_and_its_summary() {
        let event = HostEvent {
            kind: "tool".to_owned(),
            tool: Some("shell".to_owned()),
            summary: Some("cargo test".to_owned()),
            ..HostEvent::default()
        };
        let payload = relay_payload(&event);
        assert_eq!(payload["tool"], serde_json::json!("shell"));
        assert_eq!(payload["summary"], serde_json::json!("cargo test"));
        assert!(payload.get("text").is_none());
    }

    #[test]
    fn a_stopped_event_publishes_an_interruption() {
        let event = HostEvent {
            kind: "stopped".to_owned(),
            ..HostEvent::default()
        };
        assert_eq!(
            relay_payload(&event)["status"],
            serde_json::json!("interrupted")
        );
    }
}
