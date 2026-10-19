//! Host enrollment HTTP surface (task 1.2.2).
//!
//! - `POST /v1/hosts/enrollment-tokens` — mint a ten-minute single-use token
//!   for a registered host. Accepts an API key with `daemons:enroll` or an
//!   operator member token.
//! - `POST /v1/hosts/enroll` — public exchange: the host presents the token
//!   and the credential it generated. Only the credential hash is stored.
//!
//! The relationship credential is never returned by any read endpoint.

use crate::api::problem::{Problem, ProblemCode};
use crate::entities::enrollment_token::EnrollmentTokenView;
use crate::entities::host::HostView;
use crate::middleware::api_key::ApiKeyAuth;
use crate::middleware::auth::OperatorAuth;
use crate::services::api_keys::AuditActor;
use crate::services::enrollment::{self as enrollment_svc, EnrollmentError};
use actix_web::{
    Error, FromRequest, HttpRequest, HttpResponse,
    dev::Payload,
    http::{StatusCode, header},
    web,
};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use std::{future::Future, pin::Pin};
use validator::Validate;

use crate::entities::api_key::ApiKeyScope;

/// Caller allowed to mint enrollment tokens: a scoped API key or a member.
pub enum EnrollAuth {
    /// API key carrying `daemons:enroll`.
    ApiKey(ApiKeyAuth),
    /// Non-viewer operator member (control plane).
    Member(OperatorAuth),
}

impl EnrollAuth {
    /// Organization id both principals carry.
    pub fn organization_id(&self) -> u64 {
        match self {
            EnrollAuth::ApiKey(auth) => auth.organization_id(),
            EnrollAuth::Member(auth) => auth.organization_id(),
        }
    }

    /// Internal API-key id when the caller is a key.
    pub fn api_key_id(&self) -> Option<u64> {
        match self {
            EnrollAuth::ApiKey(auth) => Some(auth.key.id),
            EnrollAuth::Member(_) => None,
        }
    }

    /// Audit actor for the mint operation.
    pub fn actor(&self) -> AuditActor {
        match self {
            EnrollAuth::ApiKey(auth) => AuditActor::api_key(&auth.key.public_id, &auth.key.prefix),
            EnrollAuth::Member(auth) => {
                AuditActor::member(&auth.operator.public_id, &auth.operator.email)
            }
        }
    }
}

impl FromRequest for EnrollAuth {
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
                .is_some_and(|value| value.starts_with("tpk_"));

        let key_future = ApiKeyAuth::from_request(req, payload);
        let member_future = OperatorAuth::from_request(req, payload);

        Box::pin(async move {
            if is_api_key {
                let auth = key_future.await?;
                auth.require_scope(ApiKeyScope::DaemonsEnroll)?;
                Ok(EnrollAuth::ApiKey(auth))
            } else {
                let auth = member_future.await?;
                auth.require_mutating()?;
                Ok(EnrollAuth::Member(auth))
            }
        })
    }
}

/// Mint payload.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct MintEnrollmentTokenRequest {
    /// Public id of a registered host.
    #[validate(length(min = 36, max = 36))]
    pub host_id: String,
}

/// Mint response: the raw token is shown exactly once.
#[derive(Debug, Serialize)]
pub struct MintEnrollmentTokenResponse {
    /// `<public_id>.<secret>` enrollment token.
    pub token: String,
    /// Token metadata (never includes the hash).
    pub enrollment_token: EnrollmentTokenView,
}

/// Enrollment exchange payload.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct EnrollHostRequest {
    /// One-time enrollment token.
    #[validate(length(min = 1, max = 512))]
    pub token: String,
    /// Host-generated long-lived credential.
    #[validate(length(min = 32, max = 1024))]
    pub credential: String,
}

#[tracing::instrument(skip(pool, auth, body))]
pub(crate) async fn mint_enrollment_token(
    pool: Option<web::Data<MySqlPool>>,
    auth: EnrollAuth,
    body: web::Json<MintEnrollmentTokenRequest>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(e) = body.validate() {
        return validation_error(&req, &e.to_string());
    }

    match enrollment_svc::mint_enrollment_token(
        pool.get_ref(),
        auth.organization_id(),
        &body.host_id,
        auth.api_key_id(),
        auth.actor(),
    )
    .await
    {
        Ok((token, raw)) => HttpResponse::Created().json(MintEnrollmentTokenResponse {
            token: raw,
            enrollment_token: EnrollmentTokenView::from(&token),
        }),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, body))]
pub(crate) async fn enroll_host(
    pool: Option<web::Data<MySqlPool>>,
    body: web::Json<EnrollHostRequest>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(e) = body.validate() {
        return validation_error(&req, &e.to_string());
    }

    match enrollment_svc::enroll_host(pool.get_ref(), &body.token, &body.credential).await {
        Ok(host) => HttpResponse::Created().json(HostView::from(&host)),
        Err(e) => map_error(e, &req),
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
        format!("validation: {detail}"),
    )
    .into_response()
}

fn map_error(error: EnrollmentError, req: &HttpRequest) -> HttpResponse {
    let (status, code, detail) = match error {
        EnrollmentError::Invalid(message) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            message,
        ),
        EnrollmentError::HostNotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "host not found".to_string(),
        ),
        EnrollmentError::AlreadyEnrolled => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            "host is already enrolled".to_string(),
        ),
        EnrollmentError::TokenInvalid => (
            StatusCode::BAD_REQUEST,
            ProblemCode::InvalidRequest,
            "enrollment token is invalid".to_string(),
        ),
        EnrollmentError::TokenExpired => (
            StatusCode::GONE,
            ProblemCode::Gone,
            "enrollment token has expired".to_string(),
        ),
        EnrollmentError::TokenUsed => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            "enrollment token has already been used".to_string(),
        ),
        EnrollmentError::TokenRevoked => (
            StatusCode::GONE,
            ProblemCode::Gone,
            "enrollment token has been revoked".to_string(),
        ),
        other => {
            tracing::error!(error = %other, "host enrollment failed");
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
    use actix_web::{App, http::StatusCode as TestStatus, test};

    #[actix_web::test]
    async fn mint_requires_credentials() {
        let app = test::init_service(App::new().route(
            "/v1/hosts/enrollment-tokens",
            web::post().to(mint_enrollment_token),
        ))
        .await;
        let req = test::TestRequest::post()
            .uri("/v1/hosts/enrollment-tokens")
            .set_json(serde_json::json!({"host_id": "11111111-1111-4111-8111-111111111111"}))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            TestStatus::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn enroll_exchange_requires_a_body() {
        let app =
            test::init_service(App::new().route("/v1/hosts/enroll", web::post().to(enroll_host)))
                .await;
        let req = test::TestRequest::post()
            .uri("/v1/hosts/enroll")
            .set_json(serde_json::json!({"token": "x", "credential": "short"}))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            TestStatus::UNPROCESSABLE_ENTITY
        );
    }
}
