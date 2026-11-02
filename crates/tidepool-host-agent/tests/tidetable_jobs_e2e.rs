//! End-to-end: the pool server, the reference host agent and a loopback
//! `tidetable serve` (phase 1.7.5).
//!
//! The test binds a project to a slug the hub lists, enrolls the reference
//! agent with a one-time token, and dispatches typed `tkrun` jobs over the host
//! WebSocket. It checks a dry-run job that succeeds, a checkout refused by the
//! host's allowlist, and an offline host that refuses dispatch without queueing.
//!
//! It needs MySQL (`DATABASE_URL`), the `tidetable` CLI (`TIDETABLE_BIN`) with
//! its Python package importable by `PYTHON`, and `git`. It is ignored by
//! default; CI runs it with:
//!
//! ```sh
//! cargo test -p tidepool-host-agent --test tidetable_jobs_e2e -- --ignored
//! ```

use actix_web::{App, HttpServer, web};
use serde_json::{Value, json};
use sqlx::MySqlPool;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tidepool::{
    api,
    config::{Config, RateLimitConfig},
    db,
    entities::api_key::ApiKeyScope,
    services::{
        api_keys,
        pool_identity::{self, NewOrganization},
    },
    sockets,
};

const DEFAULT_DB_URL: &str = "mysql://tidepool:password@127.0.0.1:3306/tidepool_dev";
const JWT_SECRET: &str = "tidetable-e2e-jwt-secret-at-least-32-bytes";
const SLUG: &str = "chiton-fixture";
const CHECKOUT: &str = "fixture-main";
const TERMINAL: [&str; 5] = ["succeeded", "failed", "refused", "stopped", "killed"];

/// A scratch directory removed on drop.
struct Workdir(PathBuf);

impl Workdir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("tidepool-e2e-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).expect("create workdir");
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Workdir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Child processes killed on drop, so a failed assertion leaves nothing behind.
struct Processes {
    serve: Option<Child>,
    agent: Option<Child>,
}

impl Drop for Processes {
    fn drop(&mut self) {
        for child in [self.agent.as_mut(), self.serve.as_mut()]
            .into_iter()
            .flatten()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("free port")
        .port()
}

fn wait_for_port(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < deadline, "port {port} never opened");
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn run(program: &str, args: &[&str], dir: &Path) {
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap_or_else(|e| panic!("{program}: {e}"));
    assert!(status.success(), "{program} {args:?} failed");
}

/// A git checkout with one commit on `main`, the local directory for the job.
fn init_checkout(dir: &Path) {
    std::fs::create_dir_all(dir).expect("repo dir");
    run("git", &["init", "-q", "-b", "main", "."], dir);
    std::fs::write(dir.join("README.md"), "fixture\n").expect("readme");
    run("git", &["add", "."], dir);
    run(
        "git",
        &[
            "-c",
            "user.name=e2e",
            "-c",
            "user.email=e2e@example.invalid",
            "commit",
            "-q",
            "-m",
            "fixture",
        ],
        dir,
    );
}

/// Mint the pool's read token and the host's read-write token for `SLUG`.
fn mint_tokens(python: &str, script: &Path, tokens_db: &Path) -> (String, String) {
    let output = Command::new(python)
        .arg(script)
        .arg(tokens_db)
        .arg(SLUG)
        .output()
        .expect("run mint.py");
    assert!(
        output.status.success(),
        "mint.py failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tokens: Value = serde_json::from_slice(&output.stdout).expect("mint.py JSON");
    (
        tokens["pool_read"].as_str().expect("pool_read").to_string(),
        tokens["host"].as_str().expect("host").to_string(),
    )
}

fn test_config() -> Config {
    Config {
        host: "127.0.0.1".into(),
        port: 8080,
        database_url: env_or("DATABASE_URL", DEFAULT_DB_URL),
        redis_url: "redis://127.0.0.1:6379".into(),
        jwt_secret: JWT_SECRET.into(),
        jwt_expiry_secs: 3600,
        rate_limit: RateLimitConfig::default(),
        pool_bootstrap: None,
        public_origin: None,
    }
}

async fn reset(pool: &MySqlPool) {
    for statement in [
        "DELETE FROM projects",
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

/// One blocking HTTP call, off the server's thread. Returns status and JSON.
async fn call(
    method: &'static str,
    url: String,
    token: String,
    body: Option<Value>,
) -> (u16, Value) {
    actix_web::web::block(move || {
        let request = ureq::request(method, &url).set("Authorization", &format!("Bearer {token}"));
        let result = match body {
            Some(body) => request.send_json(body),
            None => request.call(),
        };
        match result {
            Ok(response) => {
                let status = response.status();
                (status, response.into_json().unwrap_or(Value::Null))
            }
            Err(ureq::Error::Status(status, response)) => {
                (status, response.into_json().unwrap_or(Value::Null))
            }
            Err(error) => panic!("{method} {url}: {error}"),
        }
    })
    .await
    .expect("blocking call")
}

async fn host_status(pool: &MySqlPool, host_public_id: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT CAST(status AS CHAR) FROM hosts WHERE public_id = ?")
        .bind(host_public_id)
        .fetch_one(pool)
        .await
        .expect("host status")
}

async fn wait_until<F, Fut>(timeout: Duration, mut check: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + timeout;
    loop {
        if check().await {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        actix_web::rt::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Poll a job until it reaches a terminal status; returns the final view.
async fn wait_for_job(base: &str, key: &str, job_id: &str) -> Value {
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let url = format!("{base}/v1/jobs/{job_id}");
        let (status, body) = call("GET", url, key.to_string(), None).await;
        assert_eq!(status, 200, "job read: {body}");
        if TERMINAL.contains(&body["status"].as_str().unwrap_or_default()) {
            return body;
        }
        assert!(
            Instant::now() < deadline,
            "job {job_id} never finished: {body}"
        );
        actix_web::rt::time::sleep(Duration::from_millis(200)).await;
    }
}

#[actix_web::test]
#[ignore = "needs MySQL, the tidetable CLI and python; see the module docs"]
async fn a_typed_tkrun_job_runs_end_to_end_against_a_local_hub() {
    let work = Workdir::new();
    let bin = env_or("TIDETABLE_BIN", "tidetable");
    let python = env_or("PYTHON", "python");
    let agent_bin = env!("CARGO_BIN_EXE_tidepool-host-agent");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tidetable");

    // Hub: a fixture plan, a registry that lists the slug, minted tokens.
    let plan = work.path("plan.json");
    std::fs::copy(fixtures.join("plan.json"), &plan).expect("copy plan");
    let registry = work.path("registry.json");
    let registry_json = json!({"projects": [{
        "slug": SLUG,
        "name": "Fixture",
        "repo_url": "https://example.invalid/fixture",
        "todo_path": plan,
        "default_branch": "main",
    }]});
    std::fs::write(&registry, registry_json.to_string()).expect("registry");
    let tokens_db = work.path("tokens.db");
    let (pool_read_token, host_token) = mint_tokens(&python, &fixtures.join("mint.py"), &tokens_db);
    let host_token_file = work.path("host.token");
    std::fs::write(&host_token_file, &host_token).expect("host token file");

    let repo = work.path("repo");
    init_checkout(&repo);

    let hub_port = free_port();
    let mut processes = Processes {
        serve: None,
        agent: None,
    };
    processes.serve = Some(
        Command::new(&bin)
            .args([
                "serve",
                "--bind",
                "127.0.0.1",
                "--port",
                &hub_port.to_string(),
            ])
            .arg("--registry")
            .arg(&registry)
            .arg("--sessions-db")
            .arg(work.path("sessions.db"))
            .env("TK_TOKENS_DB", &tokens_db)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start tidetable serve"),
    );
    wait_for_port(hub_port);
    let hub = format!("http://127.0.0.1:{hub_port}");

    // Pool: database, organization, an API key with the scopes this flow uses.
    let db_url = env_or("DATABASE_URL", DEFAULT_DB_URL);
    let pool = db::build_pool(&db_url).await.expect("database");
    db::run_migrations(&pool).await.expect("migrations");
    reset(&pool).await;
    let (organization, _owner) = pool_identity::setup_pool(
        &pool,
        NewOrganization {
            organization_name: "Tidetable E2E".into(),
            email: "owner-e2e@example.com".into(),
            display_name: "E2E Owner".into(),
            password: "e2e-password-long-enough-1".into(),
        },
    )
    .await
    .expect("organization");
    let (_key, key) = api_keys::create_api_key(
        &pool,
        organization.id,
        None,
        "e2e",
        &[
            ApiKeyScope::ProjectsRead,
            ApiKeyScope::ConfigurationInstall,
            ApiKeyScope::RunsDispatch,
            ApiKeyScope::DaemonsEnroll,
        ],
    )
    .await
    .expect("api key");

    // The pool reads TIDETABLE_READ_TOKEN at bind time; set it before serving.
    // SAFETY: this is the only test in the binary and no other thread is running yet.
    unsafe { std::env::set_var("TIDETABLE_READ_TOKEN", &pool_read_token) };

    let server_pool = pool.clone();
    let server = HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(test_config()))
            .app_data(web::Data::new(server_pool.clone()))
            .configure(api::jobs::config)
            .configure(api::projects::config)
            .configure(api::hosts::config)
            .route("/ws/host", web::get().to(sockets::host_ws::ws_host))
    })
    .workers(1)
    .bind(("127.0.0.1", 0))
    .expect("bind pool");
    let pool_port = server.addrs()[0].port();
    let running = server.run();
    let stop = running.handle();
    actix_web::rt::spawn(running);
    let base = format!("http://127.0.0.1:{pool_port}");

    // Host row and one-time enrollment token, then the reference agent enrolls.
    let host_public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO hosts (public_id, organization_id, slug, display_name, hostname, status, \
         execution_granted) VALUES (?, ?, 'fixture-host', 'Fixture Host', 'fixture-host', \
         'offline', 0)",
    )
    .bind(&host_public_id)
    .bind(organization.id)
    .execute(&pool)
    .await
    .expect("host row");
    let (status, minted) = call(
        "POST",
        format!("{base}/v1/hosts/enrollment-tokens"),
        key.clone(),
        Some(json!({"host_id": host_public_id})),
    )
    .await;
    assert_eq!(status, 201, "mint enrollment token: {minted}");
    let enrollment = minted["token"]
        .as_str()
        .expect("enrollment token")
        .to_string();

    let credential = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let checkouts = json!({ CHECKOUT: repo.to_string_lossy() }).to_string();
    processes.agent = Some(
        Command::new(agent_bin)
            .env("POOL_HTTP_URL", &base)
            .env("POOL_WS_URL", format!("ws://127.0.0.1:{pool_port}/ws/host"))
            .env("ENROLLMENT_TOKEN", &enrollment)
            .env("CREDENTIAL", &credential)
            .env("CHECKOUTS", &checkouts)
            .env("TIDETABLE_TOKEN_FILE", &host_token_file)
            .env("TIDETABLE_BIN", &bin)
            .env_remove("HOST_ID")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start reference agent"),
    );

    let connected = wait_until(Duration::from_secs(30), || {
        let pool = pool.clone();
        let id = host_public_id.clone();
        async move { host_status(&pool, &id).await == "connected" }
    })
    .await;
    assert!(connected, "the reference agent never connected");

    // Project: create it, bind it to the hub's slug, refuse an unlisted one.
    let project_public_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO projects (public_id, organization_id, slug, name) \
         VALUES (?, ?, 'fixture-project', 'Fixture Project')",
    )
    .bind(&project_public_id)
    .bind(organization.id)
    .execute(&pool)
    .await
    .expect("project row");
    let binding_url = format!("{base}/v1/projects/{project_public_id}/tidetable");

    let (status, body) = call(
        "PUT",
        binding_url.clone(),
        key.clone(),
        Some(json!({"server": hub.clone(), "slug": "not-listed-by-hub"})),
    )
    .await;
    assert_eq!(status, 422, "unlisted slug must be refused: {body}");

    let (status, body) = call(
        "PUT",
        binding_url.clone(),
        key.clone(),
        Some(json!({"server": hub.clone(), "slug": SLUG})),
    )
    .await;
    assert_eq!(status, 200, "bind: {body}");
    assert_eq!(body["slug"], SLUG);

    // Without the jobs.tkrun grant, dispatch is 403 and stores nothing.
    let jobs_url = format!("{base}/v1/projects/{project_public_id}/jobs");
    let (status, body) = call(
        "POST",
        jobs_url.clone(),
        key.clone(),
        Some(json!({
            "host": host_public_id,
            "checkout": CHECKOUT,
            "options": {"dryRun": true},
        })),
    )
    .await;
    assert_eq!(status, 403, "dispatch without grant: {body}");

    let grant_url =
        format!("{base}/v1/projects/{project_public_id}/hosts/{host_public_id}/grants/jobs.tkrun");
    let config_hash = "a".repeat(64);
    let (status, body) = call(
        "PUT",
        grant_url.clone(),
        key.clone(),
        Some(json!({"configHash": config_hash})),
    )
    .await;
    assert_eq!(status, 200, "grant: {body}");
    assert_eq!(body["capability"], "jobs.tkrun");

    // Dry-run job: the agent checks the hub, runs tkrun, streams output and exits.
    let (status, created) = call(
        "POST",
        jobs_url.clone(),
        key.clone(),
        Some(json!({
            "host": host_public_id,
            "checkout": CHECKOUT,
            "options": {"dryRun": true},
        })),
    )
    .await;
    assert_eq!(status, 201, "dispatch: {created}");
    let job_id = created["id"].as_str().expect("job id").to_string();

    let finished = wait_for_job(&base, &key, &job_id).await;
    assert_eq!(finished["status"], "succeeded", "{finished}");
    assert_eq!(finished["exitCode"], 0, "{finished}");
    let output = finished["output"].as_str().unwrap_or_default();
    assert!(
        output.contains("DRY RUN"),
        "output missing tkrun banner: {output}"
    );
    assert!(
        !output.contains(&host_token),
        "the host token must never reach the pool"
    );

    // A checkout the host does not allow is refused before anything spawns.
    let (status, created) = call(
        "POST",
        jobs_url.clone(),
        key.clone(),
        Some(json!({
            "host": host_public_id,
            "checkout": "elsewhere",
            "options": {"dryRun": true},
        })),
    )
    .await;
    assert_eq!(status, 201, "dispatch: {created}");
    let refused = wait_for_job(&base, &key, created["id"].as_str().expect("job id")).await;
    assert_eq!(refused["status"], "refused", "{refused}");
    assert_eq!(refused["refusalCode"], "checkout_not_allowed", "{refused}");

    // Offline host: dispatch is refused and no job row is written.
    if let Some(mut agent) = processes.agent.take() {
        let _ = agent.kill();
        let _ = agent.wait();
    }
    let went_offline = wait_until(Duration::from_secs(10), || {
        let pool = pool.clone();
        let id = host_public_id.clone();
        async move { host_status(&pool, &id).await == "offline" }
    })
    .await;
    assert!(
        went_offline,
        "host did not go offline after the agent stopped"
    );

    let rows_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM host_jobs")
        .fetch_one(&pool)
        .await
        .expect("count");
    let (status, body) = call(
        "POST",
        jobs_url,
        key.clone(),
        Some(json!({
            "host": host_public_id,
            "checkout": CHECKOUT,
            "options": {"dryRun": true},
        })),
    )
    .await;
    assert_eq!(status, 409, "offline dispatch: {body}");
    assert!(
        body.to_string().contains("daemon_not_connected"),
        "offline dispatch must name daemon_not_connected: {body}"
    );
    let rows_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM host_jobs")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(
        rows_before, rows_after,
        "offline dispatch must not queue a job"
    );

    stop.stop(true).await;
}
