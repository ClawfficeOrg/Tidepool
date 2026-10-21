//! Integration tests for trigger validation (task 1.4.2).
//!
//! Require a live MySQL; skipped by default (`#[ignore]`).
//! Run with: `cargo test --test triggers_integration -- --ignored`
//!
//! Defaults to the Docker dev stack:
//!   mysql://tidepool:password@127.0.0.1:3306/tidepool_dev
//! Override with `DATABASE_URL`.

use actix_web::{App, http::StatusCode, test, web};
use serde_json::{Value, json};
use sqlx::MySqlPool;
use tidepool::{
    api,
    config::{Config, RateLimitConfig},
    db,
    services::{
        auth as auth_svc,
        pool_identity::{self, NewOrganization},
    },
};

const DEFAULT_DB_URL: &str = "mysql://tidepool:password@127.0.0.1:3306/tidepool_dev";
const JWT_SECRET: &str = "integration-test-triggers-jwt-secret-32chars";

static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn pool() -> MySqlPool {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DB_URL.into());
    db::build_pool(&db_url).await.expect(
        "DB connection failed — is Docker up? Run: docker compose -f config/docker-compose.dev.yml up -d",
    )
}

async fn reset(pool: &MySqlPool) {
    for statement in [
        "DELETE FROM configuration_revision_files",
        "DELETE FROM configuration_revisions",
        "DELETE FROM projects",
        "DELETE FROM connection_resources",
        "DELETE FROM connections",
        "DELETE FROM webhook_deliveries",
        "DELETE FROM host_capabilities",
        "DELETE FROM host_executions",
        "DELETE FROM host_credentials",
        "DELETE FROM enrollment_tokens",
        "DELETE FROM hosts",
        "DELETE FROM audit_events",
        "DELETE FROM api_keys",
        "DELETE FROM pool_invitations",
        "DELETE FROM pool_users",
        "DELETE FROM organizations",
    ] {
        sqlx::query(statement)
            .execute(pool)
            .await
            .expect("clear table");
    }
}

fn config() -> Config {
    Config {
        host: "127.0.0.1".into(),
        port: 8080,
        database_url: DEFAULT_DB_URL.into(),
        redis_url: "redis://127.0.0.1:6379".into(),
        jwt_secret: JWT_SECRET.into(),
        jwt_expiry_secs: 3600,
        rate_limit: RateLimitConfig::default(),
        pool_bootstrap: None,
    }
}

async fn seed_organization(pool: &MySqlPool) -> (u64, String) {
    let (organization, owner) = pool_identity::setup_pool(
        pool,
        NewOrganization {
            organization_name: "Integration Pool".into(),
            email: "owner-triggers@example.com".into(),
            display_name: "Integration Owner".into(),
            password: "integration-password-1".into(),
        },
    )
    .await
    .expect("setup pool");
    (organization.id, owner.public_id)
}

/// Seed an active GitHub connection delivering for `repository`, and return its
/// internal id.
async fn seed_github_connection(
    pool: &MySqlPool,
    organization_id: u64,
    slug: &str,
    repository: &str,
) -> u64 {
    let public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO connections \
         (public_id, organization_id, provider, slug, display_name, credential_ref, status) \
         VALUES (?, ?, 'github', ?, 'GitHub', 'unused', 'active')",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(slug)
    .execute(pool)
    .await
    .expect("seed connection");
    let connection_id: u64 = sqlx::query_scalar("SELECT id FROM connections WHERE public_id = ?")
        .bind(&public_id)
        .fetch_one(pool)
        .await
        .expect("connection id");
    sqlx::query(
        "INSERT INTO connection_resources (connection_id, resource_kind, external_id, display_name) \
         VALUES (?, 'repository', ?, NULL)",
    )
    .bind(connection_id)
    .bind(repository)
    .execute(pool)
    .await
    .expect("seed resource");
    connection_id
}

/// Seed a host with the given status and execution grant. When granted, also
/// report an `anthropic/claude/default` capability so the test agent can run.
async fn seed_host(pool: &MySqlPool, organization_id: u64, status: &str, granted: bool) -> String {
    let public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO hosts \
         (public_id, organization_id, slug, display_name, hostname, status, execution_granted) \
         VALUES (?, ?, 'studio-mac', 'Studio Mac', 'studio-mac', ?, ?)",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(status)
    .bind(granted)
    .execute(pool)
    .await
    .expect("seed host");
    let host_id: u64 = sqlx::query_scalar("SELECT id FROM hosts WHERE public_id = ?")
        .bind(&public_id)
        .fetch_one(pool)
        .await
        .expect("host id");
    if granted {
        sqlx::query(
            "INSERT INTO host_capabilities (host_id, provider, model, execution_mode, thinking_option) \
             VALUES (?, 'anthropic', 'claude', 'default', NULL)",
        )
        .bind(host_id)
        .execute(pool)
        .await
        .expect("seed capability");
    }
    public_id
}

fn valid_document() -> Value {
    json!({
        "name": "auto-fix-issues",
        "event": { "provider": "github", "type": "issue.comment_created" },
        "filters": {
            "connection": "github-main",
            "resource": { "kind": "repository", "id": "org/repo" },
            "from_users": ["alice"]
        },
        "run": {
            "host": "studio-mac",
            "agent": { "provider": "anthropic", "model": "claude", "execution_mode": "default" }
        }
    })
}

fn bearer(token: &str) -> (&'static str, String) {
    ("Authorization", format!("Bearer {token}"))
}

macro_rules! validate {
    ($app:expr, $token:expr, $body:expr) => {{
        let req = test::TestRequest::post()
            .uri("/v1/triggers/validate")
            .insert_header(bearer($token))
            .set_json($body)
            .to_request();
        let resp = test::call_service($app, req).await;
        let status = resp.status();
        let body: Value = test::read_body_json(resp).await;
        (status, body)
    }};
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn validate_against_a_connected_host_succeeds() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, owner_public_id) = seed_organization(&pool).await;
    seed_github_connection(&pool, organization_id, "github-main", "org/repo").await;
    seed_host(&pool, organization_id, "connected", true).await;

    let token = auth_svc::issue_jwt(JWT_SECRET, &owner_public_id, 3600).expect("jwt");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::triggers::config),
    )
    .await;

    let (status, body) = validate!(&app, &token, valid_document());
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "auto-fix-issues");
    assert_eq!(body["valid"], true);
    assert!(body["errors"].as_array().expect("errors").is_empty());
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn validate_against_an_offline_host_still_resolves() {
    // Validation resolves the host by slug and its capability report; presence
    // (connected/offline) is a dispatch-time concern (task 1.4.6), so an
    // offline host with a matching agent still validates.
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, owner_public_id) = seed_organization(&pool).await;
    seed_github_connection(&pool, organization_id, "github-main", "org/repo").await;
    seed_host(&pool, organization_id, "offline", true).await;

    let token = auth_svc::issue_jwt(JWT_SECRET, &owner_public_id, 3600).expect("jwt");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::triggers::config),
    )
    .await;

    let (status, body) = validate!(&app, &token, valid_document());
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["valid"], true,
        "offline host must still resolve: {body}"
    );
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn validate_rejects_a_trigger_without_from_users() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, owner_public_id) = seed_organization(&pool).await;
    seed_github_connection(&pool, organization_id, "github-main", "org/repo").await;
    seed_host(&pool, organization_id, "connected", true).await;

    let token = auth_svc::issue_jwt(JWT_SECRET, &owner_public_id, 3600).expect("jwt");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::triggers::config),
    )
    .await;

    let mut missing = valid_document();
    missing["filters"]
        .as_object_mut()
        .expect("filters")
        .remove("from_users");
    let (status, body) = validate!(&app, &token, missing);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], false);
    let fields: Vec<&str> = body["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .map(|issue| issue["field"].as_str().expect("field"))
        .collect();
    assert!(fields.contains(&"filters.from_users"));

    let mut empty = valid_document();
    empty["filters"]["from_users"] = json!([]);
    let (_, body) = validate!(&app, &token, empty);
    assert_eq!(body["valid"], false);
    let fields: Vec<&str> = body["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .map(|issue| issue["field"].as_str().expect("field"))
        .collect();
    assert!(fields.contains(&"filters.from_users"));
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn validate_rejects_unknown_references_naming_fields() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, owner_public_id) = seed_organization(&pool).await;
    // Only a connection with a different slug, and no host: every reference the
    // document names is unknown.
    seed_github_connection(&pool, organization_id, "other-connection", "other/repo").await;

    let token = auth_svc::issue_jwt(JWT_SECRET, &owner_public_id, 3600).expect("jwt");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::triggers::config),
    )
    .await;

    let (status, body) = validate!(&app, &token, valid_document());
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], false);
    let issues: Vec<(String, String)> = body["errors"]
        .as_array()
        .expect("errors")
        .iter()
        .map(|issue| {
            (
                issue["field"].as_str().expect("field").to_string(),
                issue["message"].as_str().expect("message").to_string(),
            )
        })
        .collect();

    assert!(
        issues
            .iter()
            .any(|(field, _)| field == "filters.connection"),
        "unknown connection must be named: {issues:?}"
    );
    assert!(
        issues.iter().any(|(field, _)| field == "filters.resource"),
        "unknown resource must be named: {issues:?}"
    );
    assert!(
        issues.iter().any(|(field, _)| field == "run.host"),
        "unknown host must be named: {issues:?}"
    );
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn validate_rejects_an_agent_the_host_cannot_run_naming_the_field() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, owner_public_id) = seed_organization(&pool).await;
    seed_github_connection(&pool, organization_id, "github-main", "org/repo").await;
    seed_host(&pool, organization_id, "connected", true).await;

    let token = auth_svc::issue_jwt(JWT_SECRET, &owner_public_id, 3600).expect("jwt");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::triggers::config),
    )
    .await;

    let mut bad_model = valid_document();
    bad_model["run"]["agent"]["model"] = json!("gpt-4");
    let (status, body) = validate!(&app, &token, bad_model);
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], false);
    assert_eq!(body["errors"][0]["field"], "run.agent.model");
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn validate_writes_nothing() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, owner_public_id) = seed_organization(&pool).await;
    seed_github_connection(&pool, organization_id, "github-main", "org/repo").await;
    let host_public_id = seed_host(&pool, organization_id, "offline", true).await;

    let token = auth_svc::issue_jwt(JWT_SECRET, &owner_public_id, 3600).expect("jwt");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::triggers::config),
    )
    .await;

    let before: (i64, i64, i64) = (
        sqlx::query_scalar("SELECT COUNT(*) FROM hosts")
            .fetch_one(&pool)
            .await
            .expect("hosts"),
        sqlx::query_scalar("SELECT COUNT(*) FROM connections")
            .fetch_one(&pool)
            .await
            .expect("connections"),
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
            .fetch_one(&pool)
            .await
            .expect("audit"),
    );

    let (status, body) = validate!(&app, &token, valid_document());
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], true);

    // No audit event, no new rows, and the host's presence/status is untouched.
    let audit_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
        .fetch_one(&pool)
        .await
        .expect("audit after");
    assert_eq!(
        audit_after, before.2,
        "validation must not write an audit row"
    );

    let (hosts_after, connections_after): (i64, i64) = (
        sqlx::query_scalar("SELECT COUNT(*) FROM hosts")
            .fetch_one(&pool)
            .await
            .expect("hosts after"),
        sqlx::query_scalar("SELECT COUNT(*) FROM connections")
            .fetch_one(&pool)
            .await
            .expect("connections after"),
    );
    assert_eq!((hosts_after, connections_after), (before.0, before.1));

    let status_after: String = sqlx::query_scalar("SELECT status FROM hosts WHERE public_id = ?")
        .bind(&host_public_id)
        .fetch_one(&pool)
        .await
        .expect("host status");
    assert_eq!(
        status_after, "offline",
        "validation must not change host presence"
    );
}
