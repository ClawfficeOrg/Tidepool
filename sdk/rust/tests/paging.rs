//! Cursor paging follows `next`/`hasMore` and never duplicates a page.

#![cfg(feature = "blocking")]

mod support;

use support::{Reply, ScriptedServer};
use tidepool_client::blocking::Client;
use tidepool_client::paging::{
    ActivityFilters, PageOptions, activity, chat_messages, relay_events,
};

fn entry(id: &str) -> String {
    format!(r#"{{"id":"{id}","at":"2026-10-29T00:00:00Z","kind":"run","status":"succeeded"}}"#)
}

fn message(id: &str, sequence: i64) -> String {
    format!(
        r#"{{"id":"{id}","project":"proj_1","body":"hi","created_at":"2026-10-29T00:00:00Z","sequence":{sequence}}}"#
    )
}

fn event(id: &str, sequence: i64) -> String {
    format!(
        r#"{{"id":"{id}","session":"session_1","sequence":{sequence},"kind":"output","payload":{{}},"createdAt":"2026-10-29T00:00:00Z"}}"#
    )
}

#[test]
fn activity_follows_next_cursor_while_has_more() {
    let server = ScriptedServer::start(vec![
        Reply::json(
            200,
            &format!(
                r#"{{"items":[{}],"hasMore":true,"nextCursor":"c1"}}"#,
                entry("a")
            ),
        ),
        Reply::json(
            200,
            &format!(
                r#"{{"items":[{}],"hasMore":false,"nextCursor":null}}"#,
                entry("b")
            ),
        ),
    ]);
    let client = Client::from_base_url(server.url()).expect("client");
    let ids: Vec<String> = activity(
        &client,
        PageOptions::new().limit(1),
        ActivityFilters {
            kind: Some("run".to_owned()),
            ..ActivityFilters::default()
        },
    )
    .map(|entry| entry.expect("entry").id)
    .collect();
    assert_eq!(ids, vec!["a", "b"]);

    let requests = server.join();
    assert_eq!(requests[0].path, "/v1/activity?kind=run&limit=1");
    assert_eq!(requests[1].path, "/v1/activity?kind=run&cursor=c1&limit=1");
}

#[test]
fn chat_messages_follow_the_sequence_cursor() {
    let server = ScriptedServer::start(vec![
        Reply::json(
            200,
            &format!(r#"{{"messages":[{}],"next":5}}"#, message("m1", 4)),
        ),
        Reply::json(
            200,
            &format!(r#"{{"messages":[{}],"next":null}}"#, message("m2", 5)),
        ),
    ]);
    let client = Client::from_base_url(server.url()).expect("client");
    let ids: Vec<String> = chat_messages(&client, "room 1", PageOptions::new().after(0).limit(1))
        .map(|message| message.expect("message").id)
        .collect();
    assert_eq!(ids, vec!["m1", "m2"]);

    let requests = server.join();
    assert_eq!(
        requests[0].path,
        "/v1/chatrooms/room%201/messages?after=0&limit=1"
    );
    assert_eq!(
        requests[1].path,
        "/v1/chatrooms/room%201/messages?after=5&limit=1"
    );
}

#[test]
fn max_pages_stops_even_when_the_server_offers_more() {
    let server = ScriptedServer::start(vec![Reply::json(
        200,
        &format!(r#"{{"messages":[{}],"next":9}}"#, message("m1", 1)),
    )]);
    let client = Client::from_base_url(server.url()).expect("client");
    let ids: Vec<String> = chat_messages(&client, "room", PageOptions::new().max_pages(1))
        .map(|message| message.expect("message").id)
        .collect();
    assert_eq!(ids, vec!["m1"]);
    assert_eq!(server.join().len(), 1);
}

#[test]
fn relay_events_follow_the_sequence_cursor() {
    let server = ScriptedServer::start(vec![
        Reply::json(
            200,
            &format!(r#"{{"events":[{}],"next":2}}"#, event("e1", 1)),
        ),
        Reply::json(
            200,
            &format!(r#"{{"events":[{}],"next":null}}"#, event("e2", 2)),
        ),
    ]);
    let client = Client::from_base_url(server.url()).expect("client");
    let ids: Vec<String> = relay_events(&client, "session_1", PageOptions::new())
        .map(|event| event.expect("event").id)
        .collect();
    assert_eq!(ids, vec!["e1", "e2"]);
    let requests = server.join();
    assert_eq!(requests[0].path, "/v1/sessions/session_1/relay/events");
    assert_eq!(
        requests[1].path,
        "/v1/sessions/session_1/relay/events?after=2"
    );
}

#[test]
fn a_page_error_ends_the_iterator_and_is_yielded_once() {
    let server = ScriptedServer::start(vec![Reply::problem(
        500,
        r#"{"code":"internal_error","detail":"boom"}"#,
    )]);
    let client = Client::from_base_url(server.url()).expect("client");
    let mut messages = chat_messages(&client, "room", PageOptions::new());
    let error = messages.next().expect("one error").expect_err("error");
    assert_eq!(error.code(), Some("internal_error"));
    assert!(messages.next().is_none());
    assert_eq!(server.join().len(), 1);
}
