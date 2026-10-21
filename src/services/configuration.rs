//! Configuration service (task 1.4.1).
//!
//! A project's configuration is authored as a bundle of files. One of them is
//! the manifest ([`MANIFEST_FILE`]); the rest are referenced by it (prompt
//! partials). Installing a bundle stores a new immutable [`ConfigurationRevision`]
//! and either activates it or, when validation fails, records the failure and
//! leaves the previous active revision in place.
//!
//! Validation resolves every reference in the bundle:
//!
//! - **connections** — a slug must resolve to an active pool connection;
//! - **hosts** — a slug must resolve to a live (non-revoked) host;
//! - **environments** — complete named objects: a host that must resolve, an
//!   absolute working directory and an optional worktree whose branch template
//!   embeds the execution id (see [`crate::services::environments`]);
//! - **named agents** — named and well-formed (`provider`, `model`);
//! - **partials** — named, and each references a file present in the bundle.
//!
//! Every unresolvable reference produces a [`FieldIssue`] that names both the
//! field and the revision (its version number). The authored files are stored
//! verbatim; the manifest is parsed only to drive resolution.

use crate::entities::configuration_revision::{
    ConfigFile, ConfigurationRevision, ConfigurationRevisionFile, FieldIssue, RevisionMetaView,
    RevisionStatus, RevisionView,
};
use crate::entities::connection::ConnectionStatus;
use crate::entities::environment::Environment;
use crate::entities::project::Project;
use crate::entities::workflow::Workflow;
use crate::services::api_keys::{self as audit_svc, AuditActor, AuditEntry};
use crate::services::connections::{self as connection_svc, ConnectionError};
use crate::services::environments as environment_svc;
use crate::services::hosts::{self as host_svc, HostServiceError, normalize_slug};
use crate::services::workflows as workflow_svc;
use serde::Deserialize;
use sqlx::MySqlPool;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Path of the manifest file that declares the bundle's references.
pub const MANIFEST_FILE: &str = "tidepool.json";

/// Maximum number of files a bundle may declare.
pub const MAX_FILES: usize = 100;
/// Maximum length of an authored file path.
pub const MAX_PATH_CHARS: usize = 512;
/// Maximum number of characters in a single authored file.
pub const MAX_FILE_CHARS: usize = 1_000_000;
/// Maximum total bytes of prompt partial files in one bundle.
pub const MAX_PARTIAL_BYTES: usize = 5_000_000;

const PROJECT_FIELDS: &str = "id, public_id, organization_id, slug, name, created_at, updated_at";
const REVISION_FIELDS: &str =
    "id, public_id, project_id, version, status, active, validation_errors, created_at";

/// Domain errors for configuration operations.
#[derive(Debug, Error)]
pub enum ConfigurationError {
    /// Malformed bundle (missing manifest, invalid JSON, caps exceeded).
    #[error("invalid configuration: {0}")]
    Invalid(String),
    /// No project with that id in this organization.
    #[error("project not found")]
    ProjectNotFound,
    /// No revision with that id in this project.
    #[error("revision not found")]
    RevisionNotFound,
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// Result of a dry-run [`validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidateResult {
    /// True when every reference resolved.
    pub valid: bool,
    /// The version the revision would get if installed.
    pub version: u32,
    /// Field issues, empty when valid.
    pub errors: Vec<FieldIssue>,
}

/// Result of [`install`]: the stored revision plus whether it activated.
#[derive(Debug, Clone)]
pub struct InstallResult {
    /// The stored, immutable revision.
    pub revision: RevisionView,
    /// True when the revision became active.
    pub activated: bool,
}

// ── manifest parsing and pure reference resolution ─────────────────────────

/// Declared references in the manifest file.
#[derive(Debug, Clone, Deserialize)]
struct Manifest {
    /// Connection slugs to resolve.
    #[serde(default)]
    connections: Vec<String>,
    /// Host slugs to resolve.
    #[serde(default)]
    hosts: Vec<String>,
    /// Named environments: complete objects with host, directory and worktree.
    #[serde(default)]
    environments: Vec<Environment>,
    /// Named agents.
    #[serde(default)]
    agents: Vec<ManifestAgent>,
    /// Named prompt partials, each referencing a bundle file.
    #[serde(default)]
    partials: Vec<ManifestPartial>,
    /// Workflows declared inline, in authored order.
    #[serde(default)]
    workflows: Vec<Workflow>,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestAgent {
    /// Agent name.
    name: String,
    /// Provider id.
    provider: String,
    /// Model id.
    model: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestPartial {
    /// Partial name.
    name: String,
    /// Path of the file holding the partial's text.
    path: String,
}

/// Why a bundle path is unsafe or not canonical.
fn bundle_path_problem(path: &str) -> Option<&'static str> {
    if path.is_empty() {
        return Some("must not be empty");
    }
    if path != path.trim() {
        return Some("must not have leading or trailing whitespace");
    }
    if path.chars().count() > MAX_PATH_CHARS {
        return Some("is too long");
    }
    if path.chars().any(char::is_control) {
        return Some("must not contain control characters");
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Some("must be relative to the bundle root");
    }
    if path.starts_with('~') {
        return Some("must not use a home-directory shorthand");
    }
    if path.contains('\\') {
        return Some("must use '/' separators");
    }
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Some("must not carry a drive prefix");
    }
    if path.ends_with('/') {
        return Some("must not end with a separator");
    }
    for component in path.split('/') {
        if component.is_empty() {
            return Some("must not contain a repeated separator");
        }
        if component == "." {
            return Some("must not contain a '.' component");
        }
        if component == ".." {
            return Some("must not contain a '..' component");
        }
    }
    None
}

/// Validate the bundle's structure and return its exact file paths.
///
/// Every cap names the offending path: the 101st file, an over-long or
/// non-canonical path, and an over-long file content.
fn validate_bundle(files: &[ConfigFile]) -> Result<BTreeSet<String>, ConfigurationError> {
    if files.is_empty() {
        return Err(ConfigurationError::Invalid(
            "configuration bundle must contain at least one file".into(),
        ));
    }
    if files.len() > MAX_FILES {
        let offending = &files[MAX_FILES];
        return Err(ConfigurationError::Invalid(format!(
            "file '{}' exceeds the {MAX_FILES}-file bundle cap",
            offending.path
        )));
    }

    let mut paths = BTreeSet::new();
    for file in files {
        if let Some(problem) = bundle_path_problem(&file.path) {
            return Err(ConfigurationError::Invalid(format!(
                "file path '{}' {problem} (1-{MAX_PATH_CHARS} characters)",
                file.path
            )));
        }
        if file.content.chars().count() > MAX_FILE_CHARS {
            return Err(ConfigurationError::Invalid(format!(
                "file '{}' exceeds {MAX_FILE_CHARS} characters",
                file.path
            )));
        }
        if !paths.insert(file.path.clone()) {
            return Err(ConfigurationError::Invalid(format!(
                "duplicate file path '{}'",
                file.path
            )));
        }
    }
    Ok(paths)
}

/// Enforce bundle-level rules that need the parsed manifest: at most
/// [`MAX_PARTIAL_BYTES`] bytes of referenced prompt partials, and no extra
/// files beyond the manifest and the files it references.
fn bundle_reference_issues(
    files: &[ConfigFile],
    manifest: &Manifest,
    revision: u32,
) -> Vec<FieldIssue> {
    let mut issues = Vec::new();
    let referenced: BTreeSet<&str> = manifest
        .partials
        .iter()
        .map(|partial| partial.path.as_str())
        .collect();

    for file in files {
        if file.path == MANIFEST_FILE || referenced.contains(file.path.as_str()) {
            continue;
        }
        issues.push(FieldIssue {
            field: format!("files.{}", file.path),
            revision,
            message: format!(
                "extra file '{}' is not referenced by the manifest",
                file.path
            ),
        });
    }

    let mut partial_bytes = 0usize;
    for file in files {
        if !referenced.contains(file.path.as_str()) {
            continue;
        }
        partial_bytes += file.content.len();
        if partial_bytes > MAX_PARTIAL_BYTES {
            issues.push(FieldIssue {
                field: format!("partials.{}", file.path),
                revision,
                message: format!(
                    "prompt partials exceed {MAX_PARTIAL_BYTES} bytes at '{}'",
                    file.path
                ),
            });
            break;
        }
    }

    issues
}

/// Find and parse the manifest file.
fn parse_manifest(files: &[ConfigFile]) -> Result<Manifest, ConfigurationError> {
    let manifest_file = files
        .iter()
        .find(|file| file.path == MANIFEST_FILE)
        .ok_or_else(|| {
            ConfigurationError::Invalid(format!(
                "configuration bundle is missing '{MANIFEST_FILE}'"
            ))
        })?;
    serde_json::from_str::<Manifest>(&manifest_file.content).map_err(|error| {
        ConfigurationError::Invalid(format!("{MANIFEST_FILE} is not valid JSON: {error}"))
    })
}

/// Parse the environment definitions authored in a revision bundle.
///
/// Dispatch (task 1.4.6) reads the active revision's environments through this
/// function and resolves them with [`crate::services::environments`]. The
/// documents are returned as authored, never normalized or merged.
pub fn parse_environments(files: &[ConfigFile]) -> Result<Vec<Environment>, ConfigurationError> {
    Ok(parse_manifest(files)?.environments)
}

/// Parse the workflows authored in a revision bundle.
///
/// Dispatch (task 1.4.6) reads the active revision's workflows through this
/// function. Steps are returned in authored order; this function performs no
/// resolution. The same rules are enforced at activation by
/// [`workflow_svc::workflow_issues`] through [`resolve_references`].
pub fn parse_workflows(files: &[ConfigFile]) -> Result<Vec<Workflow>, ConfigurationError> {
    Ok(parse_manifest(files)?.workflows)
}

/// Map each declared partial name to its authored text.
fn partial_texts(files: &[ConfigFile], manifest: &Manifest) -> BTreeMap<String, String> {
    let mut texts = BTreeMap::new();
    for partial in &manifest.partials {
        let name = partial.name.trim();
        if name.is_empty() {
            continue;
        }
        if let Some(file) = files.iter().find(|file| file.path == partial.path) {
            texts.insert(name.to_string(), file.content.clone());
        }
    }
    texts
}

/// Collect the connection and host slugs declared in the manifest.
fn manifest_slugs(manifest: &Manifest) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut connections = BTreeSet::new();
    let mut hosts = BTreeSet::new();
    connections.extend(manifest.connections.iter().cloned());
    hosts.extend(manifest.hosts.iter().cloned());
    for environment in &manifest.environments {
        hosts.insert(environment.host.clone());
    }
    (connections, hosts)
}

/// Resolve every declared reference against the pre-computed resolution sets
/// and the bundle's own files. Pure and unit-testable: no database access.
#[cfg(test)]
fn resolve_references(
    manifest: &Manifest,
    file_paths: &BTreeSet<String>,
    resolved_connections: &BTreeSet<String>,
    resolved_hosts: &BTreeSet<String>,
    revision: u32,
) -> Vec<FieldIssue> {
    resolve_references_with_partial_texts(
        manifest,
        file_paths,
        resolved_connections,
        resolved_hosts,
        revision,
        &BTreeMap::new(),
    )
}

/// [`resolve_references`], also scanning partial text for step-output
/// expressions that must name an earlier step.
fn resolve_references_with_partial_texts(
    manifest: &Manifest,
    file_paths: &BTreeSet<String>,
    resolved_connections: &BTreeSet<String>,
    resolved_hosts: &BTreeSet<String>,
    revision: u32,
    partial_texts: &BTreeMap<String, String>,
) -> Vec<FieldIssue> {
    let mut issues = Vec::new();

    for slug in &manifest.connections {
        let key = normalize_slug(slug);
        if key.is_empty() {
            issues.push(FieldIssue {
                field: "connections".into(),
                revision,
                message: "connection slug is required".into(),
            });
        } else if !resolved_connections.contains(&key) {
            issues.push(FieldIssue {
                field: format!("connections.{slug}"),
                revision,
                message: format!("no active connection with slug '{slug}'"),
            });
        }
    }

    for slug in &manifest.hosts {
        let key = normalize_slug(slug);
        if key.is_empty() {
            issues.push(FieldIssue {
                field: "hosts".into(),
                revision,
                message: "host slug is required".into(),
            });
        } else if !resolved_hosts.contains(&key) {
            issues.push(FieldIssue {
                field: format!("hosts.{slug}"),
                revision,
                message: format!("unknown host slug '{slug}'"),
            });
        }
    }

    // Environments are complete objects: the environment service owns every
    // rule (name, host resolution, directory shape, worktree template).
    issues.extend(
        environment_svc::environment_issues(&manifest.environments, resolved_hosts)
            .into_iter()
            .map(|issue| FieldIssue {
                field: issue.field,
                revision,
                message: issue.message,
            }),
    );

    let mut seen_agents = BTreeSet::new();
    for agent in &manifest.agents {
        let name = agent.name.trim().to_string();
        if name.is_empty() {
            issues.push(FieldIssue {
                field: "agents.name".into(),
                revision,
                message: "agent name is required".into(),
            });
        } else if !seen_agents.insert(name.clone()) {
            issues.push(FieldIssue {
                field: format!("agents.{name}.name"),
                revision,
                message: format!("duplicate agent name '{name}'"),
            });
        }
        if agent.provider.trim().is_empty() {
            issues.push(FieldIssue {
                field: format!("agents.{name}.provider"),
                revision,
                message: "provider is required".into(),
            });
        }
        if agent.model.trim().is_empty() {
            issues.push(FieldIssue {
                field: format!("agents.{name}.model"),
                revision,
                message: "model is required".into(),
            });
        }
    }

    let mut seen_partials = BTreeSet::new();
    for partial in &manifest.partials {
        let name = partial.name.trim().to_string();
        if name.is_empty() {
            issues.push(FieldIssue {
                field: "partials.name".into(),
                revision,
                message: "partial name is required".into(),
            });
        } else if !seen_partials.insert(name.clone()) {
            issues.push(FieldIssue {
                field: format!("partials.{name}.name"),
                revision,
                message: format!("duplicate partial name '{name}'"),
            });
        }
        let path = partial.path.trim();
        if path.is_empty() {
            issues.push(FieldIssue {
                field: format!("partials.{name}.path"),
                revision,
                message: "path is required".into(),
            });
        } else if !file_paths.contains(partial.path.as_str()) {
            issues.push(FieldIssue {
                field: format!("partials.{name}.path"),
                revision,
                message: format!("path '{path}' is not in the bundle"),
            });
        }
    }

    // Workflows are validated against the same revision: complete
    // environments, named agents and prompt partials.
    let agent_names: BTreeSet<String> = manifest
        .agents
        .iter()
        .map(|agent| agent.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();
    let partial_names: BTreeSet<String> = manifest
        .partials
        .iter()
        .map(|partial| partial.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();
    issues.extend(
        workflow_svc::workflow_issues_with_partial_texts(
            &manifest.workflows,
            &manifest.environments,
            &agent_names,
            &partial_names,
            partial_texts,
        )
        .into_iter()
        .map(|issue| FieldIssue {
            field: issue.field,
            revision,
            message: issue.message,
        }),
    );

    issues
}

// ── database-backed resolution ─────────────────────────────────────────────

/// Resolve connection slugs to the set of active ones.
async fn resolve_connection_slugs(
    pool: &MySqlPool,
    organization_id: u64,
    slugs: &BTreeSet<String>,
) -> Result<BTreeSet<String>, ConfigurationError> {
    let mut resolved = BTreeSet::new();
    for slug in slugs {
        let key = normalize_slug(slug);
        if key.is_empty() {
            continue;
        }
        match connection_svc::find_effective_by_slug(pool, organization_id, &key).await {
            Ok(Some(connection)) if connection.status == ConnectionStatus::Active => {
                resolved.insert(normalize_slug(&connection.slug));
            }
            Ok(_) => {}
            Err(ConnectionError::Database(error)) => {
                return Err(ConfigurationError::Database(error));
            }
            Err(_) => {}
        }
    }
    Ok(resolved)
}

/// Resolve host slugs to the set of live ones.
async fn resolve_host_slugs(
    pool: &MySqlPool,
    organization_id: u64,
    slugs: &BTreeSet<String>,
) -> Result<BTreeSet<String>, ConfigurationError> {
    let mut resolved = BTreeSet::new();
    for slug in slugs {
        let key = normalize_slug(slug);
        if key.is_empty() {
            continue;
        }
        match host_svc::resolve_for_activation(pool, organization_id, &key).await {
            Ok(host) => {
                resolved.insert(normalize_slug(&host.slug));
            }
            Err(HostServiceError::Database(error)) => {
                return Err(ConfigurationError::Database(error));
            }
            Err(_) => {}
        }
    }
    Ok(resolved)
}

/// Load a project by public id, scoped to the organization.
async fn require_project(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
) -> Result<Project, ConfigurationError> {
    sqlx::query_as::<_, Project>(&format!(
        "SELECT {PROJECT_FIELDS} FROM projects WHERE public_id = ? AND organization_id = ?"
    ))
    .bind(project_public_id)
    .bind(organization_id)
    .fetch_optional(pool)
    .await
    .map_err(ConfigurationError::Database)?
    .ok_or(ConfigurationError::ProjectNotFound)
}

/// The next version number for a project.
async fn next_version(pool: &MySqlPool, project_id: u64) -> Result<u32, ConfigurationError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM configuration_revisions WHERE project_id = ?")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .map_err(ConfigurationError::Database)?;
    Ok(count as u32 + 1)
}

/// Load one revision with its files.
async fn fetch_revision_view(
    pool: &MySqlPool,
    project_id: u64,
    revision_public_id: &str,
) -> Result<Option<RevisionView>, ConfigurationError> {
    let revision = sqlx::query_as::<_, ConfigurationRevision>(&format!(
        "SELECT {REVISION_FIELDS} FROM configuration_revisions \
         WHERE public_id = ? AND project_id = ?"
    ))
    .bind(revision_public_id)
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(ConfigurationError::Database)?;

    let Some(revision) = revision else {
        return Ok(None);
    };

    let files = sqlx::query_as::<_, ConfigurationRevisionFile>(
        "SELECT id, revision_id, file_order, path, content \
         FROM configuration_revision_files WHERE revision_id = ? ORDER BY file_order",
    )
    .bind(revision.id)
    .fetch_all(pool)
    .await
    .map_err(ConfigurationError::Database)?;

    Ok(Some(RevisionView {
        id: revision.public_id.clone(),
        version: revision.version,
        active: revision.active,
        status: revision.status,
        errors: revision.errors(),
        files: files
            .iter()
            .map(crate::entities::configuration_revision::RevisionFileView::from)
            .collect(),
        created_at: revision.created_at,
    }))
}

// ── public operations ──────────────────────────────────────────────────────

/// Validate a bundle without writing anything.
///
/// Resolves every reference and returns the named field issues plus the
/// version a subsequent install would use.
pub async fn validate(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
    files: &[ConfigFile],
) -> Result<ValidateResult, ConfigurationError> {
    let file_paths = validate_bundle(files)?;
    let manifest = parse_manifest(files)?;
    let project = require_project(pool, organization_id, project_public_id).await?;
    let version = next_version(pool, project.id).await?;
    let (connection_slugs, host_slugs) = manifest_slugs(&manifest);
    let resolved_connections =
        resolve_connection_slugs(pool, organization_id, &connection_slugs).await?;
    let resolved_hosts = resolve_host_slugs(pool, organization_id, &host_slugs).await?;
    let mut errors = resolve_references_with_partial_texts(
        &manifest,
        &file_paths,
        &resolved_connections,
        &resolved_hosts,
        version,
        &partial_texts(files, &manifest),
    );
    errors.extend(bundle_reference_issues(files, &manifest, version));
    Ok(ValidateResult {
        valid: errors.is_empty(),
        version,
        errors,
    })
}

/// Install a bundle as a new immutable revision.
///
/// The revision is always stored (the exact authored files, verbatim). When
/// every reference resolves it becomes the active revision and the previous
/// active revision is deactivated; otherwise it is stored as `invalid`,
/// `active=false`, the previous active revision stays active, and the named
/// field issues are returned.
pub async fn install(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
    files: &[ConfigFile],
    actor: AuditActor,
) -> Result<InstallResult, ConfigurationError> {
    let file_paths = validate_bundle(files)?;
    let manifest = parse_manifest(files)?;
    let (connection_slugs, host_slugs) = manifest_slugs(&manifest);
    let resolved_connections =
        resolve_connection_slugs(pool, organization_id, &connection_slugs).await?;
    let resolved_hosts = resolve_host_slugs(pool, organization_id, &host_slugs).await?;

    let mut tx = pool.begin().await.map_err(ConfigurationError::Database)?;
    let project = sqlx::query_as::<_, Project>(&format!(
        "SELECT {PROJECT_FIELDS} FROM projects \
         WHERE public_id = ? AND organization_id = ? FOR UPDATE"
    ))
    .bind(project_public_id)
    .bind(organization_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(ConfigurationError::Database)?
    .ok_or(ConfigurationError::ProjectNotFound)?;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM configuration_revisions WHERE project_id = ?")
            .bind(project.id)
            .fetch_one(&mut *tx)
            .await
            .map_err(ConfigurationError::Database)?;
    let version = count as u32 + 1;

    let mut errors = resolve_references_with_partial_texts(
        &manifest,
        &file_paths,
        &resolved_connections,
        &resolved_hosts,
        version,
        &partial_texts(files, &manifest),
    );
    errors.extend(bundle_reference_issues(files, &manifest, version));
    let valid = errors.is_empty();
    let status = if valid {
        RevisionStatus::Valid
    } else {
        RevisionStatus::Invalid
    };
    let public_id = uuid::Uuid::new_v4().to_string();
    let validation_errors =
        if valid {
            None
        } else {
            Some(serde_json::to_string(&errors).map_err(|error| {
                ConfigurationError::Invalid(format!("validation errors: {error}"))
            })?)
        };

    let result = sqlx::query(
        "INSERT INTO configuration_revisions \
         (public_id, project_id, version, status, active, validation_errors) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&public_id)
    .bind(project.id)
    .bind(version)
    .bind(status.as_str())
    .bind(valid)
    .bind(validation_errors.as_deref())
    .execute(&mut *tx)
    .await
    .map_err(ConfigurationError::Database)?;
    let revision_id = result.last_insert_id();

    for (index, file) in files.iter().enumerate() {
        sqlx::query(
            "INSERT INTO configuration_revision_files \
             (revision_id, file_order, path, content) VALUES (?, ?, ?, ?)",
        )
        .bind(revision_id)
        .bind((index + 1) as u32)
        .bind(&file.path)
        .bind(&file.content)
        .execute(&mut *tx)
        .await
        .map_err(ConfigurationError::Database)?;
    }

    if valid {
        sqlx::query(
            "UPDATE configuration_revisions SET active = 0 \
             WHERE project_id = ? AND active = 1 AND id <> ?",
        )
        .bind(project.id)
        .bind(revision_id)
        .execute(&mut *tx)
        .await
        .map_err(ConfigurationError::Database)?;
    }

    tx.commit().await.map_err(ConfigurationError::Database)?;

    let revision = fetch_revision_view(pool, project.id, &public_id)
        .await?
        .ok_or(ConfigurationError::RevisionNotFound)?;

    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id,
            actor,
            action: "configuration.install",
            target_kind: Some("project"),
            target_public_id: Some(&project.public_id),
            detail: serde_json::json!({
                "revision": revision.version,
                "status": status.as_str(),
                "active": valid,
                "error_count": errors.len(),
            }),
        },
    )
    .await;

    Ok(InstallResult {
        revision,
        activated: valid,
    })
}

/// Fetch the active revision, if any.
pub async fn get_active_revision(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
) -> Result<Option<RevisionView>, ConfigurationError> {
    let project = require_project(pool, organization_id, project_public_id).await?;
    let revision = sqlx::query_as::<_, ConfigurationRevision>(&format!(
        "SELECT {REVISION_FIELDS} FROM configuration_revisions \
         WHERE project_id = ? AND active = 1 ORDER BY version DESC LIMIT 1"
    ))
    .bind(project.id)
    .fetch_optional(pool)
    .await
    .map_err(ConfigurationError::Database)?;

    let Some(revision) = revision else {
        return Ok(None);
    };
    fetch_revision_view(pool, project.id, &revision.public_id).await
}

/// List a project's revisions, newest first, metadata only.
pub async fn list_revisions(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
) -> Result<Vec<RevisionMetaView>, ConfigurationError> {
    let project = require_project(pool, organization_id, project_public_id).await?;
    let revisions = sqlx::query_as::<_, ConfigurationRevision>(&format!(
        "SELECT {REVISION_FIELDS} FROM configuration_revisions \
         WHERE project_id = ? ORDER BY version DESC"
    ))
    .bind(project.id)
    .fetch_all(pool)
    .await
    .map_err(ConfigurationError::Database)?;

    Ok(revisions
        .iter()
        .map(|revision| RevisionMetaView {
            id: revision.public_id.clone(),
            version: revision.version,
            active: revision.active,
            status: revision.status,
            error_count: revision.errors().len(),
            created_at: revision.created_at,
        })
        .collect())
}

/// Fetch one revision with its files.
pub async fn get_revision(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
    revision_public_id: &str,
) -> Result<Option<RevisionView>, ConfigurationError> {
    let project = require_project(pool, organization_id, project_public_id).await?;
    fetch_revision_view(pool, project.id, revision_public_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_json(body: &str) -> String {
        body.to_string()
    }

    fn bundle(files: &[(&str, &str)]) -> Vec<ConfigFile> {
        files
            .iter()
            .map(|(path, content)| ConfigFile {
                path: (*path).to_string(),
                content: (*content).to_string(),
            })
            .collect()
    }

    fn manifest() -> Manifest {
        Manifest {
            connections: vec!["github-main".into()],
            hosts: vec!["studio-mac".into()],
            environments: vec![Environment {
                name: "default".into(),
                host: "studio-mac".into(),
                directory: "/Users/you/code/repo".into(),
                worktree: None,
            }],
            agents: vec![ManifestAgent {
                name: "coder".into(),
                provider: "anthropic".into(),
                model: "claude-sonnet-4-6".into(),
            }],
            partials: vec![ManifestPartial {
                name: "system".into(),
                path: "partials/system.txt".into(),
            }],
            workflows: Vec::new(),
        }
    }

    fn resolved() -> (BTreeSet<String>, BTreeSet<String>) {
        (
            ["github-main".to_string()].into_iter().collect(),
            ["studio-mac".to_string()].into_iter().collect(),
        )
    }

    fn file_paths() -> BTreeSet<String> {
        ["tidepool.json", "partials/system.txt"]
            .into_iter()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn a_complete_bundle_resolves_cleanly() {
        let (connections, hosts) = resolved();
        let issues = resolve_references(&manifest(), &file_paths(), &connections, &hosts, 1);
        assert!(issues.is_empty(), "expected no issues, got {issues:?}");
    }

    #[test]
    fn unresolvable_connection_names_field_and_revision() {
        let (connections, hosts) = resolved();
        let mut manifest = manifest();
        manifest.connections.push("missing-github".into());
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 4);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "connections.missing-github");
        assert_eq!(issues[0].revision, 4);
    }

    #[test]
    fn unresolvable_host_names_field_and_revision() {
        let (connections, hosts) = resolved();
        let mut manifest = manifest();
        manifest.hosts.push("gone-host".into());
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 2);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "hosts.gone-host");
        assert_eq!(issues[0].revision, 2);
    }

    #[test]
    fn unresolvable_environment_host_is_named() {
        let (connections, hosts) = resolved();
        let mut manifest = manifest();
        manifest.environments.push(Environment {
            name: "staging".into(),
            host: "no-such-host".into(),
            directory: "/srv/staging".into(),
            worktree: None,
        });
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 3);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "environments.staging.host");
    }

    #[test]
    fn an_environment_without_a_directory_fails_activation() {
        let (connections, hosts) = resolved();
        let mut manifest = manifest();
        manifest.environments[0].directory = String::new();
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 2);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "environments.default.directory");
        assert_eq!(issues[0].revision, 2);
    }

    #[test]
    fn an_environment_worktree_template_is_checked_at_activation() {
        let (connections, hosts) = resolved();
        let mut manifest = manifest();
        let template = format!(
            "trigger-{}",
            crate::entities::environment::EXECUTION_ID_EXPRESSION
        );
        manifest.environments[0].worktree = Some(crate::entities::environment::Worktree {
            mode: crate::entities::environment::WorktreeMode::BranchOff,
            new_branch: Some(template),
            base: Some("origin/main".into()),
            branch: None,
        });
        assert!(
            resolve_references(&manifest, &file_paths(), &connections, &hosts, 1).is_empty(),
            "a branch template embedding the execution id activates"
        );

        manifest.environments[0]
            .worktree
            .as_mut()
            .expect("worktree")
            .new_branch = Some("trigger".into());
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 1);
        assert_eq!(issues[0].field, "environments.default.worktree.newBranch");
    }

    #[test]
    fn duplicate_environment_is_named() {
        let (connections, hosts) = resolved();
        let mut manifest = manifest();
        manifest.environments.push(Environment {
            name: "default".into(),
            host: "studio-mac".into(),
            directory: "/Users/you/code/repo".into(),
            worktree: None,
        });
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 1);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "environments.default.name");
    }

    #[test]
    fn duplicate_agent_is_named() {
        let (connections, hosts) = resolved();
        let mut manifest = manifest();
        manifest.agents.push(ManifestAgent {
            name: "coder".into(),
            provider: "openai".into(),
            model: "gpt".into(),
        });
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 1);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "agents.coder.name");
    }

    #[test]
    fn agent_with_missing_provider_and_model_is_named() {
        let (connections, hosts) = resolved();
        let mut manifest = manifest();
        manifest.agents.push(ManifestAgent {
            name: "broken".into(),
            provider: "".into(),
            model: "".into(),
        });
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 1);
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"agents.broken.provider"));
        assert!(fields.contains(&"agents.broken.model"));
    }

    #[test]
    fn partial_pointing_at_a_missing_file_is_named() {
        let (connections, hosts) = resolved();
        let mut manifest = manifest();
        manifest.partials.push(ManifestPartial {
            name: "extra".into(),
            path: "partials/extra.txt".into(),
        });
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 2);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "partials.extra.path");
        assert_eq!(issues[0].revision, 2);
    }

    #[test]
    fn duplicate_partial_is_named() {
        let (connections, hosts) = resolved();
        let mut manifest = manifest();
        manifest.partials.push(ManifestPartial {
            name: "system".into(),
            path: "partials/system.txt".into(),
        });
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 1);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "partials.system.name");
    }

    #[test]
    fn slugs_are_resolved_case_insensitively() {
        let (connections, hosts) = resolved();
        let mut manifest = manifest();
        manifest.hosts[0] = "Studio-Mac".into();
        manifest.connections[0] = "GITHUB-MAIN".into();
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 1);
        assert!(
            issues.is_empty(),
            "case-insensitive resolution failed: {issues:?}"
        );
    }

    #[test]
    fn missing_manifest_is_a_structural_error() {
        let files = bundle(&[("other.txt", "hello")]);
        assert!(matches!(
            parse_manifest(&files),
            Err(ConfigurationError::Invalid(_))
        ));
    }

    #[test]
    fn malformed_manifest_is_a_structural_error() {
        let files = bundle(&[(MANIFEST_FILE, "{ not json")]);
        assert!(matches!(
            parse_manifest(&files),
            Err(ConfigurationError::Invalid(_))
        ));
    }

    #[test]
    fn bundle_caps_and_duplicates_are_rejected() {
        assert!(validate_bundle(&[]).is_err());
        let dup = bundle(&[("a.txt", "1"), ("a.txt", "2")]);
        assert!(validate_bundle(&dup).is_err());
        let bad_path = bundle(&[("", "x")]);
        assert!(validate_bundle(&bad_path).is_err());
        let too_many = (0..=MAX_FILES)
            .map(|i| ConfigFile {
                path: format!("{i}.txt"),
                content: "x".into(),
            })
            .collect::<Vec<_>>();
        assert!(validate_bundle(&too_many).is_err());
    }

    #[test]
    fn manifest_parses_into_references() {
        let files = bundle(&[(
            MANIFEST_FILE,
            &manifest_json(
                r#"{
                    "connections": ["github-main"],
                    "hosts": ["studio-mac"],
                    "environments": [{
                        "name": "default",
                        "host": "studio-mac",
                        "directory": "/Users/you/code/repo"
                    }],
                    "agents": [{"name": "coder", "provider": "anthropic", "model": "claude"}],
                    "partials": [{"name": "system", "path": "partials/system.txt"}]
                }"#,
            ),
        )]);
        let parsed = parse_manifest(&files).expect("parse");
        assert_eq!(parsed.connections, vec!["github-main"]);
        assert_eq!(parsed.hosts, vec!["studio-mac"]);
        assert_eq!(parsed.environments.len(), 1);
        assert_eq!(parsed.environments[0].directory, "/Users/you/code/repo");
        assert_eq!(parsed.agents.len(), 1);
        assert_eq!(parsed.partials.len(), 1);
    }

    #[test]
    fn environments_are_parsed_for_dispatch() {
        let files = bundle(&[
            (
                MANIFEST_FILE,
                &manifest_json(
                    r#"{
                        "environments": [{
                            "name": "review",
                            "host": "studio-mac",
                            "directory": "/Users/you/code/repo",
                            "worktree": {
                                "mode": "branch-off",
                                "newBranch": "trigger-${{ paseo.execution.id }}",
                                "base": "origin/main"
                            }
                        }]
                    }"#,
                ),
            ),
            ("partials/system.txt", "unused"),
        ]);
        let environments = parse_environments(&files).expect("parse environments");
        assert_eq!(environments.len(), 1);
        let environment = &environments[0];
        assert_eq!(environment.name, "review");
        let branch = environment.branch_for("exec-1").expect("render");
        assert_eq!(branch.as_deref(), Some("trigger-exec-1"));
    }

    fn workflow_manifest() -> String {
        serde_json::json!({
            "connections": ["github-main"],
            "hosts": ["studio-mac"],
            "environments": [{
                "name": "default",
                "host": "studio-mac",
                "directory": "/Users/you/code/repo"
            }],
            "agents": [{"name": "coder", "provider": "anthropic", "model": "claude"}],
            "partials": [{"name": "system", "path": "partials/system.txt"}],
            "workflows": [{
                "name": "triage",
                "trigger": "issue-comment",
                "maxRuntime": 900,
                "inputs": [{"name": "route"}],
                "steps": [
                    {
                        "id": "classify",
                        "environment": "default",
                        "agent": "coder",
                        "prompt": {
                            "parts": [
                                {"text": "Classify the request."},
                                {"partial": "system"}
                            ]
                        },
                        "output": {
                            "properties": {"route": {"enum": ["bug", "docs"]}}
                        },
                        "maxRuntime": 300,
                        "idleTimeout": 60
                    },
                    {
                        "id": "fix",
                        "environment": "default",
                        "agent": {
                            "provider": "anthropic",
                            "model": "claude",
                            "execution_mode": "default"
                        },
                        "prompt": [{"text": "Fix it."}],
                        "if": {
                            "left": {"input": "route"},
                            "op": "in",
                            "right": {"step": "classify", "property": "route"}
                        },
                        "reads": ["classify"]
                    }
                ]
            }]
        })
        .to_string()
    }

    #[test]
    fn a_two_step_one_partial_workflow_resolves_cleanly() {
        let files = bundle(&[
            (MANIFEST_FILE, &workflow_manifest()),
            ("partials/system.txt", "You are a triage engineer."),
        ]);
        let paths = validate_bundle(&files).expect("bundle structure");
        assert!(paths.contains("partials/system.txt"));
        let manifest = parse_manifest(&files).expect("manifest");
        assert_eq!(manifest.workflows.len(), 1);
        assert_eq!(manifest.workflows[0].steps.len(), 2);
        assert!(
            bundle_reference_issues(&files, &manifest, 1).is_empty(),
            "two-step, one-partial bundle is referenced exactly"
        );
        let (connections, hosts) = resolved();
        let issues = resolve_references(&manifest, &paths, &connections, &hosts, 1);
        assert!(issues.is_empty(), "expected no issues, got {issues:?}");

        let workflows = parse_workflows(&files).expect("workflows");
        let ids: Vec<&str> = workflows[0]
            .steps
            .iter()
            .map(|step| step.id.as_str())
            .collect();
        assert_eq!(ids, vec!["classify", "fix"], "steps stay in file order");
    }

    #[test]
    fn a_configured_step_reading_a_later_step_is_rejected() {
        let mut manifest =
            parse_manifest(&bundle(&[(MANIFEST_FILE, &workflow_manifest())])).expect("manifest");
        manifest.workflows[0].steps[1].reads = vec!["fix".into()];
        manifest.workflows[0].steps[0].reads = vec!["fix".into()];
        let (connections, hosts) = resolved();
        let issues = resolve_references(&manifest, &file_paths(), &connections, &hosts, 1);
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "workflows.triage.steps[0].reads[0]"
                    && issue.revision == 1),
            "got {issues:?}"
        );
    }

    #[test]
    fn a_partial_reading_a_later_step_fails_activation() {
        let files = bundle(&[
            (MANIFEST_FILE, &workflow_manifest()),
            ("partials/system.txt", "Use ${{ steps.fix.output }}"),
        ]);
        let manifest = parse_manifest(&files).expect("manifest");
        let (connections, hosts) = resolved();
        let issues = resolve_references_with_partial_texts(
            &manifest,
            &file_paths(),
            &connections,
            &hosts,
            1,
            &partial_texts(&files, &manifest),
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "workflows.triage.steps[0].prompt[1].partial"),
            "got {issues:?}"
        );
    }

    #[test]
    fn bundle_paths_must_be_safe_and_canonical() {
        for path in [
            "../outside.txt",
            "/absolute.txt",
            "./relative.txt",
            "dir//file.txt",
            "dir/./file.txt",
            "dir/../file.txt",
            "dir/",
            "C:/windows.txt",
            "dir\\windows.txt",
            "~/.ssh/id_rsa",
            " padded.txt",
            "control\u{0000}.txt",
        ] {
            let files = bundle(&[(path, "x")]);
            let error = validate_bundle(&files).expect_err("unsafe path must fail");
            assert!(
                error.to_string().contains(path),
                "error must name '{path}': {error}"
            );
        }
    }

    #[test]
    fn every_bundle_cap_names_the_failing_path() {
        // File count: the 101st file is named.
        let too_many = (0..=MAX_FILES)
            .map(|index| ConfigFile {
                path: format!("file-{index}.txt"),
                content: "x".into(),
            })
            .collect::<Vec<_>>();
        let error = validate_bundle(&too_many).expect_err("file cap");
        assert!(error.to_string().contains("file-100.txt"), "{error}");

        // Path length: the over-long path is named.
        let long_path = format!("{}.txt", "p".repeat(MAX_PATH_CHARS));
        let error = validate_bundle(&bundle(&[(&long_path, "x")])).expect_err("path cap");
        assert!(error.to_string().contains(&long_path), "{error}");

        // File characters: the over-long file is named.
        let long_content = "x".repeat(MAX_FILE_CHARS + 1);
        let error =
            validate_bundle(&bundle(&[("partials/big.txt", &long_content)])).expect_err("file cap");
        assert!(error.to_string().contains("partials/big.txt"), "{error}");

        // Duplicate path is named.
        let duplicate = bundle(&[("a.txt", "1"), ("a.txt", "2")]);
        let error = validate_bundle(&duplicate).expect_err("duplicate");
        assert!(error.to_string().contains("a.txt"), "{error}");
    }

    #[test]
    fn extra_files_are_rejected_and_named() {
        let files = bundle(&[
            (
                MANIFEST_FILE,
                r#"{"partials":[{"name":"system","path":"partials/system.txt"}]}"#,
            ),
            ("partials/system.txt", "used"),
            ("partials/extra.txt", "not referenced"),
        ]);
        let manifest = parse_manifest(&files).expect("manifest");
        let issues = bundle_reference_issues(&files, &manifest, 2);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "files.partials/extra.txt");
        assert!(issues[0].message.contains("partials/extra.txt"));
        assert_eq!(issues[0].revision, 2);
    }

    #[test]
    fn the_total_prompt_partial_cap_names_the_crossing_path() {
        let mut files = Vec::new();
        let mut partials = Vec::new();
        for index in 0..6 {
            let path = format!("partials/part-{index}.txt");
            files.push(ConfigFile {
                path: path.clone(),
                content: "x".repeat(MAX_FILE_CHARS),
            });
            partials.push(serde_json::json!({"name": format!("part-{index}"), "path": path}));
        }
        let manifest_json = serde_json::json!({"partials": partials}).to_string();
        let mut all = vec![ConfigFile {
            path: MANIFEST_FILE.into(),
            content: manifest_json,
        }];
        all.extend(files);
        let manifest = parse_manifest(&all).expect("manifest");
        let issues = bundle_reference_issues(&all, &manifest, 3);
        assert_eq!(issues.len(), 1, "only the crossing path is reported");
        assert_eq!(issues[0].field, "partials.partials/part-5.txt");
        assert!(issues[0].message.contains("partials/part-5.txt"));
    }

    #[test]
    fn partials_within_the_total_cap_are_accepted() {
        let files = bundle(&[
            (
                MANIFEST_FILE,
                r#"{"partials":[
                    {"name":"one","path":"partials/one.txt"},
                    {"name":"two","path":"partials/two.txt"}
                ]}"#,
            ),
            ("partials/one.txt", "first"),
            ("partials/two.txt", "second"),
        ]);
        let manifest = parse_manifest(&files).expect("manifest");
        assert!(bundle_reference_issues(&files, &manifest, 1).is_empty());
    }

    #[actix_web::test]
    #[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
    async fn installs_a_two_step_one_partial_workflow() {
        use crate::services::pool_identity::{self as identity_svc, NewOrganization};

        const DEFAULT_DB_URL: &str = "mysql://tidepool:password@127.0.0.1:3306/tidepool_dev";
        let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| DEFAULT_DB_URL.into());
        let pool = crate::db::build_pool(&db_url)
            .await
            .expect("DB connection failed — is Docker up?");

        for statement in [
            "DELETE FROM configuration_revision_files",
            "DELETE FROM configuration_revisions",
            "DELETE FROM projects",
            "DELETE FROM connection_resources",
            "DELETE FROM connections",
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
                .execute(&pool)
                .await
                .expect("clear table");
        }

        let (organization, _owner) = identity_svc::setup_pool(
            &pool,
            NewOrganization {
                organization_name: "Workflow Integration Pool".into(),
                email: "owner-workflows@example.com".into(),
                display_name: "Workflow Owner".into(),
                password: "integration-password-1".into(),
            },
        )
        .await
        .expect("setup pool");

        sqlx::query(
            "INSERT INTO hosts (public_id, organization_id, slug, display_name, hostname, status) \
             VALUES (?, ?, 'studio-mac', 'Studio Mac', 'studio-mac', 'connected')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(organization.id)
        .execute(&pool)
        .await
        .expect("seed host");
        sqlx::query(
            "INSERT INTO connections \
             (public_id, organization_id, provider, slug, display_name, credential_ref, status) \
             VALUES (?, ?, 'github', 'github-main', 'GitHub', 'unused', 'active')",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(organization.id)
        .execute(&pool)
        .await
        .expect("seed connection");

        let project_public_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO projects (public_id, organization_id, slug, name) \
             VALUES (?, ?, 'workflow-project', 'Workflow Project')",
        )
        .bind(&project_public_id)
        .bind(organization.id)
        .execute(&pool)
        .await
        .expect("seed project");

        let files = bundle(&[
            (MANIFEST_FILE, &workflow_manifest()),
            ("partials/system.txt", "You are a triage engineer."),
        ]);
        let result = install(
            &pool,
            organization.id,
            &project_public_id,
            &files,
            AuditActor::member("integration-owner", "owner-workflows@example.com"),
        )
        .await
        .expect("install workflow bundle");

        assert!(result.activated, "a valid workflow bundle activates");
        assert_eq!(result.revision.version, 1);
        assert_eq!(result.revision.status, RevisionStatus::Valid);
        assert_eq!(result.revision.files.len(), 2, "manifest and one partial");
        let workflows = parse_workflows(&files).expect("workflows");
        assert_eq!(workflows[0].steps.len(), 2);
        assert_eq!(workflows[0].steps[0].id, "classify");
        assert_eq!(workflows[0].steps[1].id, "fix");
    }
}
