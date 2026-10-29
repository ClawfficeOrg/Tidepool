//! Manual run dispatch HTTP surface (task 1.4.6).
//!
//! `POST /v1/runs` turns one manual request into a workflow run. The body
//! names the project, trigger, actor, optional expected revision, a stable
//! delivery key and the input string. A non-viewer member or an API key with
//! `runs:dispatch` may call it.
//!
//! A new run is `201 Created`. Resubmitting the same delivery key returns the
//! existing run with `deduplicated: true` and `200 OK`: deduplication is the
//! promise, exactly-once delivery is not. Dispatch to an offline host is a
//! `409 daemon_not_connected` and writes nothing.

use crate::api::problem::{Problem, ProblemCode};
use crate::entities::api_key::ApiKeyScope;
use crate::entities::run::ManualDispatchRequest;
use crate::middleware::api_key::ApiKeyAuth;
use crate::middleware::auth::OperatorAuth;
use crate::services::dispatch::{self as dispatch_svc, DispatchError, ManualDispatchResponse};
use crate::services::runs::RunError;
use actix_web::{
    Error, FromRequest, HttpRequest, HttpResponse,
    dev::Payload,
    http::{StatusCode, header},
    web,
};
use sqlx::MySqlPool;
use std::{future::Future, pin::Pin};

/// Register the manual dispatch route.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/v1/runs").route("", web::post().to(dispatch_run)));
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

    /// A non-viewer member, or an API key with `runs:dispatch`.
    fn require_dispatch(&self) -> Result<(), Error> {
        match self {
            HubAuth::Key(auth) => auth.require_scope(ApiKeyScope::RunsDispatch),
            HubAuth::Member(auth) => auth.require_mutating(),
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

#[tracing::instrument(skip(pool, auth, body))]
async fn dispatch_run(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    body: web::Json<ManualDispatchRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_dispatch() {
        return error.error_response();
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };

    let request = body.into_inner();
    match dispatch_svc::dispatch_manual(pool.get_ref(), auth.organization_id(), &request).await {
        Ok(outcome) => {
            let deduplicated = outcome.deduplicated;
            let body = ManualDispatchResponse::from(outcome);
            if deduplicated {
                HttpResponse::Ok().json(body)
            } else {
                HttpResponse::Created().json(body)
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

fn map_error(error: DispatchError, req: &HttpRequest) -> HttpResponse {
    crate::middleware::metrics::metrics().dispatch_failure(error.code());
    let (status, code, detail) = match &error {
        DispatchError::ProjectNotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "project not found".to_string(),
        ),
        DispatchError::TriggerNotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "trigger not found".to_string(),
        ),
        DispatchError::ActorNotAllowed(actor) => (
            StatusCode::FORBIDDEN,
            ProblemCode::Forbidden,
            format!("actor '{actor}' is not allowed by the trigger"),
        ),
        DispatchError::RevisionNotActive { expected } => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            format!("revision '{expected}' is no longer active"),
        ),
        DispatchError::NoWorkflow(trigger) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ConfigurationInvalid,
            format!("no workflow in the active revision starts trigger '{trigger}'"),
        ),
        DispatchError::Invalid(message) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            message.clone(),
        ),
        DispatchError::Input(error) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            error.to_string(),
        ),
        DispatchError::Environment(error)
            if error.code() == ProblemCode::DaemonNotConnected.as_str() =>
        {
            (
                StatusCode::CONFLICT,
                ProblemCode::DaemonNotConnected,
                error.to_string(),
            )
        }
        DispatchError::Environment(error) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            error.to_string(),
        ),
        DispatchError::Run(RunError::Input(error)) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            error.to_string(),
        ),
        DispatchError::Run(error) if error.code() == ProblemCode::DaemonNotConnected.as_str() => (
            StatusCode::CONFLICT,
            ProblemCode::DaemonNotConnected,
            error.to_string(),
        ),
        DispatchError::Run(error) if error.code() == ProblemCode::Forbidden.as_str() => (
            StatusCode::FORBIDDEN,
            ProblemCode::Forbidden,
            error.to_string(),
        ),
        DispatchError::Run(error) if error.code() == "validation_failed" => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            error.to_string(),
        ),
        DispatchError::Run(error) => {
            tracing::error!(%error, "manual dispatch run failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error".to_string(),
            )
        }
        DispatchError::Snapshot(message) => {
            tracing::error!(%message, "manual dispatch snapshot failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error".to_string(),
            )
        }
        DispatchError::Database(error) => {
            tracing::error!(%error, "manual dispatch failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                ProblemCode::InternalError,
                "internal error".to_string(),
            )
        }
    };
    Problem::from_request(req, status, code, detail).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::StatusCode as TestStatus, test as actix_test};
    use serde_json::json;

    fn body() -> serde_json::Value {
        json!({
            "project": "11111111-1111-4111-8111-111111111111",
            "trigger": "auto-fix-issues",
            "actor": "alice",
            "deliveryKey": "delivery-1",
            "input": "repo=dev please fix it"
        })
    }

    #[actix_web::test]
    async fn manual_dispatch_requires_credentials() {
        let app = actix_test::init_service(App::new().configure(config)).await;
        let req = actix_test::TestRequest::post()
            .uri("/v1/runs")
            .set_json(body())
            .to_request();
        assert_eq!(
            actix_test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn an_unknown_body_field_is_refused() {
        let mut bad = body();
        bad["surprise"] = json!(true);
        assert!(
            serde_json::from_value::<ManualDispatchRequest>(bad).is_err(),
            "the manual dispatch body denies unknown fields"
        );

        let app = actix_test::init_service(App::new().configure(config)).await;
        let req = actix_test::TestRequest::post()
            .uri("/v1/runs")
            .set_json(body())
            .to_request();
        // Authentication runs before body handling, so an unauthenticated call
        // is still a 401 even with a well-formed body.
        assert_eq!(
            actix_test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }
}
