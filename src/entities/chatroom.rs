//! `Chatroom` entities — rooms, members and messages (task 1.5.1).
//!
//! See `migrations/20261022000001_create_chatrooms.up.sql`. A room belongs to
//! one project and carries a name, a visibility and an optional pool of
//! origin. `origin_pool` stays unset for every v1 room: federation (Rhumb) is
//! deferred to v3, and the column exists now so federation needs no data
//! migration later.
//!
//! A member and a message author are each exactly one principal — a pool user
//! or an agent session. The database enforces it with a `CHECK` constraint;
//! [`Principal`] is the in-memory form of the same rule, so a value that would
//! violate the constraint cannot be built by accident.
//!
//! Messages are ordered by their internal `id`, never by `public_id` or by
//! `created_at`. MySQL never reuses an `AUTO_INCREMENT` value, so the ordering
//! key is stable and monotonic even when two messages share a timestamp.
//!
//! This module has no database access; [`crate::services::chatrooms`] owns the
//! queries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;

/// Maximum characters in a room name.
pub const MAX_ROOM_NAME_CHARS: usize = 120;
/// Maximum bytes in a message body.
pub const MAX_MESSAGE_BODY_BYTES: usize = 1_000_000;
/// Maximum characters in a pool-of-origin label.
pub const MAX_ORIGIN_POOL_CHARS: usize = 255;

/// Who may read a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomVisibility {
    /// Members only.
    Private,
    /// Members read it; it is absent from room listings.
    Unlisted,
    /// Any member of the project may read it.
    Public,
}

impl RoomVisibility {
    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RoomVisibility::Private => "private",
            RoomVisibility::Unlisted => "unlisted",
            RoomVisibility::Public => "public",
        }
    }

    /// Parse a database spelling.
    pub fn parse(value: &str) -> Result<Self, ChatroomError> {
        match value {
            "private" => Ok(RoomVisibility::Private),
            "unlisted" => Ok(RoomVisibility::Unlisted),
            "public" => Ok(RoomVisibility::Public),
            other => Err(ChatroomError::Invalid(format!(
                "unknown room visibility: {other:?}"
            ))),
        }
    }
}

/// What kind of principal a member or author is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    /// A pool user (a person).
    User,
    /// An agent session.
    Session,
}

impl PrincipalKind {
    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            PrincipalKind::User => "user",
            PrincipalKind::Session => "session",
        }
    }

    /// Parse a database spelling.
    pub fn parse(value: &str) -> Result<Self, ChatroomError> {
        match value {
            "user" => Ok(PrincipalKind::User),
            "session" => Ok(PrincipalKind::Session),
            other => Err(ChatroomError::Invalid(format!(
                "unknown principal kind: {other:?}"
            ))),
        }
    }
}

/// A member's permissions in one room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberRole {
    /// Created the room; may change it.
    Owner,
    /// Reads and writes.
    Member,
    /// Reads only.
    Viewer,
}

impl MemberRole {
    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            MemberRole::Owner => "owner",
            MemberRole::Member => "member",
            MemberRole::Viewer => "viewer",
        }
    }

    /// Parse a database spelling.
    pub fn parse(value: &str) -> Result<Self, ChatroomError> {
        match value {
            "owner" => Ok(MemberRole::Owner),
            "member" => Ok(MemberRole::Member),
            "viewer" => Ok(MemberRole::Viewer),
            other => Err(ChatroomError::Invalid(format!(
                "unknown member role: {other:?}"
            ))),
        }
    }

    /// True when this role may write messages.
    pub fn can_write(self) -> bool {
        matches!(self, MemberRole::Owner | MemberRole::Member)
    }
}

/// The single principal behind a member row or a message author.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Principal {
    /// A pool user's `public_id`.
    User(String),
    /// An agent session id.
    Session(String),
}

impl Principal {
    /// Kind of this principal.
    pub fn kind(&self) -> PrincipalKind {
        match self {
            Principal::User(_) => PrincipalKind::User,
            Principal::Session(_) => PrincipalKind::Session,
        }
    }

    /// The principal's identifier.
    pub fn id(&self) -> &str {
        match self {
            Principal::User(id) | Principal::Session(id) => id,
        }
    }

    /// Validate the identifier that came from a caller.
    pub fn validate(&self) -> Result<(), ChatroomError> {
        let id = self.id().trim();
        if id.is_empty() {
            return Err(ChatroomError::Invalid(
                "principal id is required".to_string(),
            ));
        }
        if id.len() > 255 {
            return Err(ChatroomError::Invalid(
                "principal id must be at most 255 characters".to_string(),
            ));
        }
        Ok(())
    }
}

/// Full room row, mirroring `chat_rooms`.
#[derive(Debug, Clone, FromRow)]
pub struct Room {
    /// Internal primary key.
    pub id: u64,
    /// Immutable public UUID.
    pub public_id: String,
    /// Internal organization id.
    pub organization_id: u64,
    /// Internal project id.
    pub project_id: u64,
    /// Room name, unique within the project.
    pub name: String,
    /// Who may read the room.
    pub visibility: String,
    /// Pool the room was federated from; unset for every v1 room.
    pub origin_pool: Option<String>,
    /// When the room was created.
    pub created_at: DateTime<Utc>,
    /// When the room last changed.
    pub updated_at: DateTime<Utc>,
}

impl Room {
    /// Parsed visibility, or an error when the row is not one this build knows.
    pub fn visibility(&self) -> Result<RoomVisibility, ChatroomError> {
        RoomVisibility::parse(&self.visibility)
    }
}

/// Wire projection of a room; omits internal ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomView {
    /// Immutable public UUID.
    pub id: String,
    /// Room name.
    pub name: String,
    /// Who may read the room.
    pub visibility: RoomVisibility,
    /// Pool the room was federated from; `null` for v1 rooms.
    pub origin_pool: Option<String>,
    /// When the room was created.
    pub created_at: DateTime<Utc>,
    /// When the room last changed.
    pub updated_at: DateTime<Utc>,
}

impl RoomView {
    /// Build a projection, refusing an unknown visibility spelling.
    pub fn from_room(room: &Room) -> Result<Self, ChatroomError> {
        Ok(Self {
            id: room.public_id.clone(),
            name: room.name.clone(),
            visibility: room.visibility()?,
            origin_pool: room.origin_pool.clone(),
            created_at: room.created_at,
            updated_at: room.updated_at,
        })
    }
}

/// Full member row, mirroring `chat_room_members`.
#[derive(Debug, Clone, FromRow)]
pub struct RoomMember {
    /// Internal primary key.
    pub id: u64,
    /// Immutable public UUID.
    pub public_id: String,
    /// Internal room id.
    pub room_id: u64,
    /// `user` or `session`.
    pub member_kind: String,
    /// Pool user id when the member is a person.
    pub user_id: Option<u64>,
    /// Agent session id when the member is a session.
    pub session_id: Option<String>,
    /// Owner, member or viewer.
    pub role: String,
    /// When the principal joined.
    pub joined_at: DateTime<Utc>,
}

impl RoomMember {
    /// Parsed role.
    pub fn role(&self) -> Result<MemberRole, ChatroomError> {
        MemberRole::parse(&self.role)
    }
}

/// Wire projection of a member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberView {
    /// Immutable public UUID of the membership.
    pub id: String,
    /// The member principal.
    pub principal: Principal,
    /// The member's role.
    pub role: MemberRole,
    /// When the principal joined.
    pub joined_at: DateTime<Utc>,
}

/// Full message row, mirroring `chat_messages`.
#[derive(Debug, Clone, FromRow)]
pub struct Message {
    /// Internal ordering key; stable and never reused.
    pub id: u64,
    /// Immutable public UUID.
    pub public_id: String,
    /// Internal room id.
    pub room_id: u64,
    /// `user` or `session`.
    pub author_kind: String,
    /// Pool user id when a person wrote it.
    pub author_user_id: Option<u64>,
    /// Agent session id when a session wrote it.
    pub author_session_id: Option<String>,
    /// Markdown body.
    pub body: String,
    /// Internal id of the message this replies to.
    pub reply_to_message_id: Option<u64>,
    /// When the message was created.
    pub created_at: DateTime<Utc>,
}

/// Wire projection of a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageView {
    /// Immutable public UUID.
    pub id: String,
    /// The author principal, or `None` when the author was deleted. The serde
    /// tag (`user` or `session`) is how a reader tells a person from an agent.
    pub author: Option<Principal>,
    /// Markdown body.
    pub body: String,
    /// Public UUID of the project the room belongs to, so a session message
    /// carries the project it was posted in.
    pub project: String,
    /// Public UUID of the message this replies to, when there is one.
    pub reply_to: Option<String>,
    /// When the message was created.
    pub created_at: DateTime<Utc>,
    /// Server-defined ordering key; compare these, not timestamps.
    pub sequence: u64,
}

/// Domain errors for chatroom values.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChatroomError {
    /// A value does not satisfy the model.
    #[error("invalid chatroom value")]
    Invalid(String),
}

/// Validate a room name from a caller.
pub fn validate_room_name(name: &str) -> Result<String, ChatroomError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(ChatroomError::Invalid("room name is required".into()));
    }
    if trimmed.chars().count() > MAX_ROOM_NAME_CHARS {
        return Err(ChatroomError::Invalid(format!(
            "room name must be at most {MAX_ROOM_NAME_CHARS} characters"
        )));
    }
    Ok(trimmed.to_string())
}

/// Validate an optional pool-of-origin label from a caller.
pub fn validate_origin_pool(origin_pool: Option<&str>) -> Result<Option<String>, ChatroomError> {
    match origin_pool.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some(value) if value.chars().count() > MAX_ORIGIN_POOL_CHARS => {
            Err(ChatroomError::Invalid(format!(
                "origin pool must be at most {MAX_ORIGIN_POOL_CHARS} characters"
            )))
        }
        Some(value) => Ok(Some(value.to_string())),
    }
}

/// Validate a message body from a caller.
pub fn validate_message_body(body: &str) -> Result<String, ChatroomError> {
    if body.trim().is_empty() {
        return Err(ChatroomError::Invalid("message body is required".into()));
    }
    if body.len() > MAX_MESSAGE_BODY_BYTES {
        return Err(ChatroomError::Invalid(format!(
            "message body must be at most {MAX_MESSAGE_BODY_BYTES} bytes"
        )));
    }
    Ok(body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> Room {
        Room {
            id: 1,
            public_id: "11111111-1111-4111-8111-111111111111".into(),
            organization_id: 2,
            project_id: 3,
            name: "builds".into(),
            visibility: "public".into(),
            origin_pool: None,
            created_at: "2026-10-22T00:00:00Z".parse().expect("ts"),
            updated_at: "2026-10-22T00:00:00Z".parse().expect("ts"),
        }
    }

    #[test]
    fn spelling_round_trips_through_the_database_form() {
        for visibility in [
            RoomVisibility::Private,
            RoomVisibility::Unlisted,
            RoomVisibility::Public,
        ] {
            assert_eq!(
                RoomVisibility::parse(visibility.as_str()).expect("parse"),
                visibility
            );
        }
        for kind in [PrincipalKind::User, PrincipalKind::Session] {
            assert_eq!(PrincipalKind::parse(kind.as_str()).expect("parse"), kind);
        }
        for role in [MemberRole::Owner, MemberRole::Member, MemberRole::Viewer] {
            assert_eq!(MemberRole::parse(role.as_str()).expect("parse"), role);
        }
    }

    #[test]
    fn unknown_spellings_are_refused() {
        assert!(RoomVisibility::parse("secret").is_err());
        assert!(PrincipalKind::parse("robot").is_err());
        assert!(MemberRole::parse("admin").is_err());
    }

    #[test]
    fn only_owners_and_members_may_write() {
        assert!(MemberRole::Owner.can_write());
        assert!(MemberRole::Member.can_write());
        assert!(!MemberRole::Viewer.can_write());
    }

    #[test]
    fn room_view_hides_internal_ids_and_keeps_the_null_pool() {
        let json =
            serde_json::to_value(RoomView::from_room(&room()).expect("view")).expect("serialize");
        assert_eq!(json["id"], "11111111-1111-4111-8111-111111111111");
        assert_eq!(json["visibility"], "public");
        assert!(json["originPool"].is_null());
        assert!(json.get("project_id").is_none());
        assert!(json.get("organization_id").is_none());
    }

    #[test]
    fn principals_carry_exactly_one_identifier() {
        let user = Principal::User("u-1".into());
        let session = Principal::Session("s-1".into());
        assert_eq!(user.kind(), PrincipalKind::User);
        assert_eq!(session.kind(), PrincipalKind::Session);
        let json = serde_json::to_value(&session).expect("serialize");
        assert_eq!(json["kind"], "session");
        assert_eq!(json["id"], "s-1");
        assert!(Principal::User("   ".into()).validate().is_err());
    }

    #[test]
    fn names_bodies_and_origins_are_validated() {
        assert_eq!(
            validate_room_name("  builds ").expect("name"),
            "builds".to_string()
        );
        assert!(validate_room_name("   ").is_err());
        assert!(validate_room_name(&"a".repeat(MAX_ROOM_NAME_CHARS + 1)).is_err());

        assert_eq!(
            validate_message_body("hello").expect("body"),
            "hello".to_string()
        );
        assert!(validate_message_body("  ").is_err());
        assert!(validate_message_body(&"a".repeat(MAX_MESSAGE_BODY_BYTES + 1)).is_err());

        assert_eq!(validate_origin_pool(None).expect("origin"), None);
        assert_eq!(validate_origin_pool(Some("  ")).expect("origin"), None);
        assert_eq!(
            validate_origin_pool(Some("pool-eu")).expect("origin"),
            Some("pool-eu".to_string())
        );
    }
}
