//! Trigger HTTP surface (task 1.4.2).
//!
//! - `POST /v1/triggers/validate` — compile one trigger document and ask the
//!   named host whether it can run the agent it names, without writing pool
//!   state.
//!
//! The endpoint accepts a non-viewer member or an API key with
//! `configuration:validate`. A malformed body is a 400; a well-formed document
//! whose references fail to resolve is a 200 with `valid: false` and the named
//! field issues, mirroring configuration validation.

use crate::api::problem::{Problem, ProblemCode};
use crate::entities::api_key::ApiKeyScope;
use crate::middleware::api_key::ApiKeyAuth;
use crate::middleware::auth::OperatorAuth;
use crate::services::triggers::{self as trigger_svc, TriggerDocument, TriggerError, TriggerIssue};
use actix_web::{
    Error, FromRequest, HttpRequest, HttpResponse,
    dev::Payload,
    http::{StatusCode, header},
    web,
};
use serde::Serialize;
use sqlx::MySqlPool;
use std::{future::Future, pin::Pin};

/// Register the trigger routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/v1/triggers").route("/validate", web::post().to(validate_trigger)));
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

    /// A non-viewer member, or an API key with `configuration:validate`.
    fn require_validate(&self) -> Result<(), Error> {
        match self {
            HubAuth::Key(auth) => auth.require_scope(ApiKeyScope::ConfigurationValidate),
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

/// Trigger validate response.
#[derive(Debug, Serialize)]
pub struct TriggerValidateResponse {
    /// The trigger name, echoed back.
    pub name: String,
    /// True when the document is valid (references resolve and the host can
    /// run the agent).
    pub valid: bool,
    /// Named field issues, empty when valid.
    pub errors: Vec<TriggerIssue>,
}

#[tracing::instrument(skip(pool, auth, body))]
async fn validate_trigger(
    pool: Option<web::Data<MySqlPool>>,
    auth: HubAuth,
    body: web::Json<TriggerDocument>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(error) = auth.require_validate() {
        return error_response(error);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };

    match trigger_svc::validate_trigger(pool.get_ref(), auth.organization_id(), &body.into_inner())
        .await
    {
        Ok(outcome) => HttpResponse::Ok().json(TriggerValidateResponse {
            name: outcome.name,
            valid: outcome.valid,
            errors: outcome.errors,
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

fn error_response(error: actix_web::Error) -> HttpResponse {
    error.error_response()
}

fn map_error(error: TriggerError, req: &HttpRequest) -> HttpResponse {
    match error {
        TriggerError::Database(error) => {
            tracing::error!(error = %error, "trigger validation failed");
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

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{App, http::StatusCode as TestStatus, test};

    #[actix_web::test]
    async fn validate_requires_credentials() {
        let app = test::init_service(App::new().configure(config)).await;
        let req = test::TestRequest::post()
            .uri("/v1/triggers/validate")
            .set_json(serde_json::json!({}))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }
}
