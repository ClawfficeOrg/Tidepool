//! Session relay HTTP surface (task 1.5.5).
//!
//! - `GET /v1/sessions/{session}/relay` — publication state (off by default).
//! - `PUT /v1/sessions/{session}/relay` — opt in.
//! - `DELETE /v1/sessions/{session}/relay` — stop immediately.
//! - `POST /v1/sessions/{session}/relay/events` — publish one event.
//! - `GET /v1/sessions/{session}/relay/events?after=&limit=` — backfill.
//!
//! Publishing is refused unless the session opted in, and stopping is
//! immediate because the service reads the relay row on every publish rather
//! than trusting a cached flag.

use crate::api::problem::{Problem, ProblemCode};
use crate::middleware::auth::OperatorAuth;
use crate::services::relay::{self, RelayError};
use actix_web::{HttpRequest, HttpResponse, web};
use serde::Deserialize;
use serde_json::Value;
use sqlx::MySqlPool;

/// Register the relay routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/sessions/{session}/relay")
            .route("", web::get().to(state))
            .route("", web::put().to(enable))
            .route("", web::delete().to(stop))
            .route("/events", web::post().to(publish))
            .route("/events", web::get().to(backfill)),
    );
}

/// Query for a backfill read.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct BackfillQuery {
    /// Exclusive cursor.
    pub after: Option<u64>,
    /// Page size.
    pub limit: Option<u32>,
}

/// Body of a publish request.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublishBody {
    /// Event kind from the relay allowlist.
    pub kind: String,
    /// Small JSON object.
    pub payload: Value,
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

fn map_error(error: RelayError, req: &HttpRequest) -> HttpResponse {
    use actix_web::http::StatusCode;
    match error {
        RelayError::NotFound => Problem::from_request(
            req,
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "session not found",
        )
        .into_response(),
        RelayError::NotEnabled => Problem::from_request(
            req,
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            "relay is not enabled for this session",
        )
        .into_response(),
        RelayError::Invalid(message) => Problem::from_request(
            req,
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            &message,
        )
        .into_response(),
        RelayError::Database(error) => {
            tracing::error!(%error, "relay query failed");
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
async fn state(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    session: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match relay::state(pool.get_ref(), auth.organization_id(), &session).await {
        Ok(state) => HttpResponse::Ok().json(state),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn enable(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    session: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match relay::enable(pool.get_ref(), auth.organization_id(), &session).await {
        Ok(state) => HttpResponse::Ok().json(state),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn stop(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    session: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match relay::stop(pool.get_ref(), auth.organization_id(), &session).await {
        Ok(state) => HttpResponse::Ok().json(state),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn publish(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    session: web::Path<String>,
    body: web::Json<PublishBody>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match relay::publish(
        pool.get_ref(),
        auth.organization_id(),
        &session,
        &body.kind,
        &body.payload,
    )
    .await
    {
        Ok(event) => HttpResponse::Created().json(event),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth, query))]
async fn backfill(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    session: web::Path<String>,
    query: web::Query<BackfillQuery>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match relay::backfill(
        pool.get_ref(),
        auth.organization_id(),
        &session,
        query.after,
        query.limit,
    )
    .await
    {
        Ok(page) => HttpResponse::Ok().json(page),
        Err(error) => map_error(error, &req),
    }
}
