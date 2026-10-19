//! Integration tests for pool identity: first-run setup, `TIDEBOOT_*`
//! bootstrap and operator password rotation.
//!
//! Require a live MySQL; skipped by default (`#[ignore]`).
//! Run with: `cargo test --test pool_identity_integration -- --ignored`
//!
//! Defaults to the Docker dev stack:
//!   mysql://tidepool:password@127.0.0.1:3306/tidepool_dev
//! Override with `DATABASE_URL`.
//!
//! v1 is single-tenant, so these tests serialise on `DB_LOCK` and clear the
//! two identity tables before each case.

use actix_web::{App, http::StatusCode, test, web};
use serde_json::Value;
use sqlx::MySqlPool;
use tidepool::{
    api,
    config::{Config, PoolBootstrapConfig, RateLimitConfig},
    db,
    entities::{api_key::ApiKeyScope, pool_user::PoolRole},
    middleware::{problem::HubProblem, request_id::RequestIdMiddleware},
    services::{
        api_keys::{self as api_key_svc, AuditActor},
        auth as auth_svc, enrollment as enrollment_svc, host_presence, hosts as host_svc,
        pool_identity::{self, BootstrapOutcome, NewOrganization, PoolIdentityError},
    },
};

const DEFAULT_DB_URL: &str = "mysql://tidepool:password@127.0.0.1:3306/tidepool_dev";
const JWT_SECRET: &str = "integration-test-jwt-secret-min-32-chars";
const BOOTSTRAP_PASSWORD: &str = "temporary-bootstrap-1";
const NEW_PASSWORD: &str = "replacement-password-1";

static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn pool() -> MySqlPool {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DB_URL.into());
    db::build_pool(&db_url).await.expect(
        "DB connection failed — is Docker up? Run: docker compose -f config/docker-compose.dev.yml up -d",
    )
}

async fn reset(pool: &MySqlPool) {
    sqlx::query("DELETE FROM audit_events")
        .execute(pool)
        .await
        .expect("clear audit_events");
    sqlx::query("DELETE FROM api_keys")
        .execute(pool)
        .await
        .expect("clear api_keys");
    sqlx::query("DELETE FROM pool_invitations")
        .execute(pool)
        .await
        .expect("clear pool_invitations");
    sqlx::query("DELETE FROM pool_users")
        .execute(pool)
        .await
        .expect("clear pool_users");
    sqlx::query("DELETE FROM organizations")
        .execute(pool)
        .await
        .expect("clear organizations");
}

fn config(pool_bootstrap: Option<PoolBootstrapConfig>) -> Config {
    Config {
        host: "127.0.0.1".into(),
        port: 8080,
        database_url: DEFAULT_DB_URL.into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        jwt_secret: JWT_SECRET.into(),
        jwt_expiry_secs: 3600,
        rate_limit: RateLimitConfig::default(),
        pool_bootstrap,
    }
}

fn bootstrap_config() -> PoolBootstrapConfig {
    PoolBootstrapConfig {
        organization_name: "Integration Pool".into(),
        owner_email: "owner-integration@example.com".into(),
        owner_password: BOOTSTRAP_PASSWORD.into(),
        owner_display_name: "Integration Owner".into(),
    }
}

async fn org_count(pool: &MySqlPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM organizations")
        .fetch_one(pool)
        .await
        .expect("count organizations")
}

// ── browser setup ─────────────────────────────────────────────────────────

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn browser_setup_creates_one_owner_and_refuses_a_second_organization() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::pool_identity::config),
    )
    .await;

    let status = test::TestRequest::get()
        .uri("/v1/setup/status")
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, status).await).await;
    assert_eq!(body["needs_setup"], true);
    assert_eq!(body["bootstrap_configured"], false);

    let setup = test::TestRequest::post()
        .uri("/v1/setup")
        .set_json(serde_json::json!({
            "organization_name": "Rock Pool",
            "email": "Owner@Example.com",
            "display_name": "Owner",
            "password": BOOTSTRAP_PASSWORD
        }))
        .to_request();
    let resp = test::call_service(&app, setup).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["organization"]["name"], "Rock Pool");
    assert_eq!(body["user"]["email"], "owner@example.com");
    assert_eq!(body["user"]["role"], "owner");
    assert_eq!(body["user"]["password_change_required"], false);
    assert!(body["user"].get("password_hash").is_none());

    let status = test::TestRequest::get()
        .uri("/v1/setup/status")
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, status).await).await;
    assert_eq!(body["needs_setup"], false);

    // A second organization is refused through the only endpoint that can
    // create one.
    let second = test::TestRequest::post()
        .uri("/v1/setup")
        .set_json(serde_json::json!({
            "organization_name": "Second Pool",
            "email": "second@example.com",
            "display_name": "Second",
            "password": BOOTSTRAP_PASSWORD
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, second).await.status(),
        StatusCode::CONFLICT
    );
    assert_eq!(org_count(&pool).await, 1);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn browser_setup_rejects_short_password() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::pool_identity::config),
    )
    .await;

    let setup = test::TestRequest::post()
        .uri("/v1/setup")
        .set_json(serde_json::json!({
            "organization_name": "Rock Pool",
            "email": "owner@example.com",
            "display_name": "Owner",
            "password": "short"
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, setup).await.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(org_count(&pool).await, 0);
}

// ── environment bootstrap ─────────────────────────────────────────────────

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn environment_bootstrap_creates_owner_and_never_reapplies() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let cfg = bootstrap_config();

    let first = pool_identity::bootstrap_from_env(&pool, &cfg)
        .await
        .expect("bootstrap succeeds");
    assert!(matches!(first, BootstrapOutcome::Created { .. }));

    let user = pool_identity::authenticate(&pool, &cfg.owner_email, BOOTSTRAP_PASSWORD)
        .await
        .expect("temporary password authenticates");
    assert_eq!(user.role, PoolRole::Owner);
    assert!(user.password_change_required);
    assert!(
        user.password_hash.starts_with("$argon2id$"),
        "stored credential must be an Argon2id PHC string"
    );

    // Restart semantics: a second bootstrap leaves the same row and hash.
    let hash_before = user.password_hash.clone();
    let second = pool_identity::bootstrap_from_env(&pool, &cfg)
        .await
        .expect("second bootstrap is a no-op");
    assert_eq!(second, BootstrapOutcome::AlreadyBootstrapped);
    let after = pool_identity::find_by_public_id(&pool, &user.public_id)
        .await
        .expect("find operator")
        .expect("operator exists");
    assert_eq!(after.password_hash, hash_before);
    assert!(after.password_change_required);
    assert_eq!(org_count(&pool).await, 1);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn bootstrap_password_is_refused_after_first_change() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let cfg = bootstrap_config();
    pool_identity::bootstrap_from_env(&pool, &cfg)
        .await
        .expect("bootstrap");

    assert!(
        pool_identity::authenticate(&pool, &cfg.owner_email, BOOTSTRAP_PASSWORD)
            .await
            .is_ok()
    );

    pool_identity::change_password(
        &pool,
        &pool_identity::authenticate(&pool, &cfg.owner_email, BOOTSTRAP_PASSWORD)
            .await
            .expect("auth")
            .public_id,
        BOOTSTRAP_PASSWORD,
        NEW_PASSWORD,
    )
    .await
    .expect("change password");

    assert!(matches!(
        pool_identity::authenticate(&pool, &cfg.owner_email, BOOTSTRAP_PASSWORD).await,
        Err(PoolIdentityError::InvalidCredentials)
    ));
    let user = pool_identity::authenticate(&pool, &cfg.owner_email, NEW_PASSWORD)
        .await
        .expect("new password authenticates");
    assert!(!user.password_change_required);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn bootstrap_short_password_is_rejected_by_the_service() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;

    let mut cfg = bootstrap_config();
    cfg.owner_password = "short".into();
    let err = pool_identity::bootstrap_from_env(&pool, &cfg)
        .await
        .expect_err("short password must fail");
    assert!(matches!(err, PoolIdentityError::WeakPassword));
    assert_eq!(org_count(&pool).await, 0);
}

// ── operator API end-to-end ───────────────────────────────────────────────

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn operator_login_and_password_change_over_http() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let cfg = bootstrap_config();
    pool_identity::bootstrap_from_env(&pool, &cfg)
        .await
        .expect("bootstrap");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::pool_identity::config),
    )
    .await;

    let login = test::TestRequest::post()
        .uri("/v1/pool/login")
        .set_json(serde_json::json!({
            "email": cfg.owner_email,
            "password": BOOTSTRAP_PASSWORD
        }))
        .to_request();
    let resp = test::call_service(&app, login).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["must_change_password"], true);
    assert_eq!(body["user"]["role"], "owner");
    let token = body["token"].as_str().expect("token").to_string();

    let me = test::TestRequest::get()
        .uri("/v1/pool/me")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    let resp = test::call_service(&app, me).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["organization"]["name"], "Integration Pool");
    assert_eq!(body["user"]["password_change_required"], true);

    let change = test::TestRequest::post()
        .uri("/v1/pool/password")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .set_json(serde_json::json!({
            "current_password": BOOTSTRAP_PASSWORD,
            "new_password": NEW_PASSWORD
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, change).await.status(),
        StatusCode::NO_CONTENT
    );

    let old_login = test::TestRequest::post()
        .uri("/v1/pool/login")
        .set_json(serde_json::json!({
            "email": cfg.owner_email,
            "password": BOOTSTRAP_PASSWORD
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, old_login).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let new_login = test::TestRequest::post()
        .uri("/v1/pool/login")
        .set_json(serde_json::json!({
            "email": cfg.owner_email,
            "password": NEW_PASSWORD
        }))
        .to_request();
    let resp = test::call_service(&app, new_login).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["must_change_password"], false);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn player_token_is_not_an_operator_token() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::pool_identity::config),
    )
    .await;

    // A structurally valid token whose subject is not a pool_users row.
    let token =
        tidepool::services::auth::issue_jwt(JWT_SECRET, "00000000-0000-4000-8000-000000000000", 60)
            .expect("issue");
    let me = test::TestRequest::get()
        .uri("/v1/pool/me")
        .insert_header(("Authorization", format!("Bearer {token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, me).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

/// Setup through the service is also single-tenant, not just the endpoint.
#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn service_setup_refuses_a_second_organization() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;

    let input = NewOrganization {
        organization_name: "First".into(),
        email: "first@example.com".into(),
        display_name: "First Owner".into(),
        password: BOOTSTRAP_PASSWORD.into(),
    };
    pool_identity::setup_pool(&pool, input)
        .await
        .expect("setup");

    let second = NewOrganization {
        organization_name: "Second".into(),
        email: "second@example.com".into(),
        display_name: "Second Owner".into(),
        password: BOOTSTRAP_PASSWORD.into(),
    };
    let err = pool_identity::setup_pool(&pool, second)
        .await
        .expect_err("second setup must fail");
    assert!(matches!(err, PoolIdentityError::AlreadyInitialized));
    assert_eq!(org_count(&pool).await, 1);
}

// ── invitations, roles and removal (task 1.1.2) ───────────────────────────

/// Bootstrap an owner and return their public id plus a valid operator JWT.
async fn bootstrap_owner(pool: &MySqlPool) -> (String, String) {
    let cfg = bootstrap_config();
    pool_identity::bootstrap_from_env(pool, &cfg)
        .await
        .expect("bootstrap");
    let owner = pool_identity::authenticate(pool, &cfg.owner_email, BOOTSTRAP_PASSWORD)
        .await
        .expect("owner auth");
    let token = auth_svc::issue_jwt(JWT_SECRET, &owner.public_id, 3600).expect("jwt");
    (owner.public_id, token)
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn invitation_flow_is_owner_only_and_single_use() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::pool_identity::config),
    )
    .await;

    let invite = test::TestRequest::post()
        .uri("/v1/pool/invitations")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({
            "email": "Member@Example.com",
            "role": "member"
        }))
        .to_request();
    let resp = test::call_service(&app, invite).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["invitation"]["email"], "member@example.com");
    assert_eq!(body["invitation"]["role"], "member");
    let token = body["token"]
        .as_str()
        .expect("invitation token")
        .to_string();

    let accept = test::TestRequest::post()
        .uri("/v1/pool/invitations/accept")
        .set_json(serde_json::json!({
            "token": token,
            "display_name": "New Member",
            "password": NEW_PASSWORD
        }))
        .to_request();
    let resp = test::call_service(&app, accept).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["user"]["email"], "member@example.com");
    assert_eq!(body["user"]["role"], "member");
    assert!(!body["user"]["password_change_required"].as_bool().unwrap());
    let member_token = body["token"].as_str().expect("member jwt").to_string();

    // Single use: the same token is refused on replay.
    let replay = test::TestRequest::post()
        .uri("/v1/pool/invitations/accept")
        .set_json(serde_json::json!({
            "token": token,
            "display_name": "Replay",
            "password": NEW_PASSWORD
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, replay).await.status(),
        StatusCode::CONFLICT
    );

    let members = test::TestRequest::get()
        .uri("/v1/pool/members")
        .insert_header(("Authorization", format!("Bearer {member_token}")))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, members).await).await;
    assert_eq!(body["members"].as_array().expect("members").len(), 2);

    // The accepted member can sign in with the password chosen at acceptance.
    let login = test::TestRequest::post()
        .uri("/v1/pool/login")
        .set_json(serde_json::json!({
            "email": "member@example.com",
            "password": NEW_PASSWORD
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, login).await.status(),
        StatusCode::OK
    );
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn expired_and_revoked_invitations_are_refused() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (owner_public_id, _owner_token) = bootstrap_owner(&pool).await;

    let organization_id: u64 = sqlx::query_scalar("SELECT id FROM organizations LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("organization id");
    let owner_internal_id: u64 =
        sqlx::query_scalar("SELECT id FROM pool_users WHERE public_id = ?")
            .bind(&owner_public_id)
            .fetch_one(&pool)
            .await
            .expect("owner id");

    let (_, expired_token) = pool_identity::create_invitation(
        &pool,
        organization_id,
        Some(owner_internal_id),
        "expired@example.com",
        PoolRole::Member,
        -1,
    )
    .await
    .expect("create expired invitation");
    let err = pool_identity::accept_invitation(&pool, &expired_token, "Expired", NEW_PASSWORD)
        .await
        .expect_err("expired invitation must fail");
    assert!(matches!(err, PoolIdentityError::InvitationExpired));

    let (invitation, revoked_token) = pool_identity::create_invitation(
        &pool,
        organization_id,
        Some(owner_internal_id),
        "revoked@example.com",
        PoolRole::Viewer,
        pool_identity::INVITATION_TTL_SECS,
    )
    .await
    .expect("create invitation");
    pool_identity::revoke_invitation(&pool, organization_id, &invitation.public_id)
        .await
        .expect("revoke");
    let err = pool_identity::accept_invitation(&pool, &revoked_token, "Revoked", NEW_PASSWORD)
        .await
        .expect_err("revoked invitation must fail");
    assert!(matches!(err, PoolIdentityError::InvitationRevoked));
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn viewer_is_refused_every_mutating_pool_route() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::pool_identity::config)
            .configure(api::hosts::config),
    )
    .await;

    let invite = test::TestRequest::post()
        .uri("/v1/pool/invitations")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({
            "email": "viewer@example.com",
            "role": "viewer"
        }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, invite).await).await;
    let invite_token = body["token"].as_str().expect("token").to_string();

    let accept = test::TestRequest::post()
        .uri("/v1/pool/invitations/accept")
        .set_json(serde_json::json!({
            "token": invite_token,
            "display_name": "Read Only",
            "password": NEW_PASSWORD
        }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, accept).await).await;
    let viewer_token = body["token"].as_str().expect("viewer jwt").to_string();
    let viewer_id = body["user"]["id"].as_str().expect("viewer id").to_string();

    // Reads are allowed.
    let me = test::TestRequest::get()
        .uri("/v1/pool/me")
        .insert_header(("Authorization", format!("Bearer {viewer_token}")))
        .to_request();
    assert_eq!(test::call_service(&app, me).await.status(), StatusCode::OK);
    let members = test::TestRequest::get()
        .uri("/v1/pool/members")
        .insert_header(("Authorization", format!("Bearer {viewer_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, members).await.status(),
        StatusCode::OK
    );

    // Every mutating route refuses the viewer with 403.
    let password = test::TestRequest::post()
        .uri("/v1/pool/password")
        .insert_header(("Authorization", format!("Bearer {viewer_token}")))
        .set_json(serde_json::json!({
            "current_password": NEW_PASSWORD,
            "new_password": "another-password-1"
        }))
        .to_request();
    let resp = test::call_service(&app, password).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let problem: Value = test::read_body_json(resp).await;
    assert_eq!(problem["code"], "forbidden");

    let invite_attempt = test::TestRequest::post()
        .uri("/v1/pool/invitations")
        .insert_header(("Authorization", format!("Bearer {viewer_token}")))
        .set_json(serde_json::json!({"email": "x@example.com"}))
        .to_request();
    assert_eq!(
        test::call_service(&app, invite_attempt).await.status(),
        StatusCode::FORBIDDEN
    );

    let remove_attempt = test::TestRequest::delete()
        .uri(&format!("/v1/pool/members/{viewer_id}"))
        .insert_header(("Authorization", format!("Bearer {viewer_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, remove_attempt).await.status(),
        StatusCode::FORBIDDEN
    );

    let host_attempt = test::TestRequest::post()
        .uri("/v1/hosts")
        .insert_header(("Authorization", format!("Bearer {viewer_token}")))
        .set_json(serde_json::json!({"hostname": "viewer-host"}))
        .to_request();
    assert_eq!(
        test::call_service(&app, host_attempt).await.status(),
        StatusCode::FORBIDDEN
    );
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn last_owner_cannot_be_removed_or_demoted() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::pool_identity::config),
    )
    .await;

    let demote = test::TestRequest::patch()
        .uri(&format!("/v1/pool/members/{owner_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"role": "admin"}))
        .to_request();
    assert_eq!(
        test::call_service(&app, demote).await.status(),
        StatusCode::CONFLICT
    );

    let remove = test::TestRequest::delete()
        .uri(&format!("/v1/pool/members/{owner_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, remove).await.status(),
        StatusCode::CONFLICT
    );

    let owners: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pool_users WHERE role = 'owner'")
        .fetch_one(&pool)
        .await
        .expect("count owners");
    assert_eq!(owners, 1);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn removing_a_member_invalidates_their_token_immediately() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::pool_identity::config),
    )
    .await;

    let invite = test::TestRequest::post()
        .uri("/v1/pool/invitations")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"email": "temp@example.com", "role": "member"}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, invite).await).await;
    let invite_token = body["token"].as_str().expect("token").to_string();

    let accept = test::TestRequest::post()
        .uri("/v1/pool/invitations/accept")
        .set_json(serde_json::json!({
            "token": invite_token,
            "display_name": "Temporary",
            "password": NEW_PASSWORD
        }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, accept).await).await;
    let member_token = body["token"].as_str().expect("member jwt").to_string();
    let member_id = body["user"]["id"].as_str().expect("member id").to_string();

    let before = test::TestRequest::get()
        .uri("/v1/pool/me")
        .insert_header(("Authorization", format!("Bearer {member_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, before).await.status(),
        StatusCode::OK
    );

    let remove = test::TestRequest::delete()
        .uri(&format!("/v1/pool/members/{member_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, remove).await.status(),
        StatusCode::NO_CONTENT
    );

    let after = test::TestRequest::get()
        .uri("/v1/pool/me")
        .insert_header(("Authorization", format!("Bearer {member_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, after).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

// ── API keys and audit (task 1.1.3) ───────────────────────────────────────

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn api_key_secret_is_shown_once_scopes_are_enforced_and_revocation_is_immediate() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::api_keys::config),
    )
    .await;

    let create = test::TestRequest::post()
        .uri("/v1/api-keys")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({
            "name": "CI",
            "scopes": ["projects:read", "configuration:validate"]
        }))
        .to_request();
    let resp = test::call_service(&app, create).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    let secret = body["secret"]
        .as_str()
        .expect("one-time secret")
        .to_string();
    assert!(secret.starts_with("tpk_"));
    let prefix = body["api_key"]["prefix"]
        .as_str()
        .expect("prefix")
        .to_string();
    assert!(secret.starts_with(&prefix));
    assert_eq!(body["api_key"]["scopes"][0], "projects:read");
    assert!(body["api_key"].get("secret_hash").is_none());
    let key_id = body["api_key"]["id"].as_str().expect("key id").to_string();

    // A second read exposes only the prefix.
    let list = test::TestRequest::get()
        .uri("/v1/api-keys")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    let resp = test::call_service(&app, list).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let list_body = test::read_body(resp).await;
    let list_json: Value = serde_json::from_slice(&list_body).expect("json");
    let serialized = String::from_utf8_lossy(&list_body);
    assert!(!serialized.contains(&secret));
    assert!(!serialized.contains("secret_hash"));
    assert_eq!(list_json["api_keys"][0]["prefix"], prefix);

    // Correct scope: whoami is allowed.
    let whoami = test::TestRequest::get()
        .uri("/v1/api-keys/whoami")
        .insert_header(("X-API-Key", secret.clone()))
        .to_request();
    assert_eq!(
        test::call_service(&app, whoami).await.status(),
        StatusCode::OK
    );

    // A valid key without projects:read is 403, not 401.
    let narrow = test::TestRequest::post()
        .uri("/v1/api-keys")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({
            "name": "enroll only",
            "scopes": ["daemons:enroll"]
        }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, narrow).await).await;
    let narrow_secret = body["secret"].as_str().expect("narrow secret").to_string();
    let denied = test::TestRequest::get()
        .uri("/v1/api-keys/whoami")
        .insert_header(("X-API-Key", narrow_secret))
        .to_request();
    let resp = test::call_service(&app, denied).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let problem: Value = test::read_body_json(resp).await;
    assert_eq!(problem["code"], "missing_scope");

    // Revocation takes effect on the next request.
    let revoke = test::TestRequest::delete()
        .uri(&format!("/v1/api-keys/{key_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, revoke).await.status(),
        StatusCode::NO_CONTENT
    );
    let after = test::TestRequest::get()
        .uri("/v1/api-keys/whoami")
        .insert_header(("X-API-Key", secret))
        .to_request();
    assert_eq!(
        test::call_service(&app, after).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn non_owner_cannot_manage_api_keys() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::pool_identity::config)
            .configure(api::api_keys::config),
    )
    .await;

    let invite = test::TestRequest::post()
        .uri("/v1/pool/invitations")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"email": "admin@example.com", "role": "admin"}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, invite).await).await;
    let invite_token = body["token"].as_str().expect("token").to_string();

    let accept = test::TestRequest::post()
        .uri("/v1/pool/invitations/accept")
        .set_json(serde_json::json!({
            "token": invite_token,
            "display_name": "Admin",
            "password": NEW_PASSWORD
        }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, accept).await).await;
    let admin_token = body["token"].as_str().expect("admin jwt").to_string();

    let create = test::TestRequest::post()
        .uri("/v1/api-keys")
        .insert_header(("Authorization", format!("Bearer {admin_token}")))
        .set_json(serde_json::json!({"name": "nope", "scopes": ["projects:read"]}))
        .to_request();
    assert_eq!(
        test::call_service(&app, create).await.status(),
        StatusCode::FORBIDDEN
    );
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn hub_mutations_are_attributable_in_the_audit_trail() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::pool_identity::config)
            .configure(api::api_keys::config),
    )
    .await;

    // Create an invitation (member actor).
    let invite = test::TestRequest::post()
        .uri("/v1/pool/invitations")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"email": "audit@example.com", "role": "member"}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, invite).await).await;
    let invite_token = body["token"].as_str().expect("token").to_string();

    // Accept (the new member actor).
    let accept = test::TestRequest::post()
        .uri("/v1/pool/invitations/accept")
        .set_json(serde_json::json!({
            "token": invite_token,
            "display_name": "Audit Member",
            "password": NEW_PASSWORD
        }))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, accept).await).await;
    let member_id = body["user"]["id"].as_str().expect("member id").to_string();

    // Role change (owner actor).
    let role = test::TestRequest::patch()
        .uri(&format!("/v1/pool/members/{member_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"role": "viewer"}))
        .to_request();
    assert_eq!(
        test::call_service(&app, role).await.status(),
        StatusCode::OK
    );

    // Key create and revoke (owner actor).
    let create = test::TestRequest::post()
        .uri("/v1/api-keys")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"name": "audit key", "scopes": ["runs:dispatch"]}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, create).await).await;
    let key_id = body["api_key"]["id"].as_str().expect("key id").to_string();
    let revoke = test::TestRequest::delete()
        .uri(&format!("/v1/api-keys/{key_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, revoke).await.status(),
        StatusCode::NO_CONTENT
    );

    let actions: Vec<String> = sqlx::query_scalar("SELECT action FROM audit_events ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("audit actions");
    for expected in [
        "pool.bootstrap",
        "invitation.create",
        "invitation.accept",
        "member.role_change",
        "api_key.create",
        "api_key.revoke",
    ] {
        assert!(
            actions.iter().any(|action| action == expected),
            "missing audit action {expected}: {actions:?}"
        );
    }

    let unattributed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_events          WHERE actor_kind = 'member' AND (actor_public_id IS NULL OR actor_public_id = '')",
    )
    .fetch_one(&pool)
    .await
    .expect("unattributed count");
    assert_eq!(unattributed, 0);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn hub_failures_use_problem_details_with_a_request_id() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::pool_identity::config)
            .configure(api::api_keys::config)
            .wrap(HubProblem)
            .wrap(RequestIdMiddleware),
    )
    .await;

    // Extractor failure: no token on a member route.
    let req = test::TestRequest::post()
        .uri("/v1/pool/password")
        .insert_header(("x-request-id", "req-hub-1"))
        .set_json(serde_json::json!({
            "current_password": NEW_PASSWORD,
            "new_password": "another-password-1"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "application/problem+json"
    );
    assert_eq!(resp.headers().get("x-request-id").unwrap(), "req-hub-1");
    let problem: Value = test::read_body_json(resp).await;
    assert_eq!(problem["code"], "unauthorized");
    assert_eq!(problem["requestId"], "req-hub-1");
    assert_eq!(
        problem["type"],
        "https://tidepool.dev/problems/unauthorized"
    );

    // Unknown hub path.
    let req = test::TestRequest::get()
        .uri("/v1/pool/nope")
        .insert_header(("x-request-id", "req-hub-2"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let problem: Value = test::read_body_json(resp).await;
    assert_eq!(problem["code"], "not_found");
    assert_eq!(problem["requestId"], "req-hub-2");

    // Field validation.
    let req = test::TestRequest::post()
        .uri("/v1/setup")
        .insert_header(("x-request-id", "req-hub-3"))
        .set_json(serde_json::json!({
            "organization_name": "Rock Pool",
            "email": "owner@example.com",
            "display_name": "Owner",
            "password": "short"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let problem: Value = test::read_body_json(resp).await;
    assert_eq!(problem["code"], "validation_failed");
    assert_eq!(problem["requestId"], "req-hub-3");
}

// ── host registry (task 1.2.1) ────────────────────────────────────────────

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn hosts_register_with_unique_slugs_and_rename_without_changing_id() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::hosts::config),
    )
    .await;

    let create = test::TestRequest::post()
        .uri("/v1/hosts")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"hostname": "Studio-Mac.local"}))
        .to_request();
    let resp = test::call_service(&app, create).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["slug"], "studio-mac-local");
    assert_eq!(body["status"], "pending_approval");
    let first_id = body["id"].as_str().expect("host id").to_string();

    let duplicate = test::TestRequest::post()
        .uri("/v1/hosts")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"hostname": "studio-mac.local"}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, duplicate).await).await;
    assert_eq!(body["slug"], "studio-mac-local-2");
    let second_id = body["id"].as_str().expect("host id").to_string();
    assert_ne!(first_id, second_id);

    // Rename keeps the immutable id and is visible in the API.
    let rename = test::TestRequest::patch()
        .uri(&format!("/v1/hosts/{first_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"slug": "Studio Mac Primary", "display_name": "Studio Mac"}))
        .to_request();
    let resp = test::call_service(&app, rename).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["id"], first_id);
    assert_eq!(body["slug"], "studio-mac-primary");
    assert_eq!(body["display_name"], "Studio Mac");

    // An explicit rename onto another host's slug is a conflict.
    let collision = test::TestRequest::patch()
        .uri(&format!("/v1/hosts/{second_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"slug": "studio-mac-primary"}))
        .to_request();
    assert_eq!(
        test::call_service(&app, collision).await.status(),
        StatusCode::CONFLICT
    );

    let list = test::TestRequest::get()
        .uri("/v1/hosts")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, list).await).await;
    assert_eq!(body["hosts"].as_array().expect("hosts").len(), 2);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn revoked_hosts_disappear_and_unknown_slugs_fail_activation() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::hosts::config),
    )
    .await;

    let create = test::TestRequest::post()
        .uri("/v1/hosts")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"hostname": "studio-mac"}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, create).await).await;
    let host_id = body["id"].as_str().expect("host id").to_string();

    let organization_id: u64 = sqlx::query_scalar("SELECT id FROM organizations LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("organization id");

    let host =
        tidepool::services::hosts::resolve_for_activation(&pool, organization_id, "studio-mac")
            .await
            .expect("known slug resolves");
    assert_eq!(host.public_id, host_id);

    let revoke = test::TestRequest::delete()
        .uri(&format!("/v1/hosts/{host_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, revoke).await.status(),
        StatusCode::NO_CONTENT
    );

    // Refused at every endpoint.
    let get = test::TestRequest::get()
        .uri(&format!("/v1/hosts/{host_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, get).await.status(),
        StatusCode::NOT_FOUND
    );
    let rename = test::TestRequest::patch()
        .uri(&format!("/v1/hosts/{host_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"slug": "still-here"}))
        .to_request();
    assert_eq!(
        test::call_service(&app, rename).await.status(),
        StatusCode::NOT_FOUND
    );

    let list = test::TestRequest::get()
        .uri("/v1/hosts")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, list).await).await;
    assert_eq!(body["hosts"].as_array().expect("hosts").len(), 0);

    let error =
        tidepool::services::hosts::resolve_for_activation(&pool, organization_id, "studio-mac")
            .await
            .expect_err("revoked slug must not activate");
    assert_eq!(
        error.to_string(),
        "host_slug: unknown host slug 'studio-mac'"
    );
}

// ── host enrollment (task 1.2.2) ──────────────────────────────────────────

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn host_enrollment_is_single_use_and_stores_only_hashes() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::hosts::config)
            .configure(api::api_keys::config)
            .configure(api::pool_identity::config),
    )
    .await;

    let organization_id: u64 = sqlx::query_scalar("SELECT id FROM organizations LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("org id");
    let owner_internal: u64 = sqlx::query_scalar("SELECT id FROM pool_users LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("owner id");

    let (key, key_secret) = api_key_svc::create_api_key(
        &pool,
        organization_id,
        Some(owner_internal),
        "enrollment",
        &[ApiKeyScope::DaemonsEnroll],
    )
    .await
    .expect("create key");

    let create = test::TestRequest::post()
        .uri("/v1/hosts")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"hostname": "studio-mac"}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, create).await).await;
    let host_id = body["id"].as_str().expect("host id").to_string();

    let mint = test::TestRequest::post()
        .uri("/v1/hosts/enrollment-tokens")
        .insert_header(("X-API-Key", key_secret.clone()))
        .set_json(serde_json::json!({"host_id": host_id}))
        .to_request();
    let resp = test::call_service(&app, mint).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    let enrollment_token = body["token"]
        .as_str()
        .expect("enrollment token")
        .to_string();
    assert!(enrollment_token.contains('.'));
    assert!(body.get("token_hash").is_none());

    let credential = "host-generated-credential-0123456789abcdef";
    let enroll = test::TestRequest::post()
        .uri("/v1/hosts/enroll")
        .set_json(serde_json::json!({
            "token": enrollment_token,
            "credential": credential
        }))
        .to_request();
    let resp = test::call_service(&app, enroll).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["status"], "offline");
    let serialized = serde_json::to_string(&body).expect("json");
    assert!(!serialized.contains("credential"));
    assert!(!serialized.contains("argon2"));

    // No read endpoint returns the credential or its hash.
    let get = test::TestRequest::get()
        .uri(&format!("/v1/hosts/{host_id}"))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, get).await).await;
    assert!(!serde_json::to_string(&body).unwrap().contains("credential"));

    // Only the Argon2id hash is stored.
    let stored: String = sqlx::query_scalar("SELECT credential_hash FROM host_credentials")
        .fetch_one(&pool)
        .await
        .expect("stored credential");
    assert!(stored.starts_with("$argon2id$"));
    assert!(!stored.contains(credential));

    // The relationship credential authenticates the host.
    let host = enrollment_svc::authenticate_host(&pool, &host_id, credential)
        .await
        .expect("credential authenticates");
    assert_eq!(host.public_id, host_id);
    assert!(matches!(
        enrollment_svc::authenticate_host(&pool, &host_id, "wrong-credential").await,
        Err(enrollment_svc::EnrollmentError::InvalidCredentials)
    ));

    // Reusing the token is refused.
    let replay = test::TestRequest::post()
        .uri("/v1/hosts/enroll")
        .set_json(serde_json::json!({
            "token": enrollment_token,
            "credential": credential
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, replay).await.status(),
        StatusCode::CONFLICT
    );

    // A second relationship for the same host is refused at mint time.
    let mint_again = test::TestRequest::post()
        .uri("/v1/hosts/enrollment-tokens")
        .insert_header(("X-API-Key", key_secret.clone()))
        .set_json(serde_json::json!({"host_id": host_id}))
        .to_request();
    assert_eq!(
        test::call_service(&app, mint_again).await.status(),
        StatusCode::CONFLICT
    );

    // Expiry is enforced even when the token exists.
    let create_second = test::TestRequest::post()
        .uri("/v1/hosts")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"hostname": "expiring-host"}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, create_second).await).await;
    let expiring_id = body["id"].as_str().expect("host id").to_string();
    let mint = test::TestRequest::post()
        .uri("/v1/hosts/enrollment-tokens")
        .insert_header(("X-API-Key", key_secret.clone()))
        .set_json(serde_json::json!({"host_id": expiring_id}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, mint).await).await;
    let expiring_token = body["token"].as_str().expect("token").to_string();
    sqlx::query(
        "UPDATE enrollment_tokens SET expires_at = '2020-01-01 00:00:00' WHERE public_id = ?",
    )
    .bind(expiring_token.split('.').next().unwrap())
    .execute(&pool)
    .await
    .expect("expire token");
    let expired = test::TestRequest::post()
        .uri("/v1/hosts/enroll")
        .set_json(serde_json::json!({
            "token": expiring_token,
            "credential": credential
        }))
        .to_request();
    let resp = test::call_service(&app, expired).await;
    assert_eq!(resp.status(), StatusCode::GONE);
    let problem: Value = test::read_body_json(resp).await;
    assert_eq!(problem["code"], "gone");

    // Revoking the issuing key expires its unconsumed tokens.
    let create_third = test::TestRequest::post()
        .uri("/v1/hosts")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"hostname": "revoked-key-host"}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, create_third).await).await;
    let third_id = body["id"].as_str().expect("host id").to_string();
    let mint = test::TestRequest::post()
        .uri("/v1/hosts/enrollment-tokens")
        .insert_header(("X-API-Key", key_secret.clone()))
        .set_json(serde_json::json!({"host_id": third_id}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, mint).await).await;
    let third_token = body["token"].as_str().expect("token").to_string();

    let revoke = test::TestRequest::delete()
        .uri(&format!("/v1/api-keys/{}", key.public_id))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    assert_eq!(
        test::call_service(&app, revoke).await.status(),
        StatusCode::NO_CONTENT
    );

    let revoked = test::TestRequest::post()
        .uri("/v1/hosts/enroll")
        .set_json(serde_json::json!({
            "token": third_token,
            "credential": credential
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, revoked).await.status(),
        StatusCode::GONE
    );
    let revoked_at: Option<chrono::NaiveDateTime> =
        sqlx::query_scalar("SELECT revoked_at FROM enrollment_tokens WHERE public_id = ?")
            .bind(third_token.split('.').next().unwrap())
            .fetch_one(&pool)
            .await
            .expect("revoked token row");
    assert!(revoked_at.is_some());
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn enrollment_token_requires_the_daemons_enroll_scope() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::hosts::config)
            .configure(api::api_keys::config)
            .configure(api::pool_identity::config),
    )
    .await;

    let organization_id: u64 = sqlx::query_scalar("SELECT id FROM organizations LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("org id");
    let (_key, secret) = api_key_svc::create_api_key(
        &pool,
        organization_id,
        None,
        "read only",
        &[ApiKeyScope::ProjectsRead],
    )
    .await
    .expect("create key");

    let create = test::TestRequest::post()
        .uri("/v1/hosts")
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .set_json(serde_json::json!({"hostname": "scoped-host"}))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, create).await).await;
    let host_id = body["id"].as_str().expect("host id").to_string();

    let mint = test::TestRequest::post()
        .uri("/v1/hosts/enrollment-tokens")
        .insert_header(("X-API-Key", secret))
        .set_json(serde_json::json!({"host_id": host_id}))
        .to_request();
    let resp = test::call_service(&app, mint).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let problem: Value = test::read_body_json(resp).await;
    assert_eq!(problem["code"], "missing_scope");
}

// ── host presence (task 1.2.3) ────────────────────────────────────────────

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn host_presence_moves_online_offline_and_blocks_dispatch() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, _owner_token) = bootstrap_owner(&pool).await;

    let organization_id: u64 = sqlx::query_scalar("SELECT id FROM organizations LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("org id");

    let host = host_svc::register_host(
        &pool,
        organization_id,
        host_svc::NewHost {
            hostname: "presence-host".into(),
            display_name: None,
        },
        AuditActor::System,
    )
    .await
    .expect("register host");

    let (_, token) = enrollment_svc::mint_enrollment_token(
        &pool,
        organization_id,
        &host.public_id,
        None,
        AuditActor::System,
    )
    .await
    .expect("mint enrollment token");
    let credential = "presence-host-credential-0123456789abcdef";
    let enrolled = enrollment_svc::enroll_host(&pool, &token, credential)
        .await
        .expect("enroll host");
    assert_eq!(
        enrolled.status,
        tidepool::entities::host::HostStatus::Offline
    );

    // A connection moves the durable projection to connected.
    let connected = host_presence::host_connected(&pool, organization_id, &host.public_id)
        .await
        .expect("connect");
    assert_eq!(connected.public_id, host.public_id);
    let status: String = sqlx::query_scalar("SELECT status FROM hosts WHERE public_id = ?")
        .bind(&host.public_id)
        .fetch_one(&pool)
        .await
        .expect("status");
    assert_eq!(status, "connected");
    assert!(
        tidepool::middleware::metrics::metrics()
            .render()
            .contains("tidepool_hosts_connected 1")
    );

    host_presence::require_connected(&pool, organization_id, &host.public_id)
        .await
        .expect("connected host is dispatchable");

    // Dropping the last connection moves it offline and dispatch fails with the
    // documented code instead of queueing.
    host_presence::host_disconnected(&pool, &host.public_id)
        .await
        .expect("disconnect");
    let error = host_presence::require_connected(&pool, organization_id, &host.public_id)
        .await
        .expect_err("offline host must not dispatch");
    assert_eq!(error.code(), "daemon_not_connected");
    assert_eq!(
        error.to_string(),
        format!("daemon_not_connected: host '{}' is offline", host.slug)
    );
    assert!(
        tidepool::middleware::metrics::metrics()
            .render()
            .contains("tidepool_hosts_connected 0")
    );

    // Revoking an online host forgets its connections immediately.
    host_presence::host_connected(&pool, organization_id, &host.public_id)
        .await
        .expect("reconnect");
    host_svc::revoke_host(&pool, organization_id, &host.public_id, AuditActor::System)
        .await
        .expect("revoke");
    host_presence::forget_host(&pool, &host.public_id)
        .await
        .expect("forget");
    assert!(matches!(
        host_presence::require_connected(&pool, organization_id, &host.public_id).await,
        Err(host_presence::HostPresenceError::NotFound)
    ));
}

// ── host execution grant and capabilities (task 1.2.4) ────────────────────

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn host_execution_grant_and_capability_validation() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_owner_id, owner_token) = bootstrap_owner(&pool).await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config(None)))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::hosts::config),
    )
    .await;

    let organization_id: u64 = sqlx::query_scalar("SELECT id FROM organizations LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("org id");

    let host = host_svc::register_host(
        &pool,
        organization_id,
        host_svc::NewHost {
            hostname: "capability-host".into(),
            display_name: None,
        },
        AuditActor::System,
    )
    .await
    .expect("register host");
    let (_, token) = enrollment_svc::mint_enrollment_token(
        &pool,
        organization_id,
        &host.public_id,
        None,
        AuditActor::System,
    )
    .await
    .expect("mint");
    let credential = "capability-host-credential-0123456789abcdef";
    enrollment_svc::enroll_host(&pool, &token, credential)
        .await
        .expect("enroll");

    // Enrollment alone is connected-only.
    let get = test::TestRequest::get()
        .uri(&format!("/v1/hosts/{}", host.public_id))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, get).await).await;
    assert_eq!(body["execution_enabled"], false);
    assert_eq!(body["status"], "offline");

    let selection = tidepool::entities::host_capability::AgentSelection {
        provider: "anthropic".into(),
        model: "claude".into(),
        execution_mode: "default".into(),
        thinking_option: None,
    };
    assert!(matches!(
        tidepool::services::host_capability::validate_agent_for_host(
            &pool,
            organization_id,
            &host.public_id,
            &selection
        )
        .await,
        Err(tidepool::services::host_capability::HostCapabilityError::NotGranted)
    ));

    // Reporting before the grant is refused.
    let report = test::TestRequest::post()
        .uri(&format!("/v1/hosts/{}/capabilities", host.public_id))
        .insert_header(("X-Host-Id", host.public_id.clone()))
        .insert_header(("X-Host-Credential", credential))
        .set_json(serde_json::json!({
            "capabilities": [{
                "provider": "anthropic",
                "model": "claude",
                "execution_mode": "default"
            }]
        }))
        .to_request();
    assert_eq!(
        test::call_service(&app, report).await.status(),
        StatusCode::FORBIDDEN
    );

    // Grant host.execute from the control plane.
    let grant = test::TestRequest::post()
        .uri(&format!("/v1/hosts/{}/execution", host.public_id))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    let resp = test::call_service(&app, grant).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["execution_enabled"], true);

    // The host reports providers, models, modes and provider-native options.
    let report = test::TestRequest::post()
        .uri(&format!("/v1/hosts/{}/capabilities", host.public_id))
        .insert_header(("X-Host-Id", host.public_id.clone()))
        .insert_header(("X-Host-Credential", credential))
        .set_json(serde_json::json!({
            "capabilities": [
                {
                    "provider": "anthropic",
                    "model": "claude",
                    "execution_mode": "default",
                    "options": {"max_tokens": 8192}
                },
                {
                    "provider": "anthropic",
                    "model": "claude",
                    "execution_mode": "plan",
                    "thinking_option": "high"
                },
                {
                    "provider": "openai",
                    "model": "gpt",
                    "execution_mode": "default"
                },
                {
                    "provider": "not-grantable",
                    "model": "x",
                    "execution_mode": "default"
                }
            ]
        }))
        .to_request();
    let resp = test::call_service(&app, report).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["offered_providers"][0], "anthropic");
    assert_eq!(body["offered_providers"][1], "openai");
    assert_eq!(body["offered_providers"].as_array().unwrap().len(), 2);

    // Saving a trigger with a valid selection succeeds; an unavailable model
    // or provider is refused with the field named.
    tidepool::services::host_capability::validate_agent_for_host(
        &pool,
        organization_id,
        &host.public_id,
        &selection,
    )
    .await
    .expect("valid selection");

    let mut bad_model = selection.clone();
    bad_model.model = "missing".into();
    let error = tidepool::services::host_capability::validate_agent_for_host(
        &pool,
        organization_id,
        &host.public_id,
        &bad_model,
    )
    .await
    .expect_err("bad model");
    assert_eq!(error.to_string(), "model: host cannot run 'missing'");

    // Revoking the grant removes the execution target but keeps the host.
    let revoke = test::TestRequest::delete()
        .uri(&format!("/v1/hosts/{}/execution", host.public_id))
        .insert_header(("Authorization", format!("Bearer {owner_token}")))
        .to_request();
    let resp = test::call_service(&app, revoke).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["execution_enabled"], false);
    assert!(matches!(
        tidepool::services::host_capability::validate_agent_for_host(
            &pool,
            organization_id,
            &host.public_id,
            &selection
        )
        .await,
        Err(tidepool::services::host_capability::HostCapabilityError::NotGranted)
    ));
}
