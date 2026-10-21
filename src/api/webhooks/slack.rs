//! Slack Events API webhook ingress (task 1.3.3).
//!
//! `POST /v1/webhooks/slack/{slug}` is public: Slack cannot present pool
//! credentials. Security is the Slack signature — `X-Slack-Signature` over
//! `v0:<X-Slack-Request-Timestamp>:<body>` — checked against the connection's
//! `signing_secret` before the body is parsed, with the timestamp inside a
//! five-minute window so a captured body cannot be replayed.
//!
//! A pool with no public address does not need this route at all: the same
//! events arrive over socket mode (see
//! [`crate::services::connections::slack`]), and both transports deduplicate
//! on Slack's `event_id`.

use crate::api::problem::{Problem, ProblemCode};
use crate::services::connections::slack::{self, SlackError};
use actix_web::{HttpRequest, HttpResponse, http::StatusCode, http::header::HeaderMap, web};
use serde::Serialize;
use sqlx::MySqlPool;

/// Register the Slack webhook route.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/v1/webhooks/slack").route("/{slug}", web::post().to(receive_webhook)));
}

/// Webhook acknowledgement.
#[derive(Debug, Serialize)]
pub struct WebhookResponse {
    /// Always true once accepted (including duplicates).
    pub received: bool,
    /// True when this `event_id` was already recorded, on either transport.
    pub duplicate: bool,
    /// Internal event name.
    pub event: String,
    /// Transport the event arrived on; always `webhook` here.
    pub transport: String,
    /// False when the delivery was recorded but not matched.
    pub accepted: bool,
    /// Why it was not matched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Slack's `url_verification` answer.
#[derive(Debug, Serialize)]
pub struct ChallengeResponse {
    /// The challenge Slack sent, echoed verbatim.
    pub challenge: String,
}

#[tracing::instrument(skip(pool, req, body))]
async fn receive_webhook(
    pool: Option<web::Data<MySqlPool>>,
    req: HttpRequest,
    path: web::Path<String>,
    body: web::Bytes,
) -> HttpResponse {
    let headers: &HeaderMap = req.headers();
    // Cheap rejections first: a request without Slack's signature headers, or
    // with a replayed timestamp, never reaches the database.
    let timestamp = headers
        .get("x-slack-request-timestamp")
        .and_then(|value| value.to_str().ok());
    let Some(timestamp) = timestamp else {
        return unauthorized(&req);
    };
    if headers.get("x-slack-signature").is_none() {
        return unauthorized(&req);
    }
    let now = chrono::Utc::now().timestamp();
    if !slack::timestamp_fresh(timestamp, now) {
        return unauthorized(&req);
    }

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
                tracing::error!(error = %error, "slack webhook organization lookup failed");
                return internal_error(&req);
            }
        };
    let Some(organization_id) = organization_id else {
        return not_found(&req);
    };

    match slack::handle_webhook(
        pool.get_ref(),
        organization_id,
        &path.into_inner(),
        headers,
        &body,
        now,
    )
    .await
    {
        Ok(outcome) => {
            if let Some(challenge) = outcome.challenge {
                return HttpResponse::Ok().json(ChallengeResponse { challenge });
            }
            match outcome.delivery {
                Some(delivery) => HttpResponse::Accepted().json(WebhookResponse {
                    received: true,
                    duplicate: outcome.duplicate,
                    event: delivery.event.kind().to_string(),
                    transport: delivery.transport.as_str().to_string(),
                    accepted: delivery.accepted,
                    reason: delivery.reason.map(|reason| reason.as_str().to_string()),
                }),
                None => HttpResponse::Accepted().json(WebhookResponse {
                    received: true,
                    duplicate: outcome.duplicate,
                    event: "slack.unsupported".to_string(),
                    transport: "webhook".to_string(),
                    accepted: false,
                    reason: Some("unsupported_event".to_string()),
                }),
            }
        }
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

fn unauthorized(req: &HttpRequest) -> HttpResponse {
    Problem::from_request(
        req,
        StatusCode::UNAUTHORIZED,
        ProblemCode::Unauthorized,
        "invalid request signature",
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

fn map_error(error: SlackError, req: &HttpRequest) -> HttpResponse {
    let (status, code, detail) = match error {
        SlackError::NotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "connection not found".to_string(),
        ),
        SlackError::BadSignature | SlackError::StaleTimestamp => (
            StatusCode::UNAUTHORIZED,
            ProblemCode::Unauthorized,
            "invalid request signature".to_string(),
        ),
        SlackError::MissingCredential(field) => {
            tracing::warn!(field, "slack connection is missing a credential field");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                ProblemCode::ServiceUnavailable,
                "slack connection is not fully configured".to_string(),
            )
        }
        SlackError::Invalid(message) => (
            StatusCode::BAD_REQUEST,
            ProblemCode::InvalidRequest,
            message,
        ),
        SlackError::Database(error) => {
            tracing::error!(error = %error, "slack webhook failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error".to_string(),
            )
        }
    };
    Problem::from_request(req, status, code, detail).into_response()
}
