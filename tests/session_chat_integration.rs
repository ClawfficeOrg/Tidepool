//! Integration tests for agent sessions as chat participants (task 1.5.4).
//!
//! Require a live MySQL; skipped by default (`#[ignore]`).
//! Run with: `cargo test --test session_chat_integration -- --ignored`

use sqlx::MySqlPool;
use tidepool::{
    db,
    entities::chatroom::{MemberRole, Principal, RoomVisibility},
    services::{
        chatrooms::{self as chatroom_svc, NewMember, NewRoom},
        pool_identity::{self, NewOrganization},
        projects,
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
async fn a_session_joins_posts_and_keeps_its_messages() {
    let _guard = DB_LOCK.lock().await;
    let pool_instance = pool().await;
    let pool = &pool_instance;

    for statement in [
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
            organization_name: "Session Chat".into(),
            email: "owner-session@example.com".into(),
            display_name: "Owner".into(),
            password: "integration-password-1".into(),
        },
    )
    .await
    .expect("setup pool");

    let project = projects::create_project(
        pool,
        organization.id,
        projects::NewProject {
            slug: None,
            name: "Session Chat".into(),
        },
        tidepool::services::api_keys::AuditActor::System,
    )
    .await
    .expect("project");

    // A live host and one of its agent sessions.
    let host_public_id = uuid::Uuid::new_v4().to_string();
    let host_id: u64 = {
        sqlx::query(
            "INSERT INTO hosts (public_id, organization_id, slug, display_name, hostname, status) \
             VALUES (?, ?, 'session-host', 'Session Host', 'session-host', 'connected')",
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

    let session_public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO host_executions (public_id, host_id, prompt, status) \
         VALUES (?, ?, 'work on it', 'running')",
    )
    .bind(&session_public_id)
    .bind(host_id)
    .execute(pool)
    .await
    .expect("session");

    let room = chatroom_svc::create_room(
        pool,
        organization.id,
        &project.public_id,
        NewRoom {
            name: "agents".into(),
            visibility: RoomVisibility::Private,
            origin_pool: None,
        },
    )
    .await
    .expect("room");

    let session = Principal::Session(session_public_id.clone());
    let membership = chatroom_svc::add_member(
        pool,
        organization.id,
        &room.id,
        NewMember {
            principal: session.clone(),
            role: MemberRole::Member,
        },
    )
    .await
    .expect("session joins");
    assert_eq!(membership.principal, session);
    assert_eq!(
        serde_json::to_value(&membership.principal).expect("json")["kind"],
        "session"
    );

    let message = chatroom_svc::post_message(
        pool,
        organization.id,
        &room.id,
        &session,
        "I will take the issue",
        None,
    )
    .await
    .expect("session posts");
    assert_eq!(message.author, Some(session.clone()));
    assert_eq!(message.project, project.public_id);
    // A session author is rendered with a different tag than a person.
    let author = serde_json::to_value(message.author.as_ref().expect("author")).expect("json");
    assert_eq!(author["kind"], "session");
    assert_eq!(author["id"], session_public_id);

    // The session ends: its execution row disappears, but the message stays.
    sqlx::query("DELETE FROM host_executions WHERE public_id = ?")
        .bind(&session_public_id)
        .execute(pool)
        .await
        .expect("end session");

    let history = chatroom_svc::history(pool, organization.id, &room.id, None, None)
        .await
        .expect("history");
    assert_eq!(history.messages.len(), 1);
    assert_eq!(history.messages[0].body, "I will take the issue");
    assert_eq!(
        history.messages[0].author.as_ref().map(Principal::id),
        Some(session_public_id.as_str())
    );

    // An unknown session id cannot join or post.
    let ghost = Principal::Session(uuid::Uuid::new_v4().to_string());
    assert!(matches!(
        chatroom_svc::post_message(pool, organization.id, &room.id, &ghost, "hi", None).await,
        Err(chatroom_svc::ChatroomServiceError::Invalid(_))
    ));
    assert!(matches!(
        chatroom_svc::add_member(
            pool,
            organization.id,
            &room.id,
            NewMember {
                principal: ghost,
                role: MemberRole::Member,
            },
        )
        .await,
        Err(chatroom_svc::ChatroomServiceError::Invalid(_))
    ));

    sqlx::query("DELETE FROM organizations WHERE id = ?")
        .bind(organization.id)
        .execute(pool)
        .await
        .expect("cleanup");
}
