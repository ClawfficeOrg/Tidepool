//! Integration tests for the session relay (task 1.5.5).
//!
//! Require a live MySQL; skipped by default (`#[ignore]`).
//! Run with: `cargo test --test relay_integration -- --ignored`

use serde_json::json;
use sqlx::MySqlPool;
use tidepool::{
    db,
    services::{
        pool_identity::{self, NewOrganization},
        relay::{self, RelayError},
    },
};

const DEFAULT_DB_URL: &str = "mysql://tidepool:password@127.0.0.1:3306/tidepool_dev";

static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn pool() -> MySqlPool {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DB_URL.into());
    db::build_pool(&db_url).await.expect("connect")
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_session_publishes_only_after_opt_in_and_stops_immediately() {
    let _guard = DB_LOCK.lock().await;
    let pool_instance = pool().await;
    let pool = &pool_instance;

    for statement in [
        "DELETE FROM relay_events",
        "DELETE FROM relay_sessions",
        "DELETE FROM chat_messages",
        "DELETE FROM chat_room_members",
        "DELETE FROM chat_rooms",
        "DELETE FROM projects",
        "DELETE FROM audit_events",
        "DELETE FROM pool_invitations",
        "DELETE FROM pool_users",
        "DELETE FROM organizations",
    ] {
        sqlx::query(statement).execute(pool).await.expect("clear");
    }

    let (organization, _owner) = pool_identity::setup_pool(
        pool,
        NewOrganization {
            organization_name: "Relay Pool".into(),
            email: "owner-relay@example.com".into(),
            display_name: "Owner".into(),
            password: "integration-password-1".into(),
        },
    )
    .await
    .expect("setup pool");

    let host_public_id = uuid::Uuid::new_v4().to_string();
    let host_id: u64 = {
        sqlx::query(
            "INSERT INTO hosts (public_id, organization_id, slug, display_name, hostname, status) \
             VALUES (?, ?, 'relay-host', 'Relay Host', 'relay-host', 'connected')",
        )
        .bind(&host_public_id)
        .bind(organization.id)
        .execute(pool)
        .await
        .expect("host");
        sqlx::query_scalar("SELECT id FROM hosts WHERE public_id = ?")
            .bind(&host_public_id)
            .fetch_one(pool)
            .await
            .expect("host id")
    };

    let session = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO host_executions (public_id, host_id, prompt, status) \
         VALUES (?, ?, 'follow me', 'running')",
    )
    .bind(&session)
    .bind(host_id)
    .execute(pool)
    .await
    .expect("session");

    // Off by default, and publishing before opt-in is refused.
    let state = relay::state(pool, organization.id, &session)
        .await
        .expect("state");
    assert!(!state.enabled);
    assert!(matches!(
        relay::publish(
            pool,
            organization.id,
            &session,
            "status",
            &json!({"step": 1})
        )
        .await,
        Err(RelayError::NotEnabled)
    ));

    // Opt in, then publish two events.
    let state = relay::enable(pool, organization.id, &session)
        .await
        .expect("enable");
    assert!(state.enabled);

    let first = relay::publish(
        pool,
        organization.id,
        &session,
        "started",
        &json!({"step": 1}),
    )
    .await
    .expect("publish 1");
    let second = relay::publish(
        pool,
        organization.id,
        &session,
        "output",
        &json!({"summary": "tests pass"}),
    )
    .await
    .expect("publish 2");
    assert!(second.sequence > first.sequence);

    // A subscriber that starts late backfills exactly the events it missed,
    // and repeating the read duplicates nothing.
    let missed = relay::backfill(pool, organization.id, &session, Some(first.sequence), None)
        .await
        .expect("backfill");
    assert_eq!(missed.events.len(), 1);
    assert_eq!(missed.events[0].sequence, second.sequence);
    let repeated = relay::backfill(pool, organization.id, &session, Some(first.sequence), None)
        .await
        .expect("repeat backfill");
    assert_eq!(repeated, missed);

    // Credentials and file contents never make it into a relayed event.
    for payload in [
        json!({"token": "abc"}),
        json!({"file": {"path": "/etc/passwd"}}),
    ] {
        assert!(matches!(
            relay::publish(pool, organization.id, &session, "tool", &payload).await,
            Err(RelayError::Invalid(_))
        ));
    }

    // Stopping is immediate.
    let state = relay::stop(pool, organization.id, &session)
        .await
        .expect("stop");
    assert!(!state.enabled);
    assert!(state.stopped_at.is_some());
    assert!(matches!(
        relay::publish(
            pool,
            organization.id,
            &session,
            "status",
            &json!({"step": 9})
        )
        .await,
        Err(RelayError::NotEnabled)
    ));
    // The relay cannot be reopened after a stop.
    assert!(matches!(
        relay::enable(pool, organization.id, &session).await,
        Err(RelayError::Invalid(_))
    ));

    // Backfill still serves what was published before the stop.
    let history = relay::backfill(pool, organization.id, &session, None, None)
        .await
        .expect("history");
    assert_eq!(history.events.len(), 2);

    sqlx::query("DELETE FROM organizations WHERE id = ?")
        .bind(organization.id)
        .execute(pool)
        .await
        .expect("cleanup");
}
