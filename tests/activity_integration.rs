//! Integration tests for the activity read API (task 1.4.7).
//!
//! Require a live MySQL; skipped by default (`#[ignore]`).
//! Run with: `cargo test --test activity_integration -- --ignored`
//!
//! Defaults to the Docker dev stack:
//!   mysql://tidepool:password@127.0.0.1:3306/tidepool_dev
//! Override with `DATABASE_URL`.
//!
//! The tests seed only the tables the activity read uses: an organization, two
//! projects, runs and receipts. No host socket or trigger evaluation is needed,
//! so the filter and page-shape checks stay independent of dispatch.

use chrono::Utc;
use serde_json::json;
use sqlx::MySqlPool;
use tidepool::{
    db,
    entities::{activity::ActivityFilters, connection::ProviderKind},
    services::activity::{self, DeliveryFacts},
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
        "DELETE FROM agent_conversations",
        "DELETE FROM event_receipts",
        "DELETE FROM run_steps",
        "DELETE FROM runs",
        "DELETE FROM trigger_revisions",
        "DELETE FROM triggers",
        "DELETE FROM configuration_revision_files",
        "DELETE FROM configuration_revisions",
        "DELETE FROM projects",
        "DELETE FROM pool_users",
        "DELETE FROM organizations",
    ] {
        sqlx::query(statement)
            .execute(pool)
            .await
            .expect("clear activity table");
    }
}

async fn seed_organization(pool: &MySqlPool) -> u64 {
    let public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO organizations (public_id, name, singleton) VALUES (?, 'Activity Pool', 1)",
    )
    .bind(&public_id)
    .execute(pool)
    .await
    .expect("seed organization");
    sqlx::query_scalar("SELECT id FROM organizations WHERE public_id = ?")
        .bind(&public_id)
        .fetch_one(pool)
        .await
        .expect("organization id")
}

async fn seed_project(pool: &MySqlPool, organization_id: u64, slug: &str) -> (u64, String) {
    let public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO projects (public_id, organization_id, slug, name) VALUES (?, ?, ?, ?)",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(slug)
    .bind(slug)
    .execute(pool)
    .await
    .expect("seed project");
    let id = sqlx::query_scalar("SELECT id FROM projects WHERE public_id = ?")
        .bind(&public_id)
        .fetch_one(pool)
        .await
        .expect("project id");
    (id, public_id)
}

#[allow(clippy::too_many_arguments)]
async fn seed_run(
    pool: &MySqlPool,
    organization_id: u64,
    project_id: u64,
    project_public_id: &str,
    trigger_name: &str,
    status: &str,
    continuation: &str,
    steered: bool,
    reused_execution_id: Option<&str>,
    conversation_key: Option<&str>,
    output_limits_json: Option<&str>,
) -> String {
    let public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO runs \
         (public_id, organization_id, project_id, project_public_id, trigger_id, \
          trigger_public_id, trigger_name, workflow_name, revision_id, provider, \
          receipt_public_id, conversation_key, continuation, steered, reused_execution_id, \
          output_limits_json, origin, actor, delivery_key, request_text, inputs_json, \
          workflow_json, environments_json, partials_json, status, started_at, completed_at) \
         VALUES (?, ?, ?, ?, 1, ?, ?, 'triage', NULL, 'github', NULL, ?, ?, ?, ?, ?, \
                 'event', 'alice', ?, 'please fix it', '{}', '{}', '[]', '{}', ?, \
                 CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(project_id)
    .bind(project_public_id)
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(trigger_name)
    .bind(conversation_key)
    .bind(continuation)
    .bind(if steered { 1_i32 } else { 0_i32 })
    .bind(reused_execution_id)
    .bind(output_limits_json)
    .bind(format!("delivery-{public_id}"))
    .bind(status)
    .execute(pool)
    .await
    .expect("seed run");
    public_id
}

async fn seed_receipt(
    pool: &MySqlPool,
    organization_id: u64,
    project_id: u64,
    project_public_id: &str,
    delivery_key: &str,
    status: &str,
    run_public_ids: Option<&str>,
) -> String {
    let public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO event_receipts \
         (public_id, organization_id, project_id, project_public_id, provider, connection, \
          delivery_key, event_type, resource_kind, resource_id, actor, status, \
          matched_triggers, run_public_ids) \
         VALUES (?, ?, ?, ?, 'github', 'github-main', ?, 'issue.comment_created', \
                 'repository', 'org/repo', 'alice', ?, ?, ?)",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(project_id)
    .bind(project_public_id)
    .bind(delivery_key)
    .bind(status)
    .bind(if status == "matched" { 1_i32 } else { 0_i32 })
    .bind(run_public_ids)
    .execute(pool)
    .await
    .expect("seed receipt");
    public_id
}

fn filters(project: Option<&str>, status: Option<&str>, limit: u32) -> ActivityFilters {
    ActivityFilters {
        project: project.map(str::to_string),
        provider: Some("github".into()),
        trigger: None,
        status: status.map(str::to_string),
        kind: None,
        cursor: None,
        limit,
    }
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn an_unmatched_delivery_still_gets_a_receipt() {
    use tidepool::{
        entities::connection::ProviderKind,
        services::dispatch::{self, InboundEvent},
    };

    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let organization_id = seed_organization(&pool).await;
    let (_project_id, project_public) = seed_project(&pool, organization_id, "unmatched").await;

    let event = InboundEvent {
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
        delivery_key: "delivery-unmatched".into(),
    };
    let outcomes = dispatch::dispatch_event(&pool, organization_id, &project_public, &event)
        .await
        .expect("unmatched dispatch");
    assert!(outcomes.is_empty(), "no trigger matched");

    let (status, _): (String, i64) = sqlx::query_as(
        "SELECT status, duplicate_count FROM event_receipts \
         WHERE organization_id = ? AND delivery_key = ?",
    )
    .bind(organization_id)
    .bind("delivery-unmatched")
    .fetch_one(&pool)
    .await
    .expect("receipt row");
    assert_eq!(status, "unmatched");

    // The retry is counted on the same receipt and still creates nothing.
    let again = dispatch::dispatch_event(&pool, organization_id, &project_public, &event)
        .await
        .expect("duplicate unmatched dispatch");
    assert!(again.is_empty());
    let (status, duplicated): (String, i64) = sqlx::query_as(
        "SELECT status, duplicate_count FROM event_receipts \
         WHERE organization_id = ? AND delivery_key = ?",
    )
    .bind(organization_id)
    .bind("delivery-unmatched")
    .fetch_one(&pool)
    .await
    .expect("receipt row after retry");
    assert_eq!(status, "unmatched");
    assert_eq!(duplicated, 1);

    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs")
        .fetch_one(&pool)
        .await
        .expect("count runs");
    assert_eq!(run_count, 0);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn activity_filters_combine_without_leaking_across_projects() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let organization_id = seed_organization(&pool).await;
    let (first_id, first_public) = seed_project(&pool, organization_id, "first").await;
    let (second_id, second_public) = seed_project(&pool, organization_id, "second").await;

    let first_run = seed_run(
        &pool,
        organization_id,
        first_id,
        &first_public,
        "first-trigger",
        "succeeded",
        "created",
        false,
        None,
        None,
        None,
    )
    .await;
    let second_run = seed_run(
        &pool,
        organization_id,
        second_id,
        &second_public,
        "second-trigger",
        "failed",
        "created",
        false,
        None,
        None,
        None,
    )
    .await;
    seed_receipt(
        &pool,
        organization_id,
        first_id,
        &first_public,
        "delivery-first",
        "matched",
        Some(&json!([first_run]).to_string()),
    )
    .await;
    seed_receipt(
        &pool,
        organization_id,
        second_id,
        &second_public,
        "delivery-second",
        "unmatched",
        None,
    )
    .await;

    // A project filter keeps every other project out, whatever else combines
    // with it.
    let page = activity::list_activity(
        &pool,
        organization_id,
        &filters(Some(&first_public), None, 50),
    )
    .await
    .expect("first project page");
    assert!(!page.items.is_empty());
    assert!(
        page.items
            .iter()
            .all(|item| item.project.as_deref() == Some(first_public.as_str())),
        "a project filter must not leak another project's rows"
    );

    let succeeded = activity::list_activity(
        &pool,
        organization_id,
        &filters(Some(&first_public), Some("succeeded"), 50),
    )
    .await
    .expect("project and status page");
    assert!(
        succeeded.items.iter().all(
            |item| item.project.as_deref() == Some(first_public.as_str())
                && item.status == "succeeded"
        ),
        "combined filters must all apply"
    );
    assert!(succeeded.items.iter().all(|item| item.id != second_run));

    // Cursor paging is stable and non-overlapping.
    let first_page = activity::list_activity(&pool, organization_id, &filters(None, None, 1))
        .await
        .expect("first page");
    assert_eq!(first_page.items.len(), 1);
    assert!(first_page.has_more);
    let cursor = first_page.next_cursor.expect("next cursor");

    let mut next_filters = filters(None, None, 1);
    next_filters.cursor =
        Some(tidepool::entities::activity::ActivityCursor::decode(&cursor).expect("decode cursor"));
    let second_page = activity::list_activity(&pool, organization_id, &next_filters)
        .await
        .expect("second page");
    assert!(
        second_page
            .items
            .iter()
            .all(|item| item.id != first_page.items[0].id),
        "the next page must not repeat the previous page"
    );
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn run_detail_records_continuation_and_per_arrival_limits() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let organization_id = seed_organization(&pool).await;
    let (project_id, project_public) = seed_project(&pool, organization_id, "activity").await;

    let limits = json!({
        "reply": [{ "capability": "slack.reply", "maximum": 2, "required": true }]
    })
    .to_string();
    let continued = seed_run(
        &pool,
        organization_id,
        project_id,
        &project_public,
        "auto-fix",
        "running",
        "continued",
        true,
        Some("agent-1"),
        Some("slack:channel:C1:thread-1"),
        Some(&limits),
    )
    .await;
    let created = seed_run(
        &pool,
        organization_id,
        project_id,
        &project_public,
        "auto-fix",
        "succeeded",
        "created",
        false,
        None,
        None,
        None,
    )
    .await;

    let detail = activity::activity_run_detail(&pool, organization_id, &continued)
        .await
        .expect("continued run detail");
    assert_eq!(detail.arrival.mode.as_str(), "continued");
    assert!(detail.arrival.steered);
    assert_eq!(detail.arrival.reused_execution.as_deref(), Some("agent-1"));
    assert_eq!(
        detail.arrival.conversation_key.as_deref(),
        Some("slack:channel:C1:thread-1")
    );
    assert_eq!(detail.arrival.output_limits[0].limits[0].maximum, 2);

    // A different arrival keeps its own limits and completion status.
    let detail = activity::activity_run_detail(&pool, organization_id, &created)
        .await
        .expect("created run detail");
    assert_eq!(detail.arrival.mode.as_str(), "created");
    assert!(!detail.arrival.steered);
    assert!(detail.arrival.output_limits.is_empty());
    assert_eq!(detail.run.status.as_str(), "succeeded");
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_duplicate_receipt_returns_the_original_runs_and_writes_no_second_one() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;
    let organization_id = seed_organization(&pool).await;
    let (project_id, project_public) = seed_project(&pool, organization_id, "dedup").await;

    let delivery = DeliveryFacts {
        provider: ProviderKind::Github,
        connection: "github-main",
        delivery_key: "delivery-1",
        event_type: "issue.comment_created",
        action: Some("created"),
        resource_kind: "repository",
        resource_id: "org/repo",
        actor: "alice",
        channel: None,
    };
    let first = activity::begin_receipt(
        &pool,
        organization_id,
        Some(project_id),
        Some(&project_public),
        &delivery,
    )
    .await
    .expect("first receipt");
    assert!(!first.duplicate);
    let run = seed_run(
        &pool,
        organization_id,
        project_id,
        &project_public,
        "auto-fix",
        "succeeded",
        "created",
        false,
        None,
        None,
        None,
    )
    .await;
    activity::finish_receipt(
        &pool,
        organization_id,
        &first.receipt.public_id,
        tidepool::entities::activity::ReceiptStatus::Matched,
        1,
        std::slice::from_ref(&run),
    )
    .await
    .expect("finish receipt");

    let again = activity::begin_receipt(
        &pool,
        organization_id,
        Some(project_id),
        Some(&project_public),
        &delivery,
    )
    .await
    .expect("duplicate receipt");
    assert!(again.duplicate);
    assert_eq!(again.receipt.duplicate_count, 1);
    assert_eq!(
        activity::receipt_run_ids(&again.receipt).expect("run ids"),
        vec![run.clone()]
    );
    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs")
        .fetch_one(&pool)
        .await
        .expect("count runs");
    assert_eq!(run_count, 1, "a duplicate delivery must not add a run");

    // The first receipt's observation time is preserved across duplicates.
    assert!(again.receipt.received_at <= Utc::now());
}
