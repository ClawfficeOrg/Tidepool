//! The blocking facade against a scripted HTTP server: no runtime, no pool.

#![cfg(feature = "blocking")]
#![allow(clippy::result_large_err)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use support::{Reply, ScriptedServer};
use tidepool_client::CallOptions;
use tidepool_client::Error;
use tidepool_client::TokenUpdate;
use tidepool_client::blocking::Client;
use tidepool_client::endpoints::{
    DeleteV1HostsId, GetV1Activity, GetV1ActivityQuery, GetV1Hosts, GetV1HostsId, PostV1Hosts,
};
use tidepool_client::generated::types::{CreateHostRequest, HostView};

const HOST: &str = r#"{
    "id": "host_1",
    "slug": "build-box",
    "display_name": "Build box",
    "hostname": "build-box.local",
    "status": "connected",
    "execution_enabled": true,
    "created_at": "2026-10-29T00:00:00Z",
    "revoked_at": null
}"#;

fn client(server: &ScriptedServer) -> Client {
    Client::from_base_url(server.url())
        .expect("client")
        .with_token("jwt")
}

#[test]
fn typed_get_sends_credentials_and_decodes_the_response() {
    let server = ScriptedServer::start(vec![Reply::json(200, &format!(r#"{{"hosts":[{HOST}]}}"#))]);
    let client = Client::from_base_url(server.url())
        .expect("client")
        .with_token("jwt");
    client.set_api_key("key_1");

    let hosts = client.call(&GetV1Hosts::new()).expect("hosts");
    assert_eq!(hosts.hosts.len(), 1);
    assert_eq!(hosts.hosts[0].slug, "build-box");

    let requests = server.join();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/v1/hosts");
    assert_eq!(requests[0].header("authorization"), Some("Bearer jwt"));
    assert_eq!(requests[0].header("x-api-key"), Some("key_1"));
    assert!(
        requests[0]
            .header("user-agent")
            .expect("ua")
            .starts_with("tidepool-client/")
    );
}

#[test]
fn typed_query_is_percent_encoded() {
    let server = ScriptedServer::start(vec![Reply::json(
        200,
        r#"{"items":[],"hasMore":false,"nextCursor":null}"#,
    )]);
    let client = client(&server);
    let query = GetV1ActivityQuery {
        project: Some("proj 1".to_owned()),
        kind: Some("run".to_owned()),
        limit: Some(2),
        ..GetV1ActivityQuery::default()
    };
    let page = client
        .call(&GetV1Activity::new().with_query(query))
        .expect("activity");
    assert!(page.items.is_empty());
    assert!(!page.has_more);

    let requests = server.join();
    assert_eq!(
        requests[0].path,
        "/v1/activity?project=proj%201&kind=run&limit=2"
    );
}

#[test]
fn typed_post_sends_a_typed_json_body() {
    let server = ScriptedServer::start(vec![Reply::json(201, HOST)]);
    let client = client(&server);
    let host = client
        .call(&PostV1Hosts::new(CreateHostRequest {
            display_name: Some("Build box".to_owned()),
            hostname: "build-box.local".to_owned(),
        }))
        .expect("host");
    assert_eq!(host.id, "host_1");

    let requests = server.join();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].path, "/v1/hosts");
    assert_eq!(requests[0].header("content-type"), Some("application/json"));
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("json body");
    assert_eq!(body["hostname"], "build-box.local");
    assert_eq!(body["display_name"], "Build box");
}

#[test]
fn a_204_decodes_as_the_empty_body() {
    let server = ScriptedServer::start(vec![Reply::empty(204)]);
    let client = client(&server);
    client
        .call(&DeleteV1HostsId::new("host_1"))
        .expect("delete");
    let requests = server.join();
    assert_eq!(requests[0].method, "DELETE");
    assert_eq!(requests[0].path, "/v1/hosts/host_1");
}

#[test]
fn path_parameters_are_encoded() {
    let server = ScriptedServer::start(vec![Reply::json(200, HOST)]);
    let client = client(&server);
    let host = client.call(&GetV1HostsId::new("a/b?c")).expect("host");
    assert_eq!(host.id, "host_1");
    let requests = server.join();
    assert_eq!(requests[0].path, "/v1/hosts/a%2Fb%3Fc");
}

#[test]
fn problem_details_become_a_typed_error() {
    let server = ScriptedServer::start(vec![Reply::problem(
        404,
        r#"{"type":"https://tidepool/errors/not_found","title":"Not found","status":404,"code":"not_found","detail":"no such host","requestId":"req_body"}"#,
    )]);
    let client = client(&server);
    let error = client.call(&GetV1HostsId::new("missing")).expect_err("404");
    let problem = error.problem().expect("problem");
    assert_eq!(problem.status(), 404);
    assert_eq!(problem.code(), "not_found");
    assert_eq!(problem.detail(), Some("no such host"));
    assert_eq!(problem.request_id(), Some("req_body"));
    assert_eq!(problem.attempts(), 1);
    assert_eq!(error.code(), Some("not_found"));

    let requests = server.join();
    assert_eq!(requests.len(), 1);
}

#[test]
fn a_429_is_retried_and_then_succeeds() {
    let server = ScriptedServer::start(vec![
        Reply::problem(429, r#"{"code":"rate_limited"}"#).with_header("retry-after", "0"),
        Reply::json(200, r#"{"hosts":[]}"#),
    ]);
    let client = client(&server);
    let hosts = client.call(&GetV1Hosts::new()).expect("retried call");
    assert!(hosts.hosts.is_empty());
    let requests = server.join();
    assert_eq!(requests.len(), 2);
}

#[test]
fn retries_are_bounded_and_report_the_attempts() {
    let server = ScriptedServer::start(vec![
        Reply::problem(429, r#"{"code":"rate_limited"}"#).with_header("retry-after", "0"),
        Reply::problem(429, r#"{"code":"rate_limited"}"#).with_header("retry-after", "0"),
        Reply::problem(429, r#"{"code":"rate_limited"}"#).with_header("retry-after", "0"),
    ]);
    let client = client(&server);
    let error = client.call(&GetV1Hosts::new()).expect_err("limited");
    let problem = error.problem().expect("problem");
    assert_eq!(problem.code(), "rate_limited");
    assert_eq!(problem.attempts(), 3);
    assert_eq!(problem.retry_after_ms(), Some(0));
    assert_eq!(server.join().len(), 3);
}

#[test]
fn an_excessive_retry_after_is_surfaced_instead_of_slept_through() {
    let server = ScriptedServer::start(vec![
        Reply::problem(429, r#"{"code":"rate_limited"}"#).with_header("retry-after", "120"),
    ]);
    let client = client(&server);
    let error = client.call(&GetV1Hosts::new()).expect_err("limited");
    let problem = error.problem().expect("problem");
    assert_eq!(problem.attempts(), 1);
    assert_eq!(problem.retry_after_ms(), Some(120_000));
    assert_eq!(server.join().len(), 1);
}

#[test]
fn an_explicit_call_option_disables_retry() {
    let server = ScriptedServer::start(vec![
        Reply::problem(429, r#"{"code":"rate_limited"}"#).with_header("retry-after", "0"),
    ]);
    let client = client(&server);
    let error = client
        .call_with(&GetV1Hosts::new(), &CallOptions::new().without_retry())
        .expect_err("limited");
    assert_eq!(error.problem().expect("problem").attempts(), 1);
    assert_eq!(server.join().len(), 1);
}

#[test]
fn a_401_refreshes_once_then_replays_with_the_new_token() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let server = ScriptedServer::start(vec![
        Reply::problem(401, r#"{"code":"unauthorized"}"#),
        Reply::json(200, r#"{"hosts":[]}"#),
    ]);
    let client = Client::from_base_url(server.url())
        .expect("client")
        .with_token("stale")
        .with_refresh(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Some(
                TokenUpdate::new("fresh").with_expiry(Duration::from_secs(60)),
            ))
        }));
    let hosts = client.call(&GetV1Hosts::new()).expect("after refresh");
    assert!(hosts.hosts.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let requests = server.join();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].header("authorization"), Some("Bearer stale"));
    assert_eq!(requests[1].header("authorization"), Some("Bearer fresh"));
}

#[test]
fn a_401_without_a_refresh_hook_surfaces_once() {
    let server = ScriptedServer::start(vec![Reply::problem(401, r#"{"code":"unauthorized"}"#)]);
    let client = client(&server);
    let error = client.call(&GetV1Hosts::new()).expect_err("401");
    assert_eq!(error.code(), Some("unauthorized"));
    assert_eq!(server.join().len(), 1);
}

#[test]
fn request_ids_and_extra_headers_are_per_call() {
    let server = ScriptedServer::start(vec![Reply::json(200, r#"{"hosts":[]}"#)]);
    let client = client(&server);
    let options = CallOptions::new()
        .with_request_id("req_1")
        .with_header("x-trace", "test");
    client
        .call_with(&GetV1Hosts::new(), &options)
        .expect("hosts");
    let requests = server.join();
    assert_eq!(requests[0].header("x-request-id"), Some("req_1"));
    assert_eq!(requests[0].header("x-trace"), Some("test"));
}

#[test]
fn the_generic_request_path_reaches_a_route_by_operation_id() {
    let server = ScriptedServer::start(vec![Reply::json(200, HOST)]);
    let client = client(&server);
    let host = client
        .request_json("get_v1_hosts_id", &[("id", "a/b")], &[], None)
        .expect("host");
    assert_eq!(host["id"], "host_1");
    let requests = server.join();
    assert_eq!(requests[0].path, "/v1/hosts/a%2Fb");
}

#[test]
fn login_adopts_the_token_for_later_calls() {
    let login = r#"{
        "token": "jwt_from_login",
        "expires_in": 3600,
        "must_change_password": false,
        "user": {
            "id": "user_1",
            "email": "owner@example.com",
            "display_name": "Owner",
            "role": "owner",
            "password_change_required": false,
            "created_at": "2026-10-29T00:00:00Z"
        }
    }"#;
    let server = ScriptedServer::start(vec![
        Reply::json(200, login),
        Reply::json(200, r#"{"hosts":[]}"#),
    ]);
    let client = Client::from_base_url(server.url()).expect("client");
    let response = client
        .login("owner@example.com", "password")
        .expect("login");
    assert_eq!(response.token, "jwt_from_login");
    assert_eq!(client.token().as_deref(), Some("jwt_from_login"));
    client.call(&GetV1Hosts::new()).expect("hosts");
    let requests = server.join();
    assert_eq!(
        requests[1].header("authorization"),
        Some("Bearer jwt_from_login")
    );
}

#[test]
fn transport_failures_are_typed_not_panics() {
    let client = Client::from_base_url("http://127.0.0.1:1").expect("client");
    let error = client.call(&GetV1Hosts::new()).expect_err("unreachable");
    assert!(matches!(error, Error::Transport(_)));
}

#[test]
fn the_generated_resource_namespace_delegates_to_the_typed_route() {
    use tidepool_client::generated::resources::blocking_api as api;

    let server = ScriptedServer::start(vec![Reply::json(200, HOST)]);
    let client = client(&server);
    let host = api::hosts::get(&client, "host_1").expect("host");
    assert_eq!(host.slug, "build-box");
    let requests = server.join();
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/v1/hosts/host_1");
}

#[test]
fn a_typed_endpoint_reports_its_contract_route() {
    use tidepool_client::route::Endpoint;
    assert_eq!(GetV1Hosts::ROUTE.operation_id, "get_v1_hosts");
    assert_eq!(GetV1Hosts::ROUTE.path, "/v1/hosts");
    assert_eq!(PostV1Hosts::ROUTE.method.as_str(), "POST");
    let host: HostView = serde_json::from_str(HOST).expect("host");
    assert_eq!(host.status, "connected");
}
