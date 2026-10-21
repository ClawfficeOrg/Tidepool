//! GitHub App webhook ingress (task 1.3.2).
//!
//! `POST /v1/webhooks/github/{slug}` is public: GitHub cannot present pool
//! credentials. Security comes from `X-Hub-Signature-256`, checked against the
//! connection's `webhook_secret`; unknown slugs are 404 and bad signatures 401
//! before any event matching.

use crate::api::problem::{Problem, ProblemCode};
use crate::services::connections::github::{self, GithubWebhookError};
use actix_web::{HttpRequest, HttpResponse, http::StatusCode, http::header::HeaderMap, web};
use serde::Serialize;
use sqlx::MySqlPool;

/// Register the GitHub webhook route.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/webhooks/github").route("/{slug}", web::post().to(receive_webhook)),
    );
}

/// Webhook acknowledgement.
#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    /// Always true once accepted (including duplicates).
    pub received: bool,
    /// True when this delivery id was already recorded.
    pub duplicate: bool,
    /// Internal event name.
    pub event: String,
    /// GitHub action, when present.
    pub action: Option<String>,
    /// False when the delivery was recorded but not matched.
    pub accepted: bool,
}

#[tracing::instrument(skip(pool, req, body))]
async fn receive_webhook(
    pool: Option<web::Data<MySqlPool>>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Bytes,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    let organization_id: Option<u64> =
        match sqlx::query_scalar("SELECT id FROM organizations ORDER BY id LIMIT 1")
            .fetch_optional(pool.get_ref())
            .await
        {
            Ok(id) => id,
            Err(error) => {
                tracing::error!(error = %error, "github webhook organization lookup failed");
                return internal_error(&req);
            }
        };
    let Some(organization_id) = organization_id else {
        return not_found(&req);
    };

    let headers: &HeaderMap = req.headers();
    match github::handle_webhook(
        pool.get_ref(),
        organization_id,
        &path.into_inner(),
        headers,
        &body,
    )
    .await
    {
        Ok(outcome) => HttpResponse::Accepted().json(WebhookResponse {
            received: true,
            duplicate: outcome.duplicate,
            event: outcome.event,
            action: outcome.action,
            accepted: outcome.accepted,
        }),
        Err(error) => map_error(error, &req),
    }
}

fn db_unavailable(req: &HttpRequest) -> HttpResponse {
    Problem::from_request(
        req,
        StatusCode::SERVICE_UNAVAILABLE,
        ProblemCode::ServiceUnavailable,
        "database unavailable",
    )
    .into_response()
}

fn not_found(req: &HttpRequest) -> HttpResponse {
    Problem::from_request(
        req,
        StatusCode::NOT_FOUND,
        ProblemCode::NotFound,
        "connection not found",
    )
    .into_response()
}

fn internal_error(req: &HttpRequest) -> HttpResponse {
    Problem::from_request(
        req,
        StatusCode::INTERNAL_SERVER_ERROR,
        ProblemCode::InternalError,
        "internal error",
    )
    .into_response()
}

fn map_error(error: GithubWebhookError, req: &HttpRequest) -> HttpResponse {
    let (status, code, detail) = match error {
        GithubWebhookError::NotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "connection not found".to_string(),
        ),
        GithubWebhookError::BadSignature => (
            StatusCode::UNAUTHORIZED,
            ProblemCode::Unauthorized,
            "invalid webhook signature".to_string(),
        ),
        GithubWebhookError::Invalid(message) => (
            StatusCode::BAD_REQUEST,
            ProblemCode::InvalidRequest,
            message,
        ),
        GithubWebhookError::Database(error) => {
            tracing::error!(error = %error, "github webhook failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error".to_string(),
            )
        }
    };
    Problem::from_request(req, status, code, detail).into_response()
}
