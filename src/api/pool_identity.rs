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

use crate::api::problem::{Problem, ProblemCode};
use crate::config::Config;
use crate::entities::organization::OrganizationView;
use crate::entities::pool_invitation::PoolInvitationView;
use crate::entities::pool_user::{PoolRole, PoolUserView};
use crate::middleware::auth::OperatorAuth;
use crate::services::api_keys::{AuditActor, AuditEntry};
use crate::services::auth as auth_svc;
use crate::services::pool_identity::{
    self as identity_svc, INVITATION_TTL_SECS, NewOrganization, PoolIdentityError,
};
use actix_web::{HttpRequest, HttpResponse, http::StatusCode, web};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use validator::Validate;

/// Register the setup and pool identity routes.
pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/v1/setup")
            .route("/status", web::get().to(setup_status))
            .route("", web::post().to(setup)),
    )
    .service(
        web::scope("/v1/pool")
            .route("/login", web::post().to(login))
            .route("/me", web::get().to(me))
            .route("/password", web::post().to(change_password))
            .route("/members", web::get().to(list_members))
            .route("/members/{id}", web::delete().to(remove_member))
            .route("/members/{id}", web::patch().to(change_member_role))
            .route("/invitations", web::get().to(list_invitations))
            .route("/invitations", web::post().to(invite_member))
            .route("/invitations/accept", web::post().to(accept_invitation))
            .route("/invitations/{id}", web::delete().to(revoke_invitation)),
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
async fn setup_status(
    pool: Option<web::Data<MySqlPool>>,
    cfg: web::Data<Config>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    let bootstrap_configured = cfg.pool_bootstrap.is_some();

    match identity_svc::setup_status(pool.get_ref(), bootstrap_configured).await {
        Ok(status) => HttpResponse::Ok().json(SetupStatusResponse {
            needs_setup: status.needs_setup,
            bootstrap_configured: status.bootstrap_configured,
        }),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, body))]
async fn setup(
    pool: Option<web::Data<MySqlPool>>,
    body: web::Json<SetupRequest>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(e) = body.validate() {
        return validation_error(&req, &e.to_string());
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
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, cfg, body))]
async fn login(
    pool: Option<web::Data<MySqlPool>>,
    cfg: web::Data<Config>,
    body: web::Json<LoginRequest>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(e) = body.validate() {
        return validation_error(&req, &e.to_string());
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
                    internal_error(&req)
                }
            }
        }
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn me(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };

    match identity_svc::find_organization(pool.get_ref(), auth.organization_id()).await {
        Ok(Some(organization)) => HttpResponse::Ok().json(MeResponse {
            organization: OrganizationView::from(&organization),
            user: PoolUserView::from(&auth.operator),
        }),
        Ok(None) => {
            tracing::error!(
                organization_id = auth.organization_id(),
                "operator organization missing"
            );
            internal_error(&req)
        }
        Err(e) => map_error(e, &req),
    }
}
#[tracing::instrument(skip(pool, auth, body))]
async fn change_password(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    body: web::Json<ChangePasswordRequest>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(e) = auth.require_mutating() {
        return error_response(e);
    }
    if let Err(e) = body.validate() {
        return validation_error(&req, &e.to_string());
    }

    match identity_svc::change_password(
        pool.get_ref(),
        &auth.operator.public_id,
        &body.current_password,
        &body.new_password,
    )
    .await
    {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(e) => map_error(e, &req),
    }
}

/// Member list response.
#[derive(Debug, Serialize)]
pub struct MembersResponse {
    /// Operators in the organization.
    pub members: Vec<PoolUserView>,
}

/// Invitation list response.
#[derive(Debug, Serialize)]
pub struct InvitationsResponse {
    /// Invitations still awaiting acceptance.
    pub invitations: Vec<PoolInvitationView>,
}

/// Invite payload.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct InviteRequest {
    /// Invitee email.
    #[validate(email)]
    pub email: String,
    /// Role granted on acceptance; defaults to `member`.
    #[serde(default)]
    pub role: PoolRole,
}

/// Invitation creation response: the raw token is shown exactly once.
#[derive(Debug, Serialize)]
pub struct InviteResponse {
    /// Invitation metadata (never carries the token hash).
    pub invitation: PoolInvitationView,
    /// One-time acceptance token. Not stored in plaintext.
    pub token: String,
}

/// Invitation acceptance payload.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct AcceptInvitationRequest {
    /// The one-time token from the invitation.
    #[validate(length(min = 1, max = 512))]
    pub token: String,
    /// Display name for the new member.
    #[validate(length(min = 1, max = 128))]
    pub display_name: String,
    /// Chosen password; at least 12 characters.
    #[validate(length(min = 12, max = 1024))]
    pub password: String,
}

/// Request body for changing a member's role.
#[derive(Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct UpdateRoleRequest {
    /// New role.
    pub role: PoolRole,
}

#[tracing::instrument(skip(pool, auth))]
async fn list_members(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match identity_svc::list_members(pool.get_ref(), auth.organization_id()).await {
        Ok(members) => HttpResponse::Ok().json(MembersResponse {
            members: members.iter().map(PoolUserView::from).collect(),
        }),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn list_invitations(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(e) = auth.require_owner() {
        return error_response(e);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match identity_svc::list_pending_invitations(pool.get_ref(), auth.organization_id()).await {
        Ok(invitations) => HttpResponse::Ok().json(InvitationsResponse {
            invitations: invitations.iter().map(PoolInvitationView::from).collect(),
        }),
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn invite_member(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    body: web::Json<InviteRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(e) = auth.require_owner() {
        return error_response(e);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(e) = body.validate() {
        return validation_error(&req, &e.to_string());
    }

    match identity_svc::create_invitation(
        pool.get_ref(),
        auth.organization_id(),
        Some(auth.operator.id),
        &body.email,
        body.role,
        INVITATION_TTL_SECS,
    )
    .await
    {
        Ok((invitation, token)) => {
            crate::services::api_keys::audit(
                pool.get_ref(),
                AuditEntry {
                    organization_id: auth.organization_id(),
                    actor: AuditActor::member(&auth.operator.public_id, &auth.operator.email),
                    action: "invitation.create",
                    target_kind: Some("invitation"),
                    target_public_id: Some(&invitation.public_id),
                    detail: serde_json::json!({
                        "email": invitation.email,
                        "role": invitation.role.as_str(),
                    }),
                },
            )
            .await;
            HttpResponse::Created().json(InviteResponse {
                invitation: PoolInvitationView::from(&invitation),
                token,
            })
        }
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn revoke_invitation(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(e) = auth.require_owner() {
        return error_response(e);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    let public_id = path.into_inner();
    match identity_svc::revoke_invitation(pool.get_ref(), auth.organization_id(), &public_id).await
    {
        Ok(()) => {
            crate::services::api_keys::audit(
                pool.get_ref(),
                AuditEntry {
                    organization_id: auth.organization_id(),
                    actor: AuditActor::member(&auth.operator.public_id, &auth.operator.email),
                    action: "invitation.revoke",
                    target_kind: Some("invitation"),
                    target_public_id: Some(&public_id),
                    detail: serde_json::Value::Null,
                },
            )
            .await;
            HttpResponse::NoContent().finish()
        }
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, body))]
async fn accept_invitation(
    pool: Option<web::Data<MySqlPool>>,
    cfg: web::Data<Config>,
    body: web::Json<AcceptInvitationRequest>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    if let Err(e) = body.validate() {
        return validation_error(&req, &e.to_string());
    }

    match identity_svc::accept_invitation(
        pool.get_ref(),
        &body.token,
        &body.display_name,
        &body.password,
    )
    .await
    {
        Ok(user) => {
            match auth_svc::issue_jwt(&cfg.jwt_secret, &user.public_id, cfg.jwt_expiry_secs) {
                Ok(token) => HttpResponse::Created().json(LoginResponse {
                    token,
                    expires_in: cfg.jwt_expiry_secs,
                    must_change_password: false,
                    user: PoolUserView::from(&user),
                }),
                Err(e) => {
                    tracing::error!(error = %e, "invitation JWT issue failed");
                    internal_error(&req)
                }
            }
        }
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth, body))]
async fn change_member_role(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    path: web::Path<String>,
    body: web::Json<UpdateRoleRequest>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(e) = auth.require_owner() {
        return error_response(e);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    let public_id = path.into_inner();
    match identity_svc::change_member_role(
        pool.get_ref(),
        auth.organization_id(),
        &public_id,
        body.role,
    )
    .await
    {
        Ok(user) => {
            crate::services::api_keys::audit(
                pool.get_ref(),
                AuditEntry {
                    organization_id: auth.organization_id(),
                    actor: AuditActor::member(&auth.operator.public_id, &auth.operator.email),
                    action: "member.role_change",
                    target_kind: Some("member"),
                    target_public_id: Some(&user.public_id),
                    detail: serde_json::json!({ "role": user.role.as_str() }),
                },
            )
            .await;
            HttpResponse::Ok().json(PoolUserView::from(&user))
        }
        Err(e) => map_error(e, &req),
    }
}

#[tracing::instrument(skip(pool, auth))]
async fn remove_member(
    pool: Option<web::Data<MySqlPool>>,
    auth: OperatorAuth,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    if let Err(e) = auth.require_owner() {
        return error_response(e);
    }
    let Some(pool) = pool else {
        return db_unavailable(&req);
    };
    match identity_svc::remove_member(pool.get_ref(), auth.organization_id(), &path.into_inner())
        .await
    {
        Ok(()) => HttpResponse::NoContent().finish(),
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

fn internal_error(req: &HttpRequest) -> HttpResponse {
    Problem::from_request(
        req,
        StatusCode::INTERNAL_SERVER_ERROR,
        ProblemCode::InternalError,
        "internal error",
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

/// Convert an actix extractor error (from a role guard) into a response.
///
/// Our guards already return [`crate::api::problem::ProblemError`]; anything
/// else is converted by `HubProblem` middleware.
fn error_response(error: actix_web::Error) -> HttpResponse {
    error.error_response()
}

/// Map domain errors to problem documents without leaking credential details.
fn map_error(error: PoolIdentityError, req: &HttpRequest) -> HttpResponse {
    let (status, code, detail) = match error {
        PoolIdentityError::AlreadyInitialized => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            "pool is already initialized".to_string(),
        ),
        PoolIdentityError::AlreadyBootstrapped => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            "pool is already bootstrapped".to_string(),
        ),
        PoolIdentityError::Invalid(message) => (
            StatusCode::BAD_REQUEST,
            ProblemCode::InvalidRequest,
            message,
        ),
        PoolIdentityError::WeakPassword => (
            StatusCode::UNPROCESSABLE_ENTITY,
            ProblemCode::ValidationFailed,
            "password must be at least 12 characters".to_string(),
        ),
        PoolIdentityError::InvalidCredentials => (
            StatusCode::UNAUTHORIZED,
            ProblemCode::Unauthorized,
            "invalid credentials".to_string(),
        ),
        PoolIdentityError::Forbidden(message) => {
            (StatusCode::FORBIDDEN, ProblemCode::Forbidden, message)
        }
        PoolIdentityError::LastOwner => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            "the last owner cannot be removed or demoted".to_string(),
        ),
        PoolIdentityError::NotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "member not found".to_string(),
        ),
        PoolIdentityError::InvitationInvalid => (
            StatusCode::BAD_REQUEST,
            ProblemCode::InvalidRequest,
            "invitation is invalid".to_string(),
        ),
        PoolIdentityError::InvitationUsed => (
            StatusCode::CONFLICT,
            ProblemCode::Conflict,
            "invitation has already been used".to_string(),
        ),
        PoolIdentityError::InvitationRevoked => (
            StatusCode::GONE,
            ProblemCode::Gone,
            "invitation has been revoked".to_string(),
        ),
        PoolIdentityError::InvitationExpired => (
            StatusCode::GONE,
            ProblemCode::Gone,
            "invitation has expired".to_string(),
        ),
        PoolIdentityError::InvitationNotFound => (
            StatusCode::NOT_FOUND,
            ProblemCode::NotFound,
            "invitation not found".to_string(),
        ),
        other => {
            tracing::error!(error = %other, "pool identity operation failed");
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
            public_origin: None,
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
