//! Integration tests for the Slack connection's two transports (task 1.3.3).
//!
//! These need no live MySQL or Redis: both transports are driven from the wire
//! bytes Slack would send — a signed HTTP body for the webhook transport, and
//! socket-mode frames over a channel for socket mode — and the route itself is
//! exercised through `actix_web::test` with no database attached, which is how
//! the signature and replay-window rejections are checked over real HTTP.
//!
//! The database-backed half of the webhook path (connection lookup by slug and
//! the `webhook_deliveries` dedup insert) is covered by the `#[ignore]`d test
//! at the bottom, which runs against the Docker dev stack.

use actix_web::http::header::{HeaderMap, HeaderName, HeaderValue};
use actix_web::{App, http::StatusCode, test};
use serde_json::{Value, json};
use tidepool::api;
use tidepool::services::connections::slack::{
    self, SlackDelivery, SlackEvent, SlackIngestContext, SlackReply, SlackSocketSession,
    SlackTransport, SocketStop, WebhookIngest, run_socket_session,
};
use tidepool::services::outputs::{OutputDeclaration, OutputError, OutputLedger, SLACK_REPLY};
use tokio::sync::mpsc;

const SIGNING_SECRET: &str = "slack-integration-signing-secret";
const BOT: &str = "U0BOT";
const NOW: i64 = 1_700_000_000;

fn context() -> SlackIngestContext {
    SlackIngestContext::new("conn-slack", SIGNING_SECRET)
        .expect("signing secret")
        .with_bot_user_id(BOT)
        .with_teams(["T1"])
        .with_channels(["C1"])
        .with_socket_mode(true)
}

/// The `app_mention` callback Slack sends for "@tide deploy the site".
fn mention_callback() -> Value {
    json!({
        "token": "verification-token",
        "type": "event_callback",
        "team_id": "T1",
        "api_app_id": "A1",
        "event_id": "Ev0PV52K21",
        "event_time": NOW,
        "event": {
            "type": "app_mention",
            "user": "U2HUMAN",
            "text": "<@U0BOT> deploy the site",
            "channel": "C1",
            "ts": "1700000000.000100",
            "event_ts": "1700000000.000100",
            "thread_ts": "1699999999.000100"
        }
    })
}

fn signed_headers(body: &[u8], timestamp: i64) -> HeaderMap {
    let timestamp = timestamp.to_string();
    let signature = slack::sign(SIGNING_SECRET, &timestamp, body);
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("x-slack-request-timestamp"),
        HeaderValue::from_str(&timestamp).expect("timestamp header"),
    );
    headers.insert(
        HeaderName::from_static("x-slack-signature"),
        HeaderValue::from_str(&signature).expect("signature header"),
    );
    headers
}

/// Deliver the mention over the webhook transport, exactly as Slack would.
fn deliver_over_webhook() -> SlackDelivery {
    let body = serde_json::to_vec(&mention_callback()).expect("serialize callback");
    let headers = signed_headers(&body, NOW);
    match slack::ingest_webhook(&context(), &headers, &body, NOW).expect("webhook ingest") {
        WebhookIngest::Delivery(delivery) => *delivery,
        WebhookIngest::Challenge(_) => panic!("expected a delivery, not a challenge"),
    }
}

/// Deliver the same mention over socket mode, driving the session the way a
/// live WebSocket does: frames in, acknowledgements out.
async fn deliver_over_socket_mode() -> (SlackDelivery, Vec<String>, SocketStop) {
    let mut session = SlackSocketSession::new(context());
    let (frames, mut inbound) = mpsc::channel::<String>(8);
    let (outbound, mut acks) = mpsc::channel::<String>(8);
    let (delivery_tx, mut deliveries) = mpsc::channel::<SlackDelivery>(8);

    for frame in [
        json!({"type": "hello", "num_connections": 1}).to_string(),
        json!({
            "envelope_id": "57d6a792-4d35-4d0b-b6aa-3361493e1caf",
            "type": "events_api",
            "accepts_response_payload": false,
            "payload": mention_callback(),
        })
        .to_string(),
        json!({"type": "disconnect", "reason": "refresh_requested"}).to_string(),
    ] {
        frames.send(frame).await.expect("queue frame");
    }
    drop(frames);

    let stop = run_socket_session(&mut session, &mut inbound, &outbound, &delivery_tx).await;
    let delivery = deliveries.recv().await.expect("one delivery");
    let mut sent = Vec::new();
    while let Ok(ack) = acks.try_recv() {
        sent.push(ack);
    }
    (delivery, sent, stop)
}

#[actix_web::test]
async fn webhook_transport_delivers_a_mention_event() {
    let delivery = deliver_over_webhook();

    assert_eq!(delivery.transport, SlackTransport::Webhook);
    assert_eq!(delivery.delivery_id, "Ev0PV52K21");
    assert!(delivery.accepted, "the mention is in an owned workspace");
    assert_eq!(delivery.reason, None);
    assert!(matches!(delivery.event, SlackEvent::Mention(_)));

    let message = delivery.event.message().expect("mention carries a message");
    assert_eq!(message.channel, "C1");
    assert_eq!(message.user, "U2HUMAN");
    assert_eq!(message.team_id, "T1");
    // The mention is gone before any prompt sees the text.
    assert_eq!(message.prompt_text(), "deploy the site");
    assert!(!message.prompt_text().contains("<@"));
    assert!(message.raw_text.contains("<@U0BOT>"));
}

#[actix_web::test]
async fn socket_mode_transport_delivers_a_mention_event() {
    let (delivery, acks, stop) = deliver_over_socket_mode().await;

    assert_eq!(delivery.transport, SlackTransport::SocketMode);
    assert!(delivery.accepted);
    assert!(matches!(delivery.event, SlackEvent::Mention(_)));
    assert_eq!(
        delivery
            .event
            .message()
            .expect("mention carries a message")
            .prompt_text(),
        "deploy the site"
    );
    // Every envelope is acknowledged by echoing its id, or Slack retries it.
    assert_eq!(
        acks,
        vec![r#"{"envelope_id":"57d6a792-4d35-4d0b-b6aa-3361493e1caf"}"#.to_string()]
    );
    assert_eq!(
        stop,
        SocketStop::Disconnected {
            reason: "refresh_requested".into()
        }
    );
}

#[actix_web::test]
async fn both_transports_produce_the_same_event_and_delivery_id() {
    let webhook = deliver_over_webhook();
    let (socket, _, _) = deliver_over_socket_mode().await;

    assert_eq!(webhook.event, socket.event);
    // Same dedup key on both transports, so one event received twice — once
    // per transport — is recorded once.
    assert_eq!(webhook.delivery_id, socket.delivery_id);
    assert_ne!(webhook.transport, socket.transport);
}

#[actix_web::test]
async fn a_required_reply_blocks_completion_and_lands_in_the_events_thread() {
    let delivery = deliver_over_webhook();
    let message = delivery.event.message().expect("message").clone();
    let declarations = vec![OutputDeclaration::new(SLACK_REPLY).required()];

    // A step that declared a required reply cannot finish without emitting it.
    let ledger = OutputLedger::new(&declarations).expect("ledger");
    assert_eq!(
        ledger.finish(),
        Err(OutputError::RequiredMissing(SLACK_REPLY.into()))
    );

    let mut ledger = OutputLedger::new(&declarations).expect("ledger");
    let reply = SlackReply::to_event(&message, "deployed").expect("reply");
    ledger.emit_slack_reply(&reply).expect("emit the reply");
    // The maximum is one by default, so a second reply is refused.
    assert!(matches!(
        ledger.emit_slack_reply(&reply),
        Err(OutputError::MaximumExceeded { .. })
    ));

    let summary = ledger.finish().expect("the step can finish now");
    assert_eq!(summary.outputs.len(), 1);
    // The reply goes to the channel and thread the event came from.
    assert_eq!(
        summary.outputs[0].payload,
        json!({
            "channel": "C1",
            "thread_ts": "1699999999.000100",
            "text": "deployed"
        })
    );
    assert_eq!(reply.target().channel(), message.channel);
    assert_eq!(reply.target().thread_ts(), message.thread_key());
}

#[actix_web::test]
async fn the_webhook_answers_slacks_url_verification_challenge() {
    let body = br#"{"type":"url_verification","token":"t","challenge":"3eZbrw1a"}"#;
    let headers = signed_headers(body, NOW);
    assert_eq!(
        slack::ingest_webhook(&context(), &headers, body, NOW).expect("ingest"),
        WebhookIngest::Challenge("3eZbrw1a".into())
    );
}

#[actix_web::test]
async fn the_webhook_route_refuses_unsigned_and_replayed_requests() {
    let app = test::init_service(App::new().configure(api::webhooks::slack::config)).await;
    let body = serde_json::to_vec(&mention_callback()).expect("serialize callback");

    // No Slack signature headers at all.
    let request = test::TestRequest::post()
        .uri("/v1/webhooks/slack/acme")
        .set_payload(body.clone())
        .to_request();
    let response = test::call_service(&app, request).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // A correctly signed body whose timestamp is outside the replay window.
    let stale = chrono::Utc::now().timestamp() - 3_600;
    let mut request = test::TestRequest::post()
        .uri("/v1/webhooks/slack/acme")
        .set_payload(body.clone());
    for (name, value) in signed_headers(&body, stale).iter() {
        request = request.insert_header((name.clone(), value.clone()));
    }
    let response = test::call_service(&app, request.to_request()).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    // A fresh, signed request gets as far as the connection lookup, which has
    // no database here.
    let fresh = chrono::Utc::now().timestamp();
    let mut request = test::TestRequest::post()
        .uri("/v1/webhooks/slack/acme")
        .set_payload(body.clone());
    for (name, value) in signed_headers(&body, fresh).iter() {
        request = request.insert_header((name.clone(), value.clone()));
    }
    let response = test::call_service(&app, request.to_request()).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[actix_web::test]
async fn a_tampered_body_is_refused_before_normalization() {
    let body = serde_json::to_vec(&mention_callback()).expect("serialize callback");
    let headers = signed_headers(&body, NOW);
    let tampered = br#"{"type":"event_callback","team_id":"T1","event":{"type":"app_mention"}}"#;
    assert!(slack::ingest_webhook(&context(), &headers, tampered, NOW).is_err());
}

/// The database-backed half: connection lookup by slug plus the dedup insert.
///
/// Requires the Docker dev stack; run with
/// `cargo test --test slack_integration -- --ignored`.
#[actix_web::test]
#[ignore = "requires a live MySQL; run against the Docker dev stack"]
async fn recorded_deliveries_are_deduplicated() {
    use sqlx::MySqlPool;

    const DEFAULT_DB_URL: &str = "mysql://tidepool:password@127.0.0.1:3306/tidepool_dev";
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DB_URL.into());
    let pool = MySqlPool::connect(&db_url)
        .await
        .expect("DB connection failed — is the dev stack up?");
    // Organizations are singleton, so this creates one only when the pool has
    // none. Other suites clear the table, so the test cannot assume a row.
    sqlx::query(
        "INSERT IGNORE INTO organizations (public_id, name, singleton) \
         VALUES (?, 'Slack delivery test', 1)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .execute(&pool)
    .await
    .expect("ensure an organization exists");
    let organization_id: u64 =
        sqlx::query_scalar("SELECT id FROM organizations ORDER BY id LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("an organization must exist");

    let mut delivery = deliver_over_webhook();
    delivery.delivery_id = format!("it-{}", uuid::Uuid::new_v4());
    assert!(
        !slack::record_delivery(&pool, organization_id, &delivery)
            .await
            .expect("first insert"),
        "the first delivery is new"
    );
    assert!(
        slack::record_delivery(&pool, organization_id, &delivery)
            .await
            .expect("second insert"),
        "a retry of the same event id is a duplicate"
    );
}
