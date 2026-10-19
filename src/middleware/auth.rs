//! JWT authentication extractor for actix-web handler functions.
//!
//! Add [`AuthenticatedUser`] as a handler parameter to require a valid
//! `Authorization: Bearer <token>` header. The extractor verifies the token
//! signature and expiry using the app's [`Config`] and returns HTTP 401
//! automatically on any failure — the handler body never runs.
//!
//! # Example
//!
//! ```rust,ignore
//! use crate::middleware::auth::AuthenticatedUser;
//!
//! async fn protected(user: AuthenticatedUser) -> HttpResponse {
//!     HttpResponse::Ok().json(serde_json::json!({ "player": user.player_public_id }))
//! }
//! ```

use crate::{
    api::problem::{Problem, ProblemCode, ProblemError},
    config::Config,
    entities::pool_user::{PoolRole, PoolUser},
    middleware::request_id::RequestId,
    services::{auth as auth_svc, pool_identity},
};
use actix_web::{
    Error, FromRequest, HttpRequest,
    dev::Payload,
    error::{ErrorInternalServerError, ErrorServiceUnavailable, ErrorUnauthorized},
    http::{StatusCode, header},
    web,
};
pub use auth_svc::Claims;
use sqlx::MySqlPool;
use std::{
    future::{Future, Ready, ready},
    pin::Pin,
};

/// The authenticated caller, extracted from a valid `Authorization: Bearer <jwt>` header.
///
/// Add this as a parameter to any handler that requires authentication. The
/// extractor automatically returns HTTP 401 when:
/// - The `Authorization` header is absent.
/// - The header does not use the `Bearer` scheme.
/// - The token signature is invalid.
/// - The token has expired.
///
/// On success, `player_public_id` contains the player's UUID and `claims`
/// carries the full decoded [`Claims`] struct.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    /// The player's `public_id` (UUID) — safe to surface in API responses.
    pub player_public_id: String,
    /// Full decoded JWT claims, including `iat` and `exp` timestamps.
    /// Used by handlers that need expiry info or the raw subject claim.
    #[allow(dead_code)]
    pub claims: Claims,
}

impl FromRequest for AuthenticatedUser {
    type Error = Error;
    type Future = Ready<Result<Self, Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let token = match extract_bearer_token(req) {
            Ok(t) => t,
            Err(e) => return ready(Err(e)),
        };

        let cfg = match req.app_data::<web::Data<Config>>() {
            Some(c) => c,
            None => {
                tracing::error!("AuthenticatedUser extractor: Config not found in app data");
                return ready(Err(ErrorInternalServerError("server misconfigured")));
            }
        };

        match auth_svc::verify_jwt(&cfg.jwt_secret, &token) {
            Ok(claims) => ready(Ok(AuthenticatedUser {
                player_public_id: claims.sub.clone(),
                claims,
            })),
            Err(e) => {
                tracing::debug!(error = %e, "JWT verification failed");
                ready(Err(ErrorUnauthorized("invalid or expired token")))
            }
        }
    }
}

/// A signed-in **pool operator**, resolved from the JWT subject.
///
/// Unlike [`AuthenticatedUser`] (which only verifies the token), this extractor
/// also loads the `pool_users` row. That has three consequences:
///
/// - A player token is refused: its subject does not resolve to an operator.
/// - A removed member is refused on the next request, so revocation is
///   immediate even though the JWT itself is still cryptographically valid.
/// - Role changes take effect on the next request because the role is read
///   from the database rather than from a claim.
///
/// Mutating hub handlers must call [`OperatorAuth::require_mutating`] or
/// [`OperatorAuth::require_owner`]; a viewer is read-only.
#[derive(Debug, Clone)]
pub struct OperatorAuth {
    /// The resolved operator row.
    pub operator: PoolUser,
    /// Correlation id for 403 problem documents.
    pub request_id: String,
}

impl OperatorAuth {
    /// Reject the caller when it is a read-only viewer.
    ///
    /// Use this in every mutating hub handler. Returns HTTP 403.
    pub fn require_mutating(&self) -> Result<(), Error> {
        if self.operator.role == PoolRole::Viewer {
            return Err(forbidden(&self.request_id, "viewer role is read-only"));
        }
        Ok(())
    }

    /// Require the organization owner role.
    ///
    /// Use this for member and API-key management. Returns HTTP 403.
    pub fn require_owner(&self) -> Result<(), Error> {
        if self.operator.role != PoolRole::Owner {
            return Err(forbidden(&self.request_id, "owner role required"));
        }
        Ok(())
    }

    /// The operator's internal organization id.
    pub fn organization_id(&self) -> u64 {
        self.operator.organization_id
    }
}

impl FromRequest for OperatorAuth {
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let pool = req.app_data::<web::Data<MySqlPool>>().cloned();
        let request_id = RequestId::from_request(req);

        // Reuse the JWT extractor: its future is ready immediately.
        let authenticated =
            match <AuthenticatedUser as FromRequest>::from_request(req, payload).into_inner() {
                Ok(user) => user,
                Err(error) => return Box::pin(async move { Err(error) }),
            };

        Box::pin(async move {
            let Some(pool) = pool else {
                return Err(ErrorServiceUnavailable("database unavailable"));
            };
            match pool_identity::find_by_public_id(pool.get_ref(), &authenticated.player_public_id)
                .await
            {
                Ok(Some(operator)) => Ok(OperatorAuth {
                    operator,
                    request_id,
                }),
                Ok(None) => Err(ErrorUnauthorized("operator account required")),
                Err(error) => {
                    tracing::error!(error = %error, "operator lookup failed");
                    Err(ErrorInternalServerError("internal error"))
                }
            }
        })
    }
}

/// A 403 with the hub problem shape.
fn forbidden(request_id: &str, detail: &str) -> Error {
    let problem = Problem::new(
        StatusCode::FORBIDDEN,
        ProblemCode::Forbidden,
        detail.to_string(),
        request_id.to_string(),
    );
    ProblemError::new(problem).into()
}

/// Extract the raw token string from `Authorization: Bearer <token>`.
fn extract_bearer_token(req: &HttpRequest) -> Result<String, Error> {
    let header_value = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ErrorUnauthorized("Authorization header required"))?;

    let token = header_value
        .strip_prefix("Bearer ")
        .ok_or_else(|| ErrorUnauthorized("Authorization header must use Bearer scheme"))?;

    if token.is_empty() {
        return Err(ErrorUnauthorized("Bearer token must not be empty"));
    }

    Ok(token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RateLimitConfig;
    use crate::services::auth as auth_svc;
    use actix_web::{App, HttpResponse, http::StatusCode, test, web};

    const SECRET: &str = "test-secret-test-secret-test-secret-0000";

    fn stub_config() -> Config {
        Config {
            host: "127.0.0.1".into(),
            port: 8080,
            database_url: "mysql://test".into(),
            redis_url: "redis://test".into(),
            jwt_secret: SECRET.into(),
            jwt_expiry_secs: 3600,
            rate_limit: RateLimitConfig::default(),
            pool_bootstrap: None,
        }
    }

    /// A minimal protected handler used only in these tests.
    async fn protected_handler(user: AuthenticatedUser) -> HttpResponse {
        HttpResponse::Ok().json(serde_json::json!({ "player_id": user.player_public_id }))
    }

    fn app_with_auth() -> App<
        impl actix_web::dev::ServiceFactory<
            actix_web::dev::ServiceRequest,
            Config = (),
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
            InitError = (),
        >,
    > {
        App::new()
            .app_data(web::Data::new(stub_config()))
            .route("/protected", web::get().to(protected_handler))
    }

    #[actix_web::test]
    async fn missing_auth_header_returns_401() {
        let app = test::init_service(app_with_auth()).await;
        let req = test::TestRequest::get().uri("/protected").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn non_bearer_scheme_returns_401() {
        let app = test::init_service(app_with_auth()).await;
        let req = test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", "Basic dXNlcjpwYXNz"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn invalid_token_returns_401() {
        let app = test::init_service(app_with_auth()).await;
        let req = test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", "Bearer not.a.real.jwt"))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn expired_token_returns_401() {
        // ttl=0 → immediately expired (iat == exp, leeway=0)
        let token = auth_svc::issue_jwt(SECRET, "player-uuid", 0).expect("issue");
        // Give the token 1s to expire.
        std::thread::sleep(std::time::Duration::from_secs(1));

        let app = test::init_service(app_with_auth()).await;
        let req = test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn valid_token_passes_through_with_player_id() {
        let player_id = "00000000-0000-4000-8000-000000000042";
        let token = auth_svc::issue_jwt(SECRET, player_id, 60).expect("issue");

        let app = test::init_service(app_with_auth()).await;
        let req = test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["player_id"], player_id);
    }

    #[actix_web::test]
    async fn wrong_secret_returns_401() {
        // Token signed with a different secret.
        let token = auth_svc::issue_jwt("other-secret-other-secret-other-secret-xx", "pid", 60)
            .expect("issue");

        let app = test::init_service(app_with_auth()).await;
        let req = test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn empty_bearer_value_returns_401() {
        let app = test::init_service(app_with_auth()).await;
        let req = test::TestRequest::get()
            .uri("/protected")
            .insert_header(("Authorization", "Bearer "))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    fn operator(role: PoolRole) -> OperatorAuth {
        OperatorAuth {
            operator: PoolUser {
                id: 1,
                public_id: "11111111-1111-4111-8111-111111111111".into(),
                organization_id: 7,
                email: "member@example.com".into(),
                display_name: "Member".into(),
                password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".into(),
                role,
                password_change_required: false,
                last_login_at: None,
                created_at: "2026-10-19T12:00:00Z".parse().expect("ts"),
                updated_at: "2026-10-19T12:00:00Z".parse().expect("ts"),
            },
            request_id: "req-test".into(),
        }
    }

    #[actix_web::test]
    async fn viewer_is_read_only() {
        let viewer = operator(PoolRole::Viewer);
        assert!(viewer.require_mutating().is_err());
        assert!(viewer.require_owner().is_err());
    }

    #[actix_web::test]
    async fn member_may_mutate_but_not_manage_the_team() {
        let member = operator(PoolRole::Member);
        assert!(member.require_mutating().is_ok());
        assert!(member.require_owner().is_err());
        assert!(operator(PoolRole::Admin).require_mutating().is_ok());
    }

    #[actix_web::test]
    async fn owner_may_manage_the_team() {
        let owner = operator(PoolRole::Owner);
        assert!(owner.require_mutating().is_ok());
        assert!(owner.require_owner().is_ok());
    }
}
