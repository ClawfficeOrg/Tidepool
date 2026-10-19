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
    services::pool_identity::{self, BootstrapOutcome, NewOrganization, PoolIdentityError},
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
