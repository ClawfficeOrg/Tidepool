//! Integration tests for projects and configuration revisions (task 1.4.1).
//!
//! Require a live MySQL; skipped by default (`#[ignore]`).
//! Run with: `cargo test --test projects_integration -- --ignored`
//!
//! Defaults to the Docker dev stack:
//!   mysql://tidepool:password@127.0.0.1:3306/tidepool_dev
//! Override with `DATABASE_URL`.

use actix_web::{App, http::StatusCode, test, web};
use redis::aio::ConnectionManager;
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
const JWT_SECRET: &str = "integration-test-projects-jwt-secret-32chars";

static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// The dev stack publishes Redis on 6380 (see config/docker-compose.dev.override.yml).
const DEFAULT_REDIS_URL: &str = "redis://127.0.0.1:6380";

/// The session store operator routes require: they fail closed without one.
async fn sessions() -> web::Data<ConnectionManager> {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| DEFAULT_REDIS_URL.into());
    web::Data::new(
        tidepool::services::redis::connect(&url)
            .await
            .expect("Redis unavailable; start the dev stack or set REDIS_URL"),
    )
}

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
        public_origin: None,
        cors_origins: Vec::new(),
    }
}

/// Seed an organization + owner and return `(organization_id, owner_public_id)`.
async fn seed_organization(pool: &MySqlPool) -> (u64, String) {
    let (organization, owner) = pool_identity::setup_pool(
        pool,
        NewOrganization {
            organization_name: "Integration Pool".into(),
            email: "owner-projects@example.com".into(),
            display_name: "Integration Owner".into(),
            password: "integration-password-1".into(),
        },
    )
    .await
    .expect("setup pool");
    (organization.id, owner.public_id)
}

/// Seed a live host and an active connection for the organization.
async fn seed_references(pool: &MySqlPool, organization_id: u64) {
    sqlx::query(
        "INSERT INTO hosts (public_id, organization_id, slug, display_name, hostname, status) \
         VALUES (?, ?, 'studio-mac', 'Studio Mac', 'studio-mac', 'connected')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(organization_id)
    .execute(pool)
    .await
    .expect("seed host");

    sqlx::query(
        "INSERT INTO connections \
         (public_id, organization_id, provider, slug, display_name, credential_ref, status) \
         VALUES (?, ?, 'github', 'github-main', 'GitHub', 'unused', 'active')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(organization_id)
    .execute(pool)
    .await
    .expect("seed connection");
}

fn valid_manifest() -> String {
    json!({
        "connections": ["github-main"],
        "hosts": ["studio-mac"],
        "environments": [{
            "name": "default",
            "host": "studio-mac",
            "directory": "/Users/you/code/repo"
        }],
        "agents": [{"name": "coder", "provider": "anthropic", "model": "claude-sonnet-4-6"}],
        "partials": [{"name": "system", "path": "partials/system.txt"}]
    })
    .to_string()
}

fn install_body(files: Vec<(&str, &str)>) -> Value {
    json!({
        "files": files
            .into_iter()
            .map(|(path, content)| json!({"path": path, "content": content}))
            .collect::<Vec<_>>()
    })
}

fn bearer(token: &str) -> (&'static str, String) {
    ("Authorization", format!("Bearer {token}"))
}

macro_rules! create_project {
    ($app:expr, $token:expr) => {{
        let req = test::TestRequest::post()
            .uri("/v1/projects")
            .insert_header(bearer($token))
            .set_json(json!({"name": "Nightly Builds"}))
            .to_request();
        let resp = test::call_service($app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED, "create project");
        let body: Value = test::read_body_json(resp).await;
        body["id"].as_str().expect("project id").to_string()
    }};
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn install_then_fail_leaves_previous_active() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, owner_public_id) = seed_organization(&pool).await;
    seed_references(&pool, organization_id).await;

    let token = auth_svc::issue_jwt(JWT_SECRET, &owner_public_id, 3600).expect("jwt");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .app_data(sessions().await)
            .configure(api::projects::config),
    )
    .await;

    let project_id = create_project!(&app, &token);

    // Install a valid revision.
    let install = test::TestRequest::post()
        .uri(&format!("/v1/projects/{project_id}/configuration/install"))
        .insert_header(bearer(&token))
        .set_json(install_body(vec![
            ("tidepool.json", &valid_manifest()),
            ("partials/system.txt", "You are a build engineer."),
        ]))
        .to_request();
    let resp = test::call_service(&app, install).await;
    assert_eq!(resp.status(), StatusCode::CREATED, "first install");
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["activated"], true);
    assert_eq!(body["revision"]["version"], 1);
    assert_eq!(body["revision"]["status"], "valid");
    assert_eq!(body["revision"]["active"], true);
    let rev1_id = body["revision"]["id"]
        .as_str()
        .expect("revision id")
        .to_string();

    // Install an invalid revision naming an unknown host.
    let bad_manifest = json!({ "hosts": ["ghost-host"] }).to_string();
    let install = test::TestRequest::post()
        .uri(&format!("/v1/projects/{project_id}/configuration/install"))
        .insert_header(bearer(&token))
        .set_json(install_body(vec![("tidepool.json", &bad_manifest)]))
        .to_request();
    let resp = test::call_service(&app, install).await;
    assert_eq!(
        resp.status(),
        StatusCode::CREATED,
        "failed install still stores a revision"
    );
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["activated"], false, "failed install must not activate");
    assert_eq!(body["revision"]["version"], 2);
    assert_eq!(body["revision"]["status"], "invalid");
    assert_eq!(body["revision"]["active"], false);
    assert_eq!(body["revision"]["errors"][0]["field"], "hosts.ghost-host");
    assert_eq!(body["revision"]["errors"][0]["revision"], 2);

    // The previous revision is still active.
    let active = test::TestRequest::get()
        .uri(&format!("/v1/projects/{project_id}/configuration"))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, active).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["version"], 1, "previous revision stays active");
    assert_eq!(body["id"], rev1_id.as_str());

    // Revision history shows both, with exactly one active.
    let list = test::TestRequest::get()
        .uri(&format!("/v1/projects/{project_id}/revisions"))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, list).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    let revisions = body["revisions"].as_array().expect("revisions array");
    assert_eq!(revisions.len(), 2);
    let active_count = revisions
        .iter()
        .filter(|revision| revision["active"] == true)
        .count();
    assert_eq!(active_count, 1, "exactly one revision active");
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_revision_is_immutable_and_files_are_stored_verbatim() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, owner_public_id) = seed_organization(&pool).await;
    seed_references(&pool, organization_id).await;

    let token = auth_svc::issue_jwt(JWT_SECRET, &owner_public_id, 3600).expect("jwt");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .app_data(sessions().await)
            .configure(api::projects::config),
    )
    .await;

    let project_id = create_project!(&app, &token);

    // Authored content with deliberate non-canonical spacing that must be
    // stored exactly, not normalized.
    let manifest = "{\n  \"hosts\":   [\"studio-mac\"]\n}";
    let partial = "line one\n  line two with  two spaces\n";

    let install = test::TestRequest::post()
        .uri(&format!("/v1/projects/{project_id}/configuration/install"))
        .insert_header(bearer(&token))
        .set_json(install_body(vec![
            ("tidepool.json", manifest),
            ("partials/system.txt", partial),
        ]))
        .to_request();
    let resp = test::call_service(&app, install).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body: Value = test::read_body_json(resp).await;
    let rev_id = body["revision"]["id"]
        .as_str()
        .expect("revision id")
        .to_string();

    // Read the stored revision back: files are byte-for-byte the authored ones.
    let get = test::TestRequest::get()
        .uri(&format!("/v1/projects/{project_id}/revisions/{rev_id}"))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, get).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    let files = body["files"].as_array().expect("files array");
    let by_path = files
        .iter()
        .map(|file| {
            (
                file["path"].as_str().expect("path").to_string(),
                file["content"].as_str().expect("content").to_string(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        by_path.get("tidepool.json").map(String::as_str),
        Some(manifest)
    );
    assert_eq!(
        by_path.get("partials/system.txt").map(String::as_str),
        Some(partial)
    );
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn projects_are_scoped_to_the_organization() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (_organization_id, owner_public_id) = seed_organization(&pool).await;

    // A second organization, inserted directly (bypassing the singleton guard
    // that only ever lets the browser create one), plus a member and a project.
    let second_org_public = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO organizations (public_id, name, singleton, bootstrap_source) \
         VALUES (?, 'Second Pool', 2, 'browser')",
    )
    .bind(&second_org_public)
    .execute(&pool)
    .await
    .expect("seed second organization");
    let second_org_id: u64 = sqlx::query_scalar("SELECT id FROM organizations WHERE public_id = ?")
        .bind(&second_org_public)
        .fetch_one(&pool)
        .await
        .expect("second org id");
    let second_user_public = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO pool_users \
         (public_id, organization_id, email, display_name, password_hash, role) \
         VALUES (?, ?, 'second@example.com', 'Second', 'unused', 'member')",
    )
    .bind(&second_user_public)
    .bind(second_org_id)
    .execute(&pool)
    .await
    .expect("seed second member");
    let second_project_public = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO projects (public_id, organization_id, slug, name) VALUES (?, ?, 'other', 'Other')",
    )
    .bind(&second_project_public)
    .bind(second_org_id)
    .execute(&pool)
    .await
    .expect("seed second project");

    let first_token = auth_svc::issue_jwt(JWT_SECRET, &owner_public_id, 3600).expect("jwt");
    let second_token = auth_svc::issue_jwt(JWT_SECRET, &second_user_public, 3600).expect("jwt");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .app_data(sessions().await)
            .configure(api::projects::config),
    )
    .await;

    // The first organization's owner creates a project and sees only it.
    let first_project_id = create_project!(&app, &first_token);

    let list = test::TestRequest::get()
        .uri("/v1/projects")
        .insert_header(bearer(&first_token))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, list).await).await;
    let projects = body["projects"].as_array().expect("projects array");
    assert_eq!(projects.len(), 1, "first org sees only its own project");
    assert_eq!(projects[0]["id"], first_project_id.as_str());

    // The first org cannot read the second org's project.
    let get_other = test::TestRequest::get()
        .uri(&format!("/v1/projects/{second_project_public}"))
        .insert_header(bearer(&first_token))
        .to_request();
    assert_eq!(
        test::call_service(&app, get_other).await.status(),
        StatusCode::NOT_FOUND,
        "cross-organization read is a 404"
    );

    // The second org sees its own project and not the first's.
    let list_second = test::TestRequest::get()
        .uri("/v1/projects")
        .insert_header(bearer(&second_token))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, list_second).await).await;
    let projects = body["projects"].as_array().expect("projects array");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["id"], second_project_public.as_str());
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn validate_writes_nothing() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, owner_public_id) = seed_organization(&pool).await;
    seed_references(&pool, organization_id).await;

    let token = auth_svc::issue_jwt(JWT_SECRET, &owner_public_id, 3600).expect("jwt");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .app_data(sessions().await)
            .configure(api::projects::config),
    )
    .await;

    let project_id = create_project!(&app, &token);

    let validate = test::TestRequest::post()
        .uri(&format!("/v1/projects/{project_id}/configuration/validate"))
        .insert_header(bearer(&token))
        .set_json(install_body(vec![
            ("tidepool.json", &valid_manifest()),
            ("partials/system.txt", "You are a build engineer."),
        ]))
        .to_request();
    let resp = test::call_service(&app, validate).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value = test::read_body_json(resp).await;
    assert_eq!(body["valid"], true);
    assert_eq!(body["version"], 1);
    assert!(body["errors"].as_array().expect("errors").is_empty());

    // Validation never writes a revision.
    let list = test::TestRequest::get()
        .uri(&format!("/v1/projects/{project_id}/revisions"))
        .insert_header(bearer(&token))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, list).await).await;
    assert!(body["revisions"].as_array().expect("revisions").is_empty());

    // An invalid bundle's validate reports the named field and the revision.
    let bad = json!({ "connections": ["no-such-connection"] }).to_string();
    let validate = test::TestRequest::post()
        .uri(&format!("/v1/projects/{project_id}/configuration/validate"))
        .insert_header(bearer(&token))
        .set_json(install_body(vec![("tidepool.json", &bad)]))
        .to_request();
    let body: Value = test::read_body_json(test::call_service(&app, validate).await).await;
    assert_eq!(body["valid"], false);
    assert_eq!(body["version"], 1);
    assert_eq!(body["errors"][0]["field"], "connections.no-such-connection");
    assert_eq!(body["errors"][0]["revision"], 1);
}
