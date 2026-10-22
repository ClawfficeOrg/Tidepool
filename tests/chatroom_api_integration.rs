//! Integration tests for the chatroom HTTP API (task 1.5.2).
//!
//! Require a live MySQL; skipped by default (`#[ignore]`).
//! Run with: `cargo test --test chatroom_api_integration -- --ignored`
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
    entities::chatroom::{MemberRole, Principal, RoomVisibility},
    services::{
        auth as auth_svc,
        chatrooms::{self as chatroom_svc, NewMember, NewRoom},
        pool_identity::{self, NewOrganization},
        projects,
    },
};

const DEFAULT_DB_URL: &str = "mysql://tidepool:password@127.0.0.1:3306/tidepool_dev";
const JWT_SECRET: &str = "integration-test-chatrooms-jwt-secret-32ch";

static DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn pool() -> MySqlPool {
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DB_URL.into());
    db::build_pool(&db_url).await.expect(
        "DB connection failed — is Docker up? Run: docker compose -f config/docker-compose.dev.yml up -d",
    )
}

async fn reset(pool: &MySqlPool) {
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
    }
}

fn bearer(token: &str) -> (&'static str, String) {
    ("Authorization", format!("Bearer {token}"))
}

/// Insert a pool user with the given role and return their public id.
async fn seed_user(pool: &MySqlPool, organization_id: u64, role: &str, label: &str) -> String {
    let public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO pool_users (public_id, organization_id, email, display_name, \
                                 password_hash, role) \
         VALUES (?, ?, ?, ?, 'not-a-real-hash', ?)",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(format!("{}-{}@example.com", label, uuid::Uuid::new_v4()))
    .bind(label)
    .bind(role)
    .execute(pool)
    .await
    .expect("seed user");
    public_id
}

async fn join(
    pool: &MySqlPool,
    organization_id: u64,
    room: &str,
    user_public_id: &str,
    role: MemberRole,
) {
    chatroom_svc::add_member(
        pool,
        organization_id,
        room,
        NewMember {
            principal: Principal::User(user_public_id.to_string()),
            role,
        },
    )
    .await
    .expect("add member");
}

async fn message_count(pool: &MySqlPool, room_public_id: &str) -> i64 {
    let (count,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM chat_messages m JOIN chat_rooms r ON r.id = m.room_id \
         WHERE r.public_id = ?",
    )
    .bind(room_public_id)
    .fetch_one(pool)
    .await
    .expect("message count");
    count
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn membership_gates_reads_and_writes_indistinguishably() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;

    let (organization, owner) = pool_identity::setup_pool(
        &pool,
        NewOrganization {
            organization_name: "Chatroom API".into(),
            email: "owner-chat@example.com".into(),
            display_name: "Owner".into(),
            password: "integration-password-1".into(),
        },
    )
    .await
    .expect("setup pool");

    let project = projects::create_project(
        &pool,
        organization.id,
        projects::NewProject {
            slug: None,
            name: "Chatroom API".into(),
        },
        tidepool::services::api_keys::AuditActor::System,
    )
    .await
    .expect("project");

    let room = chatroom_svc::create_room(
        &pool,
        organization.id,
        &project.public_id,
        NewRoom {
            name: "builds".into(),
            visibility: RoomVisibility::Private,
            origin_pool: None,
        },
    )
    .await
    .expect("room");
    join(
        &pool,
        organization.id,
        &room.id,
        &owner.public_id,
        MemberRole::Owner,
    )
    .await;

    let writer = seed_user(&pool, organization.id, "member", "writer").await;
    join(
        &pool,
        organization.id,
        &room.id,
        &writer,
        MemberRole::Member,
    )
    .await;
    let viewer = seed_user(&pool, organization.id, "member", "viewer").await;
    join(
        &pool,
        organization.id,
        &room.id,
        &viewer,
        MemberRole::Viewer,
    )
    .await;
    let outsider = seed_user(&pool, organization.id, "member", "outsider").await;

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::chatrooms::config),
    )
    .await;

    let owner_token = auth_svc::issue_jwt(JWT_SECRET, &owner.public_id, 3600).expect("jwt");
    let writer_token = auth_svc::issue_jwt(JWT_SECRET, &writer, 3600).expect("jwt");
    let viewer_token = auth_svc::issue_jwt(JWT_SECRET, &viewer, 3600).expect("jwt");
    let outsider_token = auth_svc::issue_jwt(JWT_SECRET, &outsider, 3600).expect("jwt");

    // The owner sees the room in their list.
    let req = test::TestRequest::get()
        .uri("/v1/chatrooms")
        .insert_header(bearer(&owner_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let rooms: Value = test::read_body_json(resp).await;
    assert_eq!(rooms.as_array().expect("array").len(), 1);
    assert_eq!(rooms[0]["id"], room.id);

    // Members read the member list.
    let req = test::TestRequest::get()
        .uri(&format!("/v1/chatrooms/{}/members", room.id))
        .insert_header(bearer(&writer_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // A non-member gets 404 on every route, exactly like a missing room.
    for (method, uri) in [
        ("GET", format!("/v1/chatrooms/{}/members", room.id)),
        ("GET", format!("/v1/chatrooms/{}/messages", room.id)),
        ("POST", format!("/v1/chatrooms/{}/messages", room.id)),
    ] {
        let missing = format!("/v1/chatrooms/{}/messages", uuid::Uuid::new_v4());
        let request = if method == "GET" {
            test::TestRequest::get().uri(&uri)
        } else {
            test::TestRequest::post()
                .uri(&uri)
                .set_json(json!({"body": "hi"}))
        };
        let resp = test::call_service(
            &app,
            request.insert_header(bearer(&outsider_token)).to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{method} {uri}");

        let request = if method == "GET" {
            test::TestRequest::get().uri(&missing)
        } else {
            test::TestRequest::post()
                .uri(&missing)
                .set_json(json!({"body": "hi"}))
        };
        let resp = test::call_service(
            &app,
            request.insert_header(bearer(&outsider_token)).to_request(),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{method} {missing}");
    }
    assert_eq!(message_count(&pool, &room.id).await, 0, "no outsider write");

    // A viewer may read but not send.
    let req = test::TestRequest::get()
        .uri(&format!("/v1/chatrooms/{}/messages", room.id))
        .insert_header(bearer(&viewer_token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let req = test::TestRequest::post()
        .uri(&format!("/v1/chatrooms/{}/messages", room.id))
        .insert_header(bearer(&viewer_token))
        .set_json(json!({"body": "viewer cannot send"}))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert_eq!(message_count(&pool, &room.id).await, 0);

    // Empty and oversized bodies are refused before any write.
    for body in [
        json!({"body": "   "}),
        json!({"body": "a".repeat(1_000_001)}),
    ] {
        let req = test::TestRequest::post()
            .uri(&format!("/v1/chatrooms/{}/messages", room.id))
            .insert_header(bearer(&writer_token))
            .set_json(body)
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
    assert_eq!(message_count(&pool, &room.id).await, 0);
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn cursor_paging_stays_stable_while_new_messages_arrive() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;

    let (organization, owner) = pool_identity::setup_pool(
        &pool,
        NewOrganization {
            organization_name: "Chatroom Paging".into(),
            email: "owner-paging@example.com".into(),
            display_name: "Owner".into(),
            password: "integration-password-1".into(),
        },
    )
    .await
    .expect("setup pool");

    let project = projects::create_project(
        &pool,
        organization.id,
        projects::NewProject {
            slug: None,
            name: "Chatroom Paging".into(),
        },
        tidepool::services::api_keys::AuditActor::System,
    )
    .await
    .expect("project");

    let room = chatroom_svc::create_room(
        &pool,
        organization.id,
        &project.public_id,
        NewRoom {
            name: "paging".into(),
            visibility: RoomVisibility::Private,
            origin_pool: None,
        },
    )
    .await
    .expect("room");
    join(
        &pool,
        organization.id,
        &room.id,
        &owner.public_id,
        MemberRole::Owner,
    )
    .await;

    let principal = Principal::User(owner.public_id.clone());
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::chatrooms::config),
    )
    .await;
    let token = auth_svc::issue_jwt(JWT_SECRET, &owner.public_id, 3600).expect("jwt");

    for text in ["one", "two"] {
        let req = test::TestRequest::post()
            .uri(&format!("/v1/chatrooms/{}/messages", room.id))
            .insert_header(bearer(&token))
            .set_json(json!({"body": text}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    let req = test::TestRequest::get()
        .uri(&format!("/v1/chatrooms/{}/messages?limit=1", room.id))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let page: Value = test::read_body_json(resp).await;
    assert_eq!(page["messages"].as_array().expect("messages").len(), 1);
    let cursor = page["next"].as_u64().expect("next cursor");

    // New messages arrive between the two reads.
    for text in ["three", "four"] {
        chatroom_svc::post_message(&pool, organization.id, &room.id, &principal, text, None)
            .await
            .expect("concurrent message");
    }

    let req = test::TestRequest::get()
        .uri(&format!(
            "/v1/chatrooms/{}/messages?after={cursor}&limit=2",
            room.id
        ))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let page: Value = test::read_body_json(resp).await;
    let bodies: Vec<&str> = page["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .map(|message| message["body"].as_str().expect("body"))
        .collect();
    // The cursor skipped only what was already read.
    assert_eq!(bodies, vec!["two", "three"]);
    let next = page["next"].as_u64().expect("next cursor");

    let req = test::TestRequest::get()
        .uri(&format!(
            "/v1/chatrooms/{}/messages?after={next}&limit=10",
            room.id
        ))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let page: Value = test::read_body_json(resp).await;
    assert_eq!(page["messages"].as_array().expect("messages").len(), 1);

    // Nothing newer: an empty page is the currentness signal.
    let last = page["messages"][0]["sequence"].as_u64().expect("sequence");
    let req = test::TestRequest::get()
        .uri(&format!("/v1/chatrooms/{}/messages?after={last}", room.id))
        .insert_header(bearer(&token))
        .to_request();
    let resp = test::call_service(&app, req).await;
    let page: Value = test::read_body_json(resp).await;
    assert!(page["messages"].as_array().expect("messages").is_empty());
    assert!(page.get("next").is_none());

    sqlx::query("DELETE FROM organizations WHERE id = ?")
        .bind(organization.id)
        .execute(&pool)
        .await
        .expect("cleanup");
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn the_live_stream_refuses_non_members_and_bad_tokens() {
    let _guard = DB_LOCK.lock().await;
    let pool = pool().await;
    reset(&pool).await;

    let (organization, owner) = pool_identity::setup_pool(
        &pool,
        NewOrganization {
            organization_name: "Chatroom Stream".into(),
            email: "owner-stream@example.com".into(),
            display_name: "Owner".into(),
            password: "integration-password-1".into(),
        },
    )
    .await
    .expect("setup pool");

    let project = projects::create_project(
        &pool,
        organization.id,
        projects::NewProject {
            slug: None,
            name: "Chatroom Stream".into(),
        },
        tidepool::services::api_keys::AuditActor::System,
    )
    .await
    .expect("project");

    let room = chatroom_svc::create_room(
        &pool,
        organization.id,
        &project.public_id,
        NewRoom {
            name: "stream".into(),
            visibility: RoomVisibility::Private,
            origin_pool: None,
        },
    )
    .await
    .expect("room");
    join(
        &pool,
        organization.id,
        &room.id,
        &owner.public_id,
        MemberRole::Owner,
    )
    .await;

    let outsider = seed_user(&pool, organization.id, "member", "outsider").await;
    let outsider_token = auth_svc::issue_jwt(JWT_SECRET, &outsider, 3600).expect("jwt");
    let owner_token = auth_svc::issue_jwt(JWT_SECRET, &owner.public_id, 3600).expect("jwt");

    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(config()))
            .app_data(web::Data::new(pool.clone()))
            .configure(api::chatrooms::config),
    )
    .await;

    // No token: unauthorized.
    let req = test::TestRequest::get()
        .uri(&format!("/v1/chatrooms/{}/stream", room.id))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert!(
        resp.status() == StatusCode::BAD_REQUEST || resp.status() == StatusCode::UNAUTHORIZED,
        "a missing token is refused, got {}",
        resp.status()
    );

    // A non-member cannot open the stream, and is told the room is missing.
    let req = test::TestRequest::get()
        .uri(&format!(
            "/v1/chatrooms/{}/stream?token={outsider_token}",
            room.id
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    // The owner passes the gate; without the WebSocket upgrade headers the
    // handler then fails the upgrade, which proves authorisation happened.
    let req = test::TestRequest::get()
        .uri(&format!(
            "/v1/chatrooms/{}/stream?token={owner_token}",
            room.id
        ))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(resp.status(), StatusCode::NOT_FOUND);
}
