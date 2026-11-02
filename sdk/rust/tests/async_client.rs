//! The async facade against a scripted HTTP server, driven by a Tokio runtime.

#![cfg(feature = "async")]
#![allow(clippy::result_large_err)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use support::{Reply, ScriptedServer};
use tidepool_client::TokenUpdate;
use tidepool_client::asynchronous::Client;
use tidepool_client::endpoints::{
    GetV1Activity, GetV1ActivityQuery, GetV1Hosts, GetV1HostsId, PostV1Hosts,
};
use tidepool_client::generated::types::CreateHostRequest;

const HOST: &str = r#"{
    "id": "host_1",
    "slug": "build-box",
    "display_name": "Build box",
    "hostname": "build-box.local",
    "status": "connected",
    "execution_enabled": true,
    "created_at": "2026-10-29T00:00:00Z"
}"#;

#[tokio::test]
async fn typed_calls_work_without_the_caller_building_a_transport() {
    let server = ScriptedServer::start(vec![
        Reply::json(200, &format!(r#"{{"hosts":[{HOST}]}}"#)),
        Reply::json(200, r#"{"items":[],"hasMore":false}"#),
    ]);
    let client = Client::from_base_url(server.url())
        .expect("client")
        .with_token("jwt");
    let hosts = client.call(&GetV1Hosts::new()).await.expect("hosts");
    assert_eq!(hosts.hosts[0].slug, "build-box");
    let page = client
        .call(&GetV1Activity::new().with_query(GetV1ActivityQuery {
            kind: Some("run".to_owned()),
            ..GetV1ActivityQuery::default()
        }))
        .await
        .expect("activity");
    assert!(page.items.is_empty());

    let requests = server.join();
    assert_eq!(requests[0].header("authorization"), Some("Bearer jwt"));
    assert_eq!(requests[1].path, "/v1/activity?kind=run");
}

#[tokio::test]
async fn typed_post_sends_the_contract_body() {
    let server = ScriptedServer::start(vec![Reply::json(201, HOST)]);
    let client = Client::from_base_url(server.url()).expect("client");
    let host = client
        .call(&PostV1Hosts::new(CreateHostRequest {
            display_name: None,
            hostname: "build-box.local".to_owned(),
        }))
        .await
        .expect("host");
    assert_eq!(host.id, "host_1");
    let requests = server.join();
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].header("content-type"), Some("application/json"));
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("json");
    assert_eq!(body["hostname"], "build-box.local");
}

#[tokio::test]
async fn a_429_is_retried_with_backoff() {
    let server = ScriptedServer::start(vec![
        Reply::problem(429, r#"{"code":"rate_limited"}"#).with_header("retry-after", "0"),
        Reply::json(200, r#"{"hosts":[]}"#),
    ]);
    let client = Client::from_base_url(server.url()).expect("client");
    let hosts = client.call(&GetV1Hosts::new()).await.expect("hosts");
    assert!(hosts.hosts.is_empty());
    assert_eq!(server.join().len(), 2);
}

#[tokio::test]
async fn a_401_refreshes_once() {
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
            Box::pin(async {
                Ok(Some(
                    TokenUpdate::new("fresh").with_expiry(Duration::from_secs(30)),
                ))
            })
        }));
    client.call(&GetV1Hosts::new()).await.expect("hosts");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let requests = server.join();
    assert_eq!(requests[1].header("authorization"), Some("Bearer fresh"));
}

#[tokio::test]
async fn problem_details_survive_the_async_path() {
    let server = ScriptedServer::start(vec![Reply::problem(
        404,
        r#"{"code":"not_found","detail":"gone","requestId":"req_1"}"#,
    )]);
    let client = Client::from_base_url(server.url()).expect("client");
    let error = client
        .call(&GetV1HostsId::new("missing"))
        .await
        .expect_err("404");
    let problem = error.problem().expect("problem");
    assert_eq!(problem.status(), 404);
    assert_eq!(problem.code(), "not_found");
    assert_eq!(problem.request_id(), Some("req_1"));
    assert_eq!(server.join().len(), 1);
}

#[tokio::test]
async fn the_generic_path_reaches_a_route_by_id() {
    let server = ScriptedServer::start(vec![Reply::json(200, HOST)]);
    let client = Client::from_base_url(server.url()).expect("client");
    let host = client
        .request_json("get_v1_hosts_id", &[("id", "host_1")], &[], None)
        .await
        .expect("host");
    assert_eq!(host["slug"], "build-box");
    assert_eq!(server.join()[0].path, "/v1/hosts/host_1");
}
