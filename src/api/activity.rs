//! Activity HTTP surface (task 1.4.7).
//!
//! - `GET /v1/activity` — one page of receipts, runs, steps and executions,
//!   filtered by project, provider, trigger and status.
//! - `GET /v1/activity/runs/{run}` — one run plus the arrival record that says
//!   whether it created, continued or restored an agent.
//!
//! Reads accept a member of any role or an API key with `projects:read`. Every
//! query is scoped to the credential's organization; the project filter is
//! applied to the combined history, never to one branch only. Paging is
//! cursor-based: `nextCursor` is returned when another page exists.

use crate::api::problem::{Problem, ProblemCode};
use crate::entities::activity::ActivityQuery;
use crate::entities::api_key::ApiKeyScope;
use crate::middleware::api_key::ApiKeyAuth;
use crate::middleware::auth::OperatorAuth;
use crate::services::activity::{self as activity_svc, ActivityError};
use actix_web::{
    Error, FromRequest, HttpRequest, HttpResponse,
    dev::Payload,
    http::{StatusCode, header},
    web,
};
use sqlx::MySqlPool;
use std::{future::Future, pin::Pin};

/// Register the activity read routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/activity")
            .route("", web::get().to(list_activity))
            .route("/runs/{run}", web::get().to(get_run)),
    );
}

/// Caller authenticated as either an API key or a member.
enum HubAuth {
    /// API key.
    Key(ApiKeyAuth),
    /// Operator member.
    Member(OperatorAuth),
}

impl HubAuth {
    /// Organization id both principals carry.
    fn organization_id(&self) -> u64 {
        match self {
            HubAuth::Key(auth) => auth.organization_id(),
            HubAuth::Member(auth) => auth.organization_id(),
        }
    }

    /// Reads: a member of any role, or an API key with `projects:read`.
    fn require_read(&self) -> Result<(), Error> {
        match self {
            HubAuth::Key(auth) => auth.require_scope(ApiKeyScope::ProjectsRead),
            HubAuth::Member(_) => Ok(()),
        }
    }
}

impl FromRequest for HubAuth {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let is_api_key = req
            .headers()
            .get("x-api-key")
            .is_some_and(|value| !value.is_empty())
            || req
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .is_some_and(|value| {
                    value.starts_with(crate::services::api_keys::KEY_PREFIX_MARKER)
                });

        let key_future = ApiKeyAuth::from_request(req, payload);
        let member_future = OperatorAuth::from_request(req, payload);

        Box::pin(async move {
            if is_api_key {
                Ok(HubAuth::Key(key_future.await?))
            } else {
                Ok(HubAuth::Member(member_future.await?))
            }
        })
    }
}

#[tracing::instrument(skip(pool, auth, query))]
async fn list_activity(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    query: web::Query<ActivityQuery>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_read() {
        return error.error_response();
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    let filters = match query.filters() {
        Ok(filters) => filters,
        Err(error) => return validation_error(&req, &error.to_string()),
    };
    match activity_svc::list_activity(pool.get_ref(), auth.organization_id(), &filters).await {
        Ok(page) => HttpResponse::Ok().json(page),
        Err(error) => map_error(error, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn get_run(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_read() {
        return error.error_response();
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match activity_svc::activity_run_detail(
        pool.get_ref(),
        auth.organization_id(),
        &path.into_inner(),
    )
    .await
    {
        Ok(detail) => HttpResponse::Ok().json(detail),
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

fn validation_error(req: &HttpRequest, detail: &str) -> HttpResponse {
    Problem::from_request(
        req,
        StatusCode::UNPROCESSABLE_ENTITY,
        ProblemCode::ValidationFailed,
        detail,
    )
    .into_response()
}

fn map_error(error: ActivityError, req: &HttpRequest) -> HttpResponse {
    match error {
        ActivityError::Filter(error) => validation_error(req, &error.to_string()),
        ActivityError::NotFound => Problem::from_request(
            req,
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "run not found",
        )
        .into_response(),
        ActivityError::Database(error) => {
            tracing::error!(%error, "activity read failed");
            Problem::from_request(
                req,
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error",
            )
            .into_response()
        }
        ActivityError::Invalid(message) => {
            tracing::error!(%message, "activity record is invalid");
            Problem::from_request(
                req,
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error",
            )
            .into_response()
        }
        ActivityError::Run(error) => {
            if error.code() == ProblemCode::NotFound.as_str() {
                Problem::from_request(
                    req,
                    StatusCode::NOT_FOUND,
                    ProblemCode::NotFound,
                    "run not found",
                )
                .into_response()
            } else {
                tracing::error!(%error, "activity run read failed");
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::activity::{
        ActivityEntry, ActivityKind, ActivityPage, ArrivalDetail, ContinuationMode, OutputLimit,
        StepOutputLimits,
    };
    use actix_web::{App, http::StatusCode as TestStatus, test as actix_test};
    use chrono::{DateTime, Utc};

    #[actix_web::test]
    async fn listing_activity_requires_credentials() {
        let app = actix_test::init_service(App::new().configure(config)).await;
        let req = actix_test::TestRequest::get()
            .uri("/v1/activity")
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn run_detail_requires_credentials() {
        let app = actix_test::init_service(App::new().configure(config)).await;
        let req = actix_test::TestRequest::get()
            .uri("/v1/activity/runs/run-1")
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }

    #[test]
    fn the_page_shape_is_camel_case_and_carries_paging() {
        let at: DateTime<Utc> = "2026-10-21T00:00:00Z".parse().expect("time");
        let page = ActivityPage {
            items: vec![ActivityEntry {
                kind: ActivityKind::Run,
                id: "run-1".into(),
                project: Some("project-1".into()),
                provider: Some("github".into()),
                trigger: Some("trigger-1".into()),
                trigger_name: Some("auto-fix".into()),
                run: Some("run-1".into()),
                step: None,
                execution: None,
                status: "succeeded".into(),
                event_type: None,
                actor: Some("alice".into()),
                matched: None,
                duplicates: None,
                output: None,
                error: None,
                continuation: Some("continued".into()),
                at,
            }],
            next_cursor: Some("0:run-1".into()),
            has_more: true,
        };
        let json = serde_json::to_value(&page).expect("serialize");
        assert_eq!(json["items"][0]["kind"], "run");
        assert_eq!(json["items"][0]["triggerName"], "auto-fix");
        assert_eq!(json["items"][0]["continuation"], "continued");
        assert_eq!(json["nextCursor"], "0:run-1");
        assert_eq!(json["hasMore"], true);
    }

    #[test]
    fn the_arrival_detail_names_the_continuation_and_limits() {
        let detail = ArrivalDetail {
            mode: ContinuationMode::Continued,
            provider: Some("slack".into()),
            conversation_key: Some("slack:channel:C1:thread-1".into()),
            steered: true,
            receipt: Some("receipt-1".into()),
            output_limits: vec![StepOutputLimits {
                step: "reply".into(),
                limits: vec![OutputLimit {
                    capability: "slack.reply".into(),
                    maximum: 2,
                    required: true,
                }],
            }],
        };
        let json = serde_json::to_value(&detail).expect("serialize");
        assert_eq!(json["mode"], "continued");
        assert_eq!(json["steered"], true);
        assert_eq!(
            json["outputLimits"][0]["limits"][0]["capability"],
            "slack.reply"
        );
    }

    #[test]
    fn query_validation_refuses_an_unknown_provider() {
        let query = ActivityQuery {
            project: None,
            provider: Some("gitlab".into()),
            trigger: None,
            status: None,
            kind: None,
            cursor: None,
            limit: None,
        };
        assert!(query.filters().is_err());
    }
}
