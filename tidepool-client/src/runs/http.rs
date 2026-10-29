//! The pool's runs and activity routes, over `ureq`.
//!
//! All reads and the manual dispatch use the documented routes:
//!
//! ```text
//! GET  /v1/activity?project=&provider=&trigger=&status=&limit=
//! GET  /v1/activity/runs/{run}
//! POST /v1/runs
//! ```
//!
//! The operator token lives in [`Session`] and is never formatted into an error
//! or a log line. A run id is validated before it is placed in a path, and a
//! manual request is validated before it leaves the machine.

use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::host::client::Session;

use super::{
    ActivityPage, ActivityQuery, ActivityRunDetail, ManualDispatchRequest, ManualDispatchResponse,
    RunsClient, RunsError,
};

/// How long a connect to the pool may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
/// How long a read from the pool may take.
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// The pool's runs and activity routes, over `ureq`.
pub struct HttpRuns {
    agent: ureq::Agent,
    session: Session,
}

impl std::fmt::Debug for HttpRuns {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpRuns")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

impl HttpRuns {
    /// Build the runs transport for a signed-in operator session.
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

impl RunsClient for HttpRuns {
    fn list_activity(&mut self, query: &ActivityQuery) -> Result<ActivityPage, RunsError> {
        let mut request = self.authorized("GET", "/v1/activity");
        for (name, value) in query.pairs() {
            request = request.query(name, &value);
        }
        decode(request.call())
    }

    fn run_detail(&mut self, run: &str) -> Result<ActivityRunDetail, RunsError> {
        let run = safe_id(run)?;
        let response = self
            .authorized("GET", &format!("/v1/activity/runs/{run}"))
            .call();
        decode(response)
    }

    fn dispatch(
        &mut self,
        request: &ManualDispatchRequest,
    ) -> Result<ManualDispatchResponse, RunsError> {
        request.validate()?;
        let response =
            self.authorized("POST", "/v1/runs")
                .send_json(serde_json::to_value(request).map_err(|source| {
                    RunsError::Protocol {
                        detail: source.to_string(),
                    }
                })?);
        decode(response)
    }
}

/// Validate an id before it is placed in a path.
fn safe_id(id: &str) -> Result<&str, RunsError> {
    let valid = !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(id)
    } else {
        Err(RunsError::InvalidId { id: id.to_owned() })
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
) -> Result<T, RunsError> {
    let response = match result {
        Ok(response) => response,
        Err(ureq::Error::Transport(transport)) => {
            return Err(RunsError::Unreachable {
                detail: transport.to_string(),
            });
        }
        Err(ureq::Error::Status(status, response)) => {
            return Err(status_error(status, response));
        }
    };
    let body = response
        .into_string()
        .map_err(|source| RunsError::Protocol {
            detail: source.to_string(),
        })?;
    serde_json::from_str(&body).map_err(|source| RunsError::Protocol {
        detail: source.to_string(),
    })
}

/// Map a non-2xx answer to a [`RunsError`].
fn status_error(status: u16, response: ureq::Response) -> RunsError {
    let body = response.into_string().unwrap_or_default();
    let problem: Option<ProblemBody> = serde_json::from_str(&body).ok();
    let detail = problem
        .as_ref()
        .and_then(|problem| problem.detail.clone().or_else(|| problem.title.clone()))
        .unwrap_or_default();
    match status {
        401 => RunsError::Unauthorized,
        403 => RunsError::Forbidden { detail },
        409 => RunsError::Problem {
            status,
            code: problem
                .as_ref()
                .and_then(|problem| problem.code.clone())
                .unwrap_or_else(|| "conflict".to_owned()),
            detail,
        },
        other => RunsError::Problem {
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
    use crate::host::client::testing::ScriptedHttpPool;
    use crate::runs::{ActivityFilters, RunStepView, RunView};

    const PAGE: &str = r#"{
        "items": [{
            "kind": "run",
            "id": "run-1",
            "project": "proj_1",
            "provider": "github",
            "trigger": "on-push",
            "triggerName": "on-push",
            "run": "run-1",
            "status": "running",
            "at": "2026-10-29T00:00:00Z"
        }],
        "nextCursor": null,
        "hasMore": false
    }"#;

    const DETAIL: &str = r#"{
        "run": {
            "id": "run-1",
            "project": "proj_1",
            "trigger": "on-push",
            "triggerName": "on-push",
            "workflow": "deploy",
            "origin": "manual",
            "actor": "alice",
            "deliveryKey": "key-1",
            "status": "failed",
            "requestText": "deploy",
            "inputs": {},
            "createdAt": "2026-10-29T00:00:00Z",
            "steps": [{
                "id": "step-1",
                "step": "build",
                "position": 1,
                "status": "failed",
                "host": "host-local",
                "error": "daemon_not_connected: host 'studio-mac' is offline",
                "reason": "daemon_not_connected"
            }]
        },
        "arrival": {
            "mode": "created",
            "provider": "codex",
            "conversationKey": "conv-1",
            "steered": false
        }
    }"#;

    const DISPATCHED: &str = r#"{
        "run": {
            "id": "run-2",
            "project": "proj_1",
            "trigger": "on-push",
            "triggerName": "on-push",
            "workflow": "deploy",
            "origin": "manual",
            "actor": "alice",
            "deliveryKey": "key-2",
            "status": "pending",
            "requestText": "",
            "inputs": {},
            "createdAt": "2026-10-29T00:00:00Z",
            "steps": []
        },
        "deduplicated": false
    }"#;

    #[test]
    fn the_activity_page_is_read_with_the_active_filters_and_the_bearer() {
        let pool = ScriptedHttpPool::start(&[(
            "GET",
            "/v1/activity?project=proj_1&provider=github&trigger=on-push&status=running&limit=50",
            200,
            PAGE,
        )]);
        let session = Session::for_test(pool.url(), "operator@example.test", "operator-token");
        let mut http = HttpRuns::new(session);
        let mut filters = ActivityFilters::default();
        filters.set_project("proj_1");
        filters.set_provider("github");
        filters.set_trigger("on-push");
        filters.set_status("running");

        let page = http.list_activity(&filters.query()).expect("activity");
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].id, "run-1");
        assert_eq!(page.items[0].trigger_name.as_deref(), Some("on-push"));

        let captured = pool.finish();
        assert_eq!(captured.len(), 1);
        assert_eq!(
            captured[0].authorization.as_deref(),
            Some("Bearer operator-token")
        );
    }

    #[test]
    fn a_run_detail_keeps_the_step_failure_text() {
        let pool = ScriptedHttpPool::start(&[("GET", "/v1/activity/runs/run-1", 200, DETAIL)]);
        let session = Session::for_test(pool.url(), "operator@example.test", "operator-token");
        let mut http = HttpRuns::new(session);
        let detail = http.run_detail("run-1").expect("detail");
        assert_eq!(detail.run.id, "run-1");
        assert_eq!(detail.run.steps.len(), 1);
        assert_eq!(detail.run.steps[0].host.as_deref(), Some("host-local"));
        assert!(
            detail.run.steps[0]
                .error
                .as_deref()
                .is_some_and(|error| error.contains("daemon_not_connected"))
        );
        let _ = pool.finish();
    }

    #[test]
    fn a_manual_dispatch_posts_the_documented_body() {
        let pool = ScriptedHttpPool::start(&[("POST", "/v1/runs", 201, DISPATCHED)]);
        let session = Session::for_test(pool.url(), "operator@example.test", "operator-token");
        let mut http = HttpRuns::new(session);
        let request = ManualDispatchRequest {
            project: "proj_1".to_owned(),
            trigger: "on-push".to_owned(),
            actor: "alice".to_owned(),
            revision: None,
            delivery_key: "key-2".to_owned(),
            input: Some("main".to_owned()),
        };
        let response = http.dispatch(&request).expect("dispatch");
        assert_eq!(response.run.id, "run-2");
        assert!(!response.deduplicated);

        let captured = pool.finish();
        assert_eq!(
            captured[0].authorization.as_deref(),
            Some("Bearer operator-token")
        );
        let body = captured[0].json();
        assert_eq!(body["project"], serde_json::json!("proj_1"));
        assert_eq!(body["deliveryKey"], serde_json::json!("key-2"));
        assert_eq!(body["input"], serde_json::json!("main"));
        assert!(
            body.get("revision").is_none(),
            "an unset optional field must not be sent"
        );
    }

    #[test]
    fn a_run_id_that_is_not_path_safe_is_refused_before_the_request() {
        let session = Session::for_test("http://127.0.0.1:1", "operator@example.test", "token");
        let mut http = HttpRuns::new(session);
        let error = http.run_detail("../v1/pool/members").expect_err("unsafe");
        assert!(matches!(error, RunsError::InvalidId { .. }));
    }

    #[test]
    fn an_incomplete_dispatch_is_refused_before_the_request() {
        let session = Session::for_test("http://127.0.0.1:1", "operator@example.test", "token");
        let mut http = HttpRuns::new(session);
        let request = ManualDispatchRequest {
            project: "proj_1".to_owned(),
            trigger: String::new(),
            actor: "alice".to_owned(),
            revision: None,
            delivery_key: "key-1".to_owned(),
            input: None,
        };
        let error = http.dispatch(&request).expect_err("incomplete");
        assert!(matches!(error, RunsError::InvalidRequest { .. }));
    }

    #[test]
    fn a_problem_document_becomes_a_named_error() {
        let pool = ScriptedHttpPool::start(&[(
            "GET",
            "/v1/activity/runs/run-404",
            404,
            r#"{"code":"not_found","detail":"no such run"}"#,
        )]);
        let session = Session::for_test(pool.url(), "operator@example.test", "token");
        let mut http = HttpRuns::new(session);
        match http.run_detail("run-404") {
            Err(RunsError::Problem {
                status,
                code,
                detail,
            }) => {
                assert_eq!(status, 404);
                assert_eq!(code, "not_found");
                assert!(detail.contains("no such run"));
            }
            other => panic!("expected a problem, got {other:?}"),
        }
        let _ = pool.finish();
    }

    #[test]
    fn a_401_is_unauthorized_and_a_403_is_forbidden() {
        let pool = ScriptedHttpPool::start(&[
            (
                "GET",
                "/v1/activity?limit=50",
                401,
                r#"{"code":"unauthorized"}"#,
            ),
            (
                "POST",
                "/v1/runs",
                403,
                r#"{"detail":"a viewer may not dispatch"}"#,
            ),
        ]);
        let session = Session::for_test(pool.url(), "operator@example.test", "stale");
        let mut http = HttpRuns::new(session);
        let filters = ActivityFilters::default();
        assert!(matches!(
            http.list_activity(&filters.query()),
            Err(RunsError::Unauthorized)
        ));
        let request = ManualDispatchRequest {
            project: "proj_1".to_owned(),
            trigger: "on-push".to_owned(),
            actor: "alice".to_owned(),
            revision: None,
            delivery_key: "key-1".to_owned(),
            input: None,
        };
        match http.dispatch(&request) {
            Err(RunsError::Forbidden { detail }) => assert!(detail.contains("viewer")),
            other => panic!("expected forbidden, got {other:?}"),
        }
        let _ = pool.finish();
    }

    #[test]
    fn an_unreachable_pool_is_a_transport_error_not_a_panic() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        drop(listener);
        let session = Session::for_test(&format!("http://{address}"), "person@example.test", "t");
        let mut http = HttpRuns::new(session);
        let filters = ActivityFilters::default();
        let error = http
            .list_activity(&filters.query())
            .expect_err("closed port");
        assert!(matches!(error, RunsError::Unreachable { .. }));
    }

    #[test]
    fn the_wire_shapes_accept_unknown_fields_and_steps() {
        let run: RunView = serde_json::from_str(
            r#"{
                "id": "run-1",
                "project": "proj_1",
                "trigger": "on-push",
                "triggerName": "on-push",
                "workflow": "deploy",
                "origin": "manual",
                "status": "running",
                "requestText": "",
                "inputs": {},
                "createdAt": "2026-10-29T00:00:00Z",
                "somethingNew": true,
                "steps": [{
                    "id": "step-1",
                    "step": "build",
                    "position": 1,
                    "status": "running",
                    "newField": "kept only if known"
                }]
            }"#,
        )
        .expect("run");
        assert_eq!(run.steps[0].status, "running");
        assert_eq!(
            run.steps[0],
            RunStepView {
                id: "step-1".to_owned(),
                step: "build".to_owned(),
                position: 1,
                status: "running".to_owned(),
                ..RunStepView::default()
            }
        );
    }
}
