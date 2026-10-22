//! Discord connection over the bot gateway (task 1.3.4).
//!
//! Discord has no inbound webhook for bot events: the pool dials the gateway
//! WebSocket with the connection's `bot_token` and receives dispatch frames on
//! it. [`DiscordGatewaySession`] owns that protocol and
//! [`run_gateway_session`] pumps it over a frame channel, so the session is
//! driven identically by a live socket and by a scripted test feed.
//!
//! # The gateway handshake, and what a reconnect costs
//!
//! ```text
//! server  {"op":10,"d":{"heartbeat_interval":41250}}   hello
//! client  {"op":2, "d":{...}}                          identify   (first connect)
//! server  {"op":0, "t":"READY","s":1,"d":{...}}        session_id, bot user
//! server  {"op":0, "t":"MESSAGE_CREATE","s":2,"d":{}}  dispatch
//! server  {"op":7}                                     reconnect, please
//! client  {"op":6, "d":{"session_id":...,"seq":2}}      resume     (re-connect)
//! server  {"op":0, "t":"MESSAGE_CREATE","s":2,"d":{}}  replayed!
//! server  {"op":0, "t":"RESUMED","s":3,"d":{}}         replay finished
//! ```
//!
//! A resumed stream replays everything after the sequence the pool
//! acknowledged, and Discord may replay an event the pool already handled. The
//! session therefore remembers the last [`RESUME_DEDUP_WINDOW`] delivery ids —
//! the message snowflake, which is unique across the whole platform — and a
//! replayed event is recognised and handed on only once. Because the dedup
//! window lives on the session rather than on the socket, it survives the
//! reconnect that caused the replay.
//!
//! # Mentions never reach a prompt
//!
//! [`DiscordMessageEvent::content`] is already stripped of the bot mention;
//! `raw_content` keeps what Discord sent for the record. Prompt construction
//! reads [`DiscordMessageEvent::prompt_text`], so there is no path by which
//! `<@123>` reaches an agent prompt.
//!
//! # Replies
//!
//! A [`ReplyTarget`] can only be obtained from an event
//! ([`DiscordMessageEvent::reply_target`]), so a reply is addressed to the
//! channel the event came from. In Discord a thread *is* a channel, so a
//! mention inside a thread carries the thread's id as its `channel_id` and the
//! reply lands in the thread by construction. The `discord.reply` output
//! capability — the same maximum and required semantics as `slack.reply` —
//! lives in [`crate::services::outputs`].
//!
//! # No secrets here
//!
//! The session never holds the bot token. Frames that need it
//! ([`GatewayCommand::credential_key`]) are built with the field *name*, and
//! the value is injected when the frame is sent
//! ([`GatewayCommand::frame_with_token`]), which is dispatch's job
//! (task 1.4.6), as is dialling the socket.

use super::{
    ConnectionError, ConnectionStatus, CredentialMaterial, EffectiveConnection, ProviderKind,
    credential_material, find_effective_by_slug, list_connections,
};
use crate::entities::connection::ConnectionView;
use serde_json::{Value, json};
use sqlx::MySqlPool;
use std::collections::{BTreeSet, VecDeque};
use thiserror::Error;
use tokio::sync::mpsc;

/// Largest content a single reply may carry (Discord's own message limit).
pub const MAX_REPLY_CHARS: usize = 2_000;

/// How many delivery ids one gateway session remembers across resumes.
pub const RESUME_DEDUP_WINDOW: usize = 1024;

/// Discord REST API base the reply call is built against.
pub const API_BASE: &str = "https://discord.com/api/v10";

/// Gateway opcode: a dispatched event.
pub const OP_DISPATCH: u8 = 0;
/// Gateway opcode: heartbeat (both directions).
pub const OP_HEARTBEAT: u8 = 1;
/// Gateway opcode: identify a new session.
pub const OP_IDENTIFY: u8 = 2;
/// Gateway opcode: resume an existing session.
pub const OP_RESUME: u8 = 6;
/// Gateway opcode: the server asks the client to reconnect.
pub const OP_RECONNECT: u8 = 7;
/// Gateway opcode: the session is invalid; `d` says whether it may be resumed.
pub const OP_INVALID_SESSION: u8 = 9;
/// Gateway opcode: the server's greeting, carrying the heartbeat interval.
pub const OP_HELLO: u8 = 10;
/// Gateway opcode: the server acknowledged a heartbeat.
pub const OP_HEARTBEAT_ACK: u8 = 11;

/// Intents the pool identifies with: guilds, guild and direct messages, and
/// message content (without which `content` arrives empty).
pub const GATEWAY_INTENTS: u64 = (1 << 0) | (1 << 9) | (1 << 12) | (1 << 15);

/// Domain errors for the Discord gateway transport.
#[derive(Debug, Error)]
pub enum DiscordError {
    /// No active Discord connection with that slug.
    #[error("connection not found")]
    NotFound,
    /// A frame or event payload is malformed.
    #[error("invalid discord frame: {0}")]
    Invalid(String),
    /// Credentials are missing a field the transport needs.
    #[error("discord credential '{0}' is not configured")]
    MissingCredential(&'static str),
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// Why a delivery was recorded but not matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    /// The event's guild is not one of the connection's `guild` resources.
    GuildNotOwned,
    /// The connection has channel filters and this channel is not in them.
    ChannelFiltered,
    /// The message was written by this bot (or another bot).
    SelfAuthored,
    /// The message was an edit of an earlier one.
    EditedMessage,
    /// The dispatch kind is not one the pool matches on.
    UnsupportedEvent,
}

impl RejectReason {
    /// Stable reason name for logs and the delivery record.
    pub fn as_str(self) -> &'static str {
        match self {
            RejectReason::GuildNotOwned => "guild_not_owned",
            RejectReason::ChannelFiltered => "channel_filtered",
            RejectReason::SelfAuthored => "self_authored",
            RejectReason::EditedMessage => "edited_message",
            RejectReason::UnsupportedEvent => "unsupported_event",
        }
    }
}

/// Whether an event arrived on a fresh stream or on a resumed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliverySource {
    /// The stream had identified; this is the event's first appearance.
    Live,
    /// The stream had resumed, so Discord may be replaying.
    Resumed,
}

impl DeliverySource {
    /// Wire/database spelling, recorded in the delivery's `action` column.
    pub fn as_str(self) -> &'static str {
        match self {
            DeliverySource::Live => "gateway",
            DeliverySource::Resumed => "gateway_resume",
        }
    }
}

/// A Discord message-shaped event, normalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordMessageEvent {
    /// Guild the event came from; absent for a direct message.
    pub guild_id: Option<String>,
    /// Channel the event came from. For a message in a thread this *is* the
    /// thread's id, which is what makes a reply land in the thread.
    pub channel_id: String,
    /// Message snowflake, unique across Discord.
    pub message_id: String,
    /// Author's Discord user id.
    pub author_id: String,
    /// Author's username, when Discord sent one.
    pub author_username: Option<String>,
    /// Content with the bot mention removed; this is what a prompt sees.
    pub content: String,
    /// Content exactly as Discord delivered it, mention included.
    pub raw_content: String,
    /// True when the message mentioned the bot.
    pub bot_mentioned: bool,
    /// True when the message is inside a thread.
    pub in_thread: bool,
    /// Message this one replies to, when it is an inline reply.
    pub referenced_message_id: Option<String>,
}

impl DiscordMessageEvent {
    /// Prompt-safe request text: the mention is already removed.
    pub fn prompt_text(&self) -> &str {
        &self.content
    }

    /// True when the message came from a direct message channel.
    pub fn is_direct_message(&self) -> bool {
        self.guild_id.is_none()
    }

    /// Continuation key: the channel or thread the event belongs to.
    ///
    /// A thread is a channel of its own in Discord, so every message in one
    /// thread shares this key and a top-level channel keys on itself.
    pub fn thread_key(&self) -> &str {
        &self.channel_id
    }

    /// Where a reply to this event must go.
    pub fn reply_target(&self) -> ReplyTarget {
        ReplyTarget {
            channel_id: self.channel_id.clone(),
            message_id: self.message_id.clone(),
        }
    }
}

/// A normalized Discord event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscordEvent {
    /// The bot was mentioned in a guild channel or thread.
    Mention(DiscordMessageEvent),
    /// A direct message to the bot (a message with no guild).
    DirectMessage(DiscordMessageEvent),
    /// A message in a channel the connection delivers for.
    Message(DiscordMessageEvent),
    /// Deliberately ignored: the bot's own message, another bot's, an edit.
    Ignored {
        /// Discord dispatch name.
        event: String,
        /// Why it was ignored.
        reason: RejectReason,
    },
    /// A dispatch kind the pool does not match on yet.
    Unsupported {
        /// Discord dispatch name.
        event: String,
    },
}

impl DiscordEvent {
    /// Stable internal event name.
    pub fn kind(&self) -> &'static str {
        match self {
            DiscordEvent::Mention(_) => "discord.mention",
            DiscordEvent::DirectMessage(_) => "discord.direct_message",
            DiscordEvent::Message(_) => "discord.message",
            DiscordEvent::Ignored { .. } => "discord.ignored",
            DiscordEvent::Unsupported { .. } => "discord.unsupported",
        }
    }

    /// The message body, for the event kinds that carry one.
    pub fn message(&self) -> Option<&DiscordMessageEvent> {
        match self {
            DiscordEvent::Mention(message)
            | DiscordEvent::DirectMessage(message)
            | DiscordEvent::Message(message) => Some(message),
            _ => None,
        }
    }

    /// True for the events that may start or continue an agent.
    pub fn is_actionable(&self) -> bool {
        matches!(
            self,
            DiscordEvent::Mention(_) | DiscordEvent::DirectMessage(_) | DiscordEvent::Message(_)
        )
    }
}

/// One accepted or rejected event from the gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordDelivery {
    /// Connection that received it.
    pub connection_public_id: String,
    /// Whether the stream was live or resuming when it arrived.
    pub source: DeliverySource,
    /// Dedup key: the message snowflake, else `channel_id:sequence`.
    pub delivery_id: String,
    /// Gateway sequence number the event carried.
    pub sequence: Option<u64>,
    /// Normalized event.
    pub event: DiscordEvent,
    /// False when the delivery was recorded but not matched.
    pub accepted: bool,
    /// Why it was not matched.
    pub reason: Option<RejectReason>,
}

/// Where a reply goes. Obtainable only from an event, so a reply cannot be
/// addressed to a channel the event did not come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplyTarget {
    channel_id: String,
    message_id: String,
}

impl ReplyTarget {
    /// Channel — or thread — the originating event came from.
    pub fn channel_id(&self) -> &str {
        &self.channel_id
    }

    /// Message the reply is attached to.
    pub fn message_id(&self) -> &str {
        &self.message_id
    }

    /// `POST /channels/{id}/messages` endpoint for this target.
    pub fn create_message_url(&self) -> String {
        format!("{API_BASE}/channels/{}/messages", self.channel_id)
    }
}

/// A reply emitted by a step, addressed by construction to its event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordReply {
    target: ReplyTarget,
    content: String,
}

impl DiscordReply {
    /// Build a reply to `event`.
    ///
    /// Fails when the content is empty or longer than [`MAX_REPLY_CHARS`].
    pub fn to_event(
        event: &DiscordMessageEvent,
        content: impl Into<String>,
    ) -> Result<Self, DiscordError> {
        let content = content.into();
        if content.trim().is_empty() {
            return Err(DiscordError::Invalid("reply content is required".into()));
        }
        if content.chars().count() > MAX_REPLY_CHARS {
            return Err(DiscordError::Invalid(format!(
                "reply content must be at most {MAX_REPLY_CHARS} characters"
            )));
        }
        Ok(Self {
            target: event.reply_target(),
            content,
        })
    }

    /// Where this reply goes.
    pub fn target(&self) -> &ReplyTarget {
        &self.target
    }

    /// Reply content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// `POST /channels/{id}/messages` request body for this reply.
    ///
    /// `message_reference` attaches the reply to the message it answers and
    /// `fail_if_not_exists: false` keeps a deleted original from failing the
    /// send. `allowed_mentions: {"parse": []}` means no agent output can ping a
    /// user, a role or `@everyone`, whatever text it produced.
    pub fn create_message_body(&self) -> Value {
        json!({
            "content": self.content,
            "message_reference": {
                "message_id": self.target.message_id,
                "fail_if_not_exists": false,
            },
            "allowed_mentions": { "parse": [] },
        })
    }

    /// The Discord API call that sends this reply.
    ///
    /// The pool builds the request here and the dispatcher performs it with the
    /// connection's `bot_token` (task 1.4.6 owns the outbound HTTP client), so
    /// the addressing is decided and testable in one place.
    pub fn create_message_request(&self) -> DiscordApiRequest {
        DiscordApiRequest {
            method: "POST",
            url: self.target.create_message_url(),
            credential_key: "bot_token",
            body: self.create_message_body(),
        }
    }
}

/// A Discord REST call the pool wants performed.
#[derive(Debug, Clone, PartialEq)]
pub struct DiscordApiRequest {
    /// HTTP method.
    pub method: &'static str,
    /// Absolute endpoint URL.
    pub url: String,
    /// Credential field whose value authenticates the call (never the value).
    pub credential_key: &'static str,
    /// JSON request body.
    pub body: Value,
}

/// Everything the gateway transport needs to know about one Discord connection.
///
/// The bot token is deliberately absent: the session only ever names it (see
/// [`GatewayCommand::credential_key`]).
#[derive(Debug, Clone)]
pub struct DiscordIngestContext {
    /// Public id of the connection (`env:<slug>` for environment entries).
    pub connection_public_id: String,
    /// Bot user id, when configured or learned from `READY`; used to spot
    /// mentions and to ignore the bot's own messages.
    pub bot_user_id: Option<String>,
    /// Guild ids the connection delivers for. Empty allows any.
    pub guild_ids: Vec<String>,
    /// Channel filters. Empty allows any channel in an owned guild.
    pub channel_ids: Vec<String>,
}

impl DiscordIngestContext {
    /// Build a context from an effective connection and its resolved
    /// credentials.
    ///
    /// A `bot_token` must be configured — without it the gateway cannot be
    /// dialled at all — but its value is not retained here.
    pub fn from_connection(
        connection: &EffectiveConnection,
        material: &CredentialMaterial,
    ) -> Result<Self, DiscordError> {
        if connection.provider != ProviderKind::Discord
            || connection.status != ConnectionStatus::Active
        {
            return Err(DiscordError::NotFound);
        }
        if !material
            .values
            .get("bot_token")
            .is_some_and(|value| !value.is_empty())
        {
            return Err(DiscordError::MissingCredential("bot_token"));
        }
        let bot_user_id = material
            .values
            .get("bot_user_id")
            .filter(|value| !value.is_empty())
            .cloned();

        let mut guild_ids = Vec::new();
        let mut channel_ids = Vec::new();
        for resource in &connection.resources {
            match resource.resource_kind.as_str() {
                "guild" => guild_ids.push(normalize_identity(&resource.external_id)),
                "channel" => channel_ids.push(normalize_identity(&resource.external_id)),
                _ => {}
            }
        }

        Ok(Self {
            connection_public_id: connection.public_id.clone(),
            bot_user_id,
            guild_ids,
            channel_ids,
        })
    }

    /// Build a context directly, for gateway wiring and tests.
    pub fn new(connection_public_id: impl Into<String>) -> Self {
        Self {
            connection_public_id: connection_public_id.into(),
            bot_user_id: None,
            guild_ids: Vec::new(),
            channel_ids: Vec::new(),
        }
    }

    /// Set the bot user id whose mentions are stripped.
    pub fn with_bot_user_id(mut self, bot_user_id: impl Into<String>) -> Self {
        self.bot_user_id = Some(bot_user_id.into());
        self
    }

    /// Set the guilds the connection delivers for.
    pub fn with_guilds<I, S>(mut self, guild_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.guild_ids = guild_ids
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

    /// True when the guild is one the connection delivers for.
    ///
    /// A direct message has no guild, so guild resolution does not apply to it;
    /// channel filters still do.
    pub fn guild_allowed(&self, guild_id: Option<&str>) -> bool {
        match guild_id {
            None => true,
            Some(guild_id) => {
                self.guild_ids.is_empty() || self.guild_ids.contains(&normalize_identity(guild_id))
            }
        }
    }

    /// True when the channel passes the connection's filters.
    pub fn channel_allowed(&self, channel_id: &str) -> bool {
        self.channel_ids.is_empty() || self.channel_ids.contains(&normalize_identity(channel_id))
    }
}

// ── normalization ─────────────────────────────────────────────────────────

/// Remove a bot mention from message content.
///
/// Discord writes user mentions as `<@123>` and, historically, `<@!123>` for a
/// member with a nickname. With `bot_user_id` known every mention of that user
/// is removed; without it only a leading user mention is removed, which is the
/// one that addressed the bot. Role mentions (`<@&456>`) and channel mentions
/// (`<#789>`) are never touched. Runs of spaces and tabs left behind are
/// collapsed; newlines are preserved so a multi-line request reaches the prompt
/// intact.
pub fn strip_mention(content: &str, bot_user_id: Option<&str>) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    let mut removed = false;

    while let Some(start) = rest.find("<@") {
        let Some(end_offset) = rest[start..].find('>') else {
            break;
        };
        let end = start + end_offset;
        let inner = &rest[start + 2..end];
        out.push_str(&rest[..start]);
        // A role mention is `<@&id>`; only user mentions address the bot.
        let is_bot = match inner.strip_prefix('&') {
            Some(_) => false,
            None => {
                let id = inner.trim_start_matches('!');
                match bot_user_id {
                    Some(bot) => id == bot,
                    // Unknown bot id: the mention that addressed the bot is the
                    // leading one, so only strip a mention nothing precedes.
                    None => !removed && out.trim().is_empty(),
                }
            }
        };
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

/// True when `content` mentions `user_id` as a user (not as a role).
pub fn mentions_user(content: &str, user_id: &str) -> bool {
    let mut rest = content;
    while let Some(start) = rest.find("<@") {
        let Some(end_offset) = rest[start..].find('>') else {
            return false;
        };
        let end = start + end_offset;
        let inner = &rest[start + 2..end];
        if !inner.starts_with('&') && inner.trim_start_matches('!') == user_id {
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

/// True when the `mentions` array of a message payload lists the bot.
fn mentions_array_contains(data: &Value, bot_user_id: &str) -> bool {
    data.get("mentions")
        .and_then(Value::as_array)
        .is_some_and(|mentions| {
            mentions.iter().any(|mention| {
                mention.get("id").and_then(Value::as_str) == Some(bot_user_id)
                    || mention.as_str() == Some(bot_user_id)
            })
        })
}

/// Normalize one gateway dispatch payload.
///
/// `MESSAGE_CREATE` with the bot mentioned becomes [`DiscordEvent::Mention`], a
/// message with no guild a [`DiscordEvent::DirectMessage`], any other message a
/// [`DiscordEvent::Message`]. `MESSAGE_UPDATE`, the bot's own messages and
/// other bots' messages are [`DiscordEvent::Ignored`]; every other dispatch is
/// [`DiscordEvent::Unsupported`].
pub fn normalize_event(
    context: &DiscordIngestContext,
    event_name: &str,
    data: &Value,
) -> Result<DiscordEvent, DiscordError> {
    match event_name {
        "MESSAGE_UPDATE" => Ok(DiscordEvent::Ignored {
            event: event_name.to_string(),
            reason: RejectReason::EditedMessage,
        }),
        "MESSAGE_CREATE" => {
            let author = data.get("author");
            let author_id = author
                .and_then(|author| author.get("id"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let authored_by_bot = author
                .and_then(|author| author.get("bot"))
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || data.get("webhook_id").is_some()
                || context
                    .bot_user_id
                    .as_deref()
                    .is_some_and(|bot| bot == author_id);
            if authored_by_bot {
                return Ok(DiscordEvent::Ignored {
                    event: event_name.to_string(),
                    reason: RejectReason::SelfAuthored,
                });
            }

            let raw_content = data
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let content = strip_mention(&raw_content, context.bot_user_id.as_deref());
            let bot_mentioned = match context.bot_user_id.as_deref() {
                Some(bot) => mentions_array_contains(data, bot) || mentions_user(&raw_content, bot),
                // Without a known bot id a leading user mention is taken to
                // address the bot; `READY` fills the id in on the first connect.
                None => {
                    let trimmed = raw_content.trim_start();
                    trimmed.starts_with("<@") && !trimmed.starts_with("<@&")
                }
            };

            let message = DiscordMessageEvent {
                guild_id: data
                    .get("guild_id")
                    .and_then(Value::as_str)
                    .filter(|guild| !guild.is_empty())
                    .map(str::to_string),
                channel_id: data
                    .get("channel_id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                message_id: data
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                author_id,
                author_username: author
                    .and_then(|author| author.get("username"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                content,
                raw_content,
                bot_mentioned,
                // Discord sends `position` only for messages in a thread.
                in_thread: data.get("position").is_some(),
                referenced_message_id: data
                    .get("message_reference")
                    .and_then(|reference| reference.get("message_id"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            };
            if message.channel_id.is_empty() || message.message_id.is_empty() {
                return Err(DiscordError::Invalid(
                    "message events need a channel_id and an id".into(),
                ));
            }
            Ok(match (message.guild_id.is_some(), message.bot_mentioned) {
                (true, true) => DiscordEvent::Mention(message),
                (false, _) => DiscordEvent::DirectMessage(message),
                (true, false) => DiscordEvent::Message(message),
            })
        }
        other => Ok(DiscordEvent::Unsupported {
            event: other.to_string(),
        }),
    }
}

/// Apply guild resolution and channel filters to a normalized event.
pub fn decide(
    context: &DiscordIngestContext,
    event: &DiscordEvent,
) -> (bool, Option<RejectReason>) {
    match event {
        DiscordEvent::Ignored { reason, .. } => (false, Some(*reason)),
        DiscordEvent::Unsupported { .. } => (false, Some(RejectReason::UnsupportedEvent)),
        DiscordEvent::Mention(message)
        | DiscordEvent::DirectMessage(message)
        | DiscordEvent::Message(message) => {
            if !context.guild_allowed(message.guild_id.as_deref()) {
                return (false, Some(RejectReason::GuildNotOwned));
            }
            if !context.channel_allowed(&message.channel_id) {
                return (false, Some(RejectReason::ChannelFiltered));
            }
            (true, None)
        }
    }
}

/// Dedup key for a dispatch: the message snowflake, else `channel:sequence`.
fn delivery_key(data: &Value, sequence: Option<u64>) -> String {
    if let Some(id) = data
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        return id.to_string();
    }
    let channel = data
        .get("channel_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match sequence {
        Some(sequence) => format!("{channel}:{sequence}"),
        None => String::new(),
    }
}

/// Normalize and decide one dispatch, producing a delivery.
pub fn build_delivery(
    context: &DiscordIngestContext,
    source: DeliverySource,
    delivery_id: &str,
    sequence: Option<u64>,
    event_name: &str,
    data: &Value,
) -> Result<DiscordDelivery, DiscordError> {
    let event = normalize_event(context, event_name, data)?;
    let (accepted, reason) = decide(context, &event);
    Ok(DiscordDelivery {
        connection_public_id: context.connection_public_id.clone(),
        source,
        delivery_id: delivery_id.to_string(),
        sequence,
        event,
        accepted,
        reason,
    })
}

// ── gateway protocol ──────────────────────────────────────────────────────

/// A frame the client must send, with the token named rather than embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayCommand {
    /// Gateway opcode.
    pub op: u8,
    /// Credential field whose value authenticates the frame, when it needs one.
    pub credential_key: Option<&'static str>,
    /// The frame's `d` payload, without any credential value.
    pub payload: Value,
}

impl GatewayCommand {
    /// The frame as JSON, with no token in it.
    pub fn frame(&self) -> Value {
        json!({ "op": self.op, "d": self.payload })
    }

    /// Render the frame for sending, injecting the bot token when the opcode
    /// needs one. This is the only place a token meets a gateway frame.
    pub fn frame_with_token(&self, bot_token: &str) -> String {
        let mut frame = self.frame();
        if self.credential_key.is_some()
            && let Some(payload) = frame.get_mut("d").and_then(Value::as_object_mut)
        {
            payload.insert("token".into(), Value::String(bot_token.to_string()));
        }
        frame.to_string()
    }

    /// True when this command resumes a session rather than starting one.
    pub fn is_resume(&self) -> bool {
        self.op == OP_RESUME
    }
}

/// A parsed gateway frame.
#[derive(Debug, Clone, PartialEq)]
pub struct GatewayFrame {
    /// Opcode.
    pub op: u8,
    /// Sequence number, present on dispatches.
    pub sequence: Option<u64>,
    /// Dispatch name, present on dispatches.
    pub event_name: Option<String>,
    /// The frame's `d` payload.
    pub data: Value,
}

/// Parse one inbound gateway frame.
pub fn parse_frame(frame: &str) -> Result<GatewayFrame, DiscordError> {
    let value: Value = serde_json::from_str(frame)
        .map_err(|e| DiscordError::Invalid(format!("frame is not JSON: {e}")))?;
    let op = value
        .get("op")
        .and_then(Value::as_u64)
        .ok_or_else(|| DiscordError::Invalid("frame op is required".into()))?;
    let op = u8::try_from(op).map_err(|_| DiscordError::Invalid(format!("unknown op {op}")))?;
    Ok(GatewayFrame {
        op,
        sequence: value.get("s").and_then(Value::as_u64),
        event_name: value.get("t").and_then(Value::as_str).map(str::to_string),
        data: value.get("d").cloned().unwrap_or(Value::Null),
    })
}

/// What the gateway client must do with one inbound frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayOutcome {
    /// The server's `hello`: send `command`, then heartbeat on the interval.
    Handshake {
        /// `identify` on a first connect, `resume` after a reconnect.
        command: GatewayCommand,
        /// Milliseconds between heartbeats.
        heartbeat_interval_ms: u64,
    },
    /// The session is established and the bot user is known.
    Ready {
        /// Session id used by a later resume.
        session_id: String,
    },
    /// A resumed session finished replaying.
    Resumed,
    /// The server asked for a heartbeat; send `command` now.
    Heartbeat {
        /// The heartbeat frame, carrying the last sequence seen.
        command: GatewayCommand,
    },
    /// A dispatched event. `delivery` is `None` when this event was already
    /// received, which is what a resume replay produces. Boxed: a delivery is
    /// much larger than the other outcomes.
    Dispatch {
        /// The delivery, or `None` for an event already handled.
        delivery: Option<Box<DiscordDelivery>>,
    },
    /// The client must redial.
    Reconnect {
        /// True when the existing session may be resumed.
        resumable: bool,
    },
    /// A frame the client does nothing with (a heartbeat ack, say).
    Ignored,
}

/// Why a gateway session stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayStop {
    /// The server asked the client to redial.
    Reconnect {
        /// True when the existing session may be resumed.
        resumable: bool,
    },
    /// The frame stream ended.
    StreamClosed,
}

/// One gateway connection's protocol state, kept across reconnects.
///
/// The session is transport agnostic on purpose: it consumes frames as text and
/// returns the commands to send, which lets a live socket and a scripted feed
/// drive identical logic. Keeping one session across a redial is what makes a
/// resume safe — the dedup window and the `(session_id, seq)` pair both live
/// here, so a replayed event is recognised rather than handled twice.
#[derive(Debug)]
pub struct DiscordGatewaySession {
    context: DiscordIngestContext,
    session_id: Option<String>,
    resume_gateway_url: Option<String>,
    last_sequence: Option<u64>,
    heartbeat_interval_ms: Option<u64>,
    resuming: bool,
    seen: BTreeSet<String>,
    order: VecDeque<String>,
}

impl DiscordGatewaySession {
    /// Start a session for a connection.
    pub fn new(context: DiscordIngestContext) -> Self {
        Self {
            context,
            session_id: None,
            resume_gateway_url: None,
            last_sequence: None,
            heartbeat_interval_ms: None,
            resuming: false,
            seen: BTreeSet::new(),
            order: VecDeque::new(),
        }
    }

    /// The connection this session delivers for.
    pub fn context(&self) -> &DiscordIngestContext {
        &self.context
    }

    /// Session id from `READY`, once there is one.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// URL a resume must be dialled against, as `READY` gave it.
    pub fn resume_gateway_url(&self) -> Option<&str> {
        self.resume_gateway_url.as_deref()
    }

    /// Last dispatch sequence seen; what a resume asks to continue from.
    pub fn last_sequence(&self) -> Option<u64> {
        self.last_sequence
    }

    /// Heartbeat interval the server asked for, once `hello` has arrived.
    pub fn heartbeat_interval_ms(&self) -> Option<u64> {
        self.heartbeat_interval_ms
    }

    /// True when the next handshake will resume instead of identify.
    pub fn can_resume(&self) -> bool {
        self.session_id.is_some() && self.last_sequence.is_some()
    }

    /// The `identify` frame for a fresh session.
    pub fn identify_command(&self) -> GatewayCommand {
        GatewayCommand {
            op: OP_IDENTIFY,
            credential_key: Some("bot_token"),
            payload: json!({
                "intents": GATEWAY_INTENTS,
                "properties": {
                    "os": std::env::consts::OS,
                    "browser": "tidepool",
                    "device": "tidepool",
                },
            }),
        }
    }

    /// The `resume` frame for the session held, when one can be resumed.
    pub fn resume_command(&self) -> Option<GatewayCommand> {
        let session_id = self.session_id.as_deref()?;
        Some(GatewayCommand {
            op: OP_RESUME,
            credential_key: Some("bot_token"),
            payload: json!({
                "session_id": session_id,
                "seq": self.last_sequence,
            }),
        })
    }

    /// The `heartbeat` frame, carrying the last sequence seen.
    pub fn heartbeat_command(&self) -> GatewayCommand {
        GatewayCommand {
            op: OP_HEARTBEAT,
            credential_key: None,
            payload: match self.last_sequence {
                Some(sequence) => Value::from(sequence),
                None => Value::Null,
            },
        }
    }

    /// Handle one inbound text frame.
    pub fn handle_frame(&mut self, frame: &str) -> Result<GatewayOutcome, DiscordError> {
        let frame = parse_frame(frame)?;
        if let Some(sequence) = frame.sequence {
            self.last_sequence = Some(sequence);
        }

        match frame.op {
            OP_HELLO => {
                let heartbeat_interval_ms = frame
                    .data
                    .get("heartbeat_interval")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| {
                        DiscordError::Invalid("hello needs a heartbeat_interval".into())
                    })?;
                self.heartbeat_interval_ms = Some(heartbeat_interval_ms);
                let command = match self.resume_command() {
                    Some(command) => {
                        self.resuming = true;
                        command
                    }
                    None => {
                        self.resuming = false;
                        self.identify_command()
                    }
                };
                Ok(GatewayOutcome::Handshake {
                    command,
                    heartbeat_interval_ms,
                })
            }
            OP_HEARTBEAT => Ok(GatewayOutcome::Heartbeat {
                command: self.heartbeat_command(),
            }),
            OP_HEARTBEAT_ACK => Ok(GatewayOutcome::Ignored),
            OP_RECONNECT => Ok(GatewayOutcome::Reconnect { resumable: true }),
            OP_INVALID_SESSION => {
                // `d` is a bare boolean: false means the session is gone and
                // the next handshake must identify afresh.
                let resumable = frame.data.as_bool().unwrap_or(false);
                if !resumable {
                    self.session_id = None;
                    self.resume_gateway_url = None;
                    self.last_sequence = None;
                    self.resuming = false;
                }
                Ok(GatewayOutcome::Reconnect { resumable })
            }
            OP_DISPATCH => {
                let event_name = frame
                    .event_name
                    .as_deref()
                    .ok_or_else(|| DiscordError::Invalid("dispatch needs an event name".into()))?;
                match event_name {
                    "READY" => {
                        let session_id = frame
                            .data
                            .get("session_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                DiscordError::Invalid("READY needs a session_id".into())
                            })?
                            .to_string();
                        self.resume_gateway_url = frame
                            .data
                            .get("resume_gateway_url")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        if self.context.bot_user_id.is_none()
                            && let Some(bot_user_id) = frame
                                .data
                                .get("user")
                                .and_then(|user| user.get("id"))
                                .and_then(Value::as_str)
                        {
                            self.context.bot_user_id = Some(bot_user_id.to_string());
                        }
                        self.session_id = Some(session_id.clone());
                        self.resuming = false;
                        Ok(GatewayOutcome::Ready { session_id })
                    }
                    "RESUMED" => {
                        self.resuming = false;
                        Ok(GatewayOutcome::Resumed)
                    }
                    _ => {
                        let delivery_id = delivery_key(&frame.data, frame.sequence);
                        if delivery_id.is_empty() {
                            return Err(DiscordError::Invalid(
                                "dispatch has no id to deduplicate on".into(),
                            ));
                        }
                        let source = if self.resuming {
                            DeliverySource::Resumed
                        } else {
                            DeliverySource::Live
                        };
                        if !self.remember(&delivery_id) {
                            // A replay after a resume: already handled once.
                            return Ok(GatewayOutcome::Dispatch { delivery: None });
                        }
                        let delivery = build_delivery(
                            &self.context,
                            source,
                            &delivery_id,
                            frame.sequence,
                            event_name,
                            &frame.data,
                        )?;
                        Ok(GatewayOutcome::Dispatch {
                            delivery: Some(Box::new(delivery)),
                        })
                    }
                }
            }
            _ => Ok(GatewayOutcome::Ignored),
        }
    }

    /// Record a delivery id, returning false when it was already seen.
    fn remember(&mut self, delivery_id: &str) -> bool {
        if !self.seen.insert(delivery_id.to_string()) {
            return false;
        }
        self.order.push_back(delivery_id.to_string());
        if self.order.len() > RESUME_DEDUP_WINDOW
            && let Some(evicted) = self.order.pop_front()
        {
            self.seen.remove(&evicted);
        }
        true
    }
}

/// Pump a gateway session over a frame channel.
///
/// `inbound` carries text frames from the WebSocket (or a test feed),
/// `outbound` takes the commands to send — the dispatcher renders them with the
/// connection's `bot_token` — and `deliveries` receives each event once.
/// Returns when the server asks for a redial or the stream ends; call it again
/// with the *same* session to resume, which is what keeps a replayed event from
/// being handled twice.
pub async fn run_gateway_session(
    session: &mut DiscordGatewaySession,
    inbound: &mut mpsc::Receiver<String>,
    outbound: &mpsc::Sender<GatewayCommand>,
    deliveries: &mpsc::Sender<DiscordDelivery>,
) -> GatewayStop {
    while let Some(frame) = inbound.recv().await {
        let outcome = match session.handle_frame(&frame) {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::warn!(error = %error, "discord gateway frame rejected");
                continue;
            }
        };
        match outcome {
            GatewayOutcome::Ready { .. } | GatewayOutcome::Resumed | GatewayOutcome::Ignored => {}
            GatewayOutcome::Handshake { command, .. } | GatewayOutcome::Heartbeat { command } => {
                if outbound.send(command).await.is_err() {
                    return GatewayStop::StreamClosed;
                }
            }
            GatewayOutcome::Dispatch { delivery } => {
                if let Some(delivery) = delivery
                    && deliveries.send(*delivery).await.is_err()
                {
                    return GatewayStop::StreamClosed;
                }
            }
            GatewayOutcome::Reconnect { resumable } => {
                return GatewayStop::Reconnect { resumable };
            }
        }
    }
    GatewayStop::StreamClosed
}

// ── connection resolution ─────────────────────────────────────────────────

/// Resolve the ingest context for a connection slug.
pub async fn resolve_context(
    pool: &MySqlPool,
    organization_id: u64,
    slug: &str,
) -> Result<DiscordIngestContext, DiscordError> {
    let connection = find_effective_by_slug(pool, organization_id, slug)
        .await
        .map_err(connection_error)?
        .ok_or(DiscordError::NotFound)?;
    if connection.provider != ProviderKind::Discord || connection.status != ConnectionStatus::Active
    {
        return Err(DiscordError::NotFound);
    }
    let material = credential_material(&connection.credential_ref).map_err(connection_error)?;
    DiscordIngestContext::from_connection(&connection, &material)
}

/// Record a delivery, returning true when it was already recorded.
///
/// The unique `(connection_public_id, delivery_id)` key makes a gateway replay
/// after a resume a single row, even across a process restart that emptied the
/// session's in-memory dedup window.
pub async fn record_delivery(
    pool: &MySqlPool,
    organization_id: u64,
    delivery: &DiscordDelivery,
) -> Result<bool, DiscordError> {
    let result = sqlx::query(
        "INSERT IGNORE INTO webhook_deliveries \
         (organization_id, connection_public_id, provider, delivery_id, event, action, result) \
         VALUES (?, ?, 'discord', ?, ?, ?, ?)",
    )
    .bind(organization_id)
    .bind(&delivery.connection_public_id)
    .bind(&delivery.delivery_id)
    .bind(delivery.event.kind())
    .bind(delivery.source.as_str())
    .bind(if delivery.accepted {
        "accepted"
    } else {
        "ignored"
    })
    .execute(pool)
    .await
    .map_err(DiscordError::Database)?;
    let duplicate = result.rows_affected() == 0;
    crate::middleware::metrics::metrics().webhook_delivery(
        "discord",
        if duplicate {
            "deduplicated"
        } else {
            "accepted"
        },
    );
    Ok(duplicate)
}

/// Find the active Discord connection that owns a guild, among `connections`.
///
/// The pure half of [`resolve_guild_connection`]: a disabled connection, a
/// connection for another provider and a guild nobody declared all miss.
pub fn find_guild_owner<'a>(
    connections: &'a [ConnectionView],
    guild_id: &str,
) -> Option<&'a ConnectionView> {
    let guild_id = normalize_identity(guild_id);
    connections.iter().find(|connection| {
        connection.provider == ProviderKind::Discord
            && connection.status == ConnectionStatus::Active
            && connection.resources.iter().any(|resource| {
                resource.resource_kind == "guild"
                    && normalize_identity(&resource.external_id) == guild_id
            })
    })
}

/// Find the active Discord connection that owns a guild (case-insensitive).
pub async fn resolve_guild_connection(
    pool: &MySqlPool,
    organization_id: u64,
    guild_id: &str,
) -> Result<Option<ConnectionView>, ConnectionError> {
    let connections = list_connections(pool, organization_id).await?;
    Ok(find_guild_owner(&connections, guild_id).cloned())
}

/// Validate that a guild is owned by an active Discord connection.
///
/// Configuration activation calls this, so a guild the organization has no
/// connection for is refused with a named `guild` field rather than silently
/// never firing.
pub async fn validate_guild(
    pool: &MySqlPool,
    organization_id: u64,
    guild_id: &str,
) -> Result<ConnectionView, ConnectionError> {
    resolve_guild_connection(pool, organization_id, guild_id)
        .await?
        .ok_or_else(|| ConnectionError::UnknownResource {
            field: "guild",
            value: guild_id.to_string(),
        })
}

/// Validate every guild a configuration revision references, as activation
/// does: the first guild without an active connection fails the activation.
pub fn validate_guilds_for_activation(
    connections: &[ConnectionView],
    guild_ids: &[String],
) -> Result<(), ConnectionError> {
    for guild_id in guild_ids {
        if find_guild_owner(connections, guild_id).is_none() {
            return Err(ConnectionError::UnknownResource {
                field: "guild",
                value: guild_id.clone(),
            });
        }
    }
    Ok(())
}

fn connection_error(error: ConnectionError) -> DiscordError {
    match error {
        ConnectionError::NotFound => DiscordError::NotFound,
        ConnectionError::Database(error) => DiscordError::Database(error),
        other => DiscordError::Invalid(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::connection::{ConnectionResourceView, ManagedBy};
    use serde_json::json;

    const BOT: &str = "900000000000000001";
    const GUILD: &str = "100000000000000001";
    const CHANNEL: &str = "200000000000000001";

    fn context() -> DiscordIngestContext {
        DiscordIngestContext::new("conn-discord")
            .with_bot_user_id(BOT)
            .with_guilds([GUILD])
    }

    fn message_create() -> Value {
        json!({
            "id": "300000000000000001",
            "channel_id": CHANNEL,
            "guild_id": GUILD,
            "author": {"id": "400000000000000001", "username": "human", "bot": false},
            "content": format!("<@{BOT}> deploy the site"),
            "mentions": [{"id": BOT, "username": "tide"}],
        })
    }

    fn dispatch(sequence: u64, data: Value) -> String {
        json!({"op": OP_DISPATCH, "s": sequence, "t": "MESSAGE_CREATE", "d": data}).to_string()
    }

    fn connection_view(
        status: ConnectionStatus,
        provider: ProviderKind,
        guild: &str,
    ) -> ConnectionView {
        ConnectionView {
            id: "conn-1".into(),
            provider,
            slug: "acme".into(),
            display_name: "Acme".into(),
            status,
            managed_by: ManagedBy::Database,
            credential_configured: true,
            resources: vec![ConnectionResourceView {
                resource_kind: "guild".into(),
                external_id: guild.into(),
                display_name: None,
            }],
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn mentions_are_stripped_and_roles_are_left_alone() {
        assert_eq!(
            strip_mention(&format!("<@{BOT}> deploy the site"), Some(BOT)),
            "deploy the site"
        );
        // The historical nickname form is the same mention.
        assert_eq!(
            strip_mention(&format!("hey <@!{BOT}> please deploy"), Some(BOT)),
            "hey please deploy"
        );
        assert_eq!(
            strip_mention(&format!("<@{BOT}> ping <@555>"), Some(BOT)),
            "ping <@555>"
        );
        // Role mentions are never a bot mention.
        assert_eq!(
            strip_mention("<@&777> everyone look", Some(BOT)),
            "<@&777> everyone look"
        );
        assert_eq!(strip_mention("<@&777> go", None), "<@&777> go");
        // Unknown bot id: only the leading mention goes.
        assert_eq!(
            strip_mention(&format!("<@{BOT}> ping <@{BOT}>"), None),
            format!("ping <@{BOT}>")
        );
        // Newlines survive; runs of spaces collapse.
        assert_eq!(
            strip_mention(&format!("<@{BOT}>  line one\nline two"), Some(BOT)),
            "line one\nline two"
        );
        assert_eq!(
            strip_mention("no mention here", Some(BOT)),
            "no mention here"
        );
        assert_eq!(strip_mention("<@unclosed", Some(BOT)), "<@unclosed");
    }

    #[test]
    fn mentions_user_matches_both_user_forms_only() {
        assert!(mentions_user(&format!("hi <@{BOT}>"), BOT));
        assert!(mentions_user(&format!("hi <@!{BOT}>"), BOT));
        assert!(!mentions_user("hi <@&900000000000000001>", BOT));
        assert!(!mentions_user("hi <@555>", BOT));
        assert!(!mentions_user("hi <@555", BOT));
    }

    #[test]
    fn a_guild_mention_becomes_a_typed_event() {
        let normalized =
            normalize_event(&context(), "MESSAGE_CREATE", &message_create()).expect("normalize");
        assert_eq!(normalized.kind(), "discord.mention");
        assert!(normalized.is_actionable());
        let message = normalized.message().expect("message");
        assert_eq!(message.guild_id.as_deref(), Some(GUILD));
        assert_eq!(message.channel_id, CHANNEL);
        assert_eq!(message.message_id, "300000000000000001");
        assert_eq!(message.author_username.as_deref(), Some("human"));
        assert!(message.bot_mentioned);
        assert!(!message.is_direct_message());
        // The mention is gone before any prompt sees the content.
        assert_eq!(message.prompt_text(), "deploy the site");
        assert!(!message.prompt_text().contains("<@"));
        assert!(message.raw_content.contains(BOT));
    }

    #[test]
    fn a_mention_is_recognised_from_the_content_alone() {
        // Discord omits `mentions` in some payloads; the content still decides.
        let mut data = message_create();
        data.as_object_mut().expect("object").remove("mentions");
        let normalized = normalize_event(&context(), "MESSAGE_CREATE", &data).expect("normalize");
        assert_eq!(normalized.kind(), "discord.mention");
    }

    #[test]
    fn direct_messages_and_plain_guild_messages_are_distinguished() {
        let context = context();
        let mut dm = message_create();
        let object = dm.as_object_mut().expect("object");
        object.remove("guild_id");
        object.insert("content".into(), json!("status?"));
        object.remove("mentions");
        let normalized = normalize_event(&context, "MESSAGE_CREATE", &dm).expect("normalize");
        assert_eq!(normalized.kind(), "discord.direct_message");
        let message = normalized.message().expect("message");
        assert!(message.is_direct_message());
        assert!(!message.bot_mentioned);

        let mut plain = message_create();
        let object = plain.as_object_mut().expect("object");
        object.insert("content".into(), json!("unrelated chatter"));
        object.remove("mentions");
        let normalized = normalize_event(&context, "MESSAGE_CREATE", &plain).expect("normalize");
        assert_eq!(normalized.kind(), "discord.message");
        assert!(!normalized.message().expect("message").bot_mentioned);
    }

    #[test]
    fn the_bots_own_messages_other_bots_and_edits_are_ignored() {
        let context = context();
        let mut own = message_create();
        own.as_object_mut()
            .expect("object")
            .insert("author".into(), json!({"id": BOT, "username": "tide"}));
        let normalized = normalize_event(&context, "MESSAGE_CREATE", &own).expect("normalize");
        assert_eq!(normalized.kind(), "discord.ignored");
        assert!(!normalized.is_actionable());
        assert_eq!(
            decide(&context, &normalized),
            (false, Some(RejectReason::SelfAuthored))
        );

        let mut other_bot = message_create();
        other_bot.as_object_mut().expect("object").insert(
            "author".into(),
            json!({"id": "500000000000000001", "bot": true}),
        );
        assert_eq!(
            normalize_event(&context, "MESSAGE_CREATE", &other_bot)
                .expect("normalize")
                .kind(),
            "discord.ignored"
        );

        let mut hooked = message_create();
        hooked
            .as_object_mut()
            .expect("object")
            .insert("webhook_id".into(), json!("600000000000000001"));
        assert_eq!(
            normalize_event(&context, "MESSAGE_CREATE", &hooked)
                .expect("normalize")
                .kind(),
            "discord.ignored"
        );

        let edit =
            normalize_event(&context, "MESSAGE_UPDATE", &message_create()).expect("normalize");
        assert_eq!(edit.kind(), "discord.ignored");
        assert_eq!(
            decide(&context, &edit),
            (false, Some(RejectReason::EditedMessage))
        );
    }

    #[test]
    fn unknown_dispatches_are_unsupported_and_malformed_ones_fail() {
        let context = context();
        let reaction = normalize_event(&context, "MESSAGE_REACTION_ADD", &json!({"id": "1"}))
            .expect("normalize");
        assert_eq!(reaction.kind(), "discord.unsupported");
        assert_eq!(
            decide(&context, &reaction),
            (false, Some(RejectReason::UnsupportedEvent))
        );

        assert!(matches!(
            normalize_event(&context, "MESSAGE_CREATE", &json!({"id": "1"})),
            Err(DiscordError::Invalid(_))
        ));
        assert!(matches!(
            normalize_event(&context, "MESSAGE_CREATE", &json!({"channel_id": CHANNEL})),
            Err(DiscordError::Invalid(_))
        ));
    }

    #[test]
    fn guild_resolution_and_channel_filters_reject_delivery() {
        let unfiltered = context();
        let context = unfiltered.clone().with_channels([CHANNEL]);

        let mut elsewhere = message_create();
        elsewhere
            .as_object_mut()
            .expect("object")
            .insert("channel_id".into(), json!("299999999999999999"));
        let delivery = build_delivery(
            &context,
            DeliverySource::Live,
            "d1",
            Some(2),
            "MESSAGE_CREATE",
            &elsewhere,
        )
        .expect("build");
        assert!(!delivery.accepted);
        assert_eq!(delivery.reason, Some(RejectReason::ChannelFiltered));

        let mut foreign = message_create();
        foreign
            .as_object_mut()
            .expect("object")
            .insert("guild_id".into(), json!("199999999999999999"));
        let delivery = build_delivery(
            &context,
            DeliverySource::Live,
            "d2",
            Some(3),
            "MESSAGE_CREATE",
            &foreign,
        )
        .expect("build");
        assert!(!delivery.accepted);
        assert_eq!(delivery.reason, Some(RejectReason::GuildNotOwned));

        // A direct message has no guild, so guild filters do not apply.
        assert!(context.guild_allowed(None));
        // Channel names are accepted with or without a leading '#'.
        let named = unfiltered.clone().with_channels(["#general"]);
        assert!(named.channel_allowed("general"));
        assert!(named.channel_allowed("#General"));
        assert!(!named.channel_allowed("random"));
        // No filters means any channel in an owned guild.
        assert!(unfiltered.channel_allowed("anything"));
        assert!(!unfiltered.guild_allowed(Some("199999999999999999")));
    }

    #[test]
    fn a_guild_with_no_connection_fails_activation() {
        let connections = vec![connection_view(
            ConnectionStatus::Active,
            ProviderKind::Discord,
            GUILD,
        )];
        assert!(find_guild_owner(&connections, GUILD).is_some());
        assert!(validate_guilds_for_activation(&connections, &[GUILD.to_string()]).is_ok());

        let error = validate_guilds_for_activation(&connections, &["199999999999999999".into()])
            .expect_err("a guild with no connection fails activation");
        assert!(matches!(
            error,
            ConnectionError::UnknownResource {
                field: "guild",
                ref value
            } if value == "199999999999999999"
        ));
        assert_eq!(
            error.to_string(),
            "guild: no connection delivers for '199999999999999999'"
        );

        // A disabled connection, or one for another provider, owns nothing.
        let disabled = vec![connection_view(
            ConnectionStatus::Disabled,
            ProviderKind::Discord,
            GUILD,
        )];
        assert!(find_guild_owner(&disabled, GUILD).is_none());
        assert!(validate_guilds_for_activation(&disabled, &[GUILD.to_string()]).is_err());
        let other_provider = vec![connection_view(
            ConnectionStatus::Active,
            ProviderKind::Slack,
            GUILD,
        )];
        assert!(find_guild_owner(&other_provider, GUILD).is_none());
        assert!(find_guild_owner(&[], GUILD).is_none());
    }

    #[test]
    fn frames_are_parsed_and_malformed_ones_refused() {
        let frame = parse_frame(r#"{"op":10,"d":{"heartbeat_interval":41250}}"#).expect("parse");
        assert_eq!(frame.op, OP_HELLO);
        assert_eq!(frame.sequence, None);
        assert_eq!(frame.event_name, None);

        let frame = parse_frame(&dispatch(7, message_create())).expect("parse");
        assert_eq!(frame.op, OP_DISPATCH);
        assert_eq!(frame.sequence, Some(7));
        assert_eq!(frame.event_name.as_deref(), Some("MESSAGE_CREATE"));

        assert!(matches!(
            parse_frame("not json"),
            Err(DiscordError::Invalid(_))
        ));
        assert!(matches!(parse_frame("{}"), Err(DiscordError::Invalid(_))));
        assert!(matches!(
            parse_frame(r#"{"op":9999}"#),
            Err(DiscordError::Invalid(_))
        ));
    }

    #[test]
    fn the_handshake_identifies_first_and_resumes_after_a_ready() {
        let mut session = DiscordGatewaySession::new(context());
        let outcome = session
            .handle_frame(r#"{"op":10,"d":{"heartbeat_interval":41250}}"#)
            .expect("hello");
        let GatewayOutcome::Handshake {
            command,
            heartbeat_interval_ms,
        } = outcome
        else {
            panic!("expected a handshake");
        };
        assert_eq!(heartbeat_interval_ms, 41_250);
        assert_eq!(command.op, OP_IDENTIFY);
        assert!(!command.is_resume());
        assert_eq!(command.payload["intents"], json!(GATEWAY_INTENTS));
        assert_eq!(session.heartbeat_interval_ms(), Some(41_250));
        assert!(!session.can_resume());
        assert!(session.resume_command().is_none());

        // The token is named, not held, and only appears when the frame is sent.
        assert_eq!(command.frame()["d"].get("token"), None);
        let rendered = command.frame_with_token("bot-token-value");
        assert!(rendered.contains("bot-token-value"));
        assert_eq!(command.credential_key, Some("bot_token"));

        let ready = json!({
            "op": OP_DISPATCH, "s": 1, "t": "READY",
            "d": {
                "session_id": "session-abc",
                "resume_gateway_url": "wss://resume.example",
                "user": {"id": BOT, "username": "tide"},
            }
        })
        .to_string();
        assert_eq!(
            session.handle_frame(&ready).expect("ready"),
            GatewayOutcome::Ready {
                session_id: "session-abc".into()
            }
        );
        assert_eq!(session.session_id(), Some("session-abc"));
        assert_eq!(session.resume_gateway_url(), Some("wss://resume.example"));
        assert_eq!(session.last_sequence(), Some(1));
        assert!(session.can_resume());

        // A second hello, after a redial, resumes from the last sequence.
        session
            .handle_frame(&dispatch(4, message_create()))
            .expect("dispatch");
        let GatewayOutcome::Handshake { command, .. } = session
            .handle_frame(r#"{"op":10,"d":{"heartbeat_interval":41250}}"#)
            .expect("hello")
        else {
            panic!("expected a handshake");
        };
        assert!(command.is_resume());
        assert_eq!(
            command.payload,
            json!({"session_id": "session-abc", "seq": 4})
        );
    }

    #[test]
    fn ready_learns_the_bot_user_id_when_it_was_not_configured() {
        let mut session = DiscordGatewaySession::new(DiscordIngestContext::new("conn-discord"));
        assert!(session.context().bot_user_id.is_none());
        let ready = json!({
            "op": OP_DISPATCH, "s": 1, "t": "READY",
            "d": {"session_id": "s1", "user": {"id": BOT}}
        })
        .to_string();
        session.handle_frame(&ready).expect("ready");
        assert_eq!(session.context().bot_user_id.as_deref(), Some(BOT));

        // With the id learned, the bot's own message is now recognised.
        let mut own = message_create();
        own.as_object_mut()
            .expect("object")
            .insert("author".into(), json!({"id": BOT}));
        let GatewayOutcome::Dispatch {
            delivery: Some(delivery),
        } = session.handle_frame(&dispatch(2, own)).expect("dispatch")
        else {
            panic!("expected a delivery");
        };
        assert_eq!(delivery.event.kind(), "discord.ignored");
    }

    #[test]
    fn heartbeats_carry_the_last_sequence_and_need_no_token() {
        let mut session = DiscordGatewaySession::new(context());
        let GatewayOutcome::Heartbeat { command } = session
            .handle_frame(r#"{"op":1,"d":null}"#)
            .expect("heartbeat")
        else {
            panic!("expected a heartbeat");
        };
        assert_eq!(command.op, OP_HEARTBEAT);
        assert_eq!(command.credential_key, None);
        assert_eq!(command.payload, Value::Null);
        assert_eq!(command.frame_with_token("unused"), r#"{"d":null,"op":1}"#);

        session
            .handle_frame(&dispatch(9, message_create()))
            .expect("dispatch");
        let GatewayOutcome::Heartbeat { command } = session
            .handle_frame(r#"{"op":1,"d":null}"#)
            .expect("heartbeat")
        else {
            panic!("expected a heartbeat");
        };
        assert_eq!(command.payload, json!(9));
        assert_eq!(
            session.handle_frame(r#"{"op":11}"#).expect("ack"),
            GatewayOutcome::Ignored
        );
        assert_eq!(
            session.handle_frame(r#"{"op":42}"#).expect("unknown op"),
            GatewayOutcome::Ignored
        );
    }

    #[test]
    fn a_resume_replay_does_not_duplicate_an_already_received_event() {
        let mut session = DiscordGatewaySession::new(context());
        session
            .handle_frame(r#"{"op":10,"d":{"heartbeat_interval":41250}}"#)
            .expect("hello");
        let ready = json!({
            "op": OP_DISPATCH, "s": 1, "t": "READY",
            "d": {"session_id": "session-abc", "user": {"id": BOT}}
        })
        .to_string();
        session.handle_frame(&ready).expect("ready");

        let GatewayOutcome::Dispatch {
            delivery: Some(first),
        } = session
            .handle_frame(&dispatch(2, message_create()))
            .expect("dispatch")
        else {
            panic!("expected a delivery");
        };
        assert_eq!(first.source, DeliverySource::Live);
        assert_eq!(first.delivery_id, "300000000000000001");
        assert_eq!(first.sequence, Some(2));
        assert!(first.accepted);

        // The server asks for a redial, and the resumed stream replays it.
        assert_eq!(
            session.handle_frame(r#"{"op":7}"#).expect("reconnect"),
            GatewayOutcome::Reconnect { resumable: true }
        );
        session
            .handle_frame(r#"{"op":10,"d":{"heartbeat_interval":41250}}"#)
            .expect("hello");
        assert_eq!(
            session
                .handle_frame(&dispatch(2, message_create()))
                .expect("replay"),
            GatewayOutcome::Dispatch { delivery: None }
        );
        assert_eq!(
            session
                .handle_frame(r#"{"op":0,"s":3,"t":"RESUMED","d":{}}"#)
                .expect("resumed"),
            GatewayOutcome::Resumed
        );

        // A genuinely new event after the replay still gets through.
        let mut next = message_create();
        next.as_object_mut()
            .expect("object")
            .insert("id".into(), json!("300000000000000002"));
        let GatewayOutcome::Dispatch {
            delivery: Some(second),
        } = session.handle_frame(&dispatch(4, next)).expect("dispatch")
        else {
            panic!("expected a delivery");
        };
        assert_eq!(second.delivery_id, "300000000000000002");
        assert_eq!(second.source, DeliverySource::Live);
    }

    #[test]
    fn a_replay_during_a_resume_is_marked_as_resumed() {
        let mut session = DiscordGatewaySession::new(context());
        session
            .handle_frame(r#"{"op":10,"d":{"heartbeat_interval":1}}"#)
            .expect("hello");
        session
            .handle_frame(
                &json!({"op": OP_DISPATCH, "s": 1, "t": "READY",
                        "d": {"session_id": "s1", "user": {"id": BOT}}})
                .to_string(),
            )
            .expect("ready");
        session.handle_frame(r#"{"op":7}"#).expect("reconnect");
        session
            .handle_frame(r#"{"op":10,"d":{"heartbeat_interval":1}}"#)
            .expect("hello");

        // Still resuming: an event the pool had not seen is marked accordingly.
        let GatewayOutcome::Dispatch {
            delivery: Some(delivery),
        } = session
            .handle_frame(&dispatch(2, message_create()))
            .expect("dispatch")
        else {
            panic!("expected a delivery");
        };
        assert_eq!(delivery.source, DeliverySource::Resumed);
        assert_eq!(delivery.source.as_str(), "gateway_resume");
        assert_eq!(DeliverySource::Live.as_str(), "gateway");
    }

    #[test]
    fn an_invalid_session_that_cannot_resume_starts_afresh() {
        let mut session = DiscordGatewaySession::new(context());
        session
            .handle_frame(r#"{"op":10,"d":{"heartbeat_interval":1}}"#)
            .expect("hello");
        session
            .handle_frame(
                &json!({"op": OP_DISPATCH, "s": 1, "t": "READY", "d": {"session_id": "s1"}})
                    .to_string(),
            )
            .expect("ready");
        assert!(session.can_resume());

        assert_eq!(
            session
                .handle_frame(r#"{"op":9,"d":true}"#)
                .expect("invalid"),
            GatewayOutcome::Reconnect { resumable: true }
        );
        assert!(session.can_resume());

        assert_eq!(
            session
                .handle_frame(r#"{"op":9,"d":false}"#)
                .expect("invalid"),
            GatewayOutcome::Reconnect { resumable: false }
        );
        assert!(!session.can_resume());
        assert_eq!(session.session_id(), None);
        let GatewayOutcome::Handshake { command, .. } = session
            .handle_frame(r#"{"op":10,"d":{"heartbeat_interval":1}}"#)
            .expect("hello")
        else {
            panic!("expected a handshake");
        };
        assert_eq!(command.op, OP_IDENTIFY);
    }

    #[test]
    fn malformed_gateway_frames_are_refused() {
        let mut session = DiscordGatewaySession::new(context());
        assert!(matches!(
            session.handle_frame(r#"{"op":10,"d":{}}"#),
            Err(DiscordError::Invalid(_))
        ));
        assert!(matches!(
            session.handle_frame(r#"{"op":0,"s":1,"d":{}}"#),
            Err(DiscordError::Invalid(_))
        ));
        assert!(matches!(
            session.handle_frame(r#"{"op":0,"s":1,"t":"READY","d":{}}"#),
            Err(DiscordError::Invalid(_))
        ));
        // A dispatch with nothing to deduplicate on cannot be accepted.
        assert!(matches!(
            session.handle_frame(r#"{"op":0,"t":"MESSAGE_CREATE","d":{}}"#),
            Err(DiscordError::Invalid(_))
        ));
    }

    #[test]
    fn the_dedup_window_is_bounded() {
        let mut session = DiscordGatewaySession::new(context());
        for index in 0..=RESUME_DEDUP_WINDOW {
            assert!(session.remember(&format!("m{index}")));
        }
        assert_eq!(session.order.len(), RESUME_DEDUP_WINDOW);
        // The oldest id was evicted, so it is deliverable again; the database
        // unique key is the backstop for that window.
        assert!(session.remember("m0"));
        assert!(!session.remember(&format!("m{RESUME_DEDUP_WINDOW}")));
    }

    #[test]
    fn the_delivery_key_falls_back_to_the_channel_and_sequence() {
        assert_eq!(
            delivery_key(&json!({"id": "m1", "channel_id": CHANNEL}), Some(5)),
            "m1"
        );
        assert_eq!(
            delivery_key(&json!({"channel_id": CHANNEL}), Some(5)),
            format!("{CHANNEL}:5")
        );
        assert_eq!(delivery_key(&json!({}), None), "");
    }

    #[test]
    fn replies_target_the_channel_or_thread_of_the_event() {
        let context = context();
        let event =
            normalize_event(&context, "MESSAGE_CREATE", &message_create()).expect("normalize");
        let message = event.message().expect("message");
        let reply = DiscordReply::to_event(message, "deployed").expect("reply");
        assert_eq!(reply.target().channel_id(), CHANNEL);
        assert_eq!(reply.target().message_id(), "300000000000000001");
        assert_eq!(reply.content(), "deployed");
        assert_eq!(
            reply.create_message_body(),
            json!({
                "content": "deployed",
                "message_reference": {
                    "message_id": "300000000000000001",
                    "fail_if_not_exists": false,
                },
                "allowed_mentions": {"parse": []},
            })
        );
        let request = reply.create_message_request();
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.url,
            format!("{API_BASE}/channels/{CHANNEL}/messages")
        );
        assert_eq!(request.credential_key, "bot_token");

        // In a thread the thread *is* the channel, so the reply lands there.
        let mut threaded = message_create();
        let object = threaded.as_object_mut().expect("object");
        object.insert("channel_id".into(), json!("250000000000000009"));
        object.insert("position".into(), json!(0));
        let event = normalize_event(&context, "MESSAGE_CREATE", &threaded).expect("normalize");
        let message = event.message().expect("message");
        assert!(message.in_thread);
        assert_eq!(message.thread_key(), "250000000000000009");
        let reply = DiscordReply::to_event(message, "done in the thread").expect("reply");
        assert_eq!(reply.target().channel_id(), "250000000000000009");
        assert_eq!(
            reply.target().create_message_url(),
            format!("{API_BASE}/channels/250000000000000009/messages")
        );
    }

    #[test]
    fn reply_content_is_validated() {
        let context = context();
        let event =
            normalize_event(&context, "MESSAGE_CREATE", &message_create()).expect("normalize");
        let message = event.message().expect("message");
        assert!(DiscordReply::to_event(message, "   ").is_err());
        assert!(DiscordReply::to_event(message, "x".repeat(MAX_REPLY_CHARS)).is_ok());
        assert!(DiscordReply::to_event(message, "x".repeat(MAX_REPLY_CHARS + 1)).is_err());
    }

    #[test]
    fn an_inline_reply_records_the_message_it_answers() {
        let mut data = message_create();
        data.as_object_mut().expect("object").insert(
            "message_reference".into(),
            json!({"message_id": "300000000000000000"}),
        );
        let event = normalize_event(&context(), "MESSAGE_CREATE", &data).expect("normalize");
        assert_eq!(
            event
                .message()
                .expect("message")
                .referenced_message_id
                .as_deref(),
            Some("300000000000000000")
        );
    }

    #[test]
    fn context_from_connection_reads_resources_and_requires_a_bot_token() {
        let connection = EffectiveConnection {
            public_id: "conn-7".into(),
            provider: ProviderKind::Discord,
            slug: "acme".into(),
            status: ConnectionStatus::Active,
            credential_ref: "DISCORD_ACME_CREDS".into(),
            resources: vec![
                ConnectionResourceView {
                    resource_kind: "guild".into(),
                    external_id: GUILD.into(),
                    display_name: None,
                },
                ConnectionResourceView {
                    resource_kind: "channel".into(),
                    external_id: CHANNEL.into(),
                    display_name: None,
                },
            ],
        };
        let material = CredentialMaterial {
            values: [
                ("bot_token".to_string(), "bot-token-value".to_string()),
                ("bot_user_id".to_string(), BOT.to_string()),
            ]
            .into_iter()
            .collect(),
        };
        let context =
            DiscordIngestContext::from_connection(&connection, &material).expect("context");
        assert_eq!(context.guild_ids, vec![GUILD.to_string()]);
        assert_eq!(context.channel_ids, vec![CHANNEL.to_string()]);
        assert_eq!(context.bot_user_id.as_deref(), Some(BOT));
        // The context never holds the token, so it cannot leak through Debug.
        assert!(!format!("{context:?}").contains("bot-token-value"));

        let empty = CredentialMaterial::default();
        assert!(matches!(
            DiscordIngestContext::from_connection(&connection, &empty),
            Err(DiscordError::MissingCredential("bot_token"))
        ));

        // A disabled or non-Discord connection is not found.
        let mut disabled = connection.clone();
        disabled.status = ConnectionStatus::Disabled;
        assert!(matches!(
            DiscordIngestContext::from_connection(&disabled, &material),
            Err(DiscordError::NotFound)
        ));
        let mut slack = connection.clone();
        slack.provider = ProviderKind::Slack;
        assert!(matches!(
            DiscordIngestContext::from_connection(&slack, &material),
            Err(DiscordError::NotFound)
        ));
    }

    #[tokio::test]
    async fn the_session_pump_sends_commands_and_forwards_each_event_once() {
        let mut session = DiscordGatewaySession::new(context());
        let (frames, mut inbound) = mpsc::channel::<String>(8);
        let (outbound, mut commands) = mpsc::channel::<GatewayCommand>(8);
        let (delivery_tx, mut deliveries) = mpsc::channel::<DiscordDelivery>(8);

        for frame in [
            r#"{"op":10,"d":{"heartbeat_interval":41250}}"#.to_string(),
            json!({"op": OP_DISPATCH, "s": 1, "t": "READY",
                   "d": {"session_id": "s1", "user": {"id": BOT}}})
            .to_string(),
            dispatch(2, message_create()),
            "not a frame".to_string(),
            r#"{"op":7}"#.to_string(),
        ] {
            frames.send(frame).await.expect("queue frame");
        }
        drop(frames);

        let stop = run_gateway_session(&mut session, &mut inbound, &outbound, &delivery_tx).await;
        assert_eq!(stop, GatewayStop::Reconnect { resumable: true });
        let identify = commands.recv().await.expect("identify");
        assert_eq!(identify.op, OP_IDENTIFY);
        let delivery = deliveries.recv().await.expect("one delivery");
        assert_eq!(delivery.event.kind(), "discord.mention");
        assert!(delivery.accepted);
        assert!(deliveries.try_recv().is_err(), "exactly one delivery");
    }
}
