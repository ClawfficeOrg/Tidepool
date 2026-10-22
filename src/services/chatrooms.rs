//! Chatroom service (task 1.5.1).
//!
//! Schema-level operations for rooms, members and messages. Every query is
//! scoped to the organization the caller supplies, so one organization cannot
//! read or write another's rooms even if it guesses a public id.
//!
//! Message history is paged by the internal [`Message::id`], surfaced as
//! [`MessageView::sequence`]. `id` is monotonic and never reused, so a cursor
//! taken from it cannot skip or repeat a message when two rows share a
//! timestamp.

use crate::entities::chatroom::{
    MemberRole, MemberView, Message, MessageView, Principal, PrincipalKind, Room, RoomMember,
    RoomView, RoomVisibility, validate_message_body, validate_origin_pool, validate_room_name,
};
use sqlx::MySqlPool;
use thiserror::Error;

/// Default page size for message history.
pub const DEFAULT_HISTORY_LIMIT: u32 = 50;
/// Largest accepted page size for message history.
pub const MAX_HISTORY_LIMIT: u32 = 200;

/// Domain errors for chatroom operations.
#[derive(Debug, Error)]
pub enum ChatroomServiceError {
    /// A caller-supplied value is not valid.
    #[error("invalid input: {0}")]
    Invalid(String),
    /// No room, project or parent message with that id in this organization.
    #[error("chatroom resource not found")]
    NotFound,
    /// The write collides with an existing row (duplicate room name or member).
    #[error("chatroom conflict: {0}")]
    Conflict(String),
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

impl From<crate::entities::chatroom::ChatroomError> for ChatroomServiceError {
    fn from(error: crate::entities::chatroom::ChatroomError) -> Self {
        ChatroomServiceError::Invalid(error.to_string())
    }
}

/// Input for creating a room.
#[derive(Debug, Clone)]
pub struct NewRoom {
    /// Room name, unique within the project.
    pub name: String,
    /// Who may read the room.
    pub visibility: RoomVisibility,
    /// Pool the room was federated from; unset for v1 rooms.
    pub origin_pool: Option<String>,
}

/// Input for adding a member.
#[derive(Debug, Clone)]
pub struct NewMember {
    /// The member principal.
    pub principal: Principal,
    /// The member's role.
    pub role: MemberRole,
}

/// One page of message history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessagePage {
    /// Messages ordered by `sequence`, ascending.
    pub messages: Vec<MessageView>,
    /// Cursor to pass as `after` for the next page; `None` when exhausted.
    pub next: Option<u64>,
}

fn internal_id(pool_id: &str, kind: &str) -> ChatroomServiceError {
    ChatroomServiceError::Invalid(format!("{kind} id is not a valid public id: {pool_id:?}"))
}

async fn room_row(
    pool: &MySqlPool,
    organization_id: u64,
    room_public_id: &str,
) -> Result<Room, ChatroomServiceError> {
    sqlx::query_as::<_, Room>(
        "SELECT id, public_id, organization_id, project_id, name, visibility, origin_pool, \
                created_at, updated_at \
         FROM chat_rooms WHERE organization_id = ? AND public_id = ?",
    )
    .bind(organization_id)
    .bind(room_public_id)
    .fetch_optional(pool)
    .await
    .map_err(ChatroomServiceError::Database)?
    .ok_or(ChatroomServiceError::NotFound)
}

async fn project_id_for(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
) -> Result<u64, ChatroomServiceError> {
    let found: Option<(u64,)> =
        sqlx::query_as("SELECT id FROM projects WHERE organization_id = ? AND public_id = ?")
            .bind(organization_id)
            .bind(project_public_id)
            .fetch_optional(pool)
            .await
            .map_err(ChatroomServiceError::Database)?;
    found.map(|(id,)| id).ok_or(ChatroomServiceError::NotFound)
}

async fn user_id_for(
    pool: &MySqlPool,
    organization_id: u64,
    user_public_id: &str,
) -> Result<u64, ChatroomServiceError> {
    let found: Option<(u64,)> =
        sqlx::query_as("SELECT id FROM pool_users WHERE organization_id = ? AND public_id = ?")
            .bind(organization_id)
            .bind(user_public_id)
            .fetch_optional(pool)
            .await
            .map_err(ChatroomServiceError::Database)?;
    found.map(|(id,)| id).ok_or_else(|| {
        ChatroomServiceError::Invalid(format!(
            "no pool user {user_public_id:?} in this organization"
        ))
    })
}

/// Resolve a principal to the two columns the schema stores.
async fn principal_columns(
    pool: &MySqlPool,
    organization_id: u64,
    principal: &Principal,
) -> Result<(PrincipalKind, Option<u64>, Option<String>), ChatroomServiceError> {
    principal.validate()?;
    match principal {
        Principal::User(public_id) => {
            let id = user_id_for(pool, organization_id, public_id).await?;
            Ok((PrincipalKind::User, Some(id), None))
        }
        Principal::Session(session_id) => {
            Ok((PrincipalKind::Session, None, Some(session_id.clone())))
        }
    }
}

fn is_duplicate(error: &sqlx::Error) -> bool {
    matches!(
        error,
        sqlx::Error::Database(database) if database.is_unique_violation()
    )
}

/// Create a room, refusing a duplicate name inside the project.
pub async fn create_room(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
    new: NewRoom,
) -> Result<RoomView, ChatroomServiceError> {
    let name = validate_room_name(&new.name)?;
    let origin_pool = validate_origin_pool(new.origin_pool.as_deref())?;
    let project_id = project_id_for(pool, organization_id, project_public_id).await?;
    let public_id = uuid::Uuid::new_v4().to_string();

    let result = sqlx::query(
        "INSERT INTO chat_rooms (public_id, organization_id, project_id, name, visibility, \
                                 origin_pool) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(project_id)
    .bind(&name)
    .bind(new.visibility.as_str())
    .bind(origin_pool.as_deref())
    .execute(pool)
    .await;

    match result {
        Ok(_) => {}
        Err(error) if is_duplicate(&error) => {
            return Err(ChatroomServiceError::Conflict(format!(
                "a room named {name:?} already exists in this project"
            )));
        }
        Err(error) => return Err(ChatroomServiceError::Database(error)),
    }

    let room = room_row(pool, organization_id, &public_id).await?;
    Ok(RoomView::from_room(&room)?)
}

/// Every room in a project, newest last.
pub async fn list_rooms(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
) -> Result<Vec<RoomView>, ChatroomServiceError> {
    let project_id = project_id_for(pool, organization_id, project_public_id).await?;
    let rows = sqlx::query_as::<_, Room>(
        "SELECT id, public_id, organization_id, project_id, name, visibility, origin_pool, \
                created_at, updated_at \
         FROM chat_rooms WHERE organization_id = ? AND project_id = ? \
         ORDER BY created_at ASC, id ASC",
    )
    .bind(organization_id)
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(ChatroomServiceError::Database)?;

    let mut views = Vec::with_capacity(rows.len());
    for row in &rows {
        views.push(RoomView::from_room(row)?);
    }
    Ok(views)
}

/// Rooms one principal belongs to, oldest membership first.
pub async fn rooms_for_principal(
    pool: &MySqlPool,
    organization_id: u64,
    principal: &Principal,
) -> Result<Vec<RoomView>, ChatroomServiceError> {
    let (kind, user_id, session_id) = principal_columns(pool, organization_id, principal).await?;
    let rows = sqlx::query_as::<_, Room>(
        "SELECT r.id, r.public_id, r.organization_id, r.project_id, r.name, r.visibility, \
                r.origin_pool, r.created_at, r.updated_at \
         FROM chat_rooms r \
         JOIN chat_room_members m ON m.room_id = r.id \
         WHERE r.organization_id = ? AND m.member_kind = ? \
           AND ((? IS NOT NULL AND m.user_id = ?) OR (? IS NOT NULL AND m.session_id = ?)) \
         ORDER BY m.joined_at ASC, r.id ASC",
    )
    .bind(organization_id)
    .bind(kind.as_str())
    .bind(user_id)
    .bind(user_id)
    .bind(session_id.as_deref())
    .bind(session_id.as_deref())
    .fetch_all(pool)
    .await
    .map_err(ChatroomServiceError::Database)?;

    let mut views = Vec::with_capacity(rows.len());
    for row in &rows {
        views.push(RoomView::from_room(row)?);
    }
    Ok(views)
}

/// Add a member, refusing a principal that already joined.
pub async fn add_member(
    pool: &MySqlPool,
    organization_id: u64,
    room_public_id: &str,
    new: NewMember,
) -> Result<MemberView, ChatroomServiceError> {
    let room = room_row(pool, organization_id, room_public_id).await?;
    let (kind, user_id, session_id) =
        principal_columns(pool, organization_id, &new.principal).await?;
    let public_id = uuid::Uuid::new_v4().to_string();

    let result = sqlx::query(
        "INSERT INTO chat_room_members (public_id, room_id, member_kind, user_id, session_id, role) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&public_id)
    .bind(room.id)
    .bind(kind.as_str())
    .bind(user_id)
    .bind(session_id.as_deref())
    .bind(new.role.as_str())
    .execute(pool)
    .await;

    match result {
        Ok(_) => {}
        Err(error) if is_duplicate(&error) => {
            return Err(ChatroomServiceError::Conflict(
                "that principal has already joined this room".to_string(),
            ));
        }
        Err(error) => return Err(ChatroomServiceError::Database(error)),
    }

    let member = sqlx::query_as::<_, RoomMember>(
        "SELECT id, public_id, room_id, member_kind, user_id, session_id, role, joined_at \
         FROM chat_room_members WHERE public_id = ?",
    )
    .bind(&public_id)
    .fetch_one(pool)
    .await
    .map_err(ChatroomServiceError::Database)?;

    member_view(pool, organization_id, &member).await
}

async fn member_view(
    pool: &MySqlPool,
    organization_id: u64,
    member: &RoomMember,
) -> Result<MemberView, ChatroomServiceError> {
    let principal = match PrincipalKind::parse(&member.member_kind)? {
        PrincipalKind::User => {
            let public_id: String = sqlx::query_scalar(
                "SELECT public_id FROM pool_users WHERE id = ? AND organization_id = ?",
            )
            .bind(member.user_id)
            .bind(organization_id)
            .fetch_one(pool)
            .await
            .map_err(ChatroomServiceError::Database)?;
            Principal::User(public_id)
        }
        PrincipalKind::Session => Principal::Session(
            member
                .session_id
                .clone()
                .ok_or_else(|| internal_id("session", "member"))?,
        ),
    };
    Ok(MemberView {
        id: member.public_id.clone(),
        principal,
        role: MemberRole::parse(&member.role)?,
        joined_at: member.joined_at,
    })
}

/// Every member of a room, earliest join first.
pub async fn list_members(
    pool: &MySqlPool,
    organization_id: u64,
    room_public_id: &str,
) -> Result<Vec<MemberView>, ChatroomServiceError> {
    let room = room_row(pool, organization_id, room_public_id).await?;
    let rows = sqlx::query_as::<_, RoomMember>(
        "SELECT id, public_id, room_id, member_kind, user_id, session_id, role, joined_at \
         FROM chat_room_members WHERE room_id = ? ORDER BY joined_at ASC, id ASC",
    )
    .bind(room.id)
    .fetch_all(pool)
    .await
    .map_err(ChatroomServiceError::Database)?;

    let mut views = Vec::with_capacity(rows.len());
    for row in &rows {
        views.push(member_view(pool, organization_id, row).await?);
    }
    Ok(views)
}

async fn message_view(
    pool: &MySqlPool,
    organization_id: u64,
    message: &Message,
) -> Result<MessageView, ChatroomServiceError> {
    let author = match PrincipalKind::parse(&message.author_kind)? {
        PrincipalKind::User => {
            // The pool user may have been deleted since; the message keeps its
            // history and reads back with no author.
            let public_id: Option<String> = sqlx::query_scalar(
                "SELECT public_id FROM pool_users WHERE id = ? AND organization_id = ?",
            )
            .bind(message.author_user_id)
            .bind(organization_id)
            .fetch_optional(pool)
            .await
            .map_err(ChatroomServiceError::Database)?;
            public_id.map(Principal::User)
        }
        PrincipalKind::Session => Some(Principal::Session(
            message
                .author_session_id
                .clone()
                .ok_or_else(|| internal_id("session", "author"))?,
        )),
    };
    let reply_to = match message.reply_to_message_id {
        Some(id) => Some(
            sqlx::query_scalar::<_, String>(
                "SELECT public_id FROM chat_messages WHERE id = ? AND room_id = ?",
            )
            .bind(id)
            .bind(message.room_id)
            .fetch_one(pool)
            .await
            .map_err(ChatroomServiceError::Database)?,
        ),
        None => None,
    };
    Ok(MessageView {
        id: message.public_id.clone(),
        author,
        body: message.body.clone(),
        reply_to,
        created_at: message.created_at,
        sequence: message.id,
    })
}

/// Post a message to a room.
///
/// `reply_to` is the public id of a message in the same room; a reply to a
/// message from another room is refused rather than silently stored.
pub async fn post_message(
    pool: &MySqlPool,
    organization_id: u64,
    room_public_id: &str,
    author: &Principal,
    body: &str,
    reply_to: Option<&str>,
) -> Result<MessageView, ChatroomServiceError> {
    let room = room_row(pool, organization_id, room_public_id).await?;
    let body = validate_message_body(body)?;
    let (kind, user_id, session_id) = principal_columns(pool, organization_id, author).await?;

    let reply_to_id = match reply_to {
        Some(public_id) => {
            let found: Option<(u64,)> =
                sqlx::query_as("SELECT id FROM chat_messages WHERE room_id = ? AND public_id = ?")
                    .bind(room.id)
                    .bind(public_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(ChatroomServiceError::Database)?;
            Some(
                found
                    .map(|(id,)| id)
                    .ok_or(ChatroomServiceError::NotFound)?,
            )
        }
        None => None,
    };

    let public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO chat_messages (public_id, room_id, author_kind, author_user_id, \
                                    author_session_id, body, reply_to_message_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&public_id)
    .bind(room.id)
    .bind(kind.as_str())
    .bind(user_id)
    .bind(session_id.as_deref())
    .bind(&body)
    .bind(reply_to_id)
    .execute(pool)
    .await
    .map_err(ChatroomServiceError::Database)?;

    let message = sqlx::query_as::<_, Message>(
        "SELECT id, public_id, room_id, author_kind, author_user_id, author_session_id, body, \
                reply_to_message_id, created_at \
         FROM chat_messages WHERE public_id = ?",
    )
    .bind(&public_id)
    .fetch_one(pool)
    .await
    .map_err(ChatroomServiceError::Database)?;

    message_view(pool, organization_id, &message).await
}

/// Read one page of a room's history, oldest first.
///
/// `after` is the last `sequence` the caller saw; `None` starts at the
/// beginning. `limit` is clamped to [`MAX_HISTORY_LIMIT`].
pub async fn history(
    pool: &MySqlPool,
    organization_id: u64,
    room_public_id: &str,
    after: Option<u64>,
    limit: Option<u32>,
) -> Result<MessagePage, ChatroomServiceError> {
    let room = room_row(pool, organization_id, room_public_id).await?;
    let limit = limit
        .unwrap_or(DEFAULT_HISTORY_LIMIT)
        .clamp(1, MAX_HISTORY_LIMIT);

    // One extra row tells the caller whether another page exists without a
    // second count query.
    let rows = sqlx::query_as::<_, Message>(
        "SELECT id, public_id, room_id, author_kind, author_user_id, author_session_id, body, \
                reply_to_message_id, created_at \
         FROM chat_messages WHERE room_id = ? AND id > ? ORDER BY id ASC LIMIT ?",
    )
    .bind(room.id)
    .bind(after.unwrap_or(0))
    .bind(limit + 1)
    .fetch_all(pool)
    .await
    .map_err(ChatroomServiceError::Database)?;

    let has_more = rows.len() as u32 > limit;
    let page = if has_more {
        &rows[..limit as usize]
    } else {
        &rows[..]
    };

    let mut messages = Vec::with_capacity(page.len());
    for row in page {
        messages.push(message_view(pool, organization_id, row).await?);
    }
    let next = if has_more {
        messages.last().map(|message| message.sequence)
    } else {
        None
    };
    Ok(MessagePage { messages, next })
}
