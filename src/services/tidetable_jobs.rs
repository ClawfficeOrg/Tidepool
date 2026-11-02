//! TideTable bindings and typed `tkrun` jobs (phase 1.7).
//!
//! A project binds to one TideTable slug on one hub. Binding is accepted only
//! when the hub already lists that slug; the pool never registers slugs. A job
//! runs `tidetable tkrun run <slug>` on one connected host, which builds the
//! argv itself from the typed fields below. There is no queue: a job row is
//! written before `start_job` is sent, and an offline host is refused.
//!
//! The pool's TideTable token is read-only and lives only in this process
//! (`TIDETABLE_READ_TOKEN`). It is never stored, returned or logged.

use crate::entities::project::Project;
use crate::services::host_presence::{self, HostPresenceError};
use crate::services::host_protocol::{self, HostProtocolError, JOBS_TKRUN_FEATURE};
use crate::services::projects::{self as project_svc, ProjectServiceError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, MySqlPool};
use std::time::Duration;
use thiserror::Error;
use url::Url;

/// Environment variable holding the pool's read-only TideTable token.
pub const READ_TOKEN_ENV: &str = "TIDETABLE_READ_TOKEN";

/// Largest stored output per job, in bytes. Older lines are dropped first.
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;

/// Statuses that still hold the project's single live job slot.
const LIVE_STATUSES: [&str; 2] = ["queued", "running"];

/// Errors for bindings and jobs.
#[derive(Debug, Error)]
pub enum JobError {
    /// Project, host or job is unknown in this organization.
    #[error("not found")]
    NotFound,
    /// The project has no TideTable binding yet.
    #[error("project has no TideTable binding")]
    NotBound,
    /// Input failed validation; the message names the field.
    #[error("invalid input: {0}")]
    Invalid(String),
    /// The hub does not list the slug, or refused the pool's read token.
    #[error("hub refused binding: {0}")]
    HubRefused(String),
    /// The hub could not be reached or the pool has no read token.
    #[error("hub unavailable: {0}")]
    HubUnavailable(String),
    /// A live job already holds this project's slot.
    #[error("job_already_running: project already has a live job")]
    AlreadyRunning,
    /// The host has not negotiated the feature this job needs.
    #[error("host has not negotiated feature '{0}'")]
    FeatureMissing(String),
    /// The host is not connected; nothing was queued.
    #[error("daemon_not_connected: host '{slug}' is offline")]
    DaemonNotConnected {
        /// Host slug.
        slug: String,
    },
    /// The operator has not granted this host the capability for this project.
    #[error("grant '{0}' is missing for this host and project")]
    GrantMissing(String),
    /// The job is not in a state that accepts this action.
    #[error("job is not live")]
    NotLive,
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

impl From<sqlx::Error> for JobError {
    fn from(error: sqlx::Error) -> Self {
        JobError::Database(error)
    }
}

impl From<ProjectServiceError> for JobError {
    fn from(error: ProjectServiceError) -> Self {
        match error {
            ProjectServiceError::Database(e) => JobError::Database(e),
            _ => JobError::NotFound,
        }
    }
}

impl From<HostPresenceError> for JobError {
    fn from(error: HostPresenceError) -> Self {
        match error {
            HostPresenceError::NotFound => JobError::NotFound,
            HostPresenceError::DaemonNotConnected { slug } => JobError::DaemonNotConnected { slug },
            HostPresenceError::Database(e) => JobError::Database(e),
        }
    }
}

impl From<HostProtocolError> for JobError {
    fn from(error: HostProtocolError) -> Self {
        match error {
            HostProtocolError::NotFound => JobError::NotFound,
            HostProtocolError::ExecutionNotFound => JobError::NotFound,
            HostProtocolError::NotGranted => JobError::GrantMissing("host.execute".into()),
            HostProtocolError::DaemonNotConnected { slug } => JobError::DaemonNotConnected { slug },
            HostProtocolError::SocketGone => JobError::DaemonNotConnected {
                slug: String::new(),
            },
            HostProtocolError::Invalid(message) => JobError::Invalid(message),
            HostProtocolError::Database(e) => JobError::Database(e),
        }
    }
}

/// Grant a host may hold for a project so it can run tkrun jobs.
pub const TKRUN_GRANT: &str = "jobs.tkrun";

/// Body of a grant PUT. The hash pins the checkout's tkrun config.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantRequest {
    /// SHA-256 of the checkout's tkrun config, lowercase hex.
    pub config_hash: String,
}

/// One grant as the API returns it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantView {
    /// Host public id.
    pub host: String,
    /// Granted capability, e.g. `jobs.tkrun`.
    pub capability: String,
    /// Config hash the grant is pinned to.
    pub config_hash: String,
    /// When the grant was last written.
    pub granted_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct GrantRow {
    capability: String,
    config_hash: String,
    granted_at: DateTime<Utc>,
}

/// A project's TideTable binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TidetableBinding {
    /// Hub origin, for example `https://tk.example`.
    pub server: String,
    /// Slug on that hub.
    pub slug: String,
}

/// Binding request body.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingRequest {
    /// Hub URL; normalized to its origin.
    pub server: String,
    /// Slug the hub already lists.
    pub slug: String,
}

/// Typed options for one `tkrun` run. Mirrors the frame in HOST_PROTOCOL.md.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct JobOptions {
    /// Time budget in hours (0-12).
    pub hours: Option<u8>,
    /// Minutes added to the budget (0-59).
    pub minutes: Option<u8>,
    /// Task ids to run, each `X.Y.Z`; empty means run ready tasks.
    pub tasks: Vec<String>,
    /// Parallel workers (1-4).
    pub jobs: Option<u8>,
    /// Show routing and commands without running them.
    pub dry_run: bool,
}

impl JobOptions {
    /// Check every bound; the first failure names its field.
    pub fn validate(&self) -> Result<(), JobError> {
        if self.hours.is_some_and(|h| h > 12) {
            return Err(JobError::Invalid("hours must be 0-12".into()));
        }
        if self.minutes.is_some_and(|m| m > 59) {
            return Err(JobError::Invalid("minutes must be 0-59".into()));
        }
        if self.jobs.is_some_and(|j| !(1..=4).contains(&j)) {
            return Err(JobError::Invalid("jobs must be 1-4".into()));
        }
        if self.tasks.len() > 16 {
            return Err(JobError::Invalid("at most 16 tasks".into()));
        }
        if let Some(bad) = self.tasks.iter().find(|task| !is_task_id(task)) {
            return Err(JobError::Invalid(format!(
                "task id '{bad}' must look like X.Y.Z"
            )));
        }
        Ok(())
    }

    /// The JSON the host receives. An unset `hours`, `minutes` or `jobs` is
    /// `null`: the host omits that flag and TKRun applies its own default.
    pub fn to_frame(&self) -> Value {
        json!({
            "hours": self.hours,
            "minutes": self.minutes,
            "tasks": self.tasks,
            "jobs": self.jobs,
            "dryRun": self.dry_run,
        })
    }
}

/// Dispatch request body.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchRequest {
    /// Public id of a connected host.
    pub host: String,
    /// Checkout name the host maps to a local directory.
    pub checkout: String,
    /// Typed run options.
    #[serde(default)]
    pub options: JobOptions,
}

/// Stop request body.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StopRequest {
    /// `stop` finishes the current step; `kill` terminates the run.
    pub mode: String,
}

/// One job row, joined to its host public id.
#[derive(Debug, Clone, FromRow)]
pub struct JobRow {
    /// Pool-generated job id.
    pub public_id: String,
    /// Public id of the host the job runs on.
    pub host_public_id: String,
    /// Project the job belongs to.
    pub project_id: u64,
    /// Checkout name.
    pub checkout: String,
    /// Typed options as stored JSON text.
    pub options: String,
    /// Lifecycle status (`CAST` from the ENUM column).
    pub status: String,
    /// Refusal code, when refused before start.
    pub refusal_code: Option<String>,
    /// Process id reported by the host.
    pub pid: Option<i64>,
    /// TKRun owner string reported by the host.
    pub owner: Option<String>,
    /// Exit code reported at completion.
    pub exit_code: Option<i32>,
    /// Last lines reported at completion.
    pub summary: Option<String>,
    /// Output observed so far, bounded by [`MAX_OUTPUT_BYTES`].
    pub output: Option<String>,
    /// When created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When completed.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Wire projection of a job.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobView {
    /// Job id.
    pub id: String,
    /// Host public id.
    pub host: String,
    /// Checkout name.
    pub checkout: String,
    /// Lifecycle status.
    pub status: String,
    /// Refusal code when refused.
    pub refusal_code: Option<String>,
    /// Process id.
    pub pid: Option<i64>,
    /// TKRun owner string.
    pub owner: Option<String>,
    /// Exit code once completed.
    pub exit_code: Option<i32>,
    /// Final summary once completed.
    pub summary: Option<String>,
    /// Output observed so far.
    pub output: Option<String>,
    /// Typed options.
    pub options: Value,
    /// When created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When completed.
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<JobRow> for JobView {
    fn from(row: JobRow) -> Self {
        Self {
            id: row.public_id,
            host: row.host_public_id,
            checkout: row.checkout,
            status: row.status,
            refusal_code: row.refusal_code,
            pid: row.pid,
            owner: row.owner,
            exit_code: row.exit_code,
            summary: row.summary,
            output: row.output,
            options: serde_json::from_str(&row.options).unwrap_or(Value::Null),
            created_at: row.created_at,
            completed_at: row.completed_at,
        }
    }
}

/// Validate a TideTable slug: lowercase letters, digits and dashes, starting
/// with a letter or digit, at most 63 characters.
pub fn is_slug(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// Validate a checkout name: lowercase letters, digits, dashes and underscores.
pub fn is_checkout_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-' || *b == b'_')
}

/// True for a task id of the form `X.Y.Z` (digits only).
pub fn is_task_id(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Normalize a hub URL to its origin. HTTPS is required, except for loopback
/// hosts, which may use HTTP (local development and CI).
pub fn normalize_server(raw: &str) -> Result<String, JobError> {
    let url = Url::parse(raw.trim())
        .map_err(|_| JobError::Invalid("server must be an absolute URL".into()))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    match url.scheme() {
        "https" => {}
        "http" if loopback => {}
        _ => return Err(JobError::Invalid("server must use https".into())),
    }
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
        return Err(JobError::Invalid(
            "server must not carry credentials or a query".into(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| JobError::Invalid("server needs a host".into()))?;
    Ok(match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    })
}

/// Read the pool's read-only hub token, if configured.
fn read_token() -> Option<String> {
    std::env::var(READ_TOKEN_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Ask the hub whether it lists `slug`. Blocking; call from `spawn_blocking`.
fn hub_lists_slug(server: &str, token: &str, slug: &str) -> Result<bool, JobError> {
    let url = format!("{server}/api/projects");
    let response = ureq::get(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .timeout(Duration::from_secs(10))
        .call();
    let body: Value = match response {
        Ok(response) => response
            .into_json()
            .map_err(|_| JobError::HubUnavailable("hub returned a non-JSON body".into()))?,
        Err(ureq::Error::Status(401 | 403, _)) => {
            return Err(JobError::HubRefused(
                "the pool's read token was refused".into(),
            ));
        }
        Err(ureq::Error::Status(code, _)) => {
            return Err(JobError::HubUnavailable(format!("hub answered {code}")));
        }
        Err(_) => return Err(JobError::HubUnavailable("hub is unreachable".into())),
    };
    Ok(body["projects"]
        .as_array()
        .is_some_and(|projects| projects.iter().any(|p| p["slug"] == slug)))
}

/// Bind a project to a TideTable slug the hub already lists.
///
/// Fails closed: an unknown slug is refused, and an unreachable hub or a
/// missing read token keeps the previous binding unchanged.
pub async fn bind_project(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
    request: BindingRequest,
) -> Result<TidetableBinding, JobError> {
    if !is_slug(&request.slug) {
        return Err(JobError::Invalid(
            "slug must be lowercase letters, digits and dashes".into(),
        ));
    }
    let server = normalize_server(&request.server)?;
    let project = find_project(pool, organization_id, project_public_id).await?;

    let token = read_token()
        .ok_or_else(|| JobError::HubUnavailable(format!("{READ_TOKEN_ENV} is not configured")))?;
    let (hub_server, hub_slug) = (server.clone(), request.slug.clone());
    let listed =
        tokio::task::spawn_blocking(move || hub_lists_slug(&hub_server, &token, &hub_slug))
            .await
            .map_err(|_| JobError::HubUnavailable("hub check was interrupted".into()))??;
    if !listed {
        return Err(JobError::HubRefused(format!(
            "the hub does not list slug '{}'",
            request.slug
        )));
    }

    sqlx::query(
        "INSERT INTO project_tidetable_bindings (project_id, server, slug) \
         VALUES (?, ?, ?) \
         ON DUPLICATE KEY UPDATE server = VALUES(server), slug = VALUES(slug), \
                                 bound_at = CURRENT_TIMESTAMP",
    )
    .bind(project.id)
    .bind(&server)
    .bind(&request.slug)
    .execute(pool)
    .await?;

    Ok(TidetableBinding {
        server,
        slug: request.slug,
    })
}

/// The project's binding, if any.
pub async fn find_binding(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
) -> Result<Option<TidetableBinding>, JobError> {
    let project = find_project(pool, organization_id, project_public_id).await?;
    find_binding_row(pool, project.id).await
}

/// Dispatch one typed `tkrun` job to a connected host.
///
/// Order: validate, check the host is connected and negotiated `jobs.tkrun`,
/// take the project row lock, refuse a second live job for the slug, store the
/// row, then send `start_job`. An offline host stores nothing live.
pub async fn dispatch(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
    request: DispatchRequest,
) -> Result<JobView, JobError> {
    if !is_checkout_name(&request.checkout) {
        return Err(JobError::Invalid(
            "checkout must be lowercase letters, digits, dashes and underscores".into(),
        ));
    }
    request.options.validate()?;
    let project = find_project(pool, organization_id, project_public_id).await?;
    let binding = find_binding_row(pool, project.id)
        .await?
        .ok_or(JobError::NotBound)?;

    let host = host_presence::require_connected(pool, organization_id, &request.host).await?;
    if !has_grant(pool, project.id, host.id, TKRUN_GRANT).await? {
        return Err(JobError::GrantMissing(TKRUN_GRANT.into()));
    }
    if !host_protocol::has_feature(&host.public_id, JOBS_TKRUN_FEATURE) {
        return Err(JobError::FeatureMissing(JOBS_TKRUN_FEATURE.into()));
    }

    let job_id = uuid::Uuid::new_v4().to_string();
    let options = request.options.to_frame();
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT id FROM projects WHERE id = ? FOR UPDATE")
        .bind(project.id)
        .fetch_one(&mut *tx)
        .await?;
    let live: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM host_jobs WHERE project_id = ? AND status IN ('queued', 'running')",
    )
    .bind(project.id)
    .fetch_one(&mut *tx)
    .await?;
    if live > 0 {
        return Err(JobError::AlreadyRunning);
    }
    sqlx::query(
        "INSERT INTO host_jobs (public_id, host_id, project_id, checkout, options, status) \
         VALUES (?, ?, ?, ?, ?, 'queued')",
    )
    .bind(&job_id)
    .bind(host.id)
    .bind(project.id)
    .bind(&request.checkout)
    .bind(options.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let frame = host_protocol::start_job_frame(
        &job_id,
        &binding.slug,
        &binding.server,
        &request.checkout,
        &options,
    );
    if !host_protocol::send_frame(&host.public_id, frame) {
        mark_refused(pool, &job_id, "daemon_not_connected", "host socket is gone").await?;
        return Err(JobError::DaemonNotConnected { slug: host.slug });
    }

    let row = find_job_row(pool, &job_id)
        .await?
        .ok_or(JobError::NotFound)?;
    Ok(JobView::from(row))
}

/// Grant a host one capability for one project, pinned to a config hash.
/// Granting again replaces the hash and the timestamp.
pub async fn put_grant(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
    host_ref: &str,
    capability: &str,
    request: GrantRequest,
) -> Result<GrantView, JobError> {
    if capability != TKRUN_GRANT {
        return Err(JobError::Invalid(format!("unknown grant '{capability}'")));
    }
    if !is_config_hash(&request.config_hash) {
        return Err(JobError::Invalid(
            "configHash must be 64 lowercase hex characters (sha-256)".into(),
        ));
    }
    let project = find_project(pool, organization_id, project_public_id).await?;
    let (host_id, host_public_id) = find_host(pool, organization_id, host_ref).await?;
    sqlx::query(
        "INSERT INTO project_host_grants (project_id, host_id, capability, config_hash) \
         VALUES (?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE config_hash = VALUES(config_hash), granted_at = CURRENT_TIMESTAMP",
    )
    .bind(project.id)
    .bind(host_id)
    .bind(capability)
    .bind(&request.config_hash)
    .execute(pool)
    .await?;
    let row: GrantRow = sqlx::query_as(
        "SELECT capability, config_hash, granted_at FROM project_host_grants \
         WHERE project_id = ? AND host_id = ? AND capability = ?",
    )
    .bind(project.id)
    .bind(host_id)
    .bind(capability)
    .fetch_one(pool)
    .await?;
    Ok(GrantView {
        host: host_public_id,
        capability: row.capability,
        config_hash: row.config_hash,
        granted_at: row.granted_at,
    })
}

/// Revoke a grant. Dispatch needs it again before the next job.
pub async fn delete_grant(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
    host_ref: &str,
    capability: &str,
) -> Result<(), JobError> {
    if capability != TKRUN_GRANT {
        return Err(JobError::Invalid(format!("unknown grant '{capability}'")));
    }
    let project = find_project(pool, organization_id, project_public_id).await?;
    let (host_id, _) = find_host(pool, organization_id, host_ref).await?;
    let result = sqlx::query(
        "DELETE FROM project_host_grants WHERE project_id = ? AND host_id = ? AND capability = ?",
    )
    .bind(project.id)
    .bind(host_id)
    .bind(capability)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(JobError::NotFound);
    }
    Ok(())
}

async fn has_grant(
    pool: &MySqlPool,
    project_id: u64,
    host_id: u64,
    capability: &str,
) -> Result<bool, JobError> {
    let found: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM project_host_grants WHERE project_id = ? AND host_id = ? AND capability = ?",
    )
    .bind(project_id)
    .bind(host_id)
    .bind(capability)
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

/// Resolve a host by public id or slug within the caller's organization.
async fn find_host(
    pool: &MySqlPool,
    organization_id: u64,
    host_ref: &str,
) -> Result<(u64, String), JobError> {
    let row: Option<(u64, String)> = sqlx::query_as(
        "SELECT id, public_id FROM hosts \
         WHERE organization_id = ? AND (public_id = ? OR slug = ?) AND revoked_at IS NULL",
    )
    .bind(organization_id)
    .bind(host_ref)
    .bind(host_ref)
    .fetch_optional(pool)
    .await?;
    row.ok_or(JobError::NotFound)
}

/// A SHA-256 digest in lowercase hex.
fn is_config_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Ask a live job's host to stop or kill it. Returns the job as stored.
pub async fn stop(
    pool: &MySqlPool,
    organization_id: u64,
    job_public_id: &str,
    request: StopRequest,
) -> Result<JobView, JobError> {
    if request.mode != "stop" && request.mode != "kill" {
        return Err(JobError::Invalid("mode must be stop or kill".into()));
    }
    let row = find_job_scoped(pool, organization_id, job_public_id)
        .await?
        .ok_or(JobError::NotFound)?;
    if !LIVE_STATUSES.contains(&row.status.as_str()) {
        return Err(JobError::NotLive);
    }
    let frame = host_protocol::stop_job_frame(&row.public_id, &request.mode);
    if !host_protocol::send_frame(&row.host_public_id, frame) {
        return Err(JobError::DaemonNotConnected {
            slug: row.host_public_id.clone(),
        });
    }
    Ok(JobView::from(row))
}

/// Fetch one job in the organization.
pub async fn get_job(
    pool: &MySqlPool,
    organization_id: u64,
    job_public_id: &str,
) -> Result<JobView, JobError> {
    find_job_scoped(pool, organization_id, job_public_id)
        .await?
        .map(JobView::from)
        .ok_or(JobError::NotFound)
}

/// Record `job_accepted`: the host started the process.
pub async fn record_accepted(
    pool: &MySqlPool,
    job_id: &str,
    pid: Option<i64>,
    owner: Option<&str>,
) -> Result<(), JobError> {
    sqlx::query(
        "UPDATE host_jobs SET status = 'running', pid = ?, owner = ? \
         WHERE public_id = ? AND status = 'queued'",
    )
    .bind(pid)
    .bind(owner)
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record `job_refused`: nothing ran.
pub async fn record_refused(
    pool: &MySqlPool,
    job_id: &str,
    code: &str,
    detail: &str,
) -> Result<(), JobError> {
    mark_refused(pool, job_id, code, detail).await
}

async fn mark_refused(
    pool: &MySqlPool,
    job_id: &str,
    code: &str,
    detail: &str,
) -> Result<(), JobError> {
    sqlx::query(
        "UPDATE host_jobs SET status = 'refused', refusal_code = ?, refusal_detail = ?, \
                completed_at = CURRENT_TIMESTAMP \
         WHERE public_id = ? AND status = 'queued'",
    )
    .bind(code)
    .bind(truncate(detail, 500))
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Append one output line. A `seq` at or below the stored one is a replay and
/// is dropped, so reconnects never duplicate lines. Returns true when stored.
pub async fn record_output(
    pool: &MySqlPool,
    job_id: &str,
    seq: u64,
    text: &str,
) -> Result<bool, JobError> {
    let line = format!("{text}\n");
    let result = sqlx::query(
        "UPDATE host_jobs \
         SET output = RIGHT(CONCAT(COALESCE(output, ''), ?), ?), last_seq = ? \
         WHERE public_id = ? AND last_seq < ? AND status IN ('queued', 'running')",
    )
    .bind(&line)
    .bind(MAX_OUTPUT_BYTES as u64)
    .bind(seq)
    .bind(job_id)
    .bind(seq)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

/// Record `job_completed`. Terminal and idempotent: a repeat is a no-op.
pub async fn record_completed(
    pool: &MySqlPool,
    job_id: &str,
    status: &str,
    exit_code: Option<i32>,
    summary: Option<&str>,
) -> Result<(), JobError> {
    let status = match status {
        "succeeded" | "stopped" | "killed" => status,
        _ => "failed",
    };
    sqlx::query(
        "UPDATE host_jobs SET status = ?, exit_code = ?, summary = ?, \
                completed_at = CURRENT_TIMESTAMP \
         WHERE public_id = ? AND status IN ('queued', 'running')",
    )
    .bind(status)
    .bind(exit_code)
    .bind(summary.map(|s| truncate(s, 8000)))
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Re-adopt jobs from a `jobs_snapshot` after a reconnect.
///
/// Only rows belonging to `host_public_id` are touched. A job the host reports
/// running stays live with its pid; one it reports completed is recorded as
/// completed (exit 0 is `succeeded`, anything else `failed`).
pub async fn record_snapshot(
    pool: &MySqlPool,
    host_public_id: &str,
    jobs: &[Value],
) -> Result<(), JobError> {
    for job in jobs {
        let Some(job_id) = job["jobId"].as_str() else {
            continue;
        };
        match job["state"].as_str() {
            Some("running") => {
                sqlx::query(
                    "UPDATE host_jobs SET pid = ? \
                     WHERE public_id = ? AND status = 'running' \
                       AND host_id = (SELECT id FROM hosts WHERE public_id = ?)",
                )
                .bind(job["pid"].as_i64())
                .bind(job_id)
                .bind(host_public_id)
                .execute(pool)
                .await?;
            }
            Some("completed") => {
                let exit_code = job["exitCode"].as_i64().and_then(|c| i32::try_from(c).ok());
                let status = if exit_code == Some(0) {
                    "succeeded"
                } else {
                    "failed"
                };
                sqlx::query(
                    "UPDATE host_jobs SET status = ?, exit_code = ?, \
                            completed_at = CURRENT_TIMESTAMP \
                     WHERE public_id = ? AND status IN ('queued', 'running') \
                       AND host_id = (SELECT id FROM hosts WHERE public_id = ?)",
                )
                .bind(status)
                .bind(exit_code)
                .bind(job_id)
                .bind(host_public_id)
                .execute(pool)
                .await?;
            }
            _ => {}
        }
    }
    Ok(())
}

async fn find_project(
    pool: &MySqlPool,
    organization_id: u64,
    public_id: &str,
) -> Result<Project, JobError> {
    project_svc::find_project(pool, organization_id, public_id)
        .await?
        .ok_or(JobError::NotFound)
}

async fn find_binding_row(
    pool: &MySqlPool,
    project_id: u64,
) -> Result<Option<TidetableBinding>, JobError> {
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT server, slug FROM project_tidetable_bindings WHERE project_id = ?")
            .bind(project_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(server, slug)| TidetableBinding { server, slug }))
}

const JOB_SELECT: &str = "SELECT j.public_id, h.public_id AS host_public_id, j.project_id, \
        j.checkout, CAST(j.options AS CHAR) AS options, CAST(j.status AS CHAR) AS status, \
        j.refusal_code, j.pid, j.owner, j.exit_code, j.summary, j.output, \
        j.created_at, j.completed_at \
     FROM host_jobs j JOIN hosts h ON h.id = j.host_id";

async fn find_job_row(pool: &MySqlPool, job_id: &str) -> Result<Option<JobRow>, JobError> {
    let sql = format!("{JOB_SELECT} WHERE j.public_id = ?");
    Ok(sqlx::query_as::<_, JobRow>(&sql)
        .bind(job_id)
        .fetch_optional(pool)
        .await?)
}

async fn find_job_scoped(
    pool: &MySqlPool,
    organization_id: u64,
    job_id: &str,
) -> Result<Option<JobRow>, JobError> {
    let sql = format!(
        "{JOB_SELECT} JOIN projects p ON p.id = j.project_id \
         WHERE j.public_id = ? AND p.organization_id = ?"
    );
    Ok(sqlx::query_as::<_, JobRow>(&sql)
        .bind(job_id)
        .bind(organization_id)
        .fetch_optional(pool)
        .await?)
}

/// Keep the last `max_bytes` of `value`, cut on a character boundary.
fn truncate(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut start = value.len() - max_bytes;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    value[start..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_hash_is_sha256_lowercase_hex() {
        assert!(is_config_hash(&"0123456789abcdef".repeat(4)));
        assert!(!is_config_hash(&"0123456789abcdef".repeat(3)));
        assert!(!is_config_hash(&"0123456789ABCDEF".repeat(4)));
        assert!(!is_config_hash(&"g".repeat(64)));
    }

    #[test]
    fn slugs_follow_the_tidetable_rule() {
        assert!(is_slug("chiton"));
        assert!(is_slug("chiton-fixture-2"));
        assert!(!is_slug(""));
        assert!(!is_slug("Chiton"));
        assert!(!is_slug("-chiton"));
        assert!(!is_slug("chi/ton"));
        assert!(!is_slug(&"a".repeat(64)));
    }

    #[test]
    fn checkout_and_task_ids_are_bounded() {
        assert!(is_checkout_name("chiton-main"));
        assert!(!is_checkout_name("../etc"));
        assert!(is_task_id("1.3.2"));
        assert!(!is_task_id("1.3"));
        assert!(!is_task_id("1.3.x"));
        assert!(!is_task_id("1.3.2 --all"));
    }

    #[test]
    fn options_refuse_out_of_range_values() {
        assert!(JobOptions::default().validate().is_ok());
        let hours = JobOptions {
            hours: Some(13),
            ..Default::default()
        };
        assert!(hours.validate().is_err());
        let jobs = JobOptions {
            jobs: Some(0),
            ..Default::default()
        };
        assert!(jobs.validate().is_err());
        let task = JobOptions {
            tasks: vec!["1.3.2; rm".into()],
            ..Default::default()
        };
        assert!(task.validate().is_err());
    }

    #[test]
    fn options_reject_unknown_fields() {
        let parsed: Result<JobOptions, _> = serde_json::from_str(r#"{"hours":1,"argv":["x"]}"#);
        assert!(parsed.is_err());
    }

    #[test]
    fn server_must_be_https_or_loopback_origin() {
        assert_eq!(
            normalize_server("https://tk.example/").unwrap(),
            "https://tk.example"
        );
        assert_eq!(
            normalize_server("http://127.0.0.1:8471/api").unwrap(),
            "http://127.0.0.1:8471"
        );
        assert!(normalize_server("http://tk.example").is_err());
        assert!(normalize_server("https://u:p@tk.example").is_err());
        assert!(normalize_server("https://tk.example?token=x").is_err());
        assert!(normalize_server("not a url").is_err());
    }

    #[test]
    fn truncate_keeps_the_tail_on_a_char_boundary() {
        assert_eq!(truncate("abcdef", 3), "def");
        assert_eq!(truncate("ab", 3), "ab");
        assert_eq!(truncate("aé", 2), "é");
    }

    #[test]
    fn dispatch_frame_fills_defaults() {
        let frame = JobOptions::default().to_frame();
        assert!(frame["hours"].is_null());
        assert!(frame["jobs"].is_null());
        assert_eq!(frame["dryRun"], false);
    }
}
