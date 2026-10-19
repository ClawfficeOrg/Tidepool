//! Pool identity HTTP surface — first-run setup and operator sign-in.
//!
//! Routes:
//!
//! - `GET  /v1/setup/status` — does the pool still need setup?
//! - `POST /v1/setup`        — create the organization and first owner from
//!   the browser (only while no organization exists).
//! - `POST /v1/pool/login`   — operator sign-in; returns a JWT.
//! - `GET  /v1/pool/me`      — the signed-in operator and organization.
//! - `POST /v1/pool/password`— replace the bootstrap/temporary password.
//!
//! These are hub routes, distinct from the player `/v1/auth` surface. An
//! operator token is a normal JWT whose subject is a `pool_users.public_id`;
//! handlers here verify that subject exists as an operator, so a player token
//! is refused.
//!
//! `TIDEBOOT_*` is applied at startup by `main`, not through an endpoint.

use crate::config::Config;
use crate::entities::organization::OrganizationView;
use crate::entities::pool_user::PoolUserView;
use crate::middleware::auth::AuthenticatedUser;
use crate::services::auth as auth_svc;
use crate::services::pool_identity::{self as identity_svc, NewOrganization, PoolIdentityError};
use actix_web::{HttpResponse, web};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use validator::Validate;

/// Register the setup and pool identity routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1")
            .service(
                web::scope("/setup")
                    .route("/status", web::get().to(setup_status))
                    .route("", web::post().to(setup)),
            )
            .service(
                web::scope("/pool")
                    .route("/login", web::post().to(login))
                    .route("/me", web::get().to(me))
                    .route("/password", web::post().to(change_password)),
            ),
    );
}

/// `GET /v1/setup/status` response.
#[derive(Debug, Serialize)]
pub struct SetupStatusResponse {
    /// True when no organization exists and browser setup is allowed.
    pub needs_setup: bool,
    /// True when `TIDEBOOT_*` will create the first operator at startup.
    pub bootstrap_configured: bool,
}

/// First-run browser setup payload.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SetupRequest {
    /// Organization name.
    #[validate(length(min = 1, max = 128))]
    pub organization_name: String,
    /// Owner login email.
    #[validate(email)]
    pub email: String,
    /// Owner display name.
    #[validate(length(min = 1, max = 128))]
    pub display_name: String,
    /// Chosen password; at least 12 characters.
    #[validate(length(min = 12, max = 1024))]
    pub password: String,
}

/// Operator sign-in payload.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    /// Login email.
    #[validate(email)]
    pub email: String,
    /// Plaintext password.
    #[validate(length(min = 1, max = 1024))]
    pub password: String,
}

/// Successful operator sign-in.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// Signed JWT (subject = operator's public id).
    pub token: String,
    /// Token lifetime in seconds.
    pub expires_in: u64,
    /// True while the operator must replace the bootstrap password.
    pub must_change_password: bool,
    /// Operator projection.
    pub user: PoolUserView,
}

/// Password-change payload.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ChangePasswordRequest {
    /// Current (possibly temporary) password.
    #[validate(length(min = 1, max = 1024))]
    pub current_password: String,
    /// Replacement password; at least 12 characters.
    #[validate(length(min = 12, max = 1024))]
    pub new_password: String,
}

/// Response for a successful setup.
#[derive(Debug, Serialize)]
pub struct SetupResponse {
    /// Created organization.
    pub organization: OrganizationView,
    /// Created owner account (never carries a password hash).
    pub user: PoolUserView,
}

/// Response for `GET /v1/pool/me`.
#[derive(Debug, Serialize)]
pub struct MeResponse {
    /// Organization the operator belongs to.
    pub organization: OrganizationView,
    /// Signed-in operator.
    pub user: PoolUserView,
}

#[tracing::instrument(skip(pool, cfg))]
async fn setup_status(pool: Option<web::Data<MySqlPool>>, cfg: web::Data<Config>) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable();
    };
    let bootstrap_configured = cfg.pool_bootstrap.is_some();

    match identity_svc::setup_status(pool.get_ref(), bootstrap_configured).await {
        Ok(status) => HttpResponse::Ok().json(SetupStatusResponse {
            needs_setup: status.needs_setup,
            bootstrap_configured: status.bootstrap_configured,
        }),
        Err(e) => map_error(e),
    }
}

#[tracing::instrument(skip(pool, body))]
async fn setup(pool: Option<web::Data<MySqlPool>>, body: web::Json<SetupRequest>) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable();
    };
    if let Err(e) = body.validate() {
        return HttpResponse::BadRequest().json(error_body(&format!("validation: {e}")));
    }

    let input = NewOrganization {
        organization_name: body.organization_name.clone(),
        email: body.email.clone(),
        display_name: body.display_name.clone(),
        password: body.password.clone(),
    };

    match identity_svc::setup_pool(pool.get_ref(), input).await {
        Ok((organization, user)) => HttpResponse::Created().json(SetupResponse {
            organization: OrganizationView::from(&organization),
            user: PoolUserView::from(&user),
        }),
        Err(e) => map_error(e),
    }
}

#[tracing::instrument(skip(pool, cfg, body))]
async fn login(
    pool: Option<web::Data<MySqlPool>>,
    cfg: web::Data<Config>,
    body: web::Json<LoginRequest>,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable();
    };
    if let Err(e) = body.validate() {
        return HttpResponse::BadRequest().json(error_body(&format!("validation: {e}")));
    }

    match identity_svc::authenticate(pool.get_ref(), &body.email, &body.password).await {
        Ok(user) => {
            match auth_svc::issue_jwt(&cfg.jwt_secret, &user.public_id, cfg.jwt_expiry_secs) {
                Ok(token) => HttpResponse::Ok().json(LoginResponse {
                    token,
                    expires_in: cfg.jwt_expiry_secs,
                    must_change_password: user.password_change_required,
                    user: PoolUserView::from(&user),
                }),
                Err(e) => {
                    tracing::error!(error = %e, "operator JWT issue failed");
                    HttpResponse::InternalServerError().json(error_body("internal error"))
                }
            }
        }
        Err(e) => map_error(e),
    }
}

#[tracing::instrument(skip(pool, user))]
async fn me(pool: Option<web::Data<MySqlPool>>, user: AuthenticatedUser) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable();
    };

    match identity_svc::find_by_public_id(pool.get_ref(), &user.player_public_id).await {
        Ok(Some(operator)) => {
            match identity_svc::find_organization(pool.get_ref(), operator.organization_id).await {
                Ok(Some(organization)) => HttpResponse::Ok().json(MeResponse {
                    organization: OrganizationView::from(&organization),
                    user: PoolUserView::from(&operator),
                }),
                Ok(None) => {
                    tracing::error!(
                        organization_id = operator.organization_id,
                        "operator organization missing"
                    );
                    HttpResponse::InternalServerError().json(error_body("internal error"))
                }
                Err(e) => map_error(e),
            }
        }
        Ok(None) => HttpResponse::Unauthorized().json(error_body("operator account required")),
        Err(e) => map_error(e),
    }
}

#[tracing::instrument(skip(pool, user, body))]
async fn change_password(
    pool: Option<web::Data<MySqlPool>>,
    user: AuthenticatedUser,
    body: web::Json<ChangePasswordRequest>,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable();
    };
    if let Err(e) = body.validate() {
        return HttpResponse::BadRequest().json(error_body(&format!("validation: {e}")));
    }

    match identity_svc::change_password(
        pool.get_ref(),
        &user.player_public_id,
        &body.current_password,
        &body.new_password,
    )
    .await
    {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => map_error(e),
    }
}

fn db_unavailable() -> HttpResponse {
    HttpResponse::ServiceUnavailable().json(error_body("database unavailable"))
}

fn error_body(msg: &str) -> serde_json::Value {
    serde_json::json!({ "error": msg })
}

/// Map domain errors to HTTP without leaking credential details.
fn map_error(error: PoolIdentityError) -> HttpResponse {
    match error {
        PoolIdentityError::AlreadyInitialized => {
            HttpResponse::Conflict().json(error_body("pool is already initialized"))
        }
        PoolIdentityError::AlreadyBootstrapped => {
            HttpResponse::Conflict().json(error_body("pool is already bootstrapped"))
        }
        PoolIdentityError::Invalid(message) => {
            HttpResponse::BadRequest().json(error_body(&message))
        }
        PoolIdentityError::WeakPassword => {
            HttpResponse::BadRequest().json(error_body("password must be at least 12 characters"))
        }
        PoolIdentityError::InvalidCredentials => {
            HttpResponse::Unauthorized().json(error_body("invalid credentials"))
        }
        PoolIdentityError::NotFound => {
            HttpResponse::Unauthorized().json(error_body("operator account required"))
        }
        other => {
            tracing::error!(error = %other, "pool identity operation failed");
            HttpResponse::InternalServerError().json(error_body("internal error"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{PoolBootstrapConfig, RateLimitConfig};
    use actix_web::{App, http::StatusCode, test};

    fn stub_config(bootstrap: Option<PoolBootstrapConfig>) -> Config {
        Config {
            host: "127.0.0.1".into(),
            port: 8080,
            database_url: "mysql://test".into(),
            redis_url: "redis://test".into(),
            jwt_secret: "a".repeat(Config::MIN_JWT_SECRET_LEN),
            jwt_expiry_secs: 3600,
            rate_limit: RateLimitConfig::default(),
            pool_bootstrap: bootstrap,
        }
    }

    #[actix_web::test]
    async fn status_returns_503_without_pool() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(stub_config(None)))
                .configure(config),
        )
        .await;
        let req = test::TestRequest::get()
            .uri("/v1/setup/status")
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[actix_web::test]
    async fn setup_returns_503_without_pool() {
        let app = test::init_service(App::new().configure(config)).await;
        let req = test::TestRequest::post()
            .uri("/v1/setup")
            .set_json(serde_json::json!({
                "organization_name": "Rock Pool",
                "email": "owner@example.com",
                "display_name": "Owner",
                "password": "twelve-chars!"
            }))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[actix_web::test]
    async fn login_returns_503_without_pool() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(stub_config(None)))
                .configure(config),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/v1/pool/login")
            .set_json(serde_json::json!({
                "email": "owner@example.com",
                "password": "twelve-chars!"
            }))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[actix_web::test]
    async fn me_requires_a_token() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(stub_config(None)))
                .configure(config),
        )
        .await;
        let req = test::TestRequest::get().uri("/v1/pool/me").to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[actix_web::test]
    async fn change_password_requires_a_token() {
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(stub_config(None)))
                .configure(config),
        )
        .await;
        let req = test::TestRequest::post()
            .uri("/v1/pool/password")
            .set_json(serde_json::json!({
                "current_password": "twelve-chars!",
                "new_password": "another-12-chars"
            }))
            .to_request();
        assert_eq!(
            test::call_service(&app, req).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
}
