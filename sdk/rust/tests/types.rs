//! Contract types round-trip the shapes the pool sends.

use tidepool_client::generated::types::{
    ActivityEntry, HostView, MessageView, Problem, RelayEvent, RunView, TriggerDocument,
};

#[test]
fn host_views_round_trip_and_ignore_unknown_fields() {
    let json = r#"{
        "id": "host_1",
        "slug": "build-box",
        "display_name": "Build box",
        "hostname": "build-box.local",
        "status": "connected",
        "execution_enabled": true,
        "created_at": "2026-10-29T00:00:00Z",
        "revoked_at": null,
        "future_field": {"nested": 1}
    }"#;
    let host: HostView = serde_json::from_str(json).expect("host");
    assert_eq!(host.slug, "build-box");
    assert_eq!(host.status, "connected");
    assert!(host.execution_enabled);
    assert!(host.revoked_at.is_none());
    let encoded = serde_json::to_value(&host).expect("serialise");
    assert_eq!(encoded["display_name"], "Build box");
}

#[test]
fn camel_case_fields_keep_their_wire_names() {
    let json = r#"{
        "id": "run_1",
        "project": "proj_1",
        "trigger": "nightly",
        "status": "running",
        "origin": "manual",
        "deliveryKey": "key_1",
        "createdAt": "2026-10-29T00:00:00Z",
        "completedAt": null,
        "deadlineAt": null,
        "triggerName": "Nightly",
        "workflow": "wf_1",
        "requestText": "run it",
        "inputs": {"branch": "main"},
        "steps": []
    }"#;
    let run: RunView = serde_json::from_str(json).expect("run");
    assert_eq!(run.delivery_key, "key_1");
    assert_eq!(run.created_at, "2026-10-29T00:00:00Z");
    assert_eq!(run.inputs.get("branch").map(String::as_str), Some("main"));
    let encoded = serde_json::to_value(&run).expect("serialise");
    assert!(encoded.get("deliveryKey").is_some());
    assert!(encoded.get("delivery_key").is_none());
}

#[test]
fn open_enums_accept_unknown_values() {
    let json = r#"{
        "id": "entry_1",
        "at": "2026-10-29T00:00:00Z",
        "kind": "future_kind",
        "status": "future_status"
    }"#;
    let entry: ActivityEntry = serde_json::from_str(json).expect("entry");
    assert_eq!(entry.kind, "future_kind");
    assert_eq!(entry.status, "future_status");
}

#[test]
fn messages_keep_their_server_sequence_and_author_kind() {
    let json = r#"{
        "id": "msg_1",
        "project": "proj_1",
        "body": "hello",
        "created_at": "2026-10-29T00:00:00Z",
        "sequence": 42,
        "author": {"id": "session_1", "kind": "session"},
        "reply_to": null
    }"#;
    let message: MessageView = serde_json::from_str(json).expect("message");
    assert_eq!(message.sequence, 42);
    assert_eq!(message.author.expect("author").kind, "session");
}

#[test]
fn relay_events_keep_their_camel_case_timestamps() {
    let json = r#"{
        "id": "evt_1",
        "session": "session_1",
        "kind": "output",
        "createdAt": "2026-10-29T00:00:00Z",
        "sequence": 7,
        "payload": {"text": "hi"}
    }"#;
    let event: RelayEvent = serde_json::from_str(json).expect("event");
    assert_eq!(event.created_at, "2026-10-29T00:00:00Z");
    assert_eq!(event.payload["text"], "hi");
    let encoded = serde_json::to_value(&event).expect("serialise");
    assert!(encoded.get("createdAt").is_some());
}

#[test]
fn trigger_documents_round_trip_the_authored_shape() {
    let json = r#"{
        "name": "nightly",
        "event": {"provider": "github", "type": "push"},
        "filters": {"connection": "conn_1", "contains": "main", "from_users": ["alice"], "resource": {"id": "proj_1", "kind": "project"}},
        "run": {"host": "build-box", "agent": {"provider": "claude", "model": "opus", "execution_mode": "stream", "thinking_option": "high"}}
    }"#;
    let trigger: TriggerDocument = serde_json::from_str(json).expect("trigger");
    assert_eq!(trigger.name, "nightly");
    assert_eq!(trigger.event.provider, "github");
    let encoded = serde_json::to_value(&trigger).expect("serialise");
    assert_eq!(encoded["event"]["provider"], "github");
}

#[test]
fn problem_documents_tolerate_a_partial_body() {
    let problem: Problem = serde_json::from_str(r#"{"code":"not_found"}"#).expect("problem");
    assert_eq!(problem.code, "not_found");
    assert!(problem.detail.is_none());
    assert!(problem.request_id.is_none());

    let encoded = serde_json::to_value(&problem).expect("serialise");
    assert_eq!(encoded["code"], "not_found");
}

#[test]
fn a_required_field_missing_is_a_decode_error_not_a_panic() {
    let error = serde_json::from_str::<HostView>(r#"{"id":"host_1"}"#).expect_err("must fail");
    assert!(error.to_string().contains("missing field"));
}
