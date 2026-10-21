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
use crate::services::api_keys::{self as audit_svc, AuditActor, AuditEntry};
use crate::services::connections::{self as connection_svc, ConnectionError};
use crate::services::environments as environment_svc;
use crate::services::hosts::{self as host_svc, HostServiceError, normalize_slug};
use serde::Deserialize;
use sqlx::MySqlPool;
use std::collections::BTreeSet;
use thiserror::Error;

/// Path of the manifest file that declares the bundle's references.
pub const MANIFEST_FILE: &str = "tidepool.json";

/// Maximum number of files a bundle may declare.
pub const MAX_FILES: usize = 100;
/// Maximum length of an authored file path.
pub const MAX_PATH_CHARS: usize = 512;
/// Maximum number of characters in a single authored file.
pub const MAX_FILE_CHARS: usize = 1_000_000;

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

/// Validate the bundle's structure and return its exact file paths.
fn validate_bundle(files: &[ConfigFile]) -> Result<BTreeSet<String>, ConfigurationError> {
    if files.is_empty() {
        return Err(ConfigurationError::Invalid(
            "configuration bundle must contain at least one file".into(),
        ));
    }
    if files.len() > MAX_FILES {
        return Err(ConfigurationError::Invalid(format!(
            "configuration bundle exceeds {MAX_FILES} files"
        )));
    }

    let mut paths = BTreeSet::new();
    for file in files {
        let trimmed = file.path.trim();
        if trimmed.is_empty() || trimmed.chars().count() > MAX_PATH_CHARS {
            return Err(ConfigurationError::Invalid(format!(
                "file path must be 1-{MAX_PATH_CHARS} characters: '{trimmed}'"
            )));
        }
        if file.content.chars().count() > MAX_FILE_CHARS {
            return Err(ConfigurationError::Invalid(format!(
                "file '{trimmed}' exceeds {MAX_FILE_CHARS} characters"
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
fn resolve_references(
    manifest: &Manifest,
    file_paths: &BTreeSet<String>,
    resolved_connections: &BTreeSet<String>,
    resolved_hosts: &BTreeSet<String>,
    revision: u32,
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
    let errors = resolve_references(
        &manifest,
        &file_paths,
        &resolved_connections,
        &resolved_hosts,
        version,
    );
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

    let errors = resolve_references(
        &manifest,
        &file_paths,
        &resolved_connections,
        &resolved_hosts,
        version,
    );
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
}
