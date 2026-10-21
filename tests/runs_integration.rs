//! Integration tests for dispatch and the run lifecycle (task 1.4.6).
//!
//! Require a live MySQL; skipped by default (`#[ignore]`).
//! Run with: `cargo test --test runs_integration -- --ignored`
//!
//! Defaults to the Docker dev stack:
//!   mysql://tidepool:password@127.0.0.1:3306/tidepool_dev
//! Override with `DATABASE_URL`.
//!
//! No host socket is opened. A test registers host presence in the in-memory
//! registry, so dispatch passes the presence gate; the create intent is stored
//! in `host_executions` and the failed frame send leaves it queued for the
//! recovery test rather than requiring a WebSocket.

use chrono::Utc;
use serde_json::json;
use sqlx::MySqlPool;
use tidepool::{
    db,
    entities::{
        connection::ProviderKind,
        run::{ManualDispatchRequest, RunStatus, StepStatus},
    },
    services::{
        dispatch::{self, InboundEvent},
        host_presence,
        pool_identity::{self, NewOrganization},
        runs,
    },
};

const DEFAULT_DB_URL: &str = "mysql://tidepool:password@127.0.0.1:3306/tidepool_dev";

static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn pool() -> MySqlPool {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DB_URL.into());
    db::build_pool(&db_url).await.expect(
        "DB connection failed — is Docker up? Run: docker compose -f config/docker-compose.dev.yml up -d",
    )
}

async fn reset(pool: &MySqlPool) {
    for statement in [
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

async fn seed_organization(pool: &MySqlPool) -> (u64, String) {
    let (organization, owner) = pool_identity::setup_pool(
        pool,
        NewOrganization {
            organization_name: "Runs Integration Pool".into(),
            email: "owner-runs@example.com".into(),
            display_name: "Runs Owner".into(),
            password: "integration-password-1".into(),
        },
    )
    .await
    .expect("setup pool");
    (organization.id, owner.public_id)
}

async fn seed_project(pool: &MySqlPool, organization_id: u64) -> String {
    let public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO projects (public_id, organization_id, slug, name) \
         VALUES (?, ?, 'runs-project', 'Runs Project')",
    )
    .bind(&public_id)
    .bind(organization_id)
    .execute(pool)
    .await
    .expect("seed project");
    public_id
}

async fn seed_revision(pool: &MySqlPool, project_public_id: &str) -> String {
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
            { "name": "default", "host": "studio-mac", "directory": "/tmp/tidepool-runs" }
        ],
        "workflows": [{
            "name": "triage",
            "trigger": "auto-fix-issues",
            "maxRuntime": 900,
            "inputs": [
                { "name": "route", "default": "bug", "choices": ["bug", "docs"] }
            ],
            "steps": [
                {
                    "id": "first",
                    "environment": "default",
                    "agent": "coder",
                    "prompt": "Fix ${{ request }} (route ${{ inputs.route }})"
                },
                {
                    "id": "second",
                    "environment": "default",
                    "agent": "coder",
                    "prompt": "Only docs: ${{ steps.first }}",
                    "if": {
                        "left": { "input": "route" },
                        "op": "eq",
                        "right": { "value": "docs" }
                    },
                    "reads": ["first"]
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
    revision_id
}

async fn seed_trigger(pool: &MySqlPool, project_public_id: &str) -> String {
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
    trigger_id
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

fn manual(project: &str, delivery_key: &str) -> ManualDispatchRequest {
    ManualDispatchRequest {
        project: project.into(),
        trigger: "auto-fix-issues".into(),
        actor: "alice".into(),
        revision: None,
        delivery_key: delivery_key.into(),
        input: "route=docs please fix it".into(),
    }
}

fn event(delivery_key: &str) -> InboundEvent {
    InboundEvent {
        provider: ProviderKind::Github,
        event_type: "issue.comment_created".into(),
        connection: "github-main".into(),
        resource_kind: "repository".into(),
        resource_id: "org/repo".into(),
        actor: "alice".into(),
        channel: None,
        label: None,
        content: "please fix it".into(),
        text: "please fix it".into(),
        delivery_key: delivery_key.into(),
    }
}

async fn run_count(pool: &MySqlPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM runs")
        .fetch_one(pool)
        .await
        .expect("count runs")
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_matched_event_creates_exactly_one_run_for_its_delivery_key() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, _) = seed_organization(&pool).await;
    let project = seed_project(&pool, organization_id).await;
    seed_revision(&pool, &project).await;
    seed_trigger(&pool, &project).await;
    let host = seed_host(&pool, organization_id).await;
    host_presence::registry().connect(&host, Utc::now());

    let first = dispatch::dispatch_event(&pool, organization_id, &project, &event("delivery-1"))
        .await
        .expect("event dispatch");
    assert_eq!(first.len(), 1);
    assert!(!first[0].deduplicated);
    assert_eq!(run_count(&pool).await, 1);

    let again = dispatch::dispatch_event(&pool, organization_id, &project, &event("delivery-1"))
        .await
        .expect("event retry");
    assert_eq!(again.len(), 1);
    assert!(again[0].deduplicated);
    assert_eq!(again[0].run.id, first[0].run.id);
    assert_eq!(run_count(&pool).await, 1);

    host_presence::registry().forget(&host);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_manual_dispatch_parses_inputs_and_keeps_its_dedup_promise() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, _) = seed_organization(&pool).await;
    let project = seed_project(&pool, organization_id).await;
    seed_revision(&pool, &project).await;
    seed_trigger(&pool, &project).await;
    let host = seed_host(&pool, organization_id).await;
    host_presence::registry().connect(&host, Utc::now());

    let request = manual(&project, "manual-1");
    let first = dispatch::dispatch_manual(&pool, organization_id, &request)
        .await
        .expect("manual dispatch");
    assert!(!first.deduplicated);
    assert_eq!(
        first.run.inputs.get("route").map(String::as_str),
        Some("docs")
    );
    assert_eq!(first.run.request_text, "please fix it");

    // Same key, different input: still the same run. Deduplication, not
    // exactly-once delivery, is the promise.
    let mut changed = request.clone();
    changed.input = "route=bug a different request".into();
    let again = dispatch::dispatch_manual(&pool, organization_id, &changed)
        .await
        .expect("manual retry");
    assert!(again.deduplicated);
    assert_eq!(again.run.id, first.run.id);
    assert_eq!(again.run.request_text, "please fix it");
    assert_eq!(run_count(&pool).await, 1);

    host_presence::registry().forget(&host);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn dispatch_to_an_offline_host_fails_without_queueing_anything() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, _) = seed_organization(&pool).await;
    let project = seed_project(&pool, organization_id).await;
    seed_revision(&pool, &project).await;
    seed_trigger(&pool, &project).await;
    let host = seed_host(&pool, organization_id).await;
    host_presence::registry().forget(&host);

    let error = dispatch::dispatch_manual(&pool, organization_id, &manual(&project, "offline-1"))
        .await
        .expect_err("an offline host must fail dispatch");
    assert_eq!(error.code(), "daemon_not_connected");
    assert_eq!(run_count(&pool).await, 0);
    let steps: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM run_steps")
        .fetch_one(&pool)
        .await
        .expect("count steps");
    assert_eq!(steps, 0, "nothing may be queued for an offline host");
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_resubmitted_intent_after_a_restart_reuses_the_same_execution_id() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, _) = seed_organization(&pool).await;
    let project = seed_project(&pool, organization_id).await;
    seed_revision(&pool, &project).await;
    seed_trigger(&pool, &project).await;
    let host = seed_host(&pool, organization_id).await;
    host_presence::registry().connect(&host, Utc::now());

    let outcome = dispatch::dispatch_manual(&pool, organization_id, &manual(&project, "restart-1"))
        .await
        .expect("manual dispatch");
    let execution_id: String = sqlx::query_scalar(
        "SELECT execution_id FROM run_steps WHERE run_id = \
         (SELECT id FROM runs WHERE public_id = ?) ORDER BY position LIMIT 1",
    )
    .bind(&outcome.run.id)
    .fetch_one(&pool)
    .await
    .expect("execution id");
    assert_eq!(
        outcome.run.steps[0].execution_id.as_deref(),
        Some(execution_id.as_str())
    );

    // The socket is absent, so the create intent is queued. Two recovery passes
    // (a lost response, then a restart) must re-send the same id and never make
    // a second execution row.
    runs::recover_run(&pool, &outcome.run.id)
        .await
        .expect("first recovery");
    runs::recover_run(&pool, &outcome.run.id)
        .await
        .expect("second recovery");
    let executions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM host_executions WHERE public_id = ?")
            .bind(&execution_id)
            .fetch_one(&pool)
            .await
            .expect("count executions");
    assert_eq!(executions, 1, "the prompt must not run twice");

    // A terminal failure is recorded as interrupted and is never restarted.
    sqlx::query("UPDATE host_executions SET status = 'failed' WHERE public_id = ?")
        .bind(&execution_id)
        .execute(&pool)
        .await
        .expect("fail execution");
    runs::recover_run(&pool, &outcome.run.id)
        .await
        .expect("failure recovery");
    let run = runs::find_run(&pool, organization_id, &outcome.run.id)
        .await
        .expect("find run")
        .expect("run exists");
    assert_eq!(run.status, RunStatus::Interrupted);
    let executions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM host_executions WHERE public_id = ?")
            .bind(execution_id)
            .fetch_one(&pool)
            .await
            .expect("count executions");
    assert_eq!(executions, 1, "a failed execution is never re-created");

    host_presence::registry().forget(&host);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_false_condition_skips_a_step_and_the_run_succeeds() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, _) = seed_organization(&pool).await;
    let project = seed_project(&pool, organization_id).await;
    seed_revision(&pool, &project).await;
    seed_trigger(&pool, &project).await;
    let host = seed_host(&pool, organization_id).await;
    host_presence::registry().connect(&host, Utc::now());

    // route defaults to bug, so the second step's `route == docs` is false.
    let mut request = manual(&project, "condition-1");
    request.input = "please fix it".into();
    let outcome = dispatch::dispatch_manual(&pool, organization_id, &request)
        .await
        .expect("manual dispatch");

    // Complete the first step; advance_run then skips the second and finishes.
    let first_execution = outcome.run.steps[0]
        .execution_id
        .clone()
        .expect("execution id");
    runs::observe_completion(&pool, &first_execution, "succeeded", Some("done"))
        .await
        .expect("complete first");
    let run = runs::find_run(&pool, organization_id, &outcome.run.id)
        .await
        .expect("find run")
        .expect("run exists");
    assert_eq!(run.status, RunStatus::Succeeded);
    let steps = runs::run_view(&pool, &run).await.expect("view");
    assert_eq!(steps.steps[1].status, StepStatus::Skipped);
    assert_eq!(steps.steps[1].condition, Some(false));

    host_presence::registry().forget(&host);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_passed_deadline_fails_the_run_and_stops_later_steps() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, _) = seed_organization(&pool).await;
    let project = seed_project(&pool, organization_id).await;
    seed_revision(&pool, &project).await;
    seed_trigger(&pool, &project).await;
    let host = seed_host(&pool, organization_id).await;
    host_presence::registry().connect(&host, Utc::now());

    let outcome =
        dispatch::dispatch_manual(&pool, organization_id, &manual(&project, "deadline-1"))
            .await
            .expect("manual dispatch");
    // Push the run's deadline into the past and advance again.
    sqlx::query(
        "UPDATE runs SET deadline_at = DATE_SUB(NOW(), INTERVAL 1 MINUTE) WHERE public_id = ?",
    )
    .bind(&outcome.run.id)
    .execute(&pool)
    .await
    .expect("expire deadline");
    runs::advance_run(&pool, &outcome.run.id)
        .await
        .expect("advance expired run");

    let run = runs::find_run(&pool, organization_id, &outcome.run.id)
        .await
        .expect("find run")
        .expect("run exists");
    assert_eq!(run.status, RunStatus::Failed);
    let view = runs::run_view(&pool, &run).await.expect("view");
    assert!(
        view.steps
            .iter()
            .all(|step| step.status != StepStatus::Pending),
        "no later step may still be pending"
    );
    assert!(
        view.steps.iter().any(|step| {
            step.status == StepStatus::Skipped && step.reason.as_deref() == Some("run_deadline")
        }),
        "later steps are skipped with the deadline reason"
    );

    host_presence::registry().forget(&host);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_manual_dispatch_requires_an_allowed_actor_and_an_active_revision() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let (organization_id, _) = seed_organization(&pool).await;
    let project = seed_project(&pool, organization_id).await;
    let revision = seed_revision(&pool, &project).await;
    seed_trigger(&pool, &project).await;
    let host = seed_host(&pool, organization_id).await;
    host_presence::registry().connect(&host, Utc::now());

    let mut request = manual(&project, "actor-1");
    request.actor = "mallory".into();
    let error = dispatch::dispatch_manual(&pool, organization_id, &request)
        .await
        .expect_err("actor must be in the allowlist");
    assert_eq!(error.code(), "forbidden");
    assert_eq!(run_count(&pool).await, 0);

    let mut request = manual(&project, "revision-1");
    request.actor = "alice".into();
    request.revision = Some("00000000-0000-4000-8000-000000000000".into());
    let error = dispatch::dispatch_manual(&pool, organization_id, &request)
        .await
        .expect_err("a stale expected revision is refused");
    assert_eq!(error.code(), "conflict");
    assert_eq!(run_count(&pool).await, 0);

    // The actual active revision is accepted.
    request.revision = Some(revision);
    let outcome = dispatch::dispatch_manual(&pool, organization_id, &request)
        .await
        .expect("active revision dispatch");
    assert!(!outcome.deduplicated);
    assert_eq!(run_count(&pool).await, 1);

    host_presence::registry().forget(&host);
}
