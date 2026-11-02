//! One test per command, over a recorded server.
//!
//! Every command runs through the real entry point (`run_with_path`), so the
//! grammar, the credential store, the SDK call and the printed output are all
//! exercised. No live pool, database or Redis is involved.

mod support;

use std::path::{Path, PathBuf};

use support::fixtures::*;
use support::{Reply, ScriptedServer, TempDir};
use tidepool_client::types::{
    ActivityRunDetail, HostView, HostsResponse, ManualDispatchResponse, MessageView, RelayEvent,
    RevisionView, RoomView, SetupStatusResponse, TriggerInstallResponse, TriggerValidateResponse,
};

fn store_path(directory: &TempDir) -> PathBuf {
    directory.path().join("credentials.json")
}

fn signed_in_store(path: &Path, pool: &str) {
    let store = serde_json::json!({
        "pool": pool,
        "email": "ops@example.com",
        "token": FULL_TOKEN
    });
    std::fs::write(path, serde_json::to_vec_pretty(&store).expect("json")).expect("store");
}

fn read_store(path: &Path) -> serde_json::Value {
    let bytes = std::fs::read(path).expect("store exists");
    serde_json::from_slice(&bytes).expect("store is JSON")
}

fn invoke(tokens: &[&str], store: &Path, stdin: &str) -> (tidepool_cli::Result<()>, String) {
    let mut input = std::io::Cursor::new(stdin.as_bytes().to_vec());
    let mut output = Vec::new();
    let result = tidepool_cli::run_with_path(
        tokens.iter().map(|token| (*token).to_owned()).collect(),
        store.to_path_buf(),
        tidepool_cli::Runtime {
            stdin: &mut input,
            out: &mut output,
        },
    );
    (result, String::from_utf8(output).expect("utf8 output"))
}

fn invoke_ok(tokens: &[&str], store: &Path, stdin: &str) -> String {
    let (result, output) = invoke(tokens, store, stdin);
    if let Err(error) = result {
        panic!("command {tokens:?} failed: {error}");
    }
    output
}

fn write_document(directory: &TempDir) -> PathBuf {
    let path = directory.path().join("trigger.json");
    std::fs::write(&path, TRIGGER_DOCUMENT).expect("trigger file");
    path
}

#[test]
fn login_stores_the_token_and_never_prints_it() {
    let server = ScriptedServer::start(vec![Reply::json(200, LOGIN)]);
    let directory = TempDir::new("login");
    let path = store_path(&directory);

    let output = invoke_ok(
        &[
            "login",
            "--pool",
            server.url(),
            "--email",
            "ops@example.com",
            "--password-stdin",
        ],
        &path,
        "correct horse battery\n",
    );

    assert!(
        !output.contains(FULL_TOKEN),
        "the token was printed in full"
    );
    assert!(output.contains("abcd"));
    assert!(output.contains("ops@example.com"));
    assert!(output.contains("owner"));

    let stored = read_store(&path);
    assert_eq!(stored["token"], FULL_TOKEN);
    assert_eq!(stored["pool"], server.url());
    assert_eq!(stored["email"], "ops@example.com");

    let requests = server.join();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/v1/pool/login");
    assert_eq!(requests[0].json()["email"], "ops@example.com");
}

#[test]
fn login_json_is_the_sdk_type_with_a_redacted_token() {
    let server = ScriptedServer::start(vec![Reply::json(200, LOGIN)]);
    let directory = TempDir::new("login-json");
    let path = store_path(&directory);

    let output = invoke_ok(
        &[
            "login",
            "--pool",
            server.url(),
            "--email",
            "ops@example.com",
            "--password-stdin",
            "--json",
        ],
        &path,
        "correct horse battery\n",
    );

    let report: tidepool_client::types::PoolLoginResponse =
        serde_json::from_str(&output).expect("SDK type");
    assert_eq!(report.user.email, "ops@example.com");
    assert_eq!(report.expires_in, 3600);
    assert_ne!(report.token, FULL_TOKEN);
    assert!(report.token.contains("<redacted"));
    assert!(!output.contains(FULL_TOKEN));

    let stored = read_store(&path);
    assert_eq!(stored["token"], FULL_TOKEN);
}

#[test]
fn login_refuses_to_take_a_password_from_the_command_line() {
    let directory = TempDir::new("login-no-stdin");
    let path = store_path(&directory);
    let (result, output) = invoke(
        &[
            "login",
            "--pool",
            "http://127.0.0.1:1",
            "--email",
            "ops@example.com",
        ],
        &path,
        "unused\n",
    );
    let error = result.expect_err("no password flag");
    assert!(error.to_string().contains("--password-stdin"));
    assert!(output.is_empty());
    assert!(!path.exists());
}

#[test]
fn pool_status_json_is_the_sdk_type_and_needs_no_credential() {
    let server = ScriptedServer::start(vec![Reply::json(200, SETUP_STATUS)]);
    let directory = TempDir::new("pool-status");
    let path = store_path(&directory);

    let output = invoke_ok(
        &["pool", "status", "--pool", server.url(), "--json"],
        &path,
        "",
    );

    let expected: SetupStatusResponse = serde_json::from_str(SETUP_STATUS).expect("fixture");
    let actual: SetupStatusResponse = serde_json::from_str(&output).expect("SDK type");
    assert_eq!(actual, expected);
    assert!(!actual.needs_setup);
    assert!(actual.bootstrap_configured);

    let requests = server.join();
    assert_eq!(requests[0].path, "/v1/setup/status");
    assert_eq!(requests[0].header("authorization"), None);
}

#[test]
fn host_list_json_is_the_sdk_type() {
    let server = ScriptedServer::start(vec![Reply::json(200, HOSTS)]);
    let directory = TempDir::new("host-list");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(&["host", "list", "--json"], &path, "");

    let expected: HostsResponse = serde_json::from_str(HOSTS).expect("fixture");
    let actual: HostsResponse = serde_json::from_str(&output).expect("SDK type");
    assert_eq!(actual, expected);
    assert_eq!(actual.hosts.len(), 2);

    let requests = server.join();
    assert_eq!(requests.len(), 1, "host list must not page in the client");
    assert_eq!(requests[0].path, "/v1/hosts");
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer abcd1234-the-whole-operator-token")
    );
}

#[test]
fn host_list_human_mode_lists_each_host() {
    let server = ScriptedServer::start(vec![Reply::json(200, HOSTS)]);
    let directory = TempDir::new("host-list-human");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(&["host", "list"], &path, "");
    assert!(output.contains("build-box"));
    assert!(output.contains("test-box"));
    assert!(output.contains("offline"));
    server.join();
}

#[test]
fn host_enroll_completes_the_two_step_exchange_without_a_browser() {
    let server = ScriptedServer::start(vec![Reply::json(201, MINTED), Reply::json(200, HOST)]);
    let directory = TempDir::new("host-enroll-existing");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(&["host", "enroll", "--host", "host_1", "--json"], &path, "");

    let enrolled: HostView = serde_json::from_str(&output).expect("SDK type");
    assert_eq!(enrolled.id, "host_1");
    assert_eq!(enrolled.status, "connected");

    let requests = server.join();
    assert_eq!(requests.len(), 2, "mint then exchange, nothing else");
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/v1/hosts/enrollment-tokens");
    assert_eq!(requests[0].json()["host_id"], "host_1");
    assert_eq!(requests[1].method, "POST");
    assert_eq!(requests[1].path, "/v1/hosts/enroll");

    let body = requests[1].json();
    assert_eq!(body["token"], MINTED_TOKEN);
    let credential = body["credential"].as_str().expect("credential");
    assert!(credential.len() >= 32, "host credential must be long");
    assert!(
        !output.contains(credential),
        "the credential was printed in full"
    );
    assert!(
        !output.contains(MINTED_TOKEN),
        "the enrollment token was printed"
    );

    let stored = read_store(&path);
    assert_eq!(stored["host_credentials"]["host_1"], credential);
    assert_eq!(stored["active_host"], "host_1");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path)
            .expect("store metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "the store must stay private");
    }
}

#[test]
fn host_enroll_human_mode_names_the_redacted_credential() {
    let server = ScriptedServer::start(vec![Reply::json(201, MINTED), Reply::json(200, HOST)]);
    let directory = TempDir::new("host-enroll-human");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(&["host", "enroll", "--host", "host_1"], &path, "");
    assert!(output.contains("enrolled build-box (host_1)"));
    assert!(output.contains("credential"));
    assert!(output.contains("<redacted"));
    assert!(output.contains(path.to_str().expect("store path")));

    let stored = read_store(&path);
    let credential = stored["host_credentials"]["host_1"]
        .as_str()
        .expect("stored credential");
    assert!(
        !output.contains(credential),
        "the credential was printed in full"
    );
    assert!(
        !output.contains(MINTED_TOKEN),
        "the enrollment token was printed"
    );
    server.join();
}

#[test]
fn host_enroll_registers_a_new_host_first_when_none_is_named() {
    let server = ScriptedServer::start(vec![
        Reply::json(201, HOST_PENDING),
        Reply::json(201, MINTED),
        Reply::json(200, HOST),
    ]);
    let directory = TempDir::new("host-enroll-new");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(
        &[
            "host",
            "enroll",
            "--hostname",
            "build-box.local",
            "--display-name",
            "Build box",
            "--json",
        ],
        &path,
        "",
    );

    let enrolled: HostView = serde_json::from_str(&output).expect("SDK type");
    assert_eq!(enrolled.status, "connected");

    let requests = server.join();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].path, "/v1/hosts");
    assert_eq!(requests[0].json()["hostname"], "build-box.local");
    assert_eq!(requests[1].path, "/v1/hosts/enrollment-tokens");
    assert_eq!(requests[2].path, "/v1/hosts/enroll");
}

#[test]
fn host_enroll_needs_a_hostname_or_an_existing_host() {
    let directory = TempDir::new("host-enroll-usage");
    let path = store_path(&directory);
    signed_in_store(&path, "http://127.0.0.1:1");
    let (result, _) = invoke(&["host", "enroll"], &path, "");
    let error = result.expect_err("usage");
    assert!(error.to_string().contains("--hostname"));
}

#[test]
fn host_grant_calls_the_grant_endpoint_and_returns_the_host() {
    let server = ScriptedServer::start(vec![Reply::json(200, HOST)]);
    let directory = TempDir::new("host-grant");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(&["host", "grant", "--host", "host_1", "--json"], &path, "");
    let granted: HostView = serde_json::from_str(&output).expect("SDK type");
    assert!(granted.execution_enabled);

    let requests = server.join();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/v1/hosts/host_1/execution");
}

#[test]
fn host_grant_revoke_calls_the_verb_that_revokes() {
    let server = ScriptedServer::start(vec![Reply::json(200, HOST)]);
    let directory = TempDir::new("host-grant-revoke");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(
        &["host", "grant", "--host", "host_1", "--revoke"],
        &path,
        "",
    );
    assert!(output.contains("revoked"));

    let requests = server.join();
    assert_eq!(requests[0].method, "DELETE");
    assert_eq!(requests[0].path, "/v1/hosts/host_1/execution");
}

#[test]
fn trigger_list_reads_the_pools_active_revision() {
    let server = ScriptedServer::start(vec![Reply::json(200, REVISION)]);
    let directory = TempDir::new("trigger-list");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let human = invoke_ok(&["trigger", "list", "--project", "proj_1"], &path, "");
    assert!(human.contains("triggers/on-push.yaml"));
    assert!(human.contains("version 3"));

    let requests = server.join();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/projects/proj_1/configuration");

    let server = ScriptedServer::start(vec![Reply::json(200, REVISION)]);
    signed_in_store(&path, server.url());
    let output = invoke_ok(
        &["trigger", "list", "--project", "proj_1", "--json"],
        &path,
        "",
    );
    let expected: RevisionView = serde_json::from_str(REVISION).expect("fixture");
    let actual: RevisionView = serde_json::from_str(&output).expect("SDK type");
    assert_eq!(actual, expected);
    server.join();
}

#[test]
fn trigger_validate_sends_the_file_and_reports_the_pools_answer() {
    let server = ScriptedServer::start(vec![Reply::json(200, VALIDATION)]);
    let directory = TempDir::new("trigger-validate");
    let path = store_path(&directory);
    let document = write_document(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(
        &[
            "trigger",
            "validate",
            "--file",
            document.to_str().expect("path"),
            "--json",
        ],
        &path,
        "",
    );

    let expected: TriggerValidateResponse = serde_json::from_str(VALIDATION).expect("fixture");
    let actual: TriggerValidateResponse = serde_json::from_str(&output).expect("SDK type");
    assert_eq!(actual, expected);
    assert!(actual.valid);

    let requests = server.join();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/triggers/validate");
    let document: tidepool_client::types::TriggerDocument =
        serde_json::from_str(TRIGGER_DOCUMENT).expect("fixture");
    assert_eq!(
        requests[0].json(),
        serde_json::to_value(document).expect("document"),
        "the document is sent as the SDK models it"
    );
}

#[test]
fn trigger_validate_reports_the_pools_issues_in_words() {
    let invalid = r#"{"name": "on-push", "valid": false,
        "errors": [{"field": "filters.from_users", "message": "must not be empty"}]}"#;
    let server = ScriptedServer::start(vec![Reply::json(200, invalid)]);
    let directory = TempDir::new("trigger-invalid");
    let path = store_path(&directory);
    let document = write_document(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(
        &[
            "trigger",
            "validate",
            "--file",
            document.to_str().expect("path"),
        ],
        &path,
        "",
    );
    assert!(output.contains("is invalid"));
    assert!(output.contains("filters.from_users"));
    assert!(output.contains("must not be empty"));
    server.join();
}

#[test]
fn trigger_install_adds_the_project_and_returns_the_install() {
    let server = ScriptedServer::start(vec![Reply::json(200, INSTALLED)]);
    let directory = TempDir::new("trigger-install");
    let path = store_path(&directory);
    let document = write_document(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(
        &[
            "trigger",
            "install",
            "--project",
            "proj_1",
            "--file",
            document.to_str().expect("path"),
            "--json",
        ],
        &path,
        "",
    );

    let expected: TriggerInstallResponse = serde_json::from_str(INSTALLED).expect("fixture");
    let actual: TriggerInstallResponse = serde_json::from_str(&output).expect("SDK type");
    assert_eq!(actual, expected);
    assert_eq!(actual.version, 2);

    let requests = server.join();
    assert_eq!(requests.len(), 1, "install validates in the pool, not here");
    assert_eq!(requests[0].path, "/v1/triggers/install");
    let body = requests[0].json();
    assert_eq!(body["project"], "proj_1");
    assert_eq!(body["name"], "on-push");
    assert_eq!(body["filters"]["from_users"][0], "max");
}

#[test]
fn run_dispatch_uses_the_signed_in_member_as_actor() {
    let server = ScriptedServer::start(vec![Reply::json(201, DISPATCHED)]);
    let directory = TempDir::new("run-dispatch");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(
        &[
            "run",
            "dispatch",
            "--project",
            "proj_1",
            "--trigger",
            "on-push",
            "--input",
            "fix the build",
            "--json",
        ],
        &path,
        "",
    );

    let expected: ManualDispatchResponse = serde_json::from_str(DISPATCHED).expect("fixture");
    let actual: ManualDispatchResponse = serde_json::from_str(&output).expect("SDK type");
    assert_eq!(actual, expected);

    let requests = server.join();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/runs");
    let body = requests[0].json();
    assert_eq!(body["actor"], "ops@example.com");
    assert_eq!(body["project"], "proj_1");
    assert_eq!(body["trigger"], "on-push");
    let delivery_key = body["deliveryKey"].as_str().expect("deliveryKey");
    assert!(!delivery_key.is_empty());
    assert_eq!(body["input"], "fix the build");
}

#[test]
fn run_dispatch_keeps_the_delivery_key_a_script_supplies() {
    let server = ScriptedServer::start(vec![Reply::json(201, DISPATCHED)]);
    let directory = TempDir::new("run-dispatch-key");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    invoke_ok(
        &[
            "run",
            "dispatch",
            "--project",
            "proj_1",
            "--trigger",
            "on-push",
            "--delivery-key",
            "key-1",
        ],
        &path,
        "",
    );
    let requests = server.join();
    assert_eq!(requests[0].json()["deliveryKey"], "key-1");
}

#[test]
fn run_watch_follows_the_pool_until_the_run_finishes() {
    let finished = run_detail("succeeded");
    let server = ScriptedServer::start(vec![
        Reply::json(200, &run_detail("running")),
        Reply::json(200, &finished),
    ]);
    let directory = TempDir::new("run-watch");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(
        &[
            "run",
            "watch",
            "--run",
            "run_1",
            "--interval-ms",
            "0",
            "--json",
        ],
        &path,
        "",
    );

    let expected: ActivityRunDetail = serde_json::from_str(&finished).expect("fixture");
    let actual: ActivityRunDetail = serde_json::from_str(&output).expect("SDK type");
    assert_eq!(actual, expected);
    assert_eq!(actual.run.status, "succeeded");

    let requests = server.join();
    assert_eq!(requests.len(), 2, "one poll per state, then stop");
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/v1/activity/runs/run_1");
    }
}

#[test]
fn run_watch_stops_after_max_polls_on_a_run_that_never_finishes() {
    let server = ScriptedServer::start(vec![
        Reply::json(200, &run_detail("running")),
        Reply::json(200, &run_detail("running")),
    ]);
    let directory = TempDir::new("run-watch-max");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(
        &[
            "run",
            "watch",
            "--run",
            "run_1",
            "--interval-ms",
            "0",
            "--max-polls",
            "2",
        ],
        &path,
        "",
    );
    assert!(output.contains("status=running"));
    assert_eq!(server.join().len(), 2);
}

#[test]
fn run_watch_refuses_a_non_terminal_failure_from_the_pool() {
    let problem = r#"{"type": "about:blank", "title": "Not Found", "status": 404,
        "code": "not_found", "detail": "no such run", "requestId": "req_1"}"#;
    let server = ScriptedServer::start(vec![Reply::problem(404, problem)]);
    let directory = TempDir::new("run-watch-404");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let (result, _) = invoke(&["run", "watch", "--run", "run_1"], &path, "");
    let error = result.expect_err("not found");
    let rendered = error.to_string();
    assert!(rendered.contains("404"));
    assert!(rendered.contains("not_found"));
    assert!(rendered.contains("req_1"));
    assert!(!rendered.contains(FULL_TOKEN));
    server.join();
}

#[test]
fn room_list_json_is_the_sdk_type() {
    let server = ScriptedServer::start(vec![Reply::json(200, ROOMS)]);
    let directory = TempDir::new("room-list");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(&["room", "list", "--json"], &path, "");
    let expected: Vec<RoomView> = serde_json::from_str(ROOMS).expect("fixture");
    let actual: Vec<RoomView> = serde_json::from_str(&output).expect("SDK type");
    assert_eq!(actual, expected);

    let requests = server.join();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/chatrooms");
}

#[test]
fn room_send_posts_the_message_and_prints_the_server_copy() {
    let server = ScriptedServer::start(vec![Reply::json(201, MESSAGE)]);
    let directory = TempDir::new("room-send");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(
        &[
            "room",
            "send",
            "--room",
            "room_1",
            "--body",
            "hello",
            "--reply-to",
            "message_0",
            "--json",
        ],
        &path,
        "",
    );

    let expected: MessageView = serde_json::from_str(MESSAGE).expect("fixture");
    let actual: MessageView = serde_json::from_str(&output).expect("SDK type");
    assert_eq!(actual, expected);
    assert_eq!(actual.sequence, 4);

    let requests = server.join();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/chatrooms/room_1/messages");
    assert_eq!(requests[0].json()["body"], "hello");
    assert_eq!(requests[0].json()["replyTo"], "message_0");
}

#[test]
fn relay_follow_follows_the_sdks_cursor_not_its_own() {
    let first = format!(
        r#"{{"events": [{}], "next": 1}}"#,
        r#"{"id": "event_1", "session": "session_1", "sequence": 1, "kind": "started", "payload": {}, "createdAt": "2026-10-29T00:00:00Z"}"#
    );
    let second = format!(
        r#"{{"events": [{}], "next": null}}"#,
        r#"{"id": "event_2", "session": "session_1", "sequence": 2, "kind": "output", "payload": {"text": "hi"}, "createdAt": "2026-10-29T00:00:01Z"}"#
    );
    let server = ScriptedServer::start(vec![Reply::json(200, &first), Reply::json(200, &second)]);
    let directory = TempDir::new("relay-follow");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(
        &["relay", "follow", "--session", "session_1", "--json"],
        &path,
        "",
    );

    let events: Vec<RelayEvent> = output
        .lines()
        .map(|line| serde_json::from_str(line).expect("one SDK event per line"))
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[1].kind, "output");
    assert_eq!(events[1].payload["text"], "hi");

    let requests = server.join();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].path, "/v1/sessions/session_1/relay/events",
        "the first page asks for no cursor"
    );
    assert_eq!(
        requests[1].path, "/v1/sessions/session_1/relay/events?after=1",
        "the second page follows the pool's cursor"
    );
}

#[test]
fn relay_follow_resumes_after_the_sequence_a_script_names() {
    let page = r#"{"events": [], "next": null}"#;
    let server = ScriptedServer::start(vec![Reply::json(200, page)]);
    let directory = TempDir::new("relay-after");
    let path = store_path(&directory);
    signed_in_store(&path, server.url());

    let output = invoke_ok(
        &[
            "relay",
            "follow",
            "--session",
            "session_1",
            "--after",
            "9",
            "--limit",
            "50",
        ],
        &path,
        "",
    );
    assert!(output.is_empty());

    let requests = server.join();
    assert_eq!(
        requests[0].path,
        "/v1/sessions/session_1/relay/events?after=9&limit=50"
    );
}

#[test]
fn a_command_without_a_signed_in_store_says_so() {
    let directory = TempDir::new("not-signed-in");
    let path = store_path(&directory);
    let (result, _) = invoke(&["host", "list", "--pool", "http://127.0.0.1:1"], &path, "");
    let error = result.expect_err("not signed in");
    assert!(error.to_string().contains("tidepool login"));
}
