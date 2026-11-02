//! The chatroom model: wire types, author descriptors and message merging.
//!
//! Everything here is pure. A history page, a live `chat.message` frame and a
//! reconnect replay may carry the same message, so [`merge_messages`] keys on
//! the pool's immutable message id and orders by the pool's sequence, never by
//! a timestamp.
//!
//! # A person and a session are distinguishable without colour
//!
//! The pool tells the two apart only through [`ChatAuthor::kind`].
//! [`ChatAuthor::descriptor`] turns that into a word (`Person` or
//! `Agent session`), a glyph of a different shape (`●` or `◆`) and an
//! accessible label. A reader with the palette removed, or who cannot see the
//! difference between two hues, still knows who wrote a message.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// How many messages one history read asks for.
pub const ROOM_PAGE_SIZE: u32 = 50;
/// How many cursor steps one backfill or older-page walk may take.
pub const MAX_BACKFILL_STEPS: usize = 100;
/// The largest body the pool accepts, in bytes (it refuses anything longer).
pub const MAX_BODY_BYTES: usize = 1_000_000;

/// The facts about an author that a surface draws.
///
/// The kind, the word and the glyph all differ between a person and a session,
/// so colour is never the only signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuthorKind {
    /// A person.
    User,
    /// An agent session.
    Session,
    /// A deleted or unrecognised principal; never guessed as either.
    Unknown,
}

impl AuthorKind {
    /// The kind the pool's `kind` field names.
    pub fn parse(value: &str) -> Self {
        match value {
            "user" => Self::User,
            "session" => Self::Session,
            _ => Self::Unknown,
        }
    }

    /// The word a person reads.
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "Person",
            Self::Session => "Agent session",
            Self::Unknown => "Unknown author",
        }
    }

    /// A glyph that differs in shape, not only in colour.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::User => "\u{25cf}",
            Self::Session => "\u{25c6}",
            Self::Unknown => "?",
        }
    }

    /// The lowercase tag used for tests and styling hooks.
    pub fn tag(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Session => "session",
            Self::Unknown => "unknown",
        }
    }
}

/// One room member or message author, as the pool serialises it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatAuthor {
    /// `user`, `session`, or a kind this build does not know.
    #[serde(default)]
    pub kind: String,
    /// The principal's public id.
    #[serde(default)]
    pub id: String,
}

impl ChatAuthor {
    /// The parsed kind.
    pub fn author_kind(&self) -> AuthorKind {
        AuthorKind::parse(&self.kind)
    }

    /// Whether this author is the named session.
    pub fn is_session(&self, session: &str) -> bool {
        self.author_kind() == AuthorKind::Session && self.id == session
    }

    /// Whether this author is an agent session, whoever it is.
    pub fn is_any_session(&self) -> bool {
        self.author_kind() == AuthorKind::Session
    }

    /// How to draw this author, by more than colour.
    pub fn descriptor(&self) -> AuthorDescriptor {
        let kind = self.author_kind();
        let id = self.id.trim();
        AuthorDescriptor {
            kind,
            id: (!id.is_empty()).then(|| id.to_owned()),
            accessible_label: if id.is_empty() {
                kind.label().to_owned()
            } else {
                format!("{} {id}", kind.label())
            },
        }
    }
}

/// One author, reduced to the facts a row and a member list show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorDescriptor {
    /// The author kind.
    pub kind: AuthorKind,
    /// The principal id, absent when the author was deleted.
    pub id: Option<String>,
    /// One sentence that names the kind and the id.
    pub accessible_label: String,
}

impl AuthorDescriptor {
    /// The descriptor for a deleted or missing author.
    pub fn unknown() -> Self {
        Self {
            kind: AuthorKind::Unknown,
            id: None,
            accessible_label: AuthorKind::Unknown.label().to_owned(),
        }
    }

    /// The kind's word.
    pub fn label(&self) -> &'static str {
        self.kind.label()
    }

    /// The kind's shape.
    pub fn glyph(&self) -> &'static str {
        self.kind.glyph()
    }

    /// The kind's lowercase tag.
    pub fn tag(&self) -> &'static str {
        self.kind.tag()
    }
}

/// One message, exactly as the pool returns it.
///
/// The id and sequence are the pool's. A client keeps both: the id makes a
/// message unique across a page and a live frame, and the sequence orders it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    /// Immutable public id; the merge key.
    pub id: String,
    /// The author, or `None` when the author was deleted.
    #[serde(default)]
    pub author: Option<ChatAuthor>,
    /// Markdown body.
    #[serde(default)]
    pub body: String,
    /// Public id of the room's project.
    #[serde(default)]
    pub project: String,
    /// Public id of the message this replies to, when there is one.
    #[serde(default)]
    pub reply_to: Option<String>,
    /// When the pool stored the message.
    #[serde(default)]
    pub created_at: String,
    /// The pool's ordering key.
    #[serde(default)]
    pub sequence: u64,
}

impl ChatMessage {
    /// How the author is drawn.
    pub fn author_descriptor(&self) -> AuthorDescriptor {
        self.author
            .as_ref()
            .map(ChatAuthor::descriptor)
            .unwrap_or_else(AuthorDescriptor::unknown)
    }

    /// Whether this message was written by an agent session.
    pub fn is_from_session(&self) -> bool {
        self.author.as_ref().is_some_and(ChatAuthor::is_any_session)
    }
}

/// One room, as `GET /v1/chatrooms` returns it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRoom {
    /// Immutable public id.
    pub id: String,
    /// Room name.
    #[serde(default)]
    pub name: String,
    /// `private`, `unlisted` or `public`.
    #[serde(default)]
    pub visibility: String,
    /// Pool the room was federated from, when it was.
    #[serde(default)]
    pub origin_pool: Option<String>,
    /// When the room was created.
    #[serde(default)]
    pub created_at: String,
    /// When the room was last changed.
    #[serde(default)]
    pub updated_at: String,
}

/// One room member, as `GET /v1/chatrooms/{room}/members` returns it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMember {
    /// The membership's public id.
    pub id: String,
    /// The member principal.
    #[serde(default)]
    pub principal: ChatAuthor,
    /// `owner`, `member` or `viewer`.
    #[serde(default)]
    pub role: String,
    /// When the principal joined.
    #[serde(default)]
    pub joined_at: String,
}

impl ChatMember {
    /// How the member is drawn.
    pub fn descriptor(&self) -> AuthorDescriptor {
        self.principal.descriptor()
    }

    /// Whether this member is an agent session.
    pub fn is_session(&self) -> bool {
        self.principal.is_any_session()
    }
}

/// One page of history, as `GET .../messages` returns it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatPage {
    /// Messages in sequence order.
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
    /// The next `after` cursor; absent when the page is the last.
    #[serde(default)]
    pub next: Option<u64>,
}

/// Order two messages by the pool's sequence, then by id.
///
/// Timestamps are never compared: the sequence is the pool's own order and the
/// id is a deterministic tie-break.
pub fn compare_messages(left: &ChatMessage, right: &ChatMessage) -> Ordering {
    left.sequence
        .cmp(&right.sequence)
        .then_with(|| left.id.cmp(&right.id))
}

/// Merge messages into one list, keyed by the pool's id.
///
/// A message that arrives twice (a page overlapping a live frame, a reconnect
/// replay, an older page that touches the window) appears once. The newer copy
/// wins, because the pool is the source of truth for a body.
pub fn merge_messages(existing: &[ChatMessage], incoming: &[ChatMessage]) -> Vec<ChatMessage> {
    let mut merged = existing.to_vec();
    for message in incoming {
        match merged.iter_mut().find(|held| held.id == message.id) {
            Some(held) => *held = message.clone(),
            None => merged.push(message.clone()),
        }
    }
    merged.sort_by(compare_messages);
    merged
}

/// The highest sequence held, which is the cursor for "newer".
pub fn head_cursor(messages: &[ChatMessage]) -> Option<u64> {
    messages.iter().map(|message| message.sequence).max()
}

/// The lowest sequence held, which is the cursor for "older".
pub fn tail_cursor(messages: &[ChatMessage]) -> Option<u64> {
    messages.iter().map(|message| message.sequence).min()
}

/// The next `after` cursor from a page, keeping the highest of the page's own
/// cursor and its messages.
pub fn page_cursor(page: &ChatPage) -> Option<u64> {
    let head = head_cursor(&page.messages);
    match (page.next, head) {
        (Some(next), Some(head)) => Some(next.max(head)),
        (Some(next), None) => Some(next),
        (None, head) => head,
    }
}

/// Validate a message body before a request is made.
///
/// The pool refuses an empty or oversized body; refusing here keeps a
/// rejected message from replacing the composer's text with an error.
pub fn validate_body(body: &str) -> Result<(), String> {
    if body.trim().is_empty() {
        return Err("a message needs some words".to_owned());
    }
    if body.len() > MAX_BODY_BYTES {
        return Err(format!("a message must be at most {MAX_BODY_BYTES} bytes"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(id: &str, sequence: u64, body: &str) -> ChatMessage {
        ChatMessage {
            id: id.to_owned(),
            body: body.to_owned(),
            sequence,
            ..ChatMessage::default()
        }
    }

    #[test]
    fn a_person_and_a_session_have_different_words_glyphs_and_tags() {
        let person = ChatAuthor {
            kind: "user".to_owned(),
            id: "user-1".to_owned(),
        }
        .descriptor();
        let session = ChatAuthor {
            kind: "session".to_owned(),
            id: "sess-1".to_owned(),
        }
        .descriptor();

        assert_eq!(person.label(), "Person");
        assert_eq!(session.label(), "Agent session");
        assert_ne!(person.label(), session.label());
        assert_ne!(person.glyph(), session.glyph());
        assert_ne!(person.tag(), session.tag());
        assert!(person.accessible_label.contains("Person"));
        assert!(session.accessible_label.contains("Agent session"));
        assert!(session.accessible_label.contains("sess-1"));
    }

    #[test]
    fn an_unknown_author_is_never_guessed_as_a_person_or_a_session() {
        let unknown = ChatMessage::default().author_descriptor();
        assert_eq!(unknown.kind, AuthorKind::Unknown);
        assert_eq!(unknown.label(), "Unknown author");
        assert_ne!(unknown.label(), AuthorKind::User.label());
        assert_ne!(unknown.label(), AuthorKind::Session.label());

        let other = ChatAuthor {
            kind: "service".to_owned(),
            id: "svc-1".to_owned(),
        }
        .descriptor();
        assert_eq!(other.kind, AuthorKind::Unknown);
    }

    #[test]
    fn merging_keys_on_the_pool_id_and_orders_by_sequence() {
        let held = vec![message("m-2", 2, "second"), message("m-1", 1, "first")];
        let incoming = vec![
            message("m-1", 1, "first, edited"),
            message("m-3", 3, "third"),
        ];
        let merged = merge_messages(&held, &incoming);

        assert_eq!(
            merged.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["m-1", "m-2", "m-3"]
        );
        assert_eq!(merged[0].body, "first, edited");
        assert_eq!(merged.len(), 3);
    }

    #[test]
    fn merging_the_same_live_frame_twice_adds_one_row() {
        let live = vec![message("server-12", 12, "live")];
        let once = merge_messages(&[], &live);
        let twice = merge_messages(&once, &live);
        assert_eq!(twice.len(), 1);
        assert_eq!(twice[0].id, "server-12");
        assert_eq!(twice[0].sequence, 12);
    }

    #[test]
    fn cursors_are_the_highest_and_lowest_sequences() {
        let messages = vec![message("m-2", 2, "b"), message("m-10", 10, "c")];
        assert_eq!(head_cursor(&messages), Some(10));
        assert_eq!(tail_cursor(&messages), Some(2));
        assert_eq!(head_cursor(&[]), None);
        assert_eq!(tail_cursor(&[]), None);
    }

    #[test]
    fn a_page_cursor_keeps_the_newest_of_the_two() {
        let page = ChatPage {
            messages: vec![message("m-7", 7, "a")],
            next: Some(9),
        };
        assert_eq!(page_cursor(&page), Some(9));
        let last = ChatPage {
            messages: vec![message("m-7", 7, "a")],
            next: None,
        };
        assert_eq!(page_cursor(&last), Some(7));
    }

    #[test]
    fn a_body_is_validated_before_it_is_sent() {
        assert!(validate_body("hello").is_ok());
        assert!(validate_body("   ").is_err());
        assert!(validate_body(&"a".repeat(MAX_BODY_BYTES + 1)).is_err());
    }
}
