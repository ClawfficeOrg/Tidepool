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
    entities::pool_user::PoolRole,
    services::{
        auth as auth_svc,
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
        StatusCode::BAD_REQUEST
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
            .configure(api::pool_identity::config),
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
    assert_eq!(
        test::call_service(&app, password).await.status(),
        StatusCode::FORBIDDEN
    );

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
    assert_eq!(
        test::call_service(&app, denied).await.status(),
        StatusCode::FORBIDDEN
    );

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
