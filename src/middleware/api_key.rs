//! API-key authentication extractor (task 1.1.3).
//!
//! Send the key as `X-API-Key: <prefix>.<secret>` or as
//! `Authorization: Bearer <prefix>.<secret>`. Member JWTs do not start with the
//! `tpk_` marker, so the two credential types cannot be confused.
//!
//! Authentication and authorization are separate, deliberately:
//!
//! - unknown / malformed / revoked key → **401 Unauthorized**
//! - valid key without the needed scope → **403 Forbidden**
//!
//! Handlers that need a capability call [`ApiKeyAuth::require_scope`].

use crate::entities::api_key::{ApiKey, ApiKeyScope};
use crate::services::api_keys::{self as api_key_svc, ApiKeyServiceError};
use actix_web::{
    Error, FromRequest, HttpRequest,
    dev::Payload,
    error::{ErrorForbidden, ErrorInternalServerError, ErrorServiceUnavailable, ErrorUnauthorized},
    http::header,
    web,
};
use sqlx::MySqlPool;
use std::{
    future::{Future, ready},
    pin::Pin,
};

/// The authenticated API key.
#[derive(Debug, Clone)]
pub struct ApiKeyAuth {
    /// Resolved key row (never carries the secret).
    pub key: ApiKey,
}

impl ApiKeyAuth {
    /// Require one scope, returning HTTP 403 when it is absent.
    pub fn require_scope(&self, scope: ApiKeyScope) -> Result<(), Error> {
        if !self.key.has_scope(scope) {
            return Err(ErrorForbidden("API key lacks the required scope"));
        }
        Ok(())
    }

    /// The key's organization.
    pub fn organization_id(&self) -> u64 {
        self.key.organization_id
    }
}

impl FromRequest for ApiKeyAuth {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let pool = req.app_data::<web::Data<MySqlPool>>().cloned();
        let token = match extract_key(req) {
            Some(token) => token.to_string(),
            None => {
                return Box::pin(ready(Err(ErrorUnauthorized(
                    "API key required (X-API-Key or Bearer tpk_...)",
                ))));
            }
        };

        Box::pin(async move {
            let Some(pool) = pool else {
                return Err(ErrorServiceUnavailable("database unavailable"));
            };
            match api_key_svc::authenticate(pool.get_ref(), &token).await {
                Ok(key) => Ok(ApiKeyAuth { key }),
                Err(ApiKeyServiceError::InvalidCredentials) => {
                    Err(ErrorUnauthorized("invalid API key"))
                }
                Err(error) => {
                    tracing::error!(error = %error, "API key authentication failed");
                    Err(ErrorInternalServerError("internal error"))
                }
            }
        })
    }
}

/// Extract a key from `X-API-Key`, else a Bearer token with the key marker.
fn extract_key(req: &HttpRequest) -> Option<&str> {
    if let Some(value) = req
        .headers()
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(value);
    }

    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| value.starts_with(api_key_svc::KEY_PREFIX_MARKER))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::api_key::ApiKeyStatus;
    use actix_web::http::StatusCode;

    fn auth(scopes: &str) -> ApiKeyAuth {
        ApiKeyAuth {
            key: ApiKey {
                id: 1,
                public_id: "11111111-1111-4111-8111-111111111111".into(),
                organization_id: 7,
                name: "CI".into(),
                prefix: "tpk_11111111".into(),
                secret_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".into(),
                scopes: scopes.into(),
                status: ApiKeyStatus::Active,
                created_by: Some(2),
                last_used_at: None,
                revoked_at: None,
                created_at: "2026-10-19T12:00:00Z".parse().expect("ts"),
                updated_at: "2026-10-19T12:00:00Z".parse().expect("ts"),
            },
        }
    }

    #[test]
    fn missing_scope_is_403_not_401() {
        let key = auth(r#"["projects:read"]"#);
        assert!(key.require_scope(ApiKeyScope::ProjectsRead).is_ok());
        let error = key
            .require_scope(ApiKeyScope::RunsDispatch)
            .expect_err("scope missing");
        assert_eq!(
            error.as_response_error().status_code(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn key_with_no_scopes_grants_nothing() {
        let key = auth("[]");
        assert!(key.require_scope(ApiKeyScope::ProjectsRead).is_err());
    }
}
