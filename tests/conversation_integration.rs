//! Integration tests for conversation continuation and receipt recovery (task 1.4.7).
//!
//! Require a live MySQL; skipped by default (`#[ignore]`).
//! Run with: `cargo test --test conversation_integration -- --ignored`
//!
//! Defaults to the Docker dev stack:
//!   mysql://tidepool:password@127.0.0.1:3306/tidepool_dev
//! Override with `DATABASE_URL`. The tests clear every table they use, so point
//! `DATABASE_URL` at a scratch database, not one holding data you need.
//!
//! No host socket is opened. A test registers host presence in the in-memory
//! registry so dispatch passes the presence gate. Each test moves the agent
//! execution rows between states by hand to model a busy, finished or gone agent.

use chrono::Utc;
use serde_json::json;
use sqlx::MySqlPool;
use tidepool::{
    db,
    entities::connection::ProviderKind,
    services::{
        activity::{self, DeliveryFacts},
        dispatch::{self, InboundEvent},
        host_presence,
        pool_identity::{self, NewOrganization},
    },
};

const DEFAULT_DB_URL: &str = "mysql://tidepool:password@127.0.0.1:3306/tidepool_dev";
const CHANNEL: &str = "issue-7";
const CONVERSATION_KEY: &str = "github:repository:org/repo:issue-7";

static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn pool() -> MySqlPool {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DB_URL.into());
    db::build_pool(&db_url).await.expect(
        "DB connection failed — is Docker up? Run: docker compose -f config/docker-compose.dev.yml up -d",
    )
}

async fn reset(pool: &MySqlPool) {
    for statement in [
        "DELETE FROM agent_conversations",
        "DELETE FROM event_receipts",
        "DELETE FROM run_steps",
        "DELETE FROM runs",
        "DELETE FROM trigger_revisions",
        "DELETE FROM triggers",
        "DELETE FROM configuration_revision_files",
        "DELETE FROM configuration_revisions",
        "DELETE FROM projects",
        "DELETE FROM host_executions",
        "DELETE FROM host_capabilities",
        "DELETE FROM host_credentials",
        "DELETE FROM hosts",
        "DELETE FROM connection_resources",
        "DELETE FROM connections",
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

async fn seed_organization(pool: &MySqlPool) -> u64 {
    let (organization, _) = pool_identity::setup_pool(
        pool,
        NewOrganization {
            organization_name: "Conversation Integration Pool".into(),
            email: "owner-conversation@example.com".into(),
            display_name: "Conversation Owner".into(),
            password: "integration-password-1".into(),
        },
    )
    .await
    .expect("setup pool");
    organization.id
}

async fn seed_project(pool: &MySqlPool, organization_id: u64) -> String {
    let public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO projects (public_id, organization_id, slug, name) \
         VALUES (?, ?, 'conversation-project', 'Conversation Project')",
    )
    .bind(&public_id)
    .bind(organization_id)
    .execute(pool)
    .await
    .expect("seed project");
    public_id
}

async fn seed_revision(pool: &MySqlPool, project_public_id: &str) {
    let revision_id = uuid::Uuid::new_v4().to_string();
    let project_id: u64 = sqlx::query_scalar("SELECT id FROM projects WHERE public_id = ?")
        .bind(project_public_id)
        .fetch_one(pool)
        .await
        .expect("project id");
    sqlx::query(
        "INSERT INTO configuration_revisions (public_id, project_id, version, status, active) \
         VALUES (?, ?, 1, 'valid', 1)",
    )
    .bind(&revision_id)
    .bind(project_id)
    .execute(pool)
    .await
    .expect("seed revision");
    let internal: u64 =
        sqlx::query_scalar("SELECT id FROM configuration_revisions WHERE public_id = ?")
            .bind(&revision_id)
            .fetch_one(pool)
            .await
            .expect("revision id");
    let manifest = json!({
        "hosts": ["studio-mac"],
        "environments": [
            { "name": "default", "host": "studio-mac", "directory": "/tmp/tidepool-conversation" }
        ],
        "workflows": [{
            "name": "triage",
            "trigger": "auto-fix-issues",
            "maxRuntime": 900,
            "inputs": [],
            "steps": [
                {
                    "id": "first",
                    "environment": "default",
                    "agent": "coder",
                    "prompt": "Fix ${{ request }}"
                }
            ]
        }]
    });
    sqlx::query(
        "INSERT INTO configuration_revision_files (revision_id, file_order, path, content) \
         VALUES (?, 0, 'tidepool.json', ?)",
    )
    .bind(internal)
    .bind(manifest.to_string())
    .execute(pool)
    .await
    .expect("seed manifest");
}

async fn seed_trigger(pool: &MySqlPool, project_public_id: &str) {
    let trigger_id = uuid::Uuid::new_v4().to_string();
    let revision_id = uuid::Uuid::new_v4().to_string();
    let project_id: u64 = sqlx::query_scalar("SELECT id FROM projects WHERE public_id = ?")
        .bind(project_public_id)
        .fetch_one(pool)
        .await
        .expect("project id");
    sqlx::query("INSERT INTO triggers (public_id, project_id, name) VALUES (?, ?, ?)")
        .bind(&trigger_id)
        .bind(project_id)
        .bind("auto-fix-issues")
        .execute(pool)
        .await
        .expect("seed trigger");
    let internal: u64 = sqlx::query_scalar("SELECT id FROM triggers WHERE public_id = ?")
        .bind(&trigger_id)
        .fetch_one(pool)
        .await
        .expect("trigger id");
    let document = json!({
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
    });
    sqlx::query(
        "INSERT INTO trigger_revisions (public_id, trigger_id, version, active, document) \
         VALUES (?, ?, 1, 1, ?)",
    )
    .bind(&revision_id)
    .bind(internal)
    .bind(document.to_string())
    .execute(pool)
    .await
    .expect("seed trigger revision");
}

async fn seed_host(pool: &MySqlPool, organization_id: u64) -> String {
    let public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO hosts \
         (public_id, organization_id, slug, display_name, hostname, status, execution_granted) \
         VALUES (?, ?, 'studio-mac', 'Studio Mac', 'studio-mac', 'connected', 1)",
    )
    .bind(&public_id)
    .bind(organization_id)
    .execute(pool)
    .await
    .expect("seed host");
    public_id
}

/// Set up one project with a trigger, a revision and a connected host.
async fn seed_world(pool: &MySqlPool) -> (u64, String, String) {
    reset(pool).await;
    let organization_id = seed_organization(pool).await;
    let project = seed_project(pool, organization_id).await;
    seed_revision(pool, &project).await;
    seed_trigger(pool, &project).await;
    let host = seed_host(pool, organization_id).await;
    host_presence::registry().connect(&host, Utc::now());
    (organization_id, project, host)
}

fn event(delivery_key: &str) -> InboundEvent {
    InboundEvent {
        provider: ProviderKind::Github,
        event_type: "issue.comment_created".into(),
        connection: "github-main".into(),
        resource_kind: "repository".into(),
        resource_id: "org/repo".into(),
        actor: "alice".into(),
        channel: Some(CHANNEL.into()),
        label: None,
        content: "please fix it".into(),
        text: "please fix it".into(),
        delivery_key: delivery_key.into(),
    }
}

/// Dispatch one arrival and return its new run's public id.
async fn arrive(
    pool: &MySqlPool,
    organization_id: u64,
    project: &str,
    delivery_key: &str,
) -> String {
    let outcomes = dispatch::dispatch_event(pool, organization_id, project, &event(delivery_key))
        .await
        .expect("event dispatch");
    assert_eq!(outcomes.len(), 1, "one trigger matches");
    assert!(!outcomes[0].deduplicated, "a new delivery creates a run");
    outcomes[0].run.id.clone()
}

async fn trigger_id(pool: &MySqlPool) -> u64 {
    sqlx::query_scalar("SELECT id FROM triggers WHERE name = 'auto-fix-issues'")
        .fetch_one(pool)
        .await
        .expect("trigger id")
}

/// The execution id of a run's first step, the agent that run started.
async fn first_execution(pool: &MySqlPool, run_public_id: &str) -> String {
    sqlx::query_scalar(
        "SELECT rs.execution_id FROM run_steps rs \
         JOIN runs r ON r.id = rs.run_id \
         WHERE r.public_id = ? ORDER BY rs.id LIMIT 1",
    )
    .bind(run_public_id)
    .fetch_one(pool)
    .await
    .expect("first execution id")
}

async fn set_execution_status(pool: &MySqlPool, execution_id: &str, status: &str) {
    sqlx::query("UPDATE host_executions SET status = ? WHERE public_id = ?")
        .bind(status)
        .bind(execution_id)
        .execute(pool)
        .await
        .expect("set execution status");
}

/// The continuation fields recorded on a run: (mode, steered).
async fn continuation(pool: &MySqlPool, run_public_id: &str) -> (String, bool) {
    sqlx::query_as("SELECT continuation, steered FROM runs WHERE public_id = ?")
        .bind(run_public_id)
        .fetch_one(pool)
        .await
        .expect("run continuation")
}

async fn conversation_execution(pool: &MySqlPool, organization_id: u64) -> Option<String> {
    activity::find_conversation(
        pool,
        organization_id,
        trigger_id(pool).await,
        CONVERSATION_KEY,
    )
    .await
    .expect("find conversation")
    .expect("conversation exists")
    .agent_execution_id
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn each_arrival_advances_the_conversation_to_its_own_execution() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    let (organization_id, project, host) = seed_world(&pool).await;

    // Arrival 1 starts a fresh agent.
    let run_one = arrive(&pool, organization_id, &project, "delivery-1").await;
    let agent_one = first_execution(&pool, &run_one).await;
    assert_eq!(
        conversation_execution(&pool, organization_id).await,
        Some(agent_one.clone())
    );

    // Agent one is busy, so arrival 2 steers it and advances the conversation.
    set_execution_status(&pool, &agent_one, "running").await;
    let run_two = arrive(&pool, organization_id, &project, "delivery-2").await;
    let agent_two = first_execution(&pool, &run_two).await;
    assert_ne!(agent_two, agent_one);
    assert_eq!(
        continuation(&pool, &run_two).await,
        ("continued".into(), true)
    );
    assert_eq!(
        conversation_execution(&pool, organization_id).await,
        Some(agent_two.clone())
    );

    // Agent one has finished and agent two is busy. Arrival 3 must steer agent
    // two, not judge the conversation by the first arrival's stale agent.
    set_execution_status(&pool, &agent_one, "succeeded").await;
    set_execution_status(&pool, &agent_two, "running").await;
    let run_three = arrive(&pool, organization_id, &project, "delivery-3").await;
    let agent_three = first_execution(&pool, &run_three).await;
    assert_eq!(
        continuation(&pool, &run_three).await,
        ("continued".into(), true)
    );
    assert_eq!(
        conversation_execution(&pool, organization_id).await,
        Some(agent_three)
    );

    host_presence::registry().forget(&host);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_conversation_whose_agent_is_gone_is_restored_not_continued() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    let (organization_id, project, host) = seed_world(&pool).await;

    let run_one = arrive(&pool, organization_id, &project, "delivery-1").await;
    let agent_one = first_execution(&pool, &run_one).await;
    set_execution_status(&pool, &agent_one, "succeeded").await;

    // The agent's execution row is removed, as when a host forgets it.
    sqlx::query("DELETE FROM host_executions WHERE public_id = ?")
        .bind(&agent_one)
        .execute(&pool)
        .await
        .expect("delete agent execution");

    let run_two = arrive(&pool, organization_id, &project, "delivery-2").await;
    let agent_two = first_execution(&pool, &run_two).await;
    assert_eq!(
        continuation(&pool, &run_two).await,
        ("restored".into(), false)
    );
    assert_eq!(
        conversation_execution(&pool, organization_id).await,
        Some(agent_two)
    );

    host_presence::registry().forget(&host);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_receipt_left_received_by_a_crash_is_rematched_on_retry() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    let (organization_id, project, host) = seed_world(&pool).await;

    // Simulate a process that died after recording the receipt but before matching.
    let project_id: u64 = sqlx::query_scalar("SELECT id FROM projects WHERE public_id = ?")
        .bind(&project)
        .fetch_one(&pool)
        .await
        .expect("project id");
    let facts = DeliveryFacts {
        provider: ProviderKind::Github,
        connection: "github-main",
        delivery_key: "delivery-crash",
        event_type: "issue.comment_created",
        action: None,
        resource_kind: "repository",
        resource_id: "org/repo",
        actor: "alice",
        channel: Some(CHANNEL),
    };
    activity::begin_receipt(
        &pool,
        organization_id,
        Some(project_id),
        Some(&project),
        &facts,
    )
    .await
    .expect("record receipt");
    let stuck: String =
        sqlx::query_scalar("SELECT status FROM event_receipts WHERE delivery_key = ?")
            .bind("delivery-crash")
            .fetch_one(&pool)
            .await
            .expect("receipt status");
    assert_eq!(stuck, "received");

    // The provider retries. The retry must match and create the run, not return nothing.
    let retried =
        dispatch::dispatch_event(&pool, organization_id, &project, &event("delivery-crash"))
            .await
            .expect("retry dispatch");
    assert_eq!(retried.len(), 1, "retry re-matches the stuck delivery");
    assert!(!retried[0].deduplicated);
    let run_id = retried[0].run.id.clone();

    let settled: String =
        sqlx::query_scalar("SELECT status FROM event_receipts WHERE delivery_key = ?")
            .bind("delivery-crash")
            .fetch_one(&pool)
            .await
            .expect("receipt status");
    assert_eq!(settled, "matched");

    // A further retry returns the same run and creates no second one.
    let again =
        dispatch::dispatch_event(&pool, organization_id, &project, &event("delivery-crash"))
            .await
            .expect("second retry");
    assert_eq!(again.len(), 1);
    assert!(again[0].deduplicated);
    assert_eq!(again[0].run.id, run_id);
    let runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs")
        .fetch_one(&pool)
        .await
        .expect("count runs");
    assert_eq!(runs, 1);

    host_presence::registry().forget(&host);
}
