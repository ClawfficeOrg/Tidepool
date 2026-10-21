//! Integration tests for the Discord gateway connection (task 1.3.4).
//!
//! These need no live MySQL or Redis. Discord has no inbound webhook for bot
//! events, so the transport is a WebSocket the pool dials: the tests script
//! that socket instead, feeding [`run_gateway_session`] the exact frames the
//! gateway sends — `hello`, `READY`, `MESSAGE_CREATE`, `op 7` — and reading
//! back the commands the pool would send and the deliveries it hands on.
//!
//! The reconnect case is the point of driving the real pump twice: the first
//! stream dies mid-conversation, the second one replays the event the pool had
//! already handled, and the *same* session must recognise it. One session
//! across two sockets is how a resume is made safe.
//!
//! The database-backed half (connection lookup by slug and the
//! `webhook_deliveries` dedup insert, which backstops the in-memory window
//! across a restart) is covered by the `#[ignore]`d test at the bottom, which
//! runs against the Docker dev stack.

use serde_json::{Value, json};
use tidepool::services::connections::discord::{
    self, DeliverySource, DiscordDelivery, DiscordEvent, DiscordGatewaySession,
    DiscordIngestContext, DiscordReply, GatewayCommand, GatewayStop, OP_DISPATCH, OP_IDENTIFY,
    OP_RESUME, run_gateway_session,
};
use tidepool::services::outputs::{DISCORD_REPLY, OutputDeclaration, OutputError, OutputLedger};
use tokio::sync::mpsc;

const BOT: &str = "900000000000000001";
const GUILD: &str = "100000000000000001";
const CHANNEL: &str = "200000000000000001";
const THREAD: &str = "250000000000000009";
const MESSAGE: &str = "300000000000000001";

fn context() -> DiscordIngestContext {
    DiscordIngestContext::new("conn-discord")
        .with_guilds([GUILD])
        .with_channels([CHANNEL, THREAD])
}

/// The `hello` the gateway opens with.
fn hello() -> String {
    json!({"op": 10, "d": {"heartbeat_interval": 41250}}).to_string()
}

/// The `READY` dispatch, which names the session and the bot user.
fn ready() -> String {
    json!({
        "op": OP_DISPATCH,
        "s": 1,
        "t": "READY",
        "d": {
            "v": 10,
            "session_id": "a1b2c3d4e5f6",
            "resume_gateway_url": "wss://gateway-us-east1-b.discord.gg",
            "user": {"id": BOT, "username": "tide", "bot": true},
            "guilds": [{"id": GUILD, "unavailable": true}],
        }
    })
    .to_string()
}

/// `MESSAGE_CREATE` for "@tide deploy the site" in a guild channel.
fn mention_payload(channel_id: &str, message_id: &str) -> Value {
    json!({
        "id": message_id,
        "channel_id": channel_id,
        "guild_id": GUILD,
        "author": {"id": "400000000000000001", "username": "human", "bot": false},
        "member": {"nick": "Human"},
        "content": format!("<@{BOT}> deploy the site"),
        "mentions": [{"id": BOT, "username": "tide"}],
        "timestamp": "2026-10-21T07:00:00.000000+00:00",
    })
}

fn dispatch(sequence: u64, data: Value) -> String {
    json!({"op": OP_DISPATCH, "s": sequence, "t": "MESSAGE_CREATE", "d": data}).to_string()
}

/// Run one scripted socket against `session`, as a live gateway would.
async fn run_socket(
    session: &mut DiscordGatewaySession,
    frames: Vec<String>,
) -> (GatewayStop, Vec<GatewayCommand>, Vec<DiscordDelivery>) {
    let (sender, mut inbound) = mpsc::channel::<String>(16);
    let (outbound, mut commands) = mpsc::channel::<GatewayCommand>(16);
    let (delivery_tx, mut deliveries) = mpsc::channel::<DiscordDelivery>(16);
    for frame in frames {
        sender.send(frame).await.expect("queue frame");
    }
    drop(sender);

    let stop = run_gateway_session(session, &mut inbound, &outbound, &delivery_tx).await;

    let mut sent = Vec::new();
    while let Ok(command) = commands.try_recv() {
        sent.push(command);
    }
    let mut received = Vec::new();
    while let Ok(delivery) = deliveries.try_recv() {
        received.push(delivery);
    }
    (stop, sent, received)
}

#[tokio::test]
async fn the_gateway_delivers_a_guild_mention_as_a_typed_event() {
    let mut session = DiscordGatewaySession::new(context());
    let (stop, commands, deliveries) = run_socket(
        &mut session,
        vec![
            hello(),
            ready(),
            dispatch(2, mention_payload(CHANNEL, MESSAGE)),
        ],
    )
    .await;

    assert_eq!(stop, GatewayStop::StreamClosed);
    // The first connect identifies, naming the token rather than holding it.
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].op, OP_IDENTIFY);
    assert_eq!(commands[0].credential_key, Some("bot_token"));
    assert_eq!(commands[0].frame()["d"].get("token"), None);
    assert_eq!(session.session_id(), Some("a1b2c3d4e5f6"));
    assert_eq!(
        session.resume_gateway_url(),
        Some("wss://gateway-us-east1-b.discord.gg")
    );

    assert_eq!(deliveries.len(), 1);
    let delivery = &deliveries[0];
    assert_eq!(delivery.connection_public_id, "conn-discord");
    assert_eq!(delivery.source, DeliverySource::Live);
    assert_eq!(delivery.delivery_id, MESSAGE);
    assert_eq!(delivery.sequence, Some(2));
    assert!(delivery.accepted, "the mention is in an owned guild");
    assert_eq!(delivery.reason, None);
    assert_eq!(delivery.event.kind(), "discord.mention");
    assert!(matches!(delivery.event, DiscordEvent::Mention(_)));

    let message = delivery.event.message().expect("mention carries a message");
    assert_eq!(message.guild_id.as_deref(), Some(GUILD));
    assert_eq!(message.channel_id, CHANNEL);
    assert_eq!(message.author_id, "400000000000000001");
    assert!(message.bot_mentioned);
    // The mention is gone before any prompt sees the text.
    assert_eq!(message.prompt_text(), "deploy the site");
    assert!(!message.prompt_text().contains("<@"));
    assert!(message.raw_content.contains(BOT));
}

#[tokio::test]
async fn a_reconnect_resumes_without_duplicating_an_already_received_event() {
    let mut session = DiscordGatewaySession::new(context());

    // First socket: identify, READY, one event, then the server asks to redial.
    let (stop, commands, first) = run_socket(
        &mut session,
        vec![
            hello(),
            ready(),
            dispatch(2, mention_payload(CHANNEL, MESSAGE)),
            json!({"op": 7}).to_string(),
        ],
    )
    .await;
    assert_eq!(stop, GatewayStop::Reconnect { resumable: true });
    assert_eq!(commands[0].op, OP_IDENTIFY);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].delivery_id, MESSAGE);
    assert_eq!(session.last_sequence(), Some(2));
    assert!(session.can_resume());

    // Second socket, same session: Discord replays the event it already sent
    // before RESUMED, then sends a genuinely new one.
    let (stop, commands, second) = run_socket(
        &mut session,
        vec![
            hello(),
            dispatch(2, mention_payload(CHANNEL, MESSAGE)),
            json!({"op": OP_DISPATCH, "s": 3, "t": "RESUMED", "d": {}}).to_string(),
            dispatch(4, mention_payload(CHANNEL, "300000000000000002")),
        ],
    )
    .await;
    assert_eq!(stop, GatewayStop::StreamClosed);

    // The handshake resumed from the sequence the pool had acknowledged.
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].op, OP_RESUME);
    assert!(commands[0].is_resume());
    assert_eq!(
        commands[0].payload,
        json!({"session_id": "a1b2c3d4e5f6", "seq": 2})
    );

    // The replayed event was recognised; only the new one came through.
    assert_eq!(
        second.len(),
        1,
        "the replayed event must not be delivered twice"
    );
    assert_eq!(second[0].delivery_id, "300000000000000002");
    assert_eq!(second[0].sequence, Some(4));
    assert_eq!(second[0].source, DeliverySource::Live);
    assert_eq!(session.last_sequence(), Some(4));
}

#[tokio::test]
async fn a_mention_in_a_thread_is_answered_in_that_thread() {
    let mut session = DiscordGatewaySession::new(context());
    let mut threaded = mention_payload(THREAD, "300000000000000003");
    // Discord sends `position` only for messages inside a thread.
    threaded
        .as_object_mut()
        .expect("object")
        .insert("position".into(), json!(0));

    let (_, _, deliveries) =
        run_socket(&mut session, vec![hello(), ready(), dispatch(2, threaded)]).await;
    let message = deliveries[0].event.message().expect("message");
    assert!(message.in_thread);
    assert_eq!(message.thread_key(), THREAD);

    let reply = DiscordReply::to_event(message, "deployed").expect("reply");
    assert_eq!(reply.target().channel_id(), THREAD);
    let request = reply.create_message_request();
    assert_eq!(request.method, "POST");
    assert!(
        request
            .url
            .ends_with(&format!("/channels/{THREAD}/messages")),
        "the reply is posted to the thread, not its parent channel: {}",
        request.url
    );
    assert_eq!(request.credential_key, "bot_token");
}

#[tokio::test]
async fn a_required_reply_blocks_completion_and_lands_in_the_originating_channel() {
    let mut session = DiscordGatewaySession::new(context());
    let (_, _, deliveries) = run_socket(
        &mut session,
        vec![
            hello(),
            ready(),
            dispatch(2, mention_payload(CHANNEL, MESSAGE)),
        ],
    )
    .await;
    let message = deliveries[0].event.message().expect("message").clone();
    let declarations = vec![OutputDeclaration::new(DISCORD_REPLY).required()];

    // A step that declared a required reply cannot finish without emitting it.
    let ledger = OutputLedger::new(&declarations).expect("ledger");
    assert_eq!(
        ledger.finish(),
        Err(OutputError::RequiredMissing(DISCORD_REPLY.into()))
    );

    let mut ledger = OutputLedger::new(&declarations).expect("ledger");
    let reply = DiscordReply::to_event(&message, "deployed").expect("reply");
    ledger.emit_discord_reply(&reply).expect("emit the reply");
    // The maximum is one by default, so a second reply is refused.
    assert!(matches!(
        ledger.emit_discord_reply(&reply),
        Err(OutputError::MaximumExceeded { .. })
    ));

    let summary = ledger.finish().expect("the step can finish now");
    assert_eq!(summary.outputs.len(), 1);
    assert_eq!(
        summary.outputs[0].payload,
        json!({
            "content": "deployed",
            "message_reference": {"message_id": MESSAGE, "fail_if_not_exists": false},
            "allowed_mentions": {"parse": []},
        })
    );
    // The reply goes to the channel the event came from and nowhere else.
    assert_eq!(reply.target().channel_id(), message.channel_id);
    assert_eq!(reply.target().message_id(), message.message_id);
}

#[tokio::test]
async fn an_event_from_an_unowned_guild_is_recorded_but_not_matched() {
    let mut session = DiscordGatewaySession::new(context());
    let mut foreign = mention_payload(CHANNEL, "300000000000000004");
    foreign
        .as_object_mut()
        .expect("object")
        .insert("guild_id".into(), json!("199999999999999999"));

    let (_, _, deliveries) =
        run_socket(&mut session, vec![hello(), ready(), dispatch(2, foreign)]).await;
    assert_eq!(deliveries.len(), 1);
    assert!(!deliveries[0].accepted);
    assert_eq!(
        deliveries[0].reason,
        Some(discord::RejectReason::GuildNotOwned)
    );
    assert_eq!(
        deliveries[0].reason.expect("reason").as_str(),
        "guild_not_owned"
    );
}

#[tokio::test]
async fn an_invalid_session_that_cannot_resume_identifies_afresh() {
    let mut session = DiscordGatewaySession::new(context());
    let (stop, _, _) = run_socket(
        &mut session,
        vec![hello(), ready(), json!({"op": 9, "d": false}).to_string()],
    )
    .await;
    assert_eq!(stop, GatewayStop::Reconnect { resumable: false });
    assert!(!session.can_resume());

    let (_, commands, _) = run_socket(&mut session, vec![hello()]).await;
    assert_eq!(commands[0].op, OP_IDENTIFY);
}

/// The database-backed half: the dedup insert that backstops the session's
/// in-memory window across a restart.
///
/// Requires the Docker dev stack; run with
/// `cargo test --test discord_integration -- --ignored`.
#[tokio::test]
#[ignore = "requires a live MySQL; run against the Docker dev stack"]
async fn recorded_deliveries_are_deduplicated() {
    use sqlx::MySqlPool;

    const DEFAULT_DB_URL: &str = "mysql://tidepool:password@127.0.0.1:3306/tidepool_dev";
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DB_URL.into());
    let pool = MySqlPool::connect(&db_url)
        .await
        .expect("DB connection failed — is the dev stack up?");
    let organization_id: u64 =
        sqlx::query_scalar("SELECT id FROM organizations ORDER BY id LIMIT 1")
            .fetch_one(&pool)
            .await
            .expect("an organization must exist");

    let mut session = DiscordGatewaySession::new(context());
    let (_, _, deliveries) = run_socket(
        &mut session,
        vec![
            hello(),
            ready(),
            dispatch(2, mention_payload(CHANNEL, MESSAGE)),
        ],
    )
    .await;
    let mut delivery = deliveries.into_iter().next().expect("one delivery");
    delivery.delivery_id = format!("it-{}", uuid::Uuid::new_v4());
    assert!(
        !discord::record_delivery(&pool, organization_id, &delivery)
            .await
            .expect("first insert"),
        "the first delivery is new"
    );
    assert!(
        discord::record_delivery(&pool, organization_id, &delivery)
            .await
            .expect("second insert"),
        "a replayed event with the same id is a duplicate"
    );
}
