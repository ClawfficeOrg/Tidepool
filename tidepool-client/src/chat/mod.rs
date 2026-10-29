//! The client's chat surface: rooms, cursor history, live frames and posting.
//!
//! The pool owns the truth. [`ChatController`] is a headless state machine over
//! a [`ChatTransport`], so the merge rules, the cursor walks, the catch-up
//! state and session posting are tested without a window, a socket or a pool.
//!
//! # History pages forward, and merging is by the server's id
//!
//! `GET /v1/chatrooms/{room}/messages?after=` is forward-only. Opening a room
//! walks the cursor from the beginning until a page reports no `next`; that
//! empty tail is what confirms the room is current. Every message is merged by
//! [`ChatMessage::id`] and ordered by [`ChatMessage::sequence`], so a page that
//! overlaps a live frame cannot duplicate a row and a live frame keeps the id
//! the pool minted.
//!
//! Scrolling to the top walks the same forward cursor backwards one page at a
//! time and keeps only messages below the oldest held sequence
//! ([`ChatController::load_older`]).
//!
//! # Currentness is a fetched fact
//!
//! [`Currentness::CatchingUp`] is the opening state and the state after a
//! dropped stream. Only a history read that reaches the head, or the server's
//! `chat.current` frame, answers [`Currentness::Current`]. A reconnect is
//! therefore labelled as catch-up until currentness is confirmed, never before.
//!
//! # A session is a member, and its replies stay its own
//!
//! [`ChatController::bind_session`] binds a local session to the open room only
//! when the pool lists it as a member. [`ChatController::session_reply`] posts
//! through [`ChatTransport::send_as_session`] and refuses a reply that comes
//! back authored as a person; [`ChatController::post`] refuses one that comes
//! back authored as a session. That is what keeps a person's words and an
//! agent's words from being exchanged.
//!
//! The v1 member HTTP route cannot act as a session, so [`HttpChat`] refuses
//! session posting rather than misattributing it (see [`http`]).

pub mod http;
pub mod model;

use std::collections::BTreeMap;

use thiserror::Error;

pub use crate::chat::http::HttpChat;
pub use crate::chat::model::{
    AuthorDescriptor, AuthorKind, ChatAuthor, ChatMember, ChatMessage, ChatPage, ChatRoom,
    MAX_BACKFILL_STEPS, MAX_BODY_BYTES, ROOM_PAGE_SIZE,
};

/// How many of the newest messages an open room keeps without paging again.
pub const ROOM_WINDOW: usize = 200;

/// The banner while the room is behind the pool.
pub const CATCH_UP_NOTICE: &str =
    "Catching up \u{2014} showing cached history until the pool confirms this room is current.";
/// The banner once a read has confirmed currentness.
pub const CURRENT_NOTICE: &str = "Current \u{2014} every message up to now is shown.";
/// The banner while the live stream is down.
pub const RECONNECT_NOTICE: &str = "Reconnecting \u{2014} the live stream dropped, so the cached history is not current until the pool confirms it.";

/// What could not be done with a room.
///
/// No variant carries the operator's token or a request body.
#[derive(Debug, Error)]
pub enum ChatError {
    /// The pool's URL or transport could not be reached.
    #[error("the pool could not be reached: {detail}")]
    Unreachable {
        /// The transport's description.
        detail: String,
    },
    /// The pool answered `401`.
    #[error("the pool did not accept this client's credentials")]
    Unauthorized,
    /// The pool answered `403`.
    #[error("the pool refused this action: {detail}")]
    Forbidden {
        /// The problem detail, when the pool sent one.
        detail: String,
    },
    /// The pool answered with an error problem document.
    #[error("the pool answered {status} ({code}){detail}")]
    Problem {
        /// HTTP status.
        status: u16,
        /// The problem code, or `unknown` when absent.
        code: String,
        /// The problem detail, empty when absent.
        detail: String,
    },
    /// A response was not the documented shape.
    #[error("the pool's answer was not understood: {detail}")]
    Protocol {
        /// What was wrong.
        detail: String,
    },
    /// A room id is not safe to place in a path.
    #[error("`{room}` is not a usable room id")]
    InvalidRoom {
        /// The rejected id.
        room: String,
    },
    /// A body is empty or too long.
    #[error("{detail}")]
    InvalidBody {
        /// What the caller has to change.
        detail: String,
    },
    /// No room is open, so there is nothing to read or post to.
    #[error("no chatroom is open")]
    NoRoomOpen,
    /// A session is not a member of the open room.
    #[error("session `{session}` is not a member of `{room}`")]
    NotAMember {
        /// The session that was asked to join.
        session: String,
        /// The room it is not in.
        room: String,
    },
    /// A reply came back authored by the wrong principal.
    #[error("the pool authored the message as {actual}; expected {expected}")]
    AuthorMismatch {
        /// Who should have written it.
        expected: String,
        /// Who the pool says wrote it.
        actual: String,
    },
    /// The live stream dropped.
    #[error("the room's live stream dropped; cached history is not current")]
    StreamDropped,
    /// The v1 pool has no route for a session to post through the member API.
    #[error(
        "the pool cannot accept a message authored by session `{session}`: the member API has no session send route, so the client will not post it as a person"
    )]
    SessionPostingUnavailable {
        /// The session whose reply could not be posted.
        session: String,
    },
}

/// The pool calls one chat surface needs.
///
/// Implementations are intentionally synchronous and one method per route, so
/// the controller has no network code and a test can script each call.
pub trait ChatTransport {
    /// List the rooms the signed-in member belongs to.
    fn rooms(&mut self) -> Result<Vec<ChatRoom>, ChatError>;
    /// List one room's members.
    fn members(&mut self, room: &str) -> Result<Vec<ChatMember>, ChatError>;
    /// Read one page of history after an exclusive sequence cursor.
    fn history(
        &mut self,
        room: &str,
        after: Option<u64>,
        limit: u32,
    ) -> Result<ChatPage, ChatError>;
    /// Send one message as the signed-in person.
    fn send(
        &mut self,
        room: &str,
        body: &str,
        reply_to: Option<&str>,
    ) -> Result<ChatMessage, ChatError>;
    /// Send one message authored by a session running on this machine.
    fn send_as_session(
        &mut self,
        room: &str,
        session: &str,
        body: &str,
        reply_to: Option<&str>,
    ) -> Result<ChatMessage, ChatError>;
}

/// Whether the open room is up to date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Currentness {
    /// History is still being read, or the stream is not yet confirmed.
    #[default]
    CatchingUp,
    /// A read or the server's `chat.current` frame confirmed the head.
    Current,
    /// The stream dropped; cached history is shown until currentness returns.
    Reconnecting,
}

impl Currentness {
    /// Whether the room is confirmed current.
    pub fn is_current(self) -> bool {
        self == Self::Current
    }

    /// The sentence a surface draws.
    pub fn banner(self) -> &'static str {
        match self {
            Self::CatchingUp => CATCH_UP_NOTICE,
            Self::Current => CURRENT_NOTICE,
            Self::Reconnecting => RECONNECT_NOTICE,
        }
    }
}

/// What one stream frame did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameOutcome {
    /// A page of catch-up history was merged.
    History,
    /// One live message was merged.
    Message,
    /// The server confirmed the room is current.
    Current,
    /// Presence changed; no message was touched.
    Presence,
    /// The server said a fetch is needed before the room is current.
    CatchUp,
    /// The stream reported a problem.
    Error,
    /// A frame this build does not know; nothing changed.
    Ignored,
}

/// Everything a chat view draws, taken as one value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatSnapshot {
    /// The member's rooms.
    pub rooms: Vec<ChatRoom>,
    /// The open room, when one is open.
    pub selected_room: Option<ChatRoom>,
    /// The open room's messages, oldest first.
    pub messages: Vec<ChatMessage>,
    /// The open room's members.
    pub members: Vec<ChatMember>,
    /// The principals the live stream last reported watching.
    pub watchers: Vec<String>,
    /// Whether the open room is current.
    pub currentness: Currentness,
    /// Whether an older page may exist.
    pub has_older: bool,
    /// Whether the composer may send.
    pub can_post: bool,
    /// Sessions in the member list, by public id.
    pub session_members: Vec<String>,
    /// The last problem or status sentence, when there is one.
    pub notice: Option<String>,
}

/// The client's chat state machine over one transport.
pub struct ChatController<T: ChatTransport> {
    transport: T,
    rooms: Vec<ChatRoom>,
    selected: Option<String>,
    messages: Vec<ChatMessage>,
    members: Vec<ChatMember>,
    watchers: Vec<String>,
    currentness: Currentness,
    cursor: Option<u64>,
    has_older: bool,
    bindings: BTreeMap<String, String>,
    notice: Option<String>,
    last_sent: Option<ChatMessage>,
}

impl<T: ChatTransport> ChatController<T> {
    /// Build a controller over a transport. No room is open.
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            rooms: Vec::new(),
            selected: None,
            messages: Vec::new(),
            members: Vec::new(),
            watchers: Vec::new(),
            currentness: Currentness::CatchingUp,
            cursor: None,
            has_older: false,
            bindings: BTreeMap::new(),
            notice: None,
            last_sent: None,
        }
    }

    /// The rooms last listed.
    pub fn rooms(&self) -> &[ChatRoom] {
        &self.rooms
    }

    /// The open room's id.
    pub fn selected_room(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// The open room's messages, oldest first.
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// The open room's members.
    pub fn members(&self) -> &[ChatMember] {
        &self.members
    }

    /// The principals last reported watching.
    pub fn watchers(&self) -> &[String] {
        &self.watchers
    }

    /// Whether the open room is current.
    pub fn currentness(&self) -> Currentness {
        self.currentness
    }

    /// The catch-up or current banner, when a room is open.
    pub fn banner(&self) -> Option<&'static str> {
        self.selected.as_ref().map(|_| self.currentness.banner())
    }

    /// Whether an older page may exist.
    pub fn has_older(&self) -> bool {
        self.has_older
    }

    /// The last problem or status sentence.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The message this controller last sent, when a send succeeded.
    pub fn last_sent(&self) -> Option<&ChatMessage> {
        self.last_sent.as_ref()
    }

    /// The sessions bound to a room, by session id.
    pub fn bindings(&self) -> &BTreeMap<String, String> {
        &self.bindings
    }

    /// The sessions in the open room, by public id.
    pub fn session_members(&self) -> Vec<String> {
        self.members
            .iter()
            .filter(|member| member.is_session())
            .map(|member| member.principal.id.clone())
            .collect()
    }

    /// Take the whole state as one value for a view.
    pub fn snapshot(&self) -> ChatSnapshot {
        ChatSnapshot {
            rooms: self.rooms.clone(),
            selected_room: self.selected.as_ref().map(|selected| {
                self.rooms
                    .iter()
                    .find(|room| &room.id == selected)
                    .cloned()
                    .unwrap_or_else(|| ChatRoom {
                        id: selected.clone(),
                        name: selected.clone(),
                        ..ChatRoom::default()
                    })
            }),
            messages: self.messages.clone(),
            members: self.members.clone(),
            watchers: self.watchers.clone(),
            currentness: self.currentness,
            has_older: self.has_older,
            can_post: self.selected.is_some(),
            session_members: self.session_members(),
            notice: self.notice.clone(),
        }
    }

    /// List the rooms the member belongs to.
    pub fn refresh_rooms(&mut self) -> Result<(), ChatError> {
        match self.transport.rooms() {
            Ok(rooms) => {
                self.rooms = rooms;
                self.notice = None;
                Ok(())
            }
            Err(error) => {
                self.notice = Some(error.to_string());
                Err(error)
            }
        }
    }

    /// Open a room and read its history from the beginning to the head.
    ///
    /// The room is [`Currentness::Current`] only when the last page reports no
    /// `next`; a walk that hits [`MAX_BACKFILL_STEPS`] stays catching up.
    pub fn open_room(&mut self, room: &str) -> Result<(), ChatError> {
        let room = room.trim();
        if room.is_empty() {
            return Err(ChatError::InvalidRoom {
                room: room.to_owned(),
            });
        }
        self.selected = Some(room.to_owned());
        self.messages.clear();
        self.members.clear();
        self.watchers.clear();
        self.cursor = None;
        self.has_older = false;
        self.bindings.clear();
        self.currentness = Currentness::CatchingUp;

        let mut steps = 0usize;
        loop {
            let previous = self.cursor;
            let page = match self.transport.history(room, self.cursor, ROOM_PAGE_SIZE) {
                Ok(page) => page,
                Err(error) => {
                    self.notice = Some(error.to_string());
                    return Err(error);
                }
            };
            self.merge_page(&page);
            steps += 1;

            match page.next {
                None => {
                    self.currentness = Currentness::Current;
                    break;
                }
                Some(next) => {
                    if steps >= MAX_BACKFILL_STEPS || Some(next) == previous {
                        self.notice = Some(
                            "History is longer than this client could read in one go; it will catch up from the live stream."
                                .to_owned(),
                        );
                        break;
                    }
                    self.cursor = Some(next);
                }
            }
        }

        if self.messages.len() > ROOM_WINDOW {
            let excess = self.messages.len() - ROOM_WINDOW;
            self.messages.drain(0..excess);
            self.has_older = true;
        }
        self.notice = None;
        Ok(())
    }

    /// Read one page immediately older than the oldest held message.
    ///
    /// Returns how many rows were added. The forward-only cursor is walked
    /// backwards; every merge is still keyed by the pool's id, so an
    /// overlapping page adds nothing.
    pub fn load_older(&mut self) -> Result<usize, ChatError> {
        let room = self.selected.clone().ok_or(ChatError::NoRoomOpen)?;
        let Some(before) = model::tail_cursor(&self.messages) else {
            self.has_older = false;
            return Ok(0);
        };
        let mut cursor = before;
        let mut collected = Vec::new();
        let mut reached_start = false;

        for _ in 0..MAX_BACKFILL_STEPS {
            if cursor == 0 {
                reached_start = true;
                break;
            }
            let window = cursor.saturating_sub(ROOM_PAGE_SIZE as u64);
            let page = match self.transport.history(&room, Some(window), ROOM_PAGE_SIZE) {
                Ok(page) => page,
                Err(error) => {
                    self.notice = Some(error.to_string());
                    return Err(error);
                }
            };
            let below: Vec<ChatMessage> = page
                .messages
                .into_iter()
                .filter(|message| message.sequence < cursor)
                .collect();
            if below.is_empty() {
                if window == 0 {
                    reached_start = true;
                    break;
                }
                cursor = window;
                continue;
            }
            let oldest = below
                .iter()
                .map(|message| message.sequence)
                .min()
                .unwrap_or(window);
            collected.extend(below);
            cursor = oldest;
            if window == 0 {
                reached_start = true;
            }
            if collected.len() >= ROOM_PAGE_SIZE as usize {
                break;
            }
        }

        collected.sort_by(model::compare_messages);
        let overflow = collected.len().saturating_sub(ROOM_PAGE_SIZE as usize);
        if overflow > 0 {
            collected.drain(0..overflow);
        }
        self.has_older = !reached_start || overflow > 0;

        let before_len = self.messages.len();
        self.messages = model::merge_messages(&self.messages, &collected);
        if let Some(head) = model::head_cursor(&self.messages) {
            self.cursor = Some(self.cursor.map_or(head, |cursor| cursor.max(head)));
        }
        self.notice = None;
        Ok(self.messages.len().saturating_sub(before_len))
    }

    /// Read the open room's members. This is the presence the pool exposes.
    pub fn refresh_members(&mut self) -> Result<(), ChatError> {
        let room = self.selected.clone().ok_or(ChatError::NoRoomOpen)?;
        match self.transport.members(&room) {
            Ok(members) => {
                self.members = members;
                self.notice = None;
                Ok(())
            }
            Err(error) => {
                self.notice = Some(error.to_string());
                Err(error)
            }
        }
    }

    /// Bind a local session to the open room.
    ///
    /// The session has to be listed by the pool as a member, so "a session can
    /// join a room" is the pool's membership fact, not a client guess.
    pub fn bind_session(&mut self, session: &str) -> Result<(), ChatError> {
        let room = self.selected.clone().ok_or(ChatError::NoRoomOpen)?;
        let session = session.trim();
        if session.is_empty() || !self.members.iter().any(|member| member.is_session()) {
            return Err(ChatError::NotAMember {
                session: session.to_owned(),
                room,
            });
        }
        if !self
            .members
            .iter()
            .any(|member| member.principal.is_session(session))
        {
            return Err(ChatError::NotAMember {
                session: session.to_owned(),
                room,
            });
        }
        self.bindings.insert(session.to_owned(), room);
        Ok(())
    }

    /// Stop treating a session as a participant of the open room.
    pub fn unbind_session(&mut self, session: &str) -> Option<String> {
        self.bindings.remove(session)
    }

    /// Send one message as the signed-in person.
    pub fn post(&mut self, body: &str, reply_to: Option<&str>) -> Result<ChatMessage, ChatError> {
        let room = self.selected.clone().ok_or(ChatError::NoRoomOpen)?;
        model::validate_body(body).map_err(|detail| ChatError::InvalidBody { detail })?;
        let message = match self.transport.send(&room, body, reply_to) {
            Ok(message) => message,
            Err(error) => {
                self.notice = Some(error.to_string());
                return Err(error);
            }
        };
        if message.is_from_session() {
            let error = ChatError::AuthorMismatch {
                expected: AuthorKind::User.label().to_owned(),
                actual: message.author_descriptor().accessible_label,
            };
            self.notice = Some(error.to_string());
            return Err(error);
        }
        self.merge_sent(message.clone());
        Ok(message)
    }

    /// Post a session's reply into the room it is bound to.
    ///
    /// A reply the pool authors as anyone but that session is refused and not
    /// merged: an agent's words are never shown under a person's name, and a
    /// different session's words are not adopted either.
    pub fn session_reply(
        &mut self,
        session: &str,
        body: &str,
        reply_to: Option<&str>,
    ) -> Result<ChatMessage, ChatError> {
        let room = self.selected.clone().ok_or(ChatError::NoRoomOpen)?;
        if self.bindings.get(session) != Some(&room) {
            return Err(ChatError::NotAMember {
                session: session.to_owned(),
                room,
            });
        }
        model::validate_body(body).map_err(|detail| ChatError::InvalidBody { detail })?;
        let message = match self
            .transport
            .send_as_session(&room, session, body, reply_to)
        {
            Ok(message) => message,
            Err(error) => {
                self.notice = Some(error.to_string());
                return Err(error);
            }
        };
        let authored_by_session = message
            .author
            .as_ref()
            .is_some_and(|author| author.is_session(session));
        if !authored_by_session {
            let error = ChatError::AuthorMismatch {
                expected: format!("{} {session}", AuthorKind::Session.label()),
                actual: message.author_descriptor().accessible_label,
            };
            self.notice = Some(error.to_string());
            return Err(error);
        }
        self.merge_sent(message.clone());
        Ok(message)
    }

    /// Apply one frame from the room's live stream.
    pub fn handle_frame(&mut self, frame: &serde_json::Value) -> FrameOutcome {
        match frame.get("type").and_then(serde_json::Value::as_str) {
            Some("chat.history") => {
                if let Some(messages) = frame.get("messages")
                    && let Ok(messages) =
                        serde_json::from_value::<Vec<ChatMessage>>(messages.clone())
                {
                    self.merge_messages(&messages);
                }
                let catch_up = frame
                    .get("catchUp")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true);
                if catch_up && self.currentness == Currentness::Current {
                    self.currentness = Currentness::CatchingUp;
                }
                FrameOutcome::History
            }
            Some("chat.message") => {
                if let Some(message) = frame.get("message")
                    && let Ok(message) = serde_json::from_value::<ChatMessage>(message.clone())
                {
                    self.merge_messages(std::slice::from_ref(&message));
                }
                FrameOutcome::Message
            }
            Some("chat.current") => {
                if let Some(cursor) = frame.get("cursor").and_then(serde_json::Value::as_u64) {
                    self.cursor = Some(self.cursor.map_or(cursor, |held| held.max(cursor)));
                }
                self.currentness = Currentness::Current;
                self.notice = None;
                FrameOutcome::Current
            }
            Some("chat.presence") => {
                if let Ok(watchers) = serde_json::from_value::<Vec<String>>(
                    frame
                        .get("watchers")
                        .cloned()
                        .unwrap_or_else(|| serde_json::Value::Array(Vec::new())),
                ) {
                    self.watchers = watchers;
                }
                // Presence is not delivery: no message is added or removed.
                FrameOutcome::Presence
            }
            Some("chat.catchUp") => {
                if self.currentness != Currentness::Reconnecting {
                    self.currentness = Currentness::CatchingUp;
                }
                self.notice = Some(CATCH_UP_NOTICE.to_owned());
                FrameOutcome::CatchUp
            }
            Some("chat.error") => {
                let message = frame
                    .get("message")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("the room's live stream reported a problem");
                self.notice = Some(message.to_owned());
                FrameOutcome::Error
            }
            _ => FrameOutcome::Ignored,
        }
    }

    /// Record that the live stream dropped.
    ///
    /// Cached history stays on screen, but the room is labelled reconnecting
    /// until a read or `chat.current` confirms currentness.
    pub fn connection_lost(&mut self) {
        self.currentness = Currentness::Reconnecting;
    }

    /// Record that a reconnect attempt has started.
    pub fn reconnect_started(&mut self) {
        self.currentness = Currentness::Reconnecting;
    }

    /// Record that a history read reached the head.
    pub fn mark_current(&mut self) {
        self.currentness = Currentness::Current;
    }

    fn merge_page(&mut self, page: &ChatPage) {
        self.merge_messages(&page.messages);
        if let Some(cursor) = model::page_cursor(page) {
            self.cursor = Some(self.cursor.map_or(cursor, |held| held.max(cursor)));
        }
    }

    fn merge_messages(&mut self, incoming: &[ChatMessage]) {
        self.messages = model::merge_messages(&self.messages, incoming);
        if let Some(head) = model::head_cursor(incoming) {
            self.cursor = Some(self.cursor.map_or(head, |held| held.max(head)));
        }
    }

    fn merge_sent(&mut self, message: ChatMessage) {
        self.last_sent = Some(message.clone());
        self.merge_messages(std::slice::from_ref(&message));
        self.notice = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pool that lives in the test process and mints server ids.
    struct ScriptedPool {
        rooms: Vec<ChatRoom>,
        members: Vec<ChatMember>,
        messages: Vec<ChatMessage>,
        next: u64,
        misattribute_session: bool,
    }

    impl ScriptedPool {
        fn new(room_messages: Vec<ChatMessage>, members: Vec<ChatMember>) -> Self {
            Self {
                rooms: vec![ChatRoom {
                    id: "room-1".to_owned(),
                    name: "agents".to_owned(),
                    ..ChatRoom::default()
                }],
                members,
                messages: room_messages,
                next: 1_000,
                misattribute_session: false,
            }
        }

        fn store(&mut self, author: ChatAuthor, body: &str) -> ChatMessage {
            let sequence = self
                .messages
                .iter()
                .map(|message| message.sequence)
                .max()
                .unwrap_or(0)
                + 1;
            let message = ChatMessage {
                id: format!("server-{}", self.next),
                author: Some(author),
                body: body.to_owned(),
                project: "project-1".to_owned(),
                sequence,
                ..ChatMessage::default()
            };
            self.next += 1;
            self.messages.push(message.clone());
            message
        }
    }

    impl ChatTransport for ScriptedPool {
        fn rooms(&mut self) -> Result<Vec<ChatRoom>, ChatError> {
            Ok(self.rooms.clone())
        }

        fn members(&mut self, _room: &str) -> Result<Vec<ChatMember>, ChatError> {
            Ok(self.members.clone())
        }

        fn history(
            &mut self,
            _room: &str,
            after: Option<u64>,
            limit: u32,
        ) -> Result<ChatPage, ChatError> {
            let after = after.unwrap_or(0);
            let mut page: Vec<ChatMessage> = self
                .messages
                .iter()
                .filter(|message| message.sequence > after)
                .cloned()
                .collect();
            page.sort_by(model::compare_messages);
            let has_more = page.len() > limit as usize;
            page.truncate(limit as usize);
            let next = has_more
                .then(|| page.last().map(|message| message.sequence))
                .flatten();
            Ok(ChatPage {
                messages: page,
                next,
            })
        }

        fn send(
            &mut self,
            _room: &str,
            body: &str,
            _reply_to: Option<&str>,
        ) -> Result<ChatMessage, ChatError> {
            Ok(self.store(
                ChatAuthor {
                    kind: "user".to_owned(),
                    id: "user-1".to_owned(),
                },
                body,
            ))
        }

        fn send_as_session(
            &mut self,
            _room: &str,
            session: &str,
            body: &str,
            _reply_to: Option<&str>,
        ) -> Result<ChatMessage, ChatError> {
            let author = if self.misattribute_session {
                ChatAuthor {
                    kind: "user".to_owned(),
                    id: "user-1".to_owned(),
                }
            } else {
                ChatAuthor {
                    kind: "session".to_owned(),
                    id: session.to_owned(),
                }
            };
            Ok(self.store(author, body))
        }
    }

    fn history_message(sequence: u64) -> ChatMessage {
        ChatMessage {
            id: format!("m-{sequence}"),
            author: Some(ChatAuthor {
                kind: "user".to_owned(),
                id: "user-1".to_owned(),
            }),
            body: format!("message {sequence}"),
            sequence,
            ..ChatMessage::default()
        }
    }

    fn members() -> Vec<ChatMember> {
        vec![
            ChatMember {
                id: "member-1".to_owned(),
                principal: ChatAuthor {
                    kind: "user".to_owned(),
                    id: "user-1".to_owned(),
                },
                role: "owner".to_owned(),
                ..ChatMember::default()
            },
            ChatMember {
                id: "member-2".to_owned(),
                principal: ChatAuthor {
                    kind: "session".to_owned(),
                    id: "sess-1".to_owned(),
                },
                role: "member".to_owned(),
                ..ChatMember::default()
            },
        ]
    }

    fn controller(messages: Vec<ChatMessage>) -> ChatController<ScriptedPool> {
        ChatController::new(ScriptedPool::new(messages, members()))
    }

    #[test]
    fn opening_a_room_pages_the_cursor_to_the_head_and_is_current() {
        let messages: Vec<ChatMessage> = (1..=120).map(history_message).collect();
        let mut chat = controller(messages);
        chat.refresh_rooms().expect("rooms");
        assert_eq!(chat.rooms().len(), 1);

        chat.open_room("room-1").expect("open");
        assert_eq!(chat.messages().len(), 120);
        assert_eq!(chat.messages().first().map(|m| m.sequence), Some(1));
        assert_eq!(chat.messages().last().map(|m| m.sequence), Some(120));
        assert_eq!(chat.currentness(), Currentness::Current);
        assert!(chat.snapshot().can_post);
    }

    #[test]
    fn a_live_message_appends_and_keeps_its_server_id() {
        let mut chat = controller(vec![history_message(1)]);
        chat.open_room("room-1").expect("open");

        let frame = serde_json::json!({
            "type": "chat.message",
            "catchUp": false,
            "message": {
                "id": "server-777",
                "author": {"kind": "session", "id": "sess-1"},
                "body": "I am on it",
                "project": "project-1",
                "replyTo": null,
                "createdAt": "2026-10-29T00:00:00Z",
                "sequence": 2
            }
        });
        assert_eq!(chat.handle_frame(&frame), FrameOutcome::Message);
        assert_eq!(chat.messages().len(), 2);
        assert_eq!(chat.messages()[1].id, "server-777");
        assert_eq!(chat.messages()[1].sequence, 2);
        assert!(chat.messages()[1].is_from_session());
    }

    #[test]
    fn older_pages_walk_back_without_duplicating_the_window() {
        let messages: Vec<ChatMessage> = (1..=260).map(history_message).collect();
        let mut chat = controller(messages);
        chat.open_room("room-1").expect("open");
        assert_eq!(chat.messages().len(), ROOM_WINDOW);
        assert!(chat.has_older());
        assert_eq!(chat.messages()[0].sequence, 61);

        let added = chat.load_older().expect("older");
        assert_eq!(added, 50);
        assert_eq!(chat.messages().len(), 250);
        assert!(chat.has_older());

        let more = chat.load_older().expect("older again");
        assert_eq!(more, 10);
        assert_eq!(chat.messages().len(), 260);
        assert!(!chat.has_older());

        let ids: std::collections::BTreeSet<&str> = chat
            .messages()
            .iter()
            .map(|message| message.id.as_str())
            .collect();
        assert_eq!(ids.len(), 260);
    }

    #[test]
    fn a_reconnect_is_catch_up_until_currentness_is_confirmed() {
        let mut chat = controller(vec![history_message(1)]);
        chat.open_room("room-1").expect("open");
        assert_eq!(chat.currentness(), Currentness::Current);

        chat.connection_lost();
        assert_eq!(chat.currentness(), Currentness::Reconnecting);
        assert!(chat.banner().expect("banner").contains("Reconnecting"));
        assert!(!chat.currentness().is_current());

        // Cached history arrives first and is still labelled catch-up.
        let history = serde_json::json!({
            "type": "chat.history",
            "catchUp": true,
            "messages": [{
                "id": "m-2",
                "author": {"kind": "user", "id": "user-1"},
                "body": "while you were away",
                "sequence": 2
            }]
        });
        chat.handle_frame(&history);
        assert!(!chat.currentness().is_current());
        assert!(chat.banner().expect("banner").contains("Reconnecting"));

        // Only the server's current frame confirms it.
        let current = serde_json::json!({"type": "chat.current", "catchUp": false, "cursor": 2});
        chat.handle_frame(&current);
        assert_eq!(chat.currentness(), Currentness::Current);
        assert!(chat.banner().expect("banner").contains("Current"));
        assert_eq!(chat.messages().len(), 2);
    }

    #[test]
    fn a_catch_up_frame_does_not_claim_currentness() {
        let mut chat = controller(vec![history_message(1)]);
        chat.open_room("room-1").expect("open");
        chat.mark_current();
        let frame = serde_json::json!({"type": "chat.catchUp", "catchUp": true});
        assert_eq!(chat.handle_frame(&frame), FrameOutcome::CatchUp);
        assert_eq!(chat.currentness(), Currentness::CatchingUp);
    }

    #[test]
    fn presence_changes_never_remove_a_message() {
        let mut chat = controller(vec![history_message(1), history_message(2)]);
        chat.open_room("room-1").expect("open");
        let before = chat.messages().to_vec();

        let frame = serde_json::json!({
            "type": "chat.presence",
            "catchUp": false,
            "watchers": ["user-1", "user-2"]
        });
        assert_eq!(chat.handle_frame(&frame), FrameOutcome::Presence);
        assert_eq!(chat.messages(), before.as_slice());
        assert_eq!(chat.watchers(), ["user-1".to_owned(), "user-2".to_owned()]);
    }

    #[test]
    fn an_end_to_end_room_conversation_with_a_session() {
        let mut chat = controller(vec![history_message(1)]);
        chat.open_room("room-1").expect("open");
        chat.refresh_members().expect("members");
        assert_eq!(chat.session_members(), vec!["sess-1".to_owned()]);
        chat.bind_session("sess-1").expect("bind");
        assert_eq!(chat.bindings().get("sess-1"), Some(&"room-1".to_owned()));

        let asked = chat
            .post("Can you take the paging bug?", None)
            .expect("post");
        assert!(!asked.is_from_session());
        assert_eq!(asked.author_descriptor().label(), "Person");

        // The session's side of the same room sees the person's message with
        // the id the pool minted, so the two sides share one transcript.
        let session_side = chat
            .transport
            .messages
            .iter()
            .find(|message| message.id == asked.id)
            .expect("the session sees the person's message");
        assert_eq!(session_side.body, "Can you take the paging bug?");

        let reply = chat
            .session_reply("sess-1", "I will take it", None)
            .expect("session reply");
        assert_eq!(reply.author_descriptor().label(), "Agent session");
        assert!(reply.is_from_session());
        assert_eq!(
            reply.author.as_ref().map(|author| author.id.as_str()),
            Some("sess-1")
        );
        assert_eq!(chat.messages().len(), 3);
        assert_eq!(chat.messages()[2].id, reply.id);
    }

    #[test]
    fn a_session_reply_authored_as_a_person_is_refused() {
        let mut chat = controller(vec![history_message(1)]);
        chat.open_room("room-1").expect("open");
        chat.refresh_members().expect("members");
        chat.bind_session("sess-1").expect("bind");
        // Reach the scripted pool's misattribution flag through the controller.
        chat.transport.misattribute_session = true;

        let error = chat
            .session_reply("sess-1", "I will take it", None)
            .expect_err("refused");
        assert!(matches!(error, ChatError::AuthorMismatch { .. }));
        assert_eq!(chat.messages().len(), 1);
    }

    #[test]
    fn only_a_listed_session_member_can_be_bound() {
        let mut chat = controller(vec![history_message(1)]);
        chat.open_room("room-1").expect("open");
        chat.refresh_members().expect("members");
        assert!(matches!(
            chat.bind_session("ghost"),
            Err(ChatError::NotAMember { .. })
        ));
        assert!(chat.bindings().is_empty());
    }

    #[test]
    fn an_unbound_session_cannot_post_into_the_room() {
        let mut chat = controller(Vec::new());
        chat.open_room("room-1").expect("open");
        assert!(matches!(
            chat.session_reply("sess-1", "hello", None),
            Err(ChatError::NotAMember { .. })
        ));
    }

    #[test]
    fn an_empty_body_never_reaches_the_transport() {
        let mut chat = controller(Vec::new());
        chat.open_room("room-1").expect("open");
        let error = chat.post("   ", None).expect_err("refused");
        assert!(matches!(error, ChatError::InvalidBody { .. }));
        assert!(chat.messages().is_empty());
    }

    #[test]
    fn a_sent_message_is_merged_under_the_server_id() {
        let mut chat = controller(Vec::new());
        chat.open_room("room-1").expect("open");
        let sent = chat.post("hello", None).expect("post");
        let held = chat.last_sent().expect("last sent");
        assert_eq!(held.id, sent.id);
        assert_eq!(chat.messages().len(), 1);
        // The same live echo is a duplicate, not a second row.
        let frame = serde_json::json!({
            "type": "chat.message",
            "catchUp": false,
            "message": sent
        });
        chat.handle_frame(&frame);
        assert_eq!(chat.messages().len(), 1);
    }
}
