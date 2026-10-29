//! The end-to-end flow the task names: sign in, enrol a host, install a
//! trigger, dispatch a run and follow it to completion.
//!
//! One recorded server answers the whole script, one credential store carries
//! the session between commands, and every command goes through the real entry
//! point. This is hermetic: no pool, database, Redis or browser is involved.

mod support;

use std::path::{Path, PathBuf};

use support::fixtures::*;
use support::{Reply, ScriptedServer, TempDir};
use tidepool_client::types::{
    ActivityRunDetail, HostView, ManualDispatchResponse, TriggerInstallResponse,
};

fn invoke_ok(tokens: &[&str], store: &Path, stdin: &str) -> String {
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
    if let Err(error) = result {
        panic!("command {tokens:?} failed: {error}");
    }
    String::from_utf8(output).expect("utf8 output")
}

fn store_path(directory: &TempDir) -> PathBuf {
    directory.path().join("credentials.json")
}

#[test]
fn sign_in_enrol_install_dispatch_and_follow_a_run_to_completion() {
    let running = run_detail("running");
    let succeeded = run_detail("succeeded");
    let server = ScriptedServer::start(vec![
        Reply::json(200, LOGIN),
        Reply::json(201, HOST_PENDING),
        Reply::json(201, MINTED),
        Reply::json(200, HOST),
        Reply::json(200, INSTALLED),
        Reply::json(201, DISPATCHED),
        Reply::json(200, &running),
        Reply::json(200, &succeeded),
    ]);
    let directory = TempDir::new("e2e");
    let store = store_path(&directory);
    let trigger_path = directory.path().join("on-push.json");
    std::fs::write(&trigger_path, TRIGGER_DOCUMENT).expect("trigger file");
    let trigger_path = trigger_path.to_str().expect("path");

    // 1. Sign in; the token lands in the CLI's store, nowhere else.
    let mut printed = invoke_ok(
        &[
            "login",
            "--pool",
            server.url(),
            "--email",
            "ops@example.com",
            "--password-stdin",
        ],
        &store,
        "correct horse battery\n",
    );
    assert!(printed.contains("ops@example.com"));

    // 2. Enrol this machine: register, mint, exchange, store the credential.
    let enrolled_output = invoke_ok(
        &[
            "host",
            "enroll",
            "--hostname",
            "build-box.local",
            "--display-name",
            "Build box",
            "--json",
        ],
        &store,
        "",
    );
    printed.push_str(&enrolled_output);
    let enrolled: HostView =
        serde_json::from_str(&enrolled_output).expect("the enrolled host is the SDK type");
    assert_eq!(enrolled.id, "host_1");

    // 3. Install the trigger; the pool owns validation and revisioning.
    let installed_output = invoke_ok(
        &[
            "trigger",
            "install",
            "--project",
            "proj_1",
            "--file",
            trigger_path,
            "--json",
        ],
        &store,
        "",
    );
    printed.push_str(&installed_output);
    let installed: TriggerInstallResponse =
        serde_json::from_str(&installed_output).expect("the install is the SDK type");
    assert_eq!(installed.version, 2);
    assert!(installed.active);

    // 4. Dispatch a manual run.
    let dispatched_output = invoke_ok(
        &[
            "run",
            "dispatch",
            "--project",
            "proj_1",
            "--trigger",
            "on-push",
            "--input",
            "fix the build",
            "--delivery-key",
            "key-1",
            "--json",
        ],
        &store,
        "",
    );
    printed.push_str(&dispatched_output);
    let dispatched: ManualDispatchResponse =
        serde_json::from_str(&dispatched_output).expect("the dispatch is the SDK type");
    assert_eq!(dispatched.run.id, "run_1");
    assert!(!dispatched.deduplicated);

    // 5. Follow it until the pool says it finished.
    let watched_output = invoke_ok(
        &[
            "run",
            "watch",
            "--run",
            &dispatched.run.id,
            "--interval-ms",
            "0",
            "--json",
        ],
        &store,
        "",
    );
    printed.push_str(&watched_output);
    let watched: ActivityRunDetail =
        serde_json::from_str(&watched_output).expect("the run detail is the SDK type");
    assert_eq!(watched.run.status, "succeeded");

    // The whole session is one host-credential and one token: no command wrote
    // either to the terminal.
    let stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&store).expect("store")).expect("store JSON");
    let host_credential = stored["host_credentials"]["host_1"]
        .as_str()
        .expect("the host credential is stored");
    assert!(host_credential.len() >= 32);
    assert!(
        !printed.contains(FULL_TOKEN),
        "the operator token was printed"
    );
    assert!(
        !printed.contains(MINTED_TOKEN),
        "the enrollment token was printed"
    );
    assert!(
        !printed.contains(host_credential),
        "the host credential was printed"
    );
    assert_eq!(stored["token"], FULL_TOKEN);
    assert_eq!(stored["active_host"], "host_1");

    // Every step was exactly the call the pool's contract declares.
    let requests = server.join();
    let calls: Vec<(String, String)> = requests
        .iter()
        .map(|request| (request.method.clone(), request.path.clone()))
        .collect();
    assert_eq!(
        calls,
        vec![
            ("POST".to_owned(), "/v1/pool/login".to_owned()),
            ("POST".to_owned(), "/v1/hosts".to_owned()),
            ("POST".to_owned(), "/v1/hosts/enrollment-tokens".to_owned()),
            ("POST".to_owned(), "/v1/hosts/enroll".to_owned()),
            ("POST".to_owned(), "/v1/triggers/install".to_owned()),
            ("POST".to_owned(), "/v1/runs".to_owned()),
            ("GET".to_owned(), "/v1/activity/runs/run_1".to_owned()),
            ("GET".to_owned(), "/v1/activity/runs/run_1".to_owned()),
        ]
    );

    // The exchange is two steps and the credential never travels anywhere else.
    assert_eq!(requests[2].json()["host_id"], "host_1");
    assert_eq!(requests[3].json()["token"], MINTED_TOKEN);
    assert_eq!(requests[3].json()["credential"], host_credential);
    assert_eq!(requests[5].json()["deliveryKey"], "key-1");
    assert_eq!(requests[5].json()["actor"], "ops@example.com");
}
