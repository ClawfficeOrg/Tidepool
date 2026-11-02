//! Recorded bodies shared by the command tests and the end-to-end test.
//!
//! Each constant is a body the pool's contract declares; the tests assert that
//! `--json` prints those same shapes.

/// A full operator token, long enough to be redacted.
pub const FULL_TOKEN: &str = "abcd1234-the-whole-operator-token";
/// A minted enrollment token, which must never reach the terminal.
pub const MINTED_TOKEN: &str = "enroll_1.single-use-secret";

/// `POST /v1/pool/login`.
pub const LOGIN: &str = r#"{
    "token": "abcd1234-the-whole-operator-token",
    "expires_in": 3600,
    "must_change_password": false,
    "user": {
        "id": "user_1",
        "email": "ops@example.com",
        "display_name": "Ops",
        "role": "owner",
        "created_at": "2026-10-29T00:00:00Z",
        "last_login_at": null,
        "password_change_required": false
    }
}"#;

/// `GET /v1/setup/status`.
pub const SETUP_STATUS: &str = r#"{"needs_setup": false, "bootstrap_configured": true}"#;

/// A connected host.
pub const HOST: &str = r#"{
    "id": "host_1",
    "slug": "build-box",
    "display_name": "Build box",
    "hostname": "build-box.local",
    "status": "connected",
    "execution_enabled": true,
    "created_at": "2026-10-29T00:00:00Z",
    "revoked_at": null
}"#;

/// A freshly registered host, before its token is exchanged.
pub const HOST_PENDING: &str = r#"{
    "id": "host_1",
    "slug": "build-box",
    "display_name": "Build box",
    "hostname": "build-box.local",
    "status": "pending_approval",
    "execution_enabled": false,
    "created_at": "2026-10-29T00:00:00Z",
    "revoked_at": null
}"#;

/// `GET /v1/hosts`.
pub const HOSTS: &str = r#"{"hosts": [
    {"id": "host_1", "slug": "build-box", "display_name": "Build box", "hostname": "build-box.local",
     "status": "connected", "execution_enabled": true, "created_at": "2026-10-29T00:00:00Z", "revoked_at": null},
    {"id": "host_2", "slug": "test-box", "display_name": "Test box", "hostname": "test-box.local",
     "status": "offline", "execution_enabled": false, "created_at": "2026-10-29T00:00:00Z", "revoked_at": null}
]}"#;

/// `POST /v1/hosts/enrollment-tokens`.
pub const MINTED: &str = r#"{
    "token": "enroll_1.single-use-secret",
    "enrollment_token": {
        "id": "token_1",
        "created_at": "2026-10-29T00:00:00Z",
        "expires_at": "2026-10-29T00:10:00Z",
        "consumed_at": null,
        "revoked_at": null
    }
}"#;

/// `GET /v1/projects/{id}/configuration`.
pub const REVISION: &str = r#"{
    "id": "rev_1",
    "version": 3,
    "active": true,
    "status": "valid",
    "created_at": "2026-10-29T00:00:00Z",
    "files": [
        {"path": "triggers/on-push.yaml", "content": "name: on-push\n"},
        {"path": "workflows/build.yaml", "content": "steps: []\n"}
    ],
    "errors": []
}"#;

/// A trigger document, as `trigger validate` and `trigger install` read it.
pub const TRIGGER_DOCUMENT: &str = r#"{
    "name": "on-push",
    "event": {"provider": "github", "type": "push"},
    "filters": {
        "connection": "conn_1",
        "from_users": ["max"],
        "resource": {"kind": "repository", "id": "repo_1"}
    },
    "run": {
        "host": "host_1",
        "agent": {"provider": "anthropic", "model": "claude-sonnet", "execution_mode": "batch", "thinking_option": null}
    }
}"#;

/// `POST /v1/triggers/validate` for a valid document.
pub const VALIDATION: &str = r#"{"name": "on-push", "valid": true, "errors": []}"#;

/// `POST /v1/triggers/install`.
pub const INSTALLED: &str = r#"{
    "id": "trigger_1",
    "name": "on-push",
    "revision": "rev_2",
    "version": 2,
    "active": true,
    "replaced": false
}"#;

/// A finished run.
pub const RUN: &str = r#"{
    "id": "run_1",
    "project": "proj_1",
    "trigger": "trigger_1",
    "triggerName": "on-push",
    "workflow": "build",
    "status": "succeeded",
    "origin": "manual",
    "actor": "ops@example.com",
    "deliveryKey": "key-1",
    "inputs": {},
    "requestText": "fix the build",
    "createdAt": "2026-10-29T00:00:00Z",
    "startedAt": "2026-10-29T00:00:01Z",
    "completedAt": "2026-10-29T00:00:02Z",
    "deadlineAt": null,
    "steps": []
}"#;

/// `POST /v1/runs`.
pub const DISPATCHED: &str = r#"{
    "deduplicated": false,
    "run": {
        "id": "run_1", "project": "proj_1", "trigger": "trigger_1", "triggerName": "on-push",
        "workflow": "build", "status": "pending", "origin": "manual", "actor": "ops@example.com",
        "deliveryKey": "key-1", "inputs": {}, "requestText": "fix the build",
        "createdAt": "2026-10-29T00:00:00Z", "startedAt": null, "completedAt": null,
        "deadlineAt": null, "steps": []
    }
}"#;

/// The arrival half of an activity run detail.
pub const ARRIVAL: &str = r#"{
    "mode": "created",
    "conversationKey": null,
    "outputLimits": [],
    "provider": "github",
    "receipt": "receipt_1",
    "steered": false
}"#;

/// `GET /v1/chatrooms`.
pub const ROOMS: &str = r#"[
    {"id": "room_1", "name": "builds", "visibility": "private", "origin_pool": null,
     "created_at": "2026-10-29T00:00:00Z", "updated_at": "2026-10-29T00:00:00Z"}
]"#;

/// `POST /v1/chatrooms/{room}/messages`.
pub const MESSAGE: &str = r#"{
    "id": "message_1",
    "project": "proj_1",
    "body": "hello",
    "created_at": "2026-10-29T00:00:00Z",
    "sequence": 4,
    "author": {"id": "user_1", "kind": "user"},
    "reply_to": null
}"#;

/// An activity run detail for a run in `status`.
pub fn run_detail(status: &str) -> String {
    format!(
        r#"{{"arrival": {ARRIVAL}, "run": {}}}"#,
        RUN.replace("\"succeeded\"", &format!("\"{status}\""))
    )
}
