//! Slack connection transport (task 1.3.3).
//!
//! A Slack connection delivers events over one of two transports, which share
//! every piece of logic after the bytes are verified:
//!
//! - **Webhook** — `POST /v1/webhooks/slack/{slug}`. Slack signs each request
//!   with `X-Slack-Signature` over `v0:<timestamp>:<body>`; the timestamp must
//!   be within [`MAX_TIMESTAMP_SKEW_SECS`] so a captured body cannot be
//!   replayed. Slack's `url_verification` handshake is answered with the
//!   challenge it sent.
//! - **Socket mode** — no public address is needed. Slack pushes envelopes
//!   (`{"envelope_id": ..., "type": "events_api", "payload": {...}}`) over a
//!   WebSocket the pool dials; each envelope is acknowledged by echoing its
//!   `envelope_id`. [`SlackSocketSession`] owns the protocol and
//!   [`run_socket_session`] pumps it over a frame channel, so the session is
//!   driven identically by a live socket and by a scripted test feed.
//!
//! Both transports end in the same [`SlackDelivery`]: a normalized
//! [`SlackEvent`], the workspace/channel decision, and a delivery id that is
//! the Slack `event_id` whenever one is present. That makes the two transports
//! deduplicate against each other — an event delivered twice, once per
//! transport, is recorded once.
//!
//! # Mentions never reach a prompt
//!
//! [`SlackMessageEvent::text`] is already stripped of the bot mention;
//! `raw_text` keeps what Slack sent for the record. Prompt construction reads
//! [`SlackMessageEvent::prompt_text`], so there is no path by which `<@U…>`
//! reaches an agent prompt.
//!
//! # Replies
//!
//! A [`ReplyTarget`] can only be obtained from an event
//! ([`SlackMessageEvent::reply_target`]), so a reply is addressed to the
//! channel the event came from and to its thread (`thread_ts` when the event
//! was already threaded, otherwise the mention's own `ts`, which starts the
//! thread). The `slack.reply` output capability and its maximum/required
//! enforcement live in [`crate::services::outputs`].

use super::{
    ConnectionError, ConnectionStatus, CredentialMaterial, EffectiveConnection, ProviderKind,
    credential_material, find_effective_by_slug, list_connections,
};
use crate::entities::connection::ConnectionView;
use actix_web::http::header::HeaderMap;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;
use sqlx::MySqlPool;
use std::collections::{BTreeSet, VecDeque};
use thiserror::Error;
use tokio::sync::mpsc;

type HmacSha256 = Hmac<Sha256>;

/// How far a Slack request timestamp may be from now (Slack's own guidance).
pub const MAX_TIMESTAMP_SKEW_SECS: i64 = 300;

/// Largest text a single reply may carry (`chat.postMessage` limit).
pub const MAX_REPLY_CHARS: usize = 40_000;

/// How many delivery ids one socket-mode session remembers for dedup.
pub const SOCKET_DEDUP_WINDOW: usize = 1024;

/// `chat.postMessage` endpoint used by replies.
pub const POST_MESSAGE_URL: &str = "https://slack.com/api/chat.postMessage";

/// Domain errors for the Slack transports.
#[derive(Debug, Error)]
pub enum SlackError {
    /// No active Slack connection with that slug.
    #[error("connection not found")]
    NotFound,
    /// Signature missing or invalid.
    #[error("invalid request signature")]
    BadSignature,
    /// Timestamp outside the replay window.
    #[error("request timestamp is outside the replay window")]
    StaleTimestamp,
    /// Request/envelope is malformed.
    #[error("invalid slack request: {0}")]
    Invalid(String),
    /// Credentials are missing a field the transport needs.
    #[error("slack credential '{0}' is not configured")]
    MissingCredential(&'static str),
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// Transport an event arrived on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlackTransport {
    /// Slack posted to the public webhook route.
    Webhook,
    /// Slack pushed an envelope over the socket-mode WebSocket.
    SocketMode,
}

impl SlackTransport {
    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SlackTransport::Webhook => "webhook",
            SlackTransport::SocketMode => "socket_mode",
        }
    }
}

/// Why a delivery was recorded but not matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    /// The event's workspace is not one of the connection's `workspace`
    /// resources.
    TeamNotOwned,
    /// The connection has channel filters and this channel is not in them.
    ChannelFiltered,
    /// The message was written by this bot (or another bot).
    SelfAuthored,
    /// The event kind is not one the pool matches on.
    UnsupportedEvent,
}

impl RejectReason {
    /// Stable reason name for logs and the delivery record.
    pub fn as_str(self) -> &'static str {
        match self {
            RejectReason::TeamNotOwned => "team_not_owned",
            RejectReason::ChannelFiltered => "channel_filtered",
            RejectReason::SelfAuthored => "self_authored",
            RejectReason::UnsupportedEvent => "unsupported_event",
        }
    }
}

/// A Slack message-shaped event, normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackMessageEvent {
    /// Workspace (team) id the event came from.
    pub team_id: String,
    /// Channel id the event came from.
    pub channel: String,
    /// Slack channel type (`channel`, `group`, `im`, `mpim`), when sent.
    pub channel_type: Option<String>,
    /// Author's Slack user id.
    pub user: String,
    /// Message timestamp, which is also its id within the channel.
    pub ts: String,
    /// Parent thread timestamp when the message is a threaded reply.
    pub thread_ts: Option<String>,
    /// Request text with the bot mention removed; this is what a prompt sees.
    pub text: String,
    /// Text exactly as Slack delivered it, mention included.
    pub raw_text: String,
    /// True when the text mentioned the bot.
    pub bot_mentioned: bool,
}

impl SlackMessageEvent {
    /// Prompt-safe request text: the mention is already removed.
    pub fn prompt_text(&self) -> &str {
        &self.text
    }

    /// Continuation key: the thread the event belongs to.
    ///
    /// A threaded reply keys on its parent, a top-level message on itself, so
    /// every message in one thread shares a key.
    pub fn thread_key(&self) -> &str {
        self.thread_ts.as_deref().unwrap_or(&self.ts)
    }

    /// Where a reply to this event must go.
    pub fn reply_target(&self) -> ReplyTarget {
        ReplyTarget {
            channel: self.channel.clone(),
            thread_ts: self.thread_key().to_string(),
        }
    }
}

/// A normalized Slack event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlackEvent {
    /// The bot was mentioned in a channel or thread (`app_mention`).
    Mention(SlackMessageEvent),
    /// A direct message to the bot (`message` with `channel_type: im`).
    DirectMessage(SlackMessageEvent),
    /// A message in a channel the connection delivers for (`message`).
    Message(SlackMessageEvent),
    /// Deliberately ignored: the bot's own message, an edit, a join notice.
    Ignored {
        /// Slack event type.
        event: String,
        /// Why it was ignored.
        reason: RejectReason,
    },
    /// An event kind the pool does not match on yet.
    Unsupported {
        /// Slack event type.
        event: String,
    },
}

impl SlackEvent {
    /// Stable internal event name.
    pub fn kind(&self) -> &'static str {
        match self {
            SlackEvent::Mention(_) => "slack.mention",
            SlackEvent::DirectMessage(_) => "slack.direct_message",
            SlackEvent::Message(_) => "slack.message",
            SlackEvent::Ignored { .. } => "slack.ignored",
            SlackEvent::Unsupported { .. } => "slack.unsupported",
        }
    }

    /// The message body, for the event kinds that carry one.
    pub fn message(&self) -> Option<&SlackMessageEvent> {
        match self {
            SlackEvent::Mention(message)
            | SlackEvent::DirectMessage(message)
            | SlackEvent::Message(message) => Some(message),
            _ => None,
        }
    }

    /// True for the events that may start or continue an agent.
    pub fn is_actionable(&self) -> bool {
        matches!(
            self,
            SlackEvent::Mention(_) | SlackEvent::DirectMessage(_) | SlackEvent::Message(_)
        )
    }
}

/// One accepted or rejected event, identical in shape for both transports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackDelivery {
    /// Connection that received it.
    pub connection_public_id: String,
    /// Transport it arrived on.
    pub transport: SlackTransport,
    /// Dedup key: Slack's `event_id` when present, else the envelope id.
    pub delivery_id: String,
    /// Normalized event.
    pub event: SlackEvent,
    /// False when the delivery was recorded but not matched.
    pub accepted: bool,
    /// Why it was not matched.
    pub reason: Option<RejectReason>,
}

/// Where a reply goes. Obtainable only from an event, so a reply cannot be
/// addressed to a channel the event did not come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyTarget {
    channel: String,
    thread_ts: String,
}

impl ReplyTarget {
    /// Channel id the originating event came from.
    pub fn channel(&self) -> &str {
        &self.channel
    }

    /// Thread the reply joins: the event's thread, or the event itself.
    pub fn thread_ts(&self) -> &str {
        &self.thread_ts
    }
}

/// A reply emitted by a step, addressed by construction to its event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackReply {
    target: ReplyTarget,
    text: String,
}

impl SlackReply {
    /// Build a reply to `event`.
    ///
    /// Fails when the text is empty or longer than [`MAX_REPLY_CHARS`].
    pub fn to_event(
        event: &SlackMessageEvent,
        text: impl Into<String>,
    ) -> Result<Self, SlackError> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(SlackError::Invalid("reply text is required".into()));
        }
        if text.chars().count() > MAX_REPLY_CHARS {
            return Err(SlackError::Invalid(format!(
                "reply text must be at most {MAX_REPLY_CHARS} characters"
            )));
        }
        Ok(Self {
            target: event.reply_target(),
            text,
        })
    }

    /// Where this reply goes.
    pub fn target(&self) -> &ReplyTarget {
        &self.target
    }

    /// Reply text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// `chat.postMessage` request body for this reply.
    pub fn post_message_body(&self) -> Value {
        json!({
            "channel": self.target.channel,
            "thread_ts": self.target.thread_ts,
            "text": self.text,
        })
    }

    /// The Slack API call that sends this reply.
    ///
    /// The pool builds the request here and the dispatcher performs it with the
    /// connection's `bot_token` (task 1.4.6 owns the outbound HTTP client), so
    /// the addressing is decided and testable in one place.
    pub fn post_message_request(&self) -> SlackApiRequest {
        SlackApiRequest {
            method: "POST",
            url: POST_MESSAGE_URL,
            credential_key: "bot_token",
            body: self.post_message_body(),
        }
    }
}

/// A Slack Web API call the pool wants performed.
#[derive(Debug, Clone, PartialEq)]
pub struct SlackApiRequest {
    /// HTTP method.
    pub method: &'static str,
    /// Absolute endpoint URL.
    pub url: &'static str,
    /// Credential field whose value authenticates the call (never the value).
    pub credential_key: &'static str,
    /// JSON request body.
    pub body: Value,
}

/// Everything the transports need to know about one Slack connection.
///
/// `Debug` redacts the signing secret.
#[derive(Clone)]
pub struct SlackIngestContext {
    /// Public id of the connection (`env:<slug>` for environment entries).
    pub connection_public_id: String,
    /// Bot user id, when configured; used to strip mentions and to ignore the
    /// bot's own messages.
    pub bot_user_id: Option<String>,
    /// Workspace (team) ids the connection delivers for. Empty allows any.
    pub team_ids: Vec<String>,
    /// Channel filters. Empty allows any channel in an owned workspace.
    pub channel_ids: Vec<String>,
    /// True when an `app_token` is configured, so socket mode can be dialled.
    pub socket_mode_available: bool,
    signing_secret: String,
}

impl std::fmt::Debug for SlackIngestContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlackIngestContext")
            .field("connection_public_id", &self.connection_public_id)
            .field("bot_user_id", &self.bot_user_id)
            .field("team_ids", &self.team_ids)
            .field("channel_ids", &self.channel_ids)
            .field("socket_mode_available", &self.socket_mode_available)
            .field("signing_secret", &"<redacted>")
            .finish()
    }
}

impl SlackIngestContext {
    /// Build a context from an effective connection and its resolved
    /// credentials.
    ///
    /// `signing_secret` is required for the webhook transport and is also what
    /// the socket-mode handshake is configured against; `app_token` decides
    /// whether socket mode is available.
    pub fn from_connection(
        connection: &EffectiveConnection,
        material: &CredentialMaterial,
    ) -> Result<Self, SlackError> {
        if connection.provider != ProviderKind::Slack
            || connection.status != ConnectionStatus::Active
        {
            return Err(SlackError::NotFound);
        }
        let signing_secret = material
            .values
            .get("signing_secret")
            .filter(|value| !value.is_empty())
            .ok_or(SlackError::MissingCredential("signing_secret"))?
            .clone();
        let socket_mode_available = material
            .values
            .get("app_token")
            .is_some_and(|value| !value.is_empty());
        let bot_user_id = material
            .values
            .get("bot_user_id")
            .filter(|value| !value.is_empty())
            .cloned();

        let mut team_ids = Vec::new();
        let mut channel_ids = Vec::new();
        for resource in &connection.resources {
            match resource.resource_kind.as_str() {
                "workspace" => team_ids.push(normalize_identity(&resource.external_id)),
                "channel" => channel_ids.push(normalize_identity(&resource.external_id)),
                _ => {}
            }
        }

        Ok(Self {
            connection_public_id: connection.public_id.clone(),
            bot_user_id,
            team_ids,
            channel_ids,
            socket_mode_available,
            signing_secret,
        })
    }

    /// Build a context directly, for socket-mode wiring and tests.
    pub fn new(
        connection_public_id: impl Into<String>,
        signing_secret: impl Into<String>,
    ) -> Result<Self, SlackError> {
        let signing_secret = signing_secret.into();
        if signing_secret.is_empty() {
            return Err(SlackError::MissingCredential("signing_secret"));
        }
        Ok(Self {
            connection_public_id: connection_public_id.into(),
            bot_user_id: None,
            team_ids: Vec::new(),
            channel_ids: Vec::new(),
            socket_mode_available: false,
            signing_secret,
        })
    }

    /// Set the bot user id whose mentions are stripped.
    pub fn with_bot_user_id(mut self, bot_user_id: impl Into<String>) -> Self {
        self.bot_user_id = Some(bot_user_id.into());
        self
    }

    /// Set the workspaces the connection delivers for.
    pub fn with_teams<I, S>(mut self, team_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.team_ids = team_ids
            .into_iter()
            .map(|id| normalize_identity(id.as_ref()))
            .collect();
        self
    }

    /// Set the channel filters. Empty allows any channel.
    pub fn with_channels<I, S>(mut self, channel_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.channel_ids = channel_ids
            .into_iter()
            .map(|id| normalize_identity(id.as_ref()))
            .collect();
        self
    }

    /// Mark socket mode as available (an `app_token` is configured).
    pub fn with_socket_mode(mut self, available: bool) -> Self {
        self.socket_mode_available = available;
        self
    }

    /// True when the workspace is one the connection delivers for.
    pub fn team_allowed(&self, team_id: &str) -> bool {
        self.team_ids.is_empty() || self.team_ids.contains(&normalize_identity(team_id))
    }

    /// True when the channel passes the connection's filters.
    pub fn channel_allowed(&self, channel: &str) -> bool {
        self.channel_ids.is_empty() || self.channel_ids.contains(&normalize_identity(channel))
    }
}

/// A parsed Slack HTTP request body.
#[derive(Debug, Clone, PartialEq)]
pub enum SlackRequest {
    /// Slack's endpoint check; the challenge is echoed back.
    UrlVerification {
        /// Value to return verbatim.
        challenge: String,
    },
    /// An Events API callback.
    EventCallback {
        /// Slack's unique id for this event.
        event_id: String,
        /// Workspace the event came from.
        team_id: String,
        /// The inner event object.
        event: Value,
    },
}

/// Result of an accepted webhook request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookIngest {
    /// Answer Slack's `url_verification` with this challenge.
    Challenge(String),
    /// A normalized delivery. Boxed: a delivery is much larger than a
    /// challenge string.
    Delivery(Box<SlackDelivery>),
}

/// Outcome of one webhook request, after the delivery was recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlackWebhookOutcome {
    /// Challenge to echo, for `url_verification`.
    pub challenge: Option<String>,
    /// The delivery, for an event callback.
    pub delivery: Option<SlackDelivery>,
    /// True when this delivery id was already recorded (a Slack retry, or the
    /// same event already received over the other transport).
    pub duplicate: bool,
}

// ── signature verification ────────────────────────────────────────────────

/// Verify `X-Slack-Signature: v0=<hex>` over `v0:<timestamp>:<body>`.
pub fn verify_signature(
    signing_secret: &str,
    timestamp: &str,
    signature: &str,
    body: &[u8],
) -> bool {
    let Some(encoded) = signature.strip_prefix("v0=") else {
        return false;
    };
    let Ok(expected) = hex::decode(encoded) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(signing_secret.as_bytes()) else {
        return false;
    };
    mac.update(b"v0:");
    mac.update(timestamp.as_bytes());
    mac.update(b":");
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

/// Compute the `X-Slack-Signature` value for a body (tests and reference).
///
/// HMAC-SHA256 takes a key of any length, so the error branch is unreachable;
/// it returns an empty signature rather than panicking, which then fails
/// verification at the call site.
pub fn sign(signing_secret: &str, timestamp: &str, body: &[u8]) -> String {
    let Ok(mut mac) = HmacSha256::new_from_slice(signing_secret.as_bytes()) else {
        return String::new();
    };
    mac.update(b"v0:");
    mac.update(timestamp.as_bytes());
    mac.update(b":");
    mac.update(body);
    format!("v0={}", hex::encode(mac.finalize().into_bytes()))
}

/// True when the request timestamp is inside the replay window.
pub fn timestamp_fresh(timestamp: &str, now_unix: i64) -> bool {
    match timestamp.parse::<i64>() {
        Ok(sent) => (now_unix - sent).abs() <= MAX_TIMESTAMP_SKEW_SECS,
        Err(_) => false,
    }
}

// ── normalization ─────────────────────────────────────────────────────────

/// Remove a bot mention from message text.
///
/// Slack writes mentions as `<@U123>` or `<@U123|label>`. With `bot_user_id`
/// known, every mention of that user is removed; without it, only a leading
/// mention is removed, which is the one that caused an `app_mention`. Runs of
/// spaces and tabs left behind are collapsed; newlines are preserved so a
/// multi-line request reaches the prompt intact.
pub fn strip_mention(text: &str, bot_user_id: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut removed = false;

    while let Some(start) = rest.find("<@") {
        let Some(end_offset) = rest[start..].find('>') else {
            break;
        };
        let end = start + end_offset;
        let inner = &rest[start + 2..end];
        let id = inner.split('|').next().unwrap_or(inner);
        let is_bot = match bot_user_id {
            Some(bot) => id.eq_ignore_ascii_case(bot),
            // Unknown bot id: the mention that triggered the event is the
            // leading one, so only strip a mention nothing precedes.
            None => !removed && out.trim().is_empty() && rest[..start].trim().is_empty(),
        };
        out.push_str(&rest[..start]);
        if is_bot {
            removed = true;
        } else {
            out.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    collapse_inline_spaces(out.trim())
}

/// True when `text` mentions `user_id`.
pub fn mentions_user(text: &str, user_id: &str) -> bool {
    let mut rest = text;
    while let Some(start) = rest.find("<@") {
        let Some(end_offset) = rest[start..].find('>') else {
            return false;
        };
        let end = start + end_offset;
        let inner = &rest[start + 2..end];
        let id = inner.split('|').next().unwrap_or(inner);
        if id.eq_ignore_ascii_case(user_id) {
            return true;
        }
        rest = &rest[end + 1..];
    }
    false
}

fn collapse_inline_spaces(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_space = false;
    for ch in text.chars() {
        if ch == ' ' || ch == '\t' {
            in_space = true;
            continue;
        }
        if in_space && !out.is_empty() && ch != '\n' {
            out.push(' ');
        }
        in_space = false;
        out.push(ch);
    }
    out
}

fn normalize_identity(raw: &str) -> String {
    raw.trim().trim_start_matches('#').to_ascii_lowercase()
}

/// Parse a Slack HTTP request body (`url_verification` or `event_callback`).
pub fn parse_request(body: &[u8]) -> Result<SlackRequest, SlackError> {
    let payload: Value = serde_json::from_slice(body)
        .map_err(|e| SlackError::Invalid(format!("body is not JSON: {e}")))?;
    parse_payload(&payload)
}

/// Parse an already-decoded Slack payload.
pub fn parse_payload(payload: &Value) -> Result<SlackRequest, SlackError> {
    match payload.get("type").and_then(Value::as_str) {
        Some("url_verification") => {
            let challenge = payload
                .get("challenge")
                .and_then(Value::as_str)
                .ok_or_else(|| SlackError::Invalid("url_verification needs a challenge".into()))?;
            Ok(SlackRequest::UrlVerification {
                challenge: challenge.to_string(),
            })
        }
        Some("event_callback") => {
            let event = payload
                .get("event")
                .cloned()
                .ok_or_else(|| SlackError::Invalid("event_callback needs an event".into()))?;
            let team_id = payload
                .get("team_id")
                .and_then(Value::as_str)
                .or_else(|| event.get("team").and_then(Value::as_str))
                .unwrap_or_default()
                .to_string();
            let event_id = payload
                .get("event_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            Ok(SlackRequest::EventCallback {
                event_id,
                team_id,
                event,
            })
        }
        Some(other) => Err(SlackError::Invalid(format!(
            "unsupported request type '{other}'"
        ))),
        None => Err(SlackError::Invalid("request type is required".into())),
    }
}

/// Normalize one inner Slack event object.
///
/// `app_mention` becomes [`SlackEvent::Mention`], a `message` with
/// `channel_type: im` a [`SlackEvent::DirectMessage`], any other `message` a
/// [`SlackEvent::Message`]. Edits, joins and other subtypes, and anything the
/// bot itself wrote, are [`SlackEvent::Ignored`].
pub fn normalize_event(
    context: &SlackIngestContext,
    team_id: &str,
    event: &Value,
) -> Result<SlackEvent, SlackError> {
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| SlackError::Invalid("event type is required".into()))?;

    match event_type {
        "app_mention" | "message" => {
            let subtype = event.get("subtype").and_then(Value::as_str);
            if let Some(subtype) = subtype {
                // Edits, deletions, joins and bot posts all arrive as
                // subtypes; none of them starts or continues an agent.
                return Ok(SlackEvent::Ignored {
                    event: format!("{event_type}.{subtype}"),
                    reason: RejectReason::SelfAuthored,
                });
            }
            let user = event
                .get("user")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let authored_by_bot = event.get("bot_id").is_some()
                || context
                    .bot_user_id
                    .as_deref()
                    .is_some_and(|bot| bot.eq_ignore_ascii_case(&user));
            if authored_by_bot {
                return Ok(SlackEvent::Ignored {
                    event: event_type.to_string(),
                    reason: RejectReason::SelfAuthored,
                });
            }

            let raw_text = event
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let text = strip_mention(&raw_text, context.bot_user_id.as_deref());
            let bot_mentioned = match context.bot_user_id.as_deref() {
                Some(bot) => mentions_user(&raw_text, bot),
                None => raw_text.trim_start().starts_with("<@"),
            };
            let team_id = event
                .get("team")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or(team_id)
                .to_string();
            let message = SlackMessageEvent {
                team_id,
                channel: event
                    .get("channel")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                channel_type: event
                    .get("channel_type")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                user,
                ts: event
                    .get("ts")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                thread_ts: event
                    .get("thread_ts")
                    .and_then(Value::as_str)
                    .filter(|thread| !thread.is_empty())
                    .map(str::to_string),
                text,
                raw_text,
                bot_mentioned,
            };
            if message.channel.is_empty() || message.ts.is_empty() {
                return Err(SlackError::Invalid(
                    "message events need a channel and a ts".into(),
                ));
            }
            Ok(match (event_type, message.channel_type.as_deref()) {
                ("app_mention", _) => SlackEvent::Mention(message),
                ("message", Some("im")) => SlackEvent::DirectMessage(message),
                _ => SlackEvent::Message(message),
            })
        }
        other => Ok(SlackEvent::Unsupported {
            event: other.to_string(),
        }),
    }
}

/// Apply workspace resolution and channel filters to a normalized event.
pub fn decide(context: &SlackIngestContext, event: &SlackEvent) -> (bool, Option<RejectReason>) {
    match event {
        SlackEvent::Ignored { reason, .. } => (false, Some(*reason)),
        SlackEvent::Unsupported { .. } => (false, Some(RejectReason::UnsupportedEvent)),
        SlackEvent::Mention(message)
        | SlackEvent::DirectMessage(message)
        | SlackEvent::Message(message) => {
            if !context.team_allowed(&message.team_id) {
                return (false, Some(RejectReason::TeamNotOwned));
            }
            if !context.channel_allowed(&message.channel) {
                return (false, Some(RejectReason::ChannelFiltered));
            }
            (true, None)
        }
    }
}

/// Normalize and decide one event callback, producing a delivery.
pub fn build_delivery(
    context: &SlackIngestContext,
    transport: SlackTransport,
    delivery_id: &str,
    team_id: &str,
    event: &Value,
) -> Result<SlackDelivery, SlackError> {
    let event = normalize_event(context, team_id, event)?;
    let (accepted, reason) = decide(context, &event);
    Ok(SlackDelivery {
        connection_public_id: context.connection_public_id.clone(),
        transport,
        delivery_id: delivery_id.to_string(),
        event,
        accepted,
        reason,
    })
}

// ── webhook transport ─────────────────────────────────────────────────────

/// Verify and normalize one webhook request, without touching the database.
///
/// Verification happens before parsing, so an unsigned or replayed body never
/// reaches normalization or trigger matching.
pub fn ingest_webhook(
    context: &SlackIngestContext,
    headers: &HeaderMap,
    body: &[u8],
    now_unix: i64,
) -> Result<WebhookIngest, SlackError> {
    let timestamp = header(headers, "x-slack-request-timestamp").ok_or(SlackError::BadSignature)?;
    let signature = header(headers, "x-slack-signature").ok_or(SlackError::BadSignature)?;
    if !timestamp_fresh(timestamp, now_unix) {
        return Err(SlackError::StaleTimestamp);
    }
    if !verify_signature(&context.signing_secret, timestamp, signature, body) {
        return Err(SlackError::BadSignature);
    }

    match parse_request(body)? {
        SlackRequest::UrlVerification { challenge } => Ok(WebhookIngest::Challenge(challenge)),
        SlackRequest::EventCallback {
            event_id,
            team_id,
            event,
        } => {
            let delivery_id = delivery_key(&event_id, &event);
            Ok(WebhookIngest::Delivery(Box::new(build_delivery(
                context,
                SlackTransport::Webhook,
                &delivery_id,
                &team_id,
                &event,
            )?)))
        }
    }
}

/// Handle one `POST /v1/webhooks/slack/{slug}` request.
///
/// Resolves the connection by slug, verifies the signature against its
/// `signing_secret`, normalizes the event and records the delivery once.
pub async fn handle_webhook(
    pool: &MySqlPool,
    organization_id: u64,
    slug: &str,
    headers: &HeaderMap,
    body: &[u8],
    now_unix: i64,
) -> Result<SlackWebhookOutcome, SlackError> {
    let context = resolve_context(pool, organization_id, slug).await?;
    match ingest_webhook(&context, headers, body, now_unix)? {
        WebhookIngest::Challenge(challenge) => Ok(SlackWebhookOutcome {
            challenge: Some(challenge),
            delivery: None,
            duplicate: false,
        }),
        WebhookIngest::Delivery(delivery) => {
            let duplicate = record_delivery(pool, organization_id, &delivery).await?;
            Ok(SlackWebhookOutcome {
                challenge: None,
                delivery: Some(*delivery),
                duplicate,
            })
        }
    }
}

/// Resolve the ingest context for a connection slug.
pub async fn resolve_context(
    pool: &MySqlPool,
    organization_id: u64,
    slug: &str,
) -> Result<SlackIngestContext, SlackError> {
    let connection = find_effective_by_slug(pool, organization_id, slug)
        .await
        .map_err(connection_error)?
        .ok_or(SlackError::NotFound)?;
    if connection.provider != ProviderKind::Slack || connection.status != ConnectionStatus::Active {
        return Err(SlackError::NotFound);
    }
    let material = credential_material(&connection.credential_ref).map_err(connection_error)?;
    SlackIngestContext::from_connection(&connection, &material)
}

/// Record a delivery, returning true when it was already recorded.
///
/// The unique `(connection_public_id, delivery_id)` key makes a Slack retry —
/// and the same event arriving over the other transport — a single row.
pub async fn record_delivery(
    pool: &MySqlPool,
    organization_id: u64,
    delivery: &SlackDelivery,
) -> Result<bool, SlackError> {
    let result = sqlx::query(
        "INSERT IGNORE INTO webhook_deliveries \
         (organization_id, connection_public_id, provider, delivery_id, event, action, result) \
         VALUES (?, ?, 'slack', ?, ?, ?, ?)",
    )
    .bind(organization_id)
    .bind(&delivery.connection_public_id)
    .bind(&delivery.delivery_id)
    .bind(delivery.event.kind())
    .bind(delivery.transport.as_str())
    .bind(if delivery.accepted {
        "accepted"
    } else {
        "ignored"
    })
    .execute(pool)
    .await
    .map_err(SlackError::Database)?;
    Ok(result.rows_affected() == 0)
}

/// Find the active Slack connection that owns a workspace (case-insensitive).
pub async fn resolve_workspace_connection(
    pool: &MySqlPool,
    organization_id: u64,
    team_id: &str,
) -> Result<Option<ConnectionView>, ConnectionError> {
    let team_id = normalize_identity(team_id);
    let connections = list_connections(pool, organization_id).await?;
    Ok(connections.into_iter().find(|connection| {
        connection.provider == ProviderKind::Slack
            && connection.status == ConnectionStatus::Active
            && connection.resources.iter().any(|resource| {
                resource.resource_kind == "workspace"
                    && normalize_identity(&resource.external_id) == team_id
            })
    }))
}

/// Validate that a workspace is owned by an active Slack connection.
///
/// Configuration validation calls this so an unknown workspace is refused at
/// save time with a named `workspace` field.
pub async fn validate_workspace(
    pool: &MySqlPool,
    organization_id: u64,
    team_id: &str,
) -> Result<ConnectionView, ConnectionError> {
    resolve_workspace_connection(pool, organization_id, team_id)
        .await?
        .ok_or_else(|| ConnectionError::UnknownResource {
            field: "workspace",
            value: team_id.to_string(),
        })
}

fn connection_error(error: ConnectionError) -> SlackError {
    match error {
        ConnectionError::NotFound => SlackError::NotFound,
        ConnectionError::Database(error) => SlackError::Database(error),
        other => SlackError::Invalid(other.to_string()),
    }
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn delivery_key(event_id: &str, event: &Value) -> String {
    if !event_id.is_empty() {
        return event_id.to_string();
    }
    // Without an event_id, the event's own timestamp identifies it.
    let channel = event
        .get("channel")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let ts = event
        .get("event_ts")
        .or_else(|| event.get("ts"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    format!("{channel}:{ts}")
}

// ── socket mode transport ─────────────────────────────────────────────────

/// What the socket-mode client must do with one inbound frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketOutcome {
    /// Slack's greeting; the connection is live.
    Hello,
    /// An envelope: send `ack` back, then handle `delivery`.
    Envelope {
        /// Acknowledgement frame to send immediately.
        ack: String,
        /// The delivery, or `None` when this event was already seen. Boxed:
        /// a delivery is much larger than the other frame outcomes.
        delivery: Option<Box<SlackDelivery>>,
    },
    /// Slack asked the client to reconnect.
    Disconnect {
        /// Slack's stated reason (`warning`, `refresh_requested`, ...).
        reason: String,
    },
    /// A frame the client acknowledges but does nothing else with.
    Acknowledged {
        /// Acknowledgement frame to send.
        ack: String,
    },
    /// A frame the client ignores.
    Ignored,
}

/// Why a socket-mode session stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SocketStop {
    /// Slack sent a `disconnect` frame; the client should redial.
    Disconnected {
        /// Slack's stated reason.
        reason: String,
    },
    /// The frame stream ended.
    StreamClosed,
}

/// One socket-mode connection's protocol state.
///
/// Slack pushes envelopes over a WebSocket the pool dialled with its
/// `app_token`, so no public address is needed. The session is transport
/// agnostic on purpose: it consumes frames as text and returns the frames to
/// send, which lets a live socket and a scripted feed drive identical logic.
#[derive(Debug)]
pub struct SlackSocketSession {
    context: SlackIngestContext,
    seen: BTreeSet<String>,
    order: VecDeque<String>,
}

impl SlackSocketSession {
    /// Start a session for a connection.
    pub fn new(context: SlackIngestContext) -> Self {
        Self {
            context,
            seen: BTreeSet::new(),
            order: VecDeque::new(),
        }
    }

    /// The connection this session delivers for.
    pub fn context(&self) -> &SlackIngestContext {
        &self.context
    }

    /// Handle one inbound text frame.
    pub fn handle_frame(&mut self, frame: &str) -> Result<SocketOutcome, SlackError> {
        let envelope: Value = serde_json::from_str(frame)
            .map_err(|e| SlackError::Invalid(format!("frame is not JSON: {e}")))?;
        let frame_type = envelope
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| SlackError::Invalid("frame type is required".into()))?;

        match frame_type {
            "hello" => Ok(SocketOutcome::Hello),
            "disconnect" => Ok(SocketOutcome::Disconnect {
                reason: envelope
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("unspecified")
                    .to_string(),
            }),
            "events_api" => {
                let envelope_id = envelope
                    .get("envelope_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| SlackError::Invalid("envelope_id is required".into()))?;
                let ack = ack_frame(envelope_id);
                let payload = envelope
                    .get("payload")
                    .ok_or_else(|| SlackError::Invalid("envelope needs a payload".into()))?;
                let SlackRequest::EventCallback {
                    event_id,
                    team_id,
                    event,
                } = parse_payload(payload)?
                else {
                    return Ok(SocketOutcome::Acknowledged { ack });
                };
                let mut delivery_id = delivery_key(&event_id, &event);
                if delivery_id.is_empty() || delivery_id == ":" {
                    delivery_id = envelope_id.to_string();
                }
                if !self.remember(&delivery_id) {
                    // A redelivered envelope after a reconnect: acknowledge it
                    // again, but hand it on only once.
                    return Ok(SocketOutcome::Envelope {
                        ack,
                        delivery: None,
                    });
                }
                let delivery = build_delivery(
                    &self.context,
                    SlackTransport::SocketMode,
                    &delivery_id,
                    &team_id,
                    &event,
                )?;
                Ok(SocketOutcome::Envelope {
                    ack,
                    delivery: Some(Box::new(delivery)),
                })
            }
            // Slash commands and interactive payloads are acknowledged so
            // Slack does not retry them; the pool does not match on them yet.
            "slash_commands" | "interactive" => {
                let envelope_id = envelope
                    .get("envelope_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| SlackError::Invalid("envelope_id is required".into()))?;
                Ok(SocketOutcome::Acknowledged {
                    ack: ack_frame(envelope_id),
                })
            }
            _ => Ok(SocketOutcome::Ignored),
        }
    }

    /// Record a delivery id, returning false when it was already seen.
    fn remember(&mut self, delivery_id: &str) -> bool {
        if !self.seen.insert(delivery_id.to_string()) {
            return false;
        }
        self.order.push_back(delivery_id.to_string());
        if self.order.len() > SOCKET_DEDUP_WINDOW
            && let Some(evicted) = self.order.pop_front()
        {
            self.seen.remove(&evicted);
        }
        true
    }
}

/// The acknowledgement frame for an envelope id.
pub fn ack_frame(envelope_id: &str) -> String {
    json!({ "envelope_id": envelope_id }).to_string()
}

/// Pump a socket-mode session over a frame channel.
///
/// `inbound` carries text frames from the WebSocket (or a test feed),
/// `outbound` takes the acknowledgements to send, and `deliveries` receives
/// each event once. Returns when Slack asks for a reconnect or the stream ends.
pub async fn run_socket_session(
    session: &mut SlackSocketSession,
    inbound: &mut mpsc::Receiver<String>,
    outbound: &mpsc::Sender<String>,
    deliveries: &mpsc::Sender<SlackDelivery>,
) -> SocketStop {
    while let Some(frame) = inbound.recv().await {
        let outcome = match session.handle_frame(&frame) {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::warn!(error = %error, "slack socket frame rejected");
                continue;
            }
        };
        match outcome {
            SocketOutcome::Hello | SocketOutcome::Ignored => {}
            SocketOutcome::Acknowledged { ack } => {
                if outbound.send(ack).await.is_err() {
                    return SocketStop::StreamClosed;
                }
            }
            SocketOutcome::Envelope { ack, delivery } => {
                if outbound.send(ack).await.is_err() {
                    return SocketStop::StreamClosed;
                }
                if let Some(delivery) = delivery
                    && deliveries.send(*delivery).await.is_err()
                {
                    return SocketStop::StreamClosed;
                }
            }
            SocketOutcome::Disconnect { reason } => return SocketStop::Disconnected { reason },
        }
    }
    SocketStop::StreamClosed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::connection::ConnectionResourceView;
    use serde_json::json;

    const SECRET: &str = "slack-signing-secret";
    const BOT: &str = "U0BOT";

    fn context() -> SlackIngestContext {
        SlackIngestContext::new("conn-1", SECRET)
            .expect("secret")
            .with_bot_user_id(BOT)
            .with_teams(["T1"])
    }

    fn mention_payload() -> Value {
        json!({
            "type": "event_callback",
            "team_id": "T1",
            "event_id": "Ev1",
            "event": {
                "type": "app_mention",
                "user": "U2",
                "text": "<@U0BOT> deploy the site",
                "channel": "C1",
                "ts": "1700000000.000100",
                "event_ts": "1700000000.000100"
            }
        })
    }

    #[test]
    fn signature_verification_rejects_tampering_and_wrong_secret() {
        let body = br#"{"type":"event_callback"}"#;
        let signature = sign(SECRET, "1700000000", body);
        assert!(verify_signature(SECRET, "1700000000", &signature, body));
        assert!(!verify_signature("other", "1700000000", &signature, body));
        assert!(!verify_signature(SECRET, "1700000001", &signature, body));
        assert!(!verify_signature(SECRET, "1700000000", &signature, b"{}"));
        assert!(!verify_signature(SECRET, "1700000000", "v0=zz", body));
        assert!(!verify_signature(SECRET, "1700000000", "sha256=abc", body));
    }

    #[test]
    fn stale_timestamps_are_outside_the_replay_window() {
        assert!(timestamp_fresh("1700000000", 1_700_000_100));
        assert!(timestamp_fresh("1700000000", 1_699_999_900));
        assert!(!timestamp_fresh("1700000000", 1_700_000_000 + 301));
        assert!(!timestamp_fresh("not-a-number", 1_700_000_000));
    }

    #[test]
    fn mention_is_stripped_from_request_text() {
        assert_eq!(
            strip_mention("<@U0BOT> deploy the site", Some(BOT)),
            "deploy the site"
        );
        assert_eq!(
            strip_mention("hey <@U0BOT|tide> please deploy", Some(BOT)),
            "hey please deploy"
        );
        assert_eq!(
            strip_mention("<@U0BOT> ping <@U9OTHER>", Some(BOT)),
            "ping <@U9OTHER>"
        );
        // Unknown bot id: only the leading mention goes.
        assert_eq!(
            strip_mention("<@U0BOT> ping <@U0BOT>", None),
            "ping <@U0BOT>"
        );
        // Newlines survive; runs of spaces collapse.
        assert_eq!(
            strip_mention("<@U0BOT>  line one\nline two", Some(BOT)),
            "line one\nline two"
        );
        assert_eq!(
            strip_mention("no mention here", Some(BOT)),
            "no mention here"
        );
        assert_eq!(strip_mention("<@U0BOT", Some(BOT)), "<@U0BOT");
    }

    #[test]
    fn mentions_user_finds_labelled_and_plain_mentions() {
        assert!(mentions_user("hi <@U0BOT>", BOT));
        assert!(mentions_user("hi <@U0BOT|tide>", BOT));
        assert!(!mentions_user("hi <@U9OTHER>", BOT));
        assert!(!mentions_user("hi U0BOT", BOT));
    }

    #[test]
    fn app_mention_normalizes_and_hides_the_mention_from_prompts() {
        let payload = mention_payload();
        let SlackRequest::EventCallback {
            event_id,
            team_id,
            event,
        } = parse_payload(&payload).expect("parse")
        else {
            panic!("expected an event callback");
        };
        assert_eq!(event_id, "Ev1");
        let normalized = normalize_event(&context(), &team_id, &event).expect("normalize");
        assert_eq!(normalized.kind(), "slack.mention");
        let message = normalized.message().expect("message");
        assert_eq!(message.prompt_text(), "deploy the site");
        assert!(!message.prompt_text().contains("<@"));
        assert_eq!(message.raw_text, "<@U0BOT> deploy the site");
        assert!(message.bot_mentioned);
        assert_eq!(message.channel, "C1");
        assert_eq!(message.thread_key(), "1700000000.000100");
    }

    #[test]
    fn direct_messages_and_plain_messages_are_distinguished() {
        let context = context();
        let dm = json!({
            "type": "message", "channel_type": "im", "user": "U2",
            "text": "status?", "channel": "D1", "ts": "1.1"
        });
        assert_eq!(
            normalize_event(&context, "T1", &dm)
                .expect("normalize")
                .kind(),
            "slack.direct_message"
        );

        let channel_message = json!({
            "type": "message", "channel_type": "channel", "user": "U2",
            "text": "unrelated", "channel": "C1", "ts": "1.2"
        });
        let normalized = normalize_event(&context, "T1", &channel_message).expect("normalize");
        assert_eq!(normalized.kind(), "slack.message");
        assert!(!normalized.message().expect("message").bot_mentioned);
    }

    #[test]
    fn the_bots_own_messages_and_subtypes_are_ignored() {
        let context = context();
        let own = json!({
            "type": "message", "user": BOT, "text": "<@U0BOT> hi",
            "channel": "C1", "ts": "1.3"
        });
        let normalized = normalize_event(&context, "T1", &own).expect("normalize");
        assert_eq!(normalized.kind(), "slack.ignored");
        assert!(!normalized.is_actionable());

        let bot_post = json!({
            "type": "message", "bot_id": "B1", "text": "automated",
            "channel": "C1", "ts": "1.4"
        });
        assert_eq!(
            normalize_event(&context, "T1", &bot_post)
                .expect("normalize")
                .kind(),
            "slack.ignored"
        );

        let edit = json!({
            "type": "message", "subtype": "message_changed",
            "channel": "C1", "ts": "1.5"
        });
        assert_eq!(
            normalize_event(&context, "T1", &edit)
                .expect("normalize")
                .kind(),
            "slack.ignored"
        );
    }

    #[test]
    fn unknown_event_kinds_are_unsupported_and_malformed_ones_fail() {
        let context = context();
        let reaction = json!({"type": "reaction_added", "user": "U2"});
        let normalized = normalize_event(&context, "T1", &reaction).expect("normalize");
        assert_eq!(normalized.kind(), "slack.unsupported");
        assert_eq!(
            decide(&context, &normalized),
            (false, Some(RejectReason::UnsupportedEvent))
        );

        let no_channel = json!({"type": "app_mention", "user": "U2", "ts": "1.6"});
        assert!(matches!(
            normalize_event(&context, "T1", &no_channel),
            Err(SlackError::Invalid(_))
        ));
        assert!(matches!(
            normalize_event(&context, "T1", &json!({})),
            Err(SlackError::Invalid(_))
        ));
    }

    #[test]
    fn workspace_resolution_and_channel_filters_reject_delivery() {
        let unfiltered = context();
        let context = unfiltered.clone().with_channels(["C1"]);
        let event = json!({
            "type": "app_mention", "user": "U2", "text": "<@U0BOT> go",
            "channel": "C9", "ts": "1.7"
        });
        let delivery =
            build_delivery(&context, SlackTransport::Webhook, "Ev9", "T1", &event).expect("build");
        assert!(!delivery.accepted);
        assert_eq!(delivery.reason, Some(RejectReason::ChannelFiltered));

        let foreign = json!({
            "type": "app_mention", "user": "U2", "text": "<@U0BOT> go",
            "channel": "C1", "ts": "1.8"
        });
        let delivery = build_delivery(&context, SlackTransport::Webhook, "Ev8", "T9", &foreign)
            .expect("build");
        assert!(!delivery.accepted);
        assert_eq!(delivery.reason, Some(RejectReason::TeamNotOwned));

        // Channel names are accepted with or without the leading '#'.
        let named = context.clone().with_channels(["#general"]);
        assert!(named.channel_allowed("general"));
        assert!(named.channel_allowed("#General"));
        assert!(!named.channel_allowed("random"));
        // No filters means any channel in an owned workspace.
        assert!(unfiltered.channel_allowed("C-anything"));
    }

    #[test]
    fn url_verification_is_parsed_and_unknown_types_refused() {
        let body = br#"{"type":"url_verification","challenge":"abc123"}"#;
        assert_eq!(
            parse_request(body).expect("parse"),
            SlackRequest::UrlVerification {
                challenge: "abc123".into()
            }
        );
        assert!(matches!(
            parse_request(br#"{"type":"url_verification"}"#),
            Err(SlackError::Invalid(_))
        ));
        assert!(matches!(
            parse_request(br#"{"type":"something_else"}"#),
            Err(SlackError::Invalid(_))
        ));
        assert!(matches!(
            parse_request(b"not json"),
            Err(SlackError::Invalid(_))
        ));
        assert!(matches!(parse_request(b"{}"), Err(SlackError::Invalid(_))));
        assert!(matches!(
            parse_request(br#"{"type":"event_callback"}"#),
            Err(SlackError::Invalid(_))
        ));
    }

    #[test]
    fn webhook_ingest_requires_a_valid_fresh_signature() {
        let context = context();
        let body = serde_json::to_vec(&mention_payload()).expect("serialize");
        let now = 1_700_000_000;
        let timestamp = now.to_string();

        let mut headers = HeaderMap::new();
        assert!(matches!(
            ingest_webhook(&context, &headers, &body, now),
            Err(SlackError::BadSignature)
        ));

        headers.insert(
            actix_web::http::header::HeaderName::from_static("x-slack-request-timestamp"),
            actix_web::http::header::HeaderValue::from_str(&timestamp).expect("header"),
        );
        headers.insert(
            actix_web::http::header::HeaderName::from_static("x-slack-signature"),
            actix_web::http::header::HeaderValue::from_str("v0=deadbeef").expect("header"),
        );
        assert!(matches!(
            ingest_webhook(&context, &headers, &body, now),
            Err(SlackError::BadSignature)
        ));

        let signature = sign(SECRET, &timestamp, &body);
        headers.insert(
            actix_web::http::header::HeaderName::from_static("x-slack-signature"),
            actix_web::http::header::HeaderValue::from_str(&signature).expect("header"),
        );
        assert!(matches!(
            ingest_webhook(&context, &headers, &body, now + 600),
            Err(SlackError::StaleTimestamp)
        ));

        let ingest = ingest_webhook(&context, &headers, &body, now).expect("ingest");
        let WebhookIngest::Delivery(delivery) = ingest else {
            panic!("expected a delivery");
        };
        assert!(delivery.accepted);
        assert_eq!(delivery.delivery_id, "Ev1");
        assert_eq!(delivery.transport, SlackTransport::Webhook);
        assert_eq!(
            delivery.event.message().expect("message").prompt_text(),
            "deploy the site"
        );
    }

    #[test]
    fn socket_mode_acks_envelopes_and_delivers_each_event_once() {
        let mut session = SlackSocketSession::new(context());
        assert_eq!(
            session
                .handle_frame(r#"{"type":"hello","num_connections":1}"#)
                .expect("hello"),
            SocketOutcome::Hello
        );

        let envelope = json!({
            "envelope_id": "env-1",
            "type": "events_api",
            "accepts_response_payload": false,
            "payload": mention_payload(),
        })
        .to_string();
        let outcome = session.handle_frame(&envelope).expect("frame");
        let SocketOutcome::Envelope { ack, delivery } = outcome else {
            panic!("expected an envelope");
        };
        assert_eq!(ack, r#"{"envelope_id":"env-1"}"#);
        let delivery = delivery.expect("first delivery");
        assert_eq!(delivery.transport, SlackTransport::SocketMode);
        assert_eq!(delivery.delivery_id, "Ev1");
        assert_eq!(
            delivery.event.message().expect("message").prompt_text(),
            "deploy the site"
        );

        // A redelivery after a reconnect is acked again but handed on once.
        let redelivered = json!({
            "envelope_id": "env-2",
            "type": "events_api",
            "payload": mention_payload(),
        })
        .to_string();
        let outcome = session.handle_frame(&redelivered).expect("frame");
        assert_eq!(
            outcome,
            SocketOutcome::Envelope {
                ack: r#"{"envelope_id":"env-2"}"#.to_string(),
                delivery: None
            }
        );

        assert_eq!(
            session
                .handle_frame(r#"{"type":"disconnect","reason":"refresh_requested"}"#)
                .expect("disconnect"),
            SocketOutcome::Disconnect {
                reason: "refresh_requested".into()
            }
        );
        assert_eq!(
            session
                .handle_frame(r#"{"type":"something_new"}"#)
                .expect("ignored"),
            SocketOutcome::Ignored
        );
        assert!(matches!(
            session.handle_frame(r#"{"envelope_id":"x","type":"events_api"}"#),
            Err(SlackError::Invalid(_))
        ));
        assert!(matches!(
            session.handle_frame("not json"),
            Err(SlackError::Invalid(_))
        ));
    }

    #[test]
    fn socket_mode_acknowledges_payloads_it_does_not_match_on() {
        let mut session = SlackSocketSession::new(context());
        let outcome = session
            .handle_frame(r#"{"envelope_id":"env-9","type":"slash_commands","payload":{}}"#)
            .expect("frame");
        assert_eq!(
            outcome,
            SocketOutcome::Acknowledged {
                ack: r#"{"envelope_id":"env-9"}"#.to_string()
            }
        );
    }

    #[test]
    fn socket_dedup_window_is_bounded() {
        let mut session = SlackSocketSession::new(context());
        for index in 0..=SOCKET_DEDUP_WINDOW {
            assert!(session.remember(&format!("Ev{index}")));
        }
        assert_eq!(session.order.len(), SOCKET_DEDUP_WINDOW);
        // The oldest id was evicted, so it is deliverable again.
        assert!(session.remember("Ev0"));
        assert!(!session.remember("Ev1000"));
    }

    #[test]
    fn both_transports_produce_the_same_delivery_id() {
        let context = context();
        let payload = mention_payload();
        let SlackRequest::EventCallback {
            event_id,
            team_id,
            event,
        } = parse_payload(&payload).expect("parse")
        else {
            panic!("expected an event callback");
        };
        let webhook = build_delivery(
            &context,
            SlackTransport::Webhook,
            &delivery_key(&event_id, &event),
            &team_id,
            &event,
        )
        .expect("webhook delivery");
        let mut session = SlackSocketSession::new(context);
        let frame =
            json!({"envelope_id":"env-1","type":"events_api","payload":payload}).to_string();
        let SocketOutcome::Envelope {
            delivery: Some(socket),
            ..
        } = session.handle_frame(&frame).expect("frame")
        else {
            panic!("expected a delivery");
        };
        assert_eq!(webhook.delivery_id, socket.delivery_id);
        assert_eq!(webhook.event, socket.event);
        assert_ne!(webhook.transport, socket.transport);
    }

    #[test]
    fn delivery_key_falls_back_to_the_channel_and_timestamp() {
        let event = json!({"channel": "C1", "event_ts": "1700000000.000100"});
        assert_eq!(delivery_key("", &event), "C1:1700000000.000100");
        assert_eq!(delivery_key("Ev1", &event), "Ev1");
    }

    #[test]
    fn replies_target_the_channel_and_thread_of_the_event() {
        let context = context();
        let threaded = json!({
            "type": "app_mention", "user": "U2", "text": "<@U0BOT> status",
            "channel": "C1", "ts": "1700000002.000100",
            "thread_ts": "1700000001.000100"
        });
        let event = normalize_event(&context, "T1", &threaded).expect("normalize");
        let message = event.message().expect("message");
        let reply = SlackReply::to_event(message, "done").expect("reply");
        assert_eq!(reply.target().channel(), "C1");
        assert_eq!(reply.target().thread_ts(), "1700000001.000100");
        assert_eq!(
            reply.post_message_body(),
            json!({
                "channel": "C1",
                "thread_ts": "1700000001.000100",
                "text": "done"
            })
        );
        let request = reply.post_message_request();
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, POST_MESSAGE_URL);
        assert_eq!(request.credential_key, "bot_token");

        // A top-level mention is answered in the thread it starts.
        let top_level = json!({
            "type": "app_mention", "user": "U2", "text": "<@U0BOT> status",
            "channel": "C2", "ts": "1700000003.000100"
        });
        let event = normalize_event(&context, "T1", &top_level).expect("normalize");
        let message = event.message().expect("message");
        let reply = SlackReply::to_event(message, "ok").expect("reply");
        assert_eq!(reply.target().channel(), "C2");
        assert_eq!(reply.target().thread_ts(), "1700000003.000100");
    }

    #[test]
    fn reply_text_is_validated() {
        let context = context();
        let event = normalize_event(
            &context,
            "T1",
            &json!({
                "type": "app_mention", "user": "U2", "text": "<@U0BOT> hi",
                "channel": "C1", "ts": "1.9"
            }),
        )
        .expect("normalize");
        let message = event.message().expect("message");
        assert!(SlackReply::to_event(message, "   ").is_err());
        assert!(SlackReply::to_event(message, "x".repeat(MAX_REPLY_CHARS)).is_ok());
        assert!(SlackReply::to_event(message, "x".repeat(MAX_REPLY_CHARS + 1)).is_err());
    }

    #[test]
    fn context_from_connection_reads_resources_and_hides_the_secret() {
        let connection = EffectiveConnection {
            public_id: "conn-7".into(),
            provider: ProviderKind::Slack,
            slug: "acme".into(),
            status: ConnectionStatus::Active,
            credential_ref: "SLACK_ACME_CREDS".into(),
            resources: vec![
                ConnectionResourceView {
                    resource_kind: "workspace".into(),
                    external_id: "T1".into(),
                    display_name: None,
                },
                ConnectionResourceView {
                    resource_kind: "channel".into(),
                    external_id: "C1".into(),
                    display_name: None,
                },
            ],
        };
        let material = CredentialMaterial {
            values: [
                ("bot_token".to_string(), "xoxb-test".to_string()),
                ("signing_secret".to_string(), SECRET.to_string()),
                ("app_token".to_string(), "xapp-test".to_string()),
                ("bot_user_id".to_string(), BOT.to_string()),
            ]
            .into_iter()
            .collect(),
        };
        let context = SlackIngestContext::from_connection(&connection, &material).expect("context");
        assert_eq!(context.team_ids, vec!["t1".to_string()]);
        assert_eq!(context.channel_ids, vec!["c1".to_string()]);
        assert!(context.socket_mode_available);
        assert_eq!(context.bot_user_id.as_deref(), Some(BOT));
        let debug = format!("{context:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains(SECRET));

        // Without a signing secret neither transport can verify anything.
        let material = CredentialMaterial {
            values: [("bot_token".to_string(), "xoxb-test".to_string())]
                .into_iter()
                .collect(),
        };
        assert!(matches!(
            SlackIngestContext::from_connection(&connection, &material),
            Err(SlackError::MissingCredential("signing_secret"))
        ));

        // A disabled or non-Slack connection is not found.
        let mut disabled = connection.clone();
        disabled.status = ConnectionStatus::Disabled;
        let full = CredentialMaterial {
            values: [("signing_secret".to_string(), SECRET.to_string())]
                .into_iter()
                .collect(),
        };
        assert!(matches!(
            SlackIngestContext::from_connection(&disabled, &full),
            Err(SlackError::NotFound)
        ));
    }

    #[test]
    fn socket_mode_requires_an_app_token_to_be_available() {
        let context = SlackIngestContext::new("conn-1", SECRET).expect("context");
        assert!(!context.socket_mode_available);
        assert!(context.clone().with_socket_mode(true).socket_mode_available);
        assert!(SlackIngestContext::new("conn-1", "").is_err());
    }

    #[tokio::test]
    async fn socket_session_pump_acks_and_forwards_deliveries() {
        let mut session = SlackSocketSession::new(context());
        let (frame_tx, mut frame_rx) = mpsc::channel::<String>(8);
        let (ack_tx, mut ack_rx) = mpsc::channel::<String>(8);
        let (delivery_tx, mut delivery_rx) = mpsc::channel::<SlackDelivery>(8);

        frame_tx
            .send(r#"{"type":"hello"}"#.to_string())
            .await
            .expect("send hello");
        frame_tx
            .send(
                json!({"envelope_id":"env-1","type":"events_api","payload":mention_payload()})
                    .to_string(),
            )
            .await
            .expect("send envelope");
        frame_tx
            .send(r#"{"type":"disconnect","reason":"warning"}"#.to_string())
            .await
            .expect("send disconnect");

        let stop = run_socket_session(&mut session, &mut frame_rx, &ack_tx, &delivery_tx).await;
        assert_eq!(
            stop,
            SocketStop::Disconnected {
                reason: "warning".into()
            }
        );
        assert_eq!(
            ack_rx.recv().await.as_deref(),
            Some(r#"{"envelope_id":"env-1"}"#)
        );
        let delivery = delivery_rx.recv().await.expect("delivery");
        assert!(delivery.accepted);
        assert_eq!(delivery.event.kind(), "slack.mention");
    }
}
