//! Integration tests for the chatroom schema and service (task 1.5.1).
//!
//! Require a live MySQL; skipped by default (`#[ignore]`).
//! Run with: `cargo test --test chatroom_schema_integration -- --ignored`
//!
//! Defaults to the Docker dev stack:
//!   mysql://tidepool:password@127.0.0.1:3306/tidepool_dev
//! Override with `DATABASE_URL`.
//!
//! The schema test applies every migration in `migrations/` to a throwaway
//! database of its own, so it never touches the dev data. It then runs the
//! chatroom down file and checks the tables are gone, which is the same
//! up/down contract the game migrations are held to.

use sqlx::MySqlPool;
use sqlx::mysql::MySqlPoolOptions;
use std::path::Path;
use tidepool::{
    db,
    entities::chatroom::{MemberRole, Principal, RoomVisibility},
    services::chatrooms::{self, NewMember, NewRoom},
};

const DEFAULT_DB_URL: &str = "mysql://tidepool:password@127.0.0.1:3306/tidepool_dev";
/// The dev stack's root account. Creating and dropping a throwaway database
/// needs privileges the application user deliberately does not have.
const DEFAULT_ADMIN_URL: &str = "mysql://root:rootpassword@127.0.0.1:3306";

static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn database_url() -> String {
    std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DB_URL.into())
}

/// Replace or append the database name in a MySQL URL.
fn with_database(url: &str, database: &str) -> String {
    let authority_start = url.find("://").map(|index| index + 3).unwrap_or(0);
    match url[authority_start..].find('/') {
        Some(offset) => format!("{}/{}", &url[..authority_start + offset], database),
        None => format!("{}/{database}", url.trim_end_matches('/')),
    }
}

fn admin_url() -> String {
    std::env::var("TIDEPOOL_TEST_ADMIN_URL").unwrap_or_else(|_| DEFAULT_ADMIN_URL.into())
}

async fn admin_pool() -> MySqlPool {
    MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&admin_url())
        .await
        .expect("MySQL admin connection failed — is Docker up?")
}

/// Statements in a migration file, with `--` comments and blank lines gone.
///
/// Comments are stripped before splitting on `;`, because a comment may itself
/// contain a semicolon.
fn statements(sql: &str) -> Vec<String> {
    let mut cleaned = String::with_capacity(sql.len());
    let mut in_comment = false;
    let mut in_single = false;
    let mut previous = '\0';
    for character in sql.chars() {
        if in_comment {
            if character == '\n' {
                in_comment = false;
                cleaned.push('\n');
            }
            previous = character;
            continue;
        }
        if !in_single && character == '-' && previous == '-' {
            in_comment = true;
            cleaned.pop();
            previous = character;
            continue;
        }
        if character == '\'' && previous != '\\' {
            in_single = !in_single;
        }
        cleaned.push(character);
        previous = character;
    }

    cleaned
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
        .map(str::to_string)
        .collect()
}

async fn table_exists(pool: &MySqlPool, table: &str) -> bool {
    let found: Option<(String,)> = sqlx::query_as(
        "SELECT CAST(table_name AS CHAR) FROM information_schema.tables \
         WHERE table_schema = DATABASE() AND table_name = ?",
    )
    .bind(table)
    .fetch_optional(pool)
    .await
    .expect("information_schema query");
    found.is_some()
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn every_migration_applies_and_the_chatroom_down_reverses_it() {
    let _guard = DB_LOCK.lock().await;
    let admin = admin_pool().await;
    let database = format!("tidepool_chatrooms_{}", uuid::Uuid::new_v4().simple());

    sqlx::query(&format!("CREATE DATABASE `{database}`"))
        .execute(&admin)
        .await
        .expect("create throwaway database");

    let pool = db::build_pool(&with_database(&admin_url(), &database))
        .await
        .expect("connect to throwaway database");

    // Apply every up migration in filename order, exactly as a fresh install
    // would.
    let mut files: Vec<_> = std::fs::read_dir("migrations")
        .expect("migrations directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.to_string_lossy().ends_with(".up.sql"))
        .collect();
    files.sort();

    for path in &files {
        let sql = std::fs::read_to_string(path).expect("read migration");
        for statement in statements(&sql) {
            sqlx::query(&statement)
                .execute(&pool)
                .await
                .unwrap_or_else(|error| {
                    panic!(
                        "{} failed: {error}",
                        path.file_name().unwrap().to_string_lossy()
                    )
                });
        }
    }

    for table in ["chat_rooms", "chat_room_members", "chat_messages"] {
        assert!(table_exists(&pool, table).await, "{table} should exist");
    }

    // `origin_pool` exists, is nullable, and no row sets it (v1 rooms).
    let column: (String, String) = sqlx::query_as(
        "SELECT CAST(is_nullable AS CHAR), CAST(data_type AS CHAR) FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND table_name = 'chat_rooms' \
           AND column_name = 'origin_pool'",
    )
    .fetch_one(&pool)
    .await
    .expect("origin_pool column");
    assert_eq!(column.0, "YES");
    assert_eq!(column.1, "varchar");

    let (auto_increment,): (String,) = sqlx::query_as(
        "SELECT CAST(extra AS CHAR) FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND table_name = 'chat_messages' \
           AND column_name = 'id'",
    )
    .fetch_one(&pool)
    .await
    .expect("message id column");
    assert!(auto_increment.contains("auto_increment"));

    // The history index is on (room_id, id), which is what ordering needs.
    let indexes: Vec<(String,)> = sqlx::query_as(
        "SELECT CAST(index_name AS CHAR) FROM information_schema.statistics \
         WHERE table_schema = DATABASE() AND table_name = 'chat_messages' \
           AND column_name = 'id'",
    )
    .fetch_all(&pool)
    .await
    .expect("indexes");
    assert!(
        indexes
            .iter()
            .any(|(name,)| name == "idx_chat_messages_room_id"),
        "expected a (room_id, id) history index, got {indexes:?}"
    );

    // The down file reverses only the chatroom tables.
    let down = std::fs::read_to_string(
        Path::new("migrations").join("20261022000001_create_chatrooms.down.sql"),
    )
    .expect("read chatroom down migration");
    for statement in statements(&down) {
        sqlx::query(&statement)
            .execute(&pool)
            .await
            .expect("apply chatroom down migration");
    }
    for table in ["chat_rooms", "chat_room_members", "chat_messages"] {
        assert!(
            !table_exists(&pool, table).await,
            "{table} should be dropped"
        );
    }
    // Other migrations are untouched by the chatroom down file.
    assert!(table_exists(&pool, "projects").await);
    assert!(table_exists(&pool, "runs").await);

    pool.close().await;
    sqlx::query(&format!("DROP DATABASE `{database}`"))
        .execute(&admin)
        .await
        .expect("drop throwaway database");
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn messages_are_ordered_by_a_stable_sequence() {
    let _guard = DB_LOCK.lock().await;
    let pool = db::build_pool(&database_url()).await.expect("connect");

    // One organization per pool, so reuse the existing one when the dev stack
    // is already initialized.
    let existing: Option<(u64,)> = sqlx::query_as("SELECT id FROM organizations LIMIT 1")
        .fetch_optional(&pool)
        .await
        .expect("organization lookup");
    let organization_id = match existing {
        Some((id,)) => id,
        None => {
            let (organization, _owner) = tidepool::services::pool_identity::setup_pool(
                &pool,
                tidepool::services::pool_identity::NewOrganization {
                    organization_name: "Chatroom Tests".into(),
                    email: format!("owner-{}@example.com", uuid::Uuid::new_v4()),
                    display_name: "Owner".into(),
                    password: "correct horse battery staple".into(),
                },
            )
            .await
            .expect("setup pool");
            organization.id
        }
    };

    let project = tidepool::services::projects::create_project(
        &pool,
        organization_id,
        tidepool::services::projects::NewProject {
            slug: None,
            name: format!("Chatroom Tests {}", uuid::Uuid::new_v4()),
        },
        tidepool::services::api_keys::AuditActor::System,
    )
    .await
    .expect("project");

    // A member of the pool, inserted directly: these tests never sign in, so
    // the password hash is never exercised.
    let user_public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO pool_users (public_id, organization_id, email, display_name, \
                                 password_hash, role) \
         VALUES (?, ?, ?, 'Chat Author', 'not-a-real-hash', 'member')",
    )
    .bind(&user_public_id)
    .bind(organization_id)
    .bind(format!("chat-{}@example.com", uuid::Uuid::new_v4()))
    .execute(&pool)
    .await
    .expect("member");

    let room = chatrooms::create_room(
        &pool,
        organization_id,
        &project.public_id,
        NewRoom {
            name: "builds".into(),
            visibility: RoomVisibility::Private,
            origin_pool: None,
        },
    )
    .await
    .expect("room");
    assert!(room.origin_pool.is_none());

    // A duplicate room name in the same project is a conflict.
    assert!(matches!(
        chatrooms::create_room(
            &pool,
            organization_id,
            &project.public_id,
            NewRoom {
                name: "builds".into(),
                visibility: RoomVisibility::Private,
                origin_pool: None,
            },
        )
        .await,
        Err(chatrooms::ChatroomServiceError::Conflict(_))
    ));

    let unknown = Principal::Session(format!("missing-{}", uuid::Uuid::new_v4()));
    assert!(
        chatrooms::post_message(&pool, organization_id, &room.id, &unknown, "hi", None)
            .await
            .is_err()
    );

    let principal = Principal::User(user_public_id.clone());
    let member = chatrooms::add_member(
        &pool,
        organization_id,
        &room.id,
        NewMember {
            principal: principal.clone(),
            role: MemberRole::Member,
        },
    )
    .await
    .expect("add member");
    assert_eq!(member.principal, principal);

    // Duplicate membership is a conflict, not a second row.
    assert!(matches!(
        chatrooms::add_member(
            &pool,
            organization_id,
            &room.id,
            NewMember {
                principal: principal.clone(),
                role: MemberRole::Member,
            },
        )
        .await,
        Err(chatrooms::ChatroomServiceError::Conflict(_))
    ));

    let first =
        chatrooms::post_message(&pool, organization_id, &room.id, &principal, "first", None)
            .await
            .expect("first message");
    let reply = chatrooms::post_message(
        &pool,
        organization_id,
        &room.id,
        &principal,
        "second",
        Some(&first.id),
    )
    .await
    .expect("reply");

    assert!(reply.sequence > first.sequence);
    assert_eq!(reply.reply_to.as_deref(), Some(first.id.as_str()));

    let page = chatrooms::history(&pool, organization_id, &room.id, None, Some(1))
        .await
        .expect("history");
    assert_eq!(page.messages.len(), 1);
    assert_eq!(page.messages[0].id, first.id);
    assert_eq!(page.next, Some(first.sequence));

    let rest = chatrooms::history(&pool, organization_id, &room.id, page.next, None)
        .await
        .expect("next page");
    assert_eq!(rest.messages.len(), 1);
    assert_eq!(rest.messages[0].id, reply.id);
    assert_eq!(rest.next, None);

    // A message's sequence is stable across reads.
    let again = chatrooms::history(&pool, organization_id, &room.id, None, None)
        .await
        .expect("history again");
    assert_eq!(again.messages[0].sequence, first.sequence);

    let rooms = chatrooms::rooms_for_principal(&pool, organization_id, &principal)
        .await
        .expect("rooms for principal");
    assert_eq!(rooms.len(), 1);
    assert_eq!(rooms[0].id, room.id);

    // Cleanup: deleting the project cascades to rooms, members and messages.
    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(project.id)
        .execute(&pool)
        .await
        .expect("cleanup project");
    sqlx::query("DELETE FROM pool_users WHERE public_id = ?")
        .bind(&user_public_id)
        .execute(&pool)
        .await
        .expect("cleanup member");
}
