//! Chatroom HTTP surface (task 1.5.2).
//!
//! - `GET /v1/chatrooms` — the rooms the signed-in member belongs to.
//! - `GET /v1/chatrooms/{room}/members` — who is in a room.
//! - `GET /v1/chatrooms/{room}/messages?after=&limit=` — cursor-paged history.
//! - `POST /v1/chatrooms/{room}/messages` — send a message.
//!
//! Every route resolves the caller to a [`Principal`] and then asks the service
//! whether that principal is a member. A room the caller does not belong to
//! answers exactly like a room that does not exist, so membership cannot be
//! probed by comparing a 403 against a 404.
//!
//! History is cursor-based on the message sequence. `after` is exclusive, so a
//! client that keeps its last sequence sees only messages it has not read.
//! An empty page is the currentness signal: there is nothing newer to read.

use crate::api::problem::{Problem, ProblemCode};
use crate::entities::chatroom::Principal;
use crate::middleware::auth::OperatorAuth;
use crate::services::chatrooms::{self as chatroom_svc, ChatroomServiceError};
use actix_web::{HttpRequest, HttpResponse, web};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;

/// Register the chatroom routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/chatrooms")
            .route("", web::get().to(list_rooms))
            .route("/{room}/members", web::get().to(list_members))
            .route("/{room}/messages", web::get().to(history))
            .route("/{room}/messages", web::post().to(post_message))
            .route(
                "/{room}/stream",
                web::get().to(crate::sockets::chatroom_ws::stream),
            ),
    );
}

/// Cursor and page size for a history read.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct HistoryQuery {
    /// Return only messages whose sequence is greater than this.
    pub after: Option<u64>,
    /// Page size; clamped to the service's bounds.
    pub limit: Option<u32>,
}

/// Body of a send request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendMessageBody {
    /// Markdown body.
    pub body: String,
    /// Public id of a message in the same room.
    #[serde(default)]
    pub reply_to: Option<String>,
}

/// A page of history.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryResponse {
    /// Messages in sequence order.
    pub messages: Vec<crate::entities::chatroom::MessageView>,
    /// Cursor for the next read; absent when this page is the last.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<u64>,
}

/// The caller as a chatroom principal. Members act as themselves; API keys
/// are not chat participants, so only operator credentials are accepted.
fn caller(auth: &OperatorAuth) -> Principal {
    Principal::User(auth.operator.public_id.clone())
}

fn db_unavailable(req: &HttpRequest) -> HttpResponse {
    Problem::from_request(
        req,
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
        ProblemCode::ServiceUnavailable,
        "database unavailable",
    )
    .into_response()
}

fn validation_error(req: &HttpRequest, detail: &str) -> HttpResponse {
    Problem::from_request(
        req,
        actix_web::http::StatusCode::UNPROCESSABLE_ENTITY,
        ProblemCode::ValidationFailed,
        detail,
    )
    .into_response()
}

fn map_error(error: ChatroomServiceError, req: &HttpRequest) -> HttpResponse {
    use actix_web::http::StatusCode;
    match error {
        // A non-member and a missing room are deliberately indistinguishable.
        ChatroomServiceError::NotFound => Problem::from_request(
            req,
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "chatroom not found",
        )
        .into_response(),
        ChatroomServiceError::Forbidden(message) => {
            Problem::from_request(req, StatusCode::FORBIDDEN, ProblemCode::Forbidden, &message)
                .into_response()
        }
        ChatroomServiceError::Invalid(message) => validation_error(req, &message),
        ChatroomServiceError::Conflict(message) => {
            Problem::from_request(req, StatusCode::CONFLICT, ProblemCode::Conflict, &message)
                .into_response()
        }
        ChatroomServiceError::Database(error) => {
            tracing::error!(%error, "chatroom query failed");
            Problem::from_request(
                req,
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error",
            )
            .into_response()
        }
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn list_rooms(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match chatroom_svc::rooms_for_principal(pool.get_ref(), auth.organization_id(), &caller(&auth))
        .await
    {
        Ok(rooms) => HttpResponse::Ok().json(rooms),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn list_members(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    room: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match chatroom_svc::list_members_as(
        pool.get_ref(),
        auth.organization_id(),
        &room.into_inner(),
        &caller(&auth),
    )
    .await
    {
        Ok(members) => HttpResponse::Ok().json(members),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, query))]
async fn history(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    room: web::Path<String>,
    query: web::Query<HistoryQuery>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match chatroom_svc::history_as(
        pool.get_ref(),
        auth.organization_id(),
        &room.into_inner(),
        &caller(&auth),
        query.after,
        query.limit,
    )
    .await
    {
        Ok(page) => HttpResponse::Ok().json(HistoryResponse {
            messages: page.messages,
            next: page.next,
        }),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn post_message(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    room: web::Path<String>,
    body: web::Json<SendMessageBody>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    let room = room.into_inner();
    match chatroom_svc::post_message_as(
        pool.get_ref(),
        auth.organization_id(),
        &room,
        &caller(&auth),
        &body.body,
        body.reply_to.as_deref(),
    )
    .await
    {
        Ok(message) => {
            // Live watchers get the same envelope the HTTP API returns; the
            // database remains the source of truth for anyone catching up.
            crate::sockets::rooms::broadcast_message(
                &room,
                crate::sockets::rooms::message_envelope(&message),
            );
            HttpResponse::Created().json(message)
        }
        Err(error) => map_error(error, &req),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::chatroom::MemberRole;

    #[test]
    fn a_viewer_may_not_write() {
        assert!(!MemberRole::Viewer.can_write());
        assert!(MemberRole::Member.can_write());
        assert!(MemberRole::Owner.can_write());
    }

    #[test]
    fn history_query_defaults_to_the_head_of_the_room() {
        let query = HistoryQuery::default();
        assert_eq!(query.after, None);
        assert_eq!(chatroom_svc::page_limit(query.limit), 50);
    }

    #[test]
    fn send_bodies_reject_unknown_fields() {
        let parsed: Result<SendMessageBody, _> =
            serde_json::from_str(r#"{"body":"hi","authorId":"someone-else"}"#);
        assert!(parsed.is_err());
        let ok: SendMessageBody =
            serde_json::from_str(r#"{"body":"hi","replyTo":"m-1"}"#).expect("parse");
        assert_eq!(ok.reply_to.as_deref(), Some("m-1"));
    }
}
