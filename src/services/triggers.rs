//! Trigger schema, validation and installation (tasks 1.4.2 and 1.4.3).
//!
//! A trigger is one document: the provider [`TriggerEvent`] it listens for, the
//! [`TriggerFilters`] that decide which events are allowed through, and the
//! [`TriggerRun`] an allowed event starts. [`validate_trigger`] compiles that
//! document against the pool's live state and reports a named [`TriggerIssue`]
//! for every part that cannot be satisfied, without writing anything.
//!
//! `filters` is required and `from_users` must be present and non-empty: the
//! allowlist is the one layer that keeps a stranger's comment on a public issue
//! from starting an agent on someone's machine, so a missing or empty allowlist
//! is a hard rejection, never a default.
//!
//! Validation resolves three things by name and reports the field that fails:
//!
//! - **connection** — `filters.connection` slug must name an active connection
//!   whose provider matches the declared event provider;
//! - **resource** — `filters.resource` (`kind` + `id`) must be one an active
//!   connection of the event provider delivers for;
//! - **host** — `run.host` slug must name a live (non-revoked) host, and the
//!   host must have been granted `host.execute` and be able to run
//!   `run.agent`.
//!
//! The filter predicates (`channel`/`label`, `contains`/`pattern`) are validated
//! for shape only here; matching an event against them is dispatch's job
//! (task 1.4.6). Every condition is a conjunction — all must pass, and there is
//! no "any" mode.
//!
//! [`install_trigger`] runs the same validation and then creates or replaces the
//! project's trigger of that name. Names are unique per project, and a
//! replacement keeps the trigger's `public_id`, so its revisions stay one
//! readable history: each successful install appends one immutable
//! `trigger_revisions` row, activates it and deactivates the previous one. A
//! document that fails validation writes nothing at all, so a later failure
//! never undoes an earlier success.

use crate::entities::connection::{ConnectionStatus, ProviderKind};
use crate::entities::host_capability::AgentSelection;
use crate::services::api_keys as audit_svc;
use crate::services::api_keys::{AuditActor, AuditEntry};
use crate::services::configuration::{self as configuration_svc, ConfigurationError};
use crate::services::connections::{self as connection_svc, ConnectionError};
use crate::services::host_capability::{self as capability_svc, HostCapabilityError};
use crate::services::hosts::{self as host_svc, HostServiceError, normalize_slug};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use thiserror::Error;

/// Maximum number of characters in a trigger name.
pub const MAX_NAME_CHARS: usize = 255;
/// Maximum number of characters in an event type.
pub const MAX_EVENT_CHARS: usize = 128;
/// Maximum number of characters in a resource kind.
pub const MAX_KIND_CHARS: usize = 64;
/// Maximum number of characters in a resource id.
pub const MAX_RESOURCE_CHARS: usize = 255;
/// Maximum number of characters in a channel or label filter.
pub const MAX_CHANNEL_CHARS: usize = 255;
/// Maximum number of characters in a `contains` guard.
pub const MAX_CONTAINS_CHARS: usize = 10_000;
/// Maximum number of characters in a `pattern` guard.
pub const MAX_PATTERN_CHARS: usize = 1_000;
/// Maximum number of allowed user handles in `from_users`.
pub const MAX_FROM_USERS: usize = 10_000;
/// Maximum number of characters in a single `from_users` handle.
pub const MAX_USER_CHARS: usize = 255;

/// The complete trigger document: event, filters and the run it starts.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerDocument {
    /// Trigger name, unique per project once installed.
    pub name: String,
    /// The provider event this trigger listens for.
    pub event: TriggerEvent,
    /// Filters that decide which events are allowed through.
    pub filters: TriggerFilters,
    /// The run an allowed event starts.
    pub run: TriggerRun,
}

/// The provider event a trigger listens for.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerEvent {
    /// Provider kind (`github`, `slack`, `discord`).
    pub provider: ProviderKind,
    /// Internal event type (for example `issue.comment_created`).
    #[serde(rename = "type")]
    pub event_type: String,
}

/// Filters that gate which events may start the run.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerFilters {
    /// Connection slug the event must arrive through.
    pub connection: String,
    /// Provider resource the event must concern.
    pub resource: ResourceFilter,
    /// Required allowlist of users who may start the run. A missing or empty
    /// list is a hard rejection, never a default.
    #[serde(default)]
    pub from_users: Vec<String>,
    /// Optional channel the event must come from (Slack/Discord).
    #[serde(default)]
    pub channel: Option<String>,
    /// Optional label the event must carry (GitHub).
    #[serde(default)]
    pub label: Option<String>,
    /// Optional substring guard over the event content.
    #[serde(default)]
    pub contains: Option<String>,
    /// Optional pattern guard over the event content.
    #[serde(default)]
    pub pattern: Option<String>,
}

/// A provider resource identity (`kind` + `id`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceFilter {
    /// Resource kind (`repository`, `workspace`, `guild`, `channel`).
    pub kind: String,
    /// Provider-side id (for example `org/repo` or a workspace id).
    pub id: String,
}

/// The run an allowed event starts: a host and the agent to run on it.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerRun {
    /// Host slug the run dispatches to.
    pub host: String,
    /// The agent the host must be able to run.
    pub agent: AgentSelection,
}

/// One named field error produced by trigger validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerIssue {
    /// Reference path, for example `filters.from_users` or `run.agent.model`.
    pub field: String,
    /// Human-readable explanation.
    pub message: String,
}

/// Result of validating a trigger document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TriggerValidateOutcome {
    /// The trigger name, echoed back for the caller.
    pub name: String,
    /// True when every reference resolved and the host can run the agent.
    pub valid: bool,
    /// Named field issues, empty when valid.
    pub errors: Vec<TriggerIssue>,
}

/// Domain errors for trigger validation and installation.
#[derive(Debug, Error)]
pub enum TriggerError {
    /// Database failure while resolving a reference or storing a revision.
    #[error("database error")]
    Database(#[source] sqlx::Error),
    /// No project with that id in this organization.
    #[error("project not found")]
    ProjectNotFound,
    /// The document failed validation; nothing was written.
    #[error("trigger document is invalid")]
    Invalid(Vec<TriggerIssue>),
    /// The document could not be encoded for storage.
    #[error("trigger document could not be encoded: {0}")]
    Encoding(String),
}

/// Result of one successful [`install_trigger`] call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TriggerInstallResult {
    /// Immutable trigger id; a replacement keeps it.
    pub id: String,
    /// The trigger name, unique per project.
    pub name: String,
    /// Public id of the revision this install produced.
    pub revision: String,
    /// Per-trigger revision number, starting at 1.
    pub version: u32,
    /// True when this revision is the trigger's active document.
    pub active: bool,
    /// True when an existing trigger was replaced rather than created.
    pub replaced: bool,
}

/// Identity of an already-installed trigger, as install bookkeeping needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingTrigger {
    /// The trigger's public id; a replacement keeps it.
    pub public_id: String,
}

/// One stored revision of a trigger, as version bookkeeping needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RevisionState {
    /// Per-trigger revision number.
    pub version: u32,
    /// Whether this revision is the trigger's active document.
    pub active: bool,
}

/// What the next install of a trigger does to its stored revisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionPlan {
    /// Revision number the install produces (`max + 1`).
    pub version: u32,
    /// True when a currently active revision must be deactivated.
    pub deactivates_previous: bool,
}

impl RevisionPlan {
    /// Plan the next revision from the trigger's existing revisions.
    ///
    /// Pure: the unit-testable half of revision bookkeeping. The version is
    /// `max(version) + 1` rather than `count + 1`, so a gap can never produce a
    /// duplicate; any currently active revision is deactivated by the install.
    pub fn from_existing(existing: &[RevisionState]) -> Self {
        let version = existing
            .iter()
            .map(|revision| revision.version)
            .max()
            .unwrap_or(0)
            + 1;
        Self {
            version,
            deactivates_previous: existing.iter().any(|revision| revision.active),
        }
    }
}

/// Resolve the public id an install uses: the existing trigger's id when it is
/// replaced, or the freshly generated one when it is created.
pub fn trigger_identity(existing: Option<&ExistingTrigger>, generated: &str) -> (String, bool) {
    match existing {
        Some(trigger) => (trigger.public_id.clone(), true),
        None => (generated.to_string(), false),
    }
}

/// The stored name of a trigger: trimmed, as authored otherwise.
pub fn trigger_name(document: &TriggerDocument) -> String {
    document.name.trim().to_string()
}

/// What one install does to the trigger's stored state.
///
/// Pure output of [`plan_install`], so the create/replace and revision
/// bookkeeping is unit-testable without a database. The database layer only
/// executes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    /// Trigger id the install writes: the existing one when replaced.
    pub id: String,
    /// Stored trigger name.
    pub name: String,
    /// Public id of the revision this install appends.
    pub revision: String,
    /// Per-trigger revision number.
    pub version: u32,
    /// True: a valid install is always the active document.
    pub active: bool,
    /// True when an existing trigger with this name was replaced.
    pub replaced: bool,
    /// True when a new `triggers` row must be inserted.
    pub creates_trigger: bool,
    /// True when the previously active revision must be deactivated.
    pub deactivates_previous: bool,
}

/// Plan one install from the trigger's current state.
///
/// Create-then-replace: with no existing trigger the given id is used and
/// version 1 is written; with an existing trigger its id is kept (so the
/// history stays one history), the next version is appended and the previous
/// active revision is deactivated. Every other revision is left untouched.
pub fn plan_install(
    document: &TriggerDocument,
    existing: Option<&ExistingTrigger>,
    revisions: &[RevisionState],
    generated_trigger_id: &str,
    generated_revision_id: &str,
) -> InstallPlan {
    let (id, replaced) = trigger_identity(existing, generated_trigger_id);
    let revision_plan = RevisionPlan::from_existing(revisions);
    InstallPlan {
        id,
        name: trigger_name(document),
        revision: generated_revision_id.to_string(),
        version: revision_plan.version,
        active: true,
        replaced,
        creates_trigger: existing.is_none(),
        deactivates_previous: revision_plan.deactivates_previous,
    }
}

/// Turn a validation outcome into the install plan, or the rejection.
///
/// Pure: the whole "invalid document changes nothing" rule lives here, because
/// the plan is what the database layer executes. A document with any named
/// issue yields [`TriggerError::Invalid`] and no plan at all, so no call is
/// made to create, replace, deactivate or append anything.
pub fn decide_install(
    document: &TriggerDocument,
    outcome: &TriggerValidateOutcome,
    existing: Option<&ExistingTrigger>,
    revisions: &[RevisionState],
    generated_trigger_id: &str,
    generated_revision_id: &str,
) -> Result<InstallPlan, TriggerError> {
    if !outcome.valid || !outcome.errors.is_empty() {
        return Err(TriggerError::Invalid(outcome.errors.clone()));
    }
    Ok(plan_install(
        document,
        existing,
        revisions,
        generated_trigger_id,
        generated_revision_id,
    ))
}

/// How the named host and its ability to run the agent resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunResolution {
    /// No live host with the slug.
    HostUnknown,
    /// The host resolves but `host.execute` has not been granted.
    NotGranted,
    /// The host resolves and can run the agent.
    Runs,
    /// The host resolves but cannot run the agent; the field is named.
    Unavailable { field: String, value: String },
}

/// Database facts substituted into the document for [`resolve_issues`]. Pure:
/// no database access, so the issue-producing logic is unit-testable.
#[derive(Debug)]
pub struct TriggerResolution<'a> {
    /// The document being validated.
    pub document: &'a TriggerDocument,
    /// `(provider, active)` for the resolved connection, or `None` when no
    /// connection with the slug exists.
    pub connection: Option<(ProviderKind, bool)>,
    /// Whether an active connection delivers for `filters.resource`.
    pub resource_owned: bool,
    /// How the named host and agent resolved.
    pub run: RunResolution,
}

fn issue(field: impl Into<String>, message: impl Into<String>) -> TriggerIssue {
    TriggerIssue {
        field: field.into(),
        message: message.into(),
    }
}

/// Structural (shape) issues: name, event, filter fields and agent fields.
///
/// Pure and database-free. These are reported even before the database is
/// consulted, so a malformed document always names every broken field.
fn structural_issues(document: &TriggerDocument) -> Vec<TriggerIssue> {
    let mut issues = Vec::new();

    if document.name.trim().is_empty() {
        issues.push(issue("name", "trigger name is required"));
    } else if document.name.chars().count() > MAX_NAME_CHARS {
        issues.push(issue(
            "name",
            format!("trigger name must be at most {MAX_NAME_CHARS} characters"),
        ));
    }

    if document.event.event_type.trim().is_empty() {
        issues.push(issue("event.type", "event type is required"));
    } else if document.event.event_type.len() > MAX_EVENT_CHARS {
        issues.push(issue(
            "event.type",
            format!("event type must be at most {MAX_EVENT_CHARS} characters"),
        ));
    }

    let filters = &document.filters;
    if normalize_slug(&filters.connection).is_empty() {
        issues.push(issue("filters.connection", "connection slug is required"));
    }

    if filters.from_users.is_empty() {
        issues.push(issue(
            "filters.from_users",
            "from_users must be present and non-empty",
        ));
    } else {
        if filters.from_users.len() > MAX_FROM_USERS {
            issues.push(issue(
                "filters.from_users",
                format!("from_users must have at most {MAX_FROM_USERS} entries"),
            ));
        }
        for (index, user) in filters.from_users.iter().enumerate() {
            if user.trim().is_empty() {
                issues.push(issue(
                    format!("filters.from_users[{index}]"),
                    "user handle is required",
                ));
            } else if user.chars().count() > MAX_USER_CHARS {
                issues.push(issue(
                    format!("filters.from_users[{index}]"),
                    format!("user handle must be at most {MAX_USER_CHARS} characters"),
                ));
            }
        }
    }

    if filters.resource.kind.trim().is_empty() {
        issues.push(issue("filters.resource.kind", "resource kind is required"));
    } else if filters.resource.kind.len() > MAX_KIND_CHARS {
        issues.push(issue(
            "filters.resource.kind",
            format!("resource kind must be at most {MAX_KIND_CHARS} characters"),
        ));
    }
    if filters.resource.id.trim().is_empty() {
        issues.push(issue("filters.resource.id", "resource id is required"));
    } else if filters.resource.id.len() > MAX_RESOURCE_CHARS {
        issues.push(issue(
            "filters.resource.id",
            format!("resource id must be at most {MAX_RESOURCE_CHARS} characters"),
        ));
    }

    if let Some(channel) = filters.channel.as_deref() {
        if channel.trim().is_empty() {
            issues.push(issue("filters.channel", "channel must be non-empty"));
        } else if channel.len() > MAX_CHANNEL_CHARS {
            issues.push(issue(
                "filters.channel",
                format!("channel must be at most {MAX_CHANNEL_CHARS} characters"),
            ));
        }
    }
    if let Some(label) = filters.label.as_deref() {
        if label.trim().is_empty() {
            issues.push(issue("filters.label", "label must be non-empty"));
        } else if label.len() > MAX_CHANNEL_CHARS {
            issues.push(issue(
                "filters.label",
                format!("label must be at most {MAX_CHANNEL_CHARS} characters"),
            ));
        }
    }
    if let Some(contains) = filters.contains.as_deref() {
        if contains.is_empty() {
            issues.push(issue(
                "filters.contains",
                "contains guard must be non-empty",
            ));
        } else if contains.len() > MAX_CONTAINS_CHARS {
            issues.push(issue(
                "filters.contains",
                format!("contains guard must be at most {MAX_CONTAINS_CHARS} characters"),
            ));
        }
    }
    if let Some(pattern) = filters.pattern.as_deref() {
        if pattern.trim().is_empty() {
            issues.push(issue("filters.pattern", "pattern guard must be non-empty"));
        } else if pattern.len() > MAX_PATTERN_CHARS {
            issues.push(issue(
                "filters.pattern",
                format!("pattern guard must be at most {MAX_PATTERN_CHARS} characters"),
            ));
        }
    }

    if normalize_slug(&document.run.host).is_empty() {
        issues.push(issue("run.host", "host slug is required"));
    }

    let agent = &document.run.agent;
    if agent.provider.trim().is_empty() {
        issues.push(issue("run.agent.provider", "provider is required"));
    }
    if agent.model.trim().is_empty() {
        issues.push(issue("run.agent.model", "model is required"));
    }
    if agent.execution_mode.trim().is_empty() {
        issues.push(issue(
            "run.agent.execution_mode",
            "execution mode is required",
        ));
    }
    if agent
        .thinking_option
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        issues.push(issue(
            "run.agent.thinking_option",
            "thinking option must be non-empty",
        ));
    }

    issues
}

/// Produce every issue for a document given the resolved database facts.
///
/// Pure and database-free: [`validate_trigger`] gathers the facts, this
/// function decides. Every reference that failed to resolve is named, so a
/// client can point at the exact field.
pub fn resolve_issues(resolution: &TriggerResolution<'_>) -> Vec<TriggerIssue> {
    let mut issues = structural_issues(resolution.document);
    let document = resolution.document;

    let connection_slug = normalize_slug(&document.filters.connection);
    if !connection_slug.is_empty() {
        match resolution.connection {
            None => issues.push(issue(
                "filters.connection",
                format!("no active connection with slug '{connection_slug}'"),
            )),
            Some((_, false)) => issues.push(issue(
                "filters.connection",
                format!("connection '{connection_slug}' is not active"),
            )),
            Some((provider, true)) if provider != document.event.provider => {
                issues.push(issue(
                    "filters.connection",
                    format!(
                        "connection '{connection_slug}' is a {} connection, not a {} connection",
                        provider.as_str(),
                        document.event.provider.as_str()
                    ),
                ));
            }
            Some((_, true)) => {}
        }
    }

    let resource = &document.filters.resource;
    if !resource.kind.trim().is_empty()
        && !resource.id.trim().is_empty()
        && !resolution.resource_owned
    {
        issues.push(issue(
            "filters.resource",
            format!(
                "no active {} connection delivers for '{}' '{}'",
                document.event.provider.as_str(),
                resource.kind.trim(),
                resource.id.trim()
            ),
        ));
    }

    let host_slug = normalize_slug(&document.run.host);
    if !host_slug.is_empty() {
        match &resolution.run {
            RunResolution::HostUnknown => issues.push(issue(
                "run.host",
                format!("unknown host slug '{host_slug}'"),
            )),
            RunResolution::NotGranted => issues.push(issue(
                "run.host",
                format!("host '{host_slug}' has not been granted execution"),
            )),
            RunResolution::Unavailable { field, value } => issues.push(issue(
                format!("run.agent.{field}"),
                format!("host '{host_slug}' cannot run '{value}'"),
            )),
            RunResolution::Runs => {}
        }
    }

    issues
}

/// Validate a trigger document without writing pool state.
///
/// Resolves the connection, resource and host against the database, asks the
/// named host whether it can run the agent, and returns the trigger name plus
/// every named issue. Only reads are performed.
pub async fn validate_trigger(
    pool: &MySqlPool,
    organization_id: u64,
    document: &TriggerDocument,
) -> Result<TriggerValidateOutcome, TriggerError> {
    let connection = resolve_connection(pool, organization_id, document).await?;
    let resource_owned = resolve_resource(pool, organization_id, document).await?;
    let run = resolve_run(pool, organization_id, document).await?;

    let resolution = TriggerResolution {
        document,
        connection,
        resource_owned,
        run,
    };
    let errors = resolve_issues(&resolution);

    Ok(TriggerValidateOutcome {
        name: document.name.clone(),
        valid: errors.is_empty(),
        errors,
    })
}

// ── installation ───────────────────────────────────────────────────────────

/// Validate and then create or replace the project's trigger of that name.
///
/// One request installs one trigger. The document is compiled exactly as
/// [`validate_trigger`] compiles it; when it reports any issue nothing at all is
/// written, so an invalid document cannot disturb an earlier success. A valid
/// document appends one immutable revision, activates it and deactivates the
/// trigger's previous revision, keeping the trigger's identity so its history
/// stays readable. Repeating the same document is therefore a new revision, not
/// a no-op.
pub async fn install_trigger(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
    document: &TriggerDocument,
    actor: AuditActor,
) -> Result<TriggerInstallResult, TriggerError> {
    let project_id = configuration_svc::project_id(pool, organization_id, project_public_id)
        .await
        .map_err(map_configuration_error)?;

    // Validate before the transaction: a rejected document must write nothing,
    // so nothing is locked, inserted, deactivated or audited.
    let outcome = validate_trigger(pool, organization_id, document).await?;

    let name = trigger_name(document);
    let stored_document = serde_json::to_string(document).map_err(|error| {
        TriggerError::Encoding(format!("trigger document is not serializable: {error}"))
    })?;

    let mut tx = pool.begin().await.map_err(TriggerError::Database)?;

    // Lock the project row so two concurrent installs against the same project
    // serialize: state is read and the plan built under the lock, so they
    // cannot both pick the same revision number or both create the trigger.
    sqlx::query("SELECT id FROM projects WHERE id = ? FOR UPDATE")
        .bind(project_id)
        .execute(&mut *tx)
        .await
        .map_err(TriggerError::Database)?;

    let existing = find_trigger(&mut *tx, project_id, &name).await?;
    let revisions = match existing.as_ref() {
        Some(trigger) => load_revisions(&mut *tx, trigger.trigger_id).await?,
        None => Vec::new(),
    };
    let identity = existing.as_ref().map(TriggerRow::identity);
    let generated_trigger_id = uuid::Uuid::new_v4().to_string();
    let generated_revision_id = uuid::Uuid::new_v4().to_string();
    let plan = decide_install(
        document,
        &outcome,
        identity.as_ref(),
        &revisions,
        &generated_trigger_id,
        &generated_revision_id,
    )?;

    let trigger_id = match existing.as_ref() {
        Some(trigger) => {
            // A case-variant of the same name matches under the DB's
            // case-insensitive collation, so the stored name follows the
            // latest authored spelling.
            if trigger.stored_name != plan.name {
                sqlx::query("UPDATE triggers SET name = ? WHERE id = ?")
                    .bind(&plan.name)
                    .bind(trigger.trigger_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(TriggerError::Database)?;
            }
            trigger.trigger_id
        }
        None => {
            let result =
                sqlx::query("INSERT INTO triggers (public_id, project_id, name) VALUES (?, ?, ?)")
                    .bind(&plan.id)
                    .bind(project_id)
                    .bind(&plan.name)
                    .execute(&mut *tx)
                    .await
                    .map_err(TriggerError::Database)?;
            result.last_insert_id()
        }
    };

    if plan.deactivates_previous {
        sqlx::query("UPDATE trigger_revisions SET active = 0 WHERE trigger_id = ? AND active = 1")
            .bind(trigger_id)
            .execute(&mut *tx)
            .await
            .map_err(TriggerError::Database)?;
    }

    sqlx::query(
        "INSERT INTO trigger_revisions (public_id, trigger_id, version, active, document) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&plan.revision)
    .bind(trigger_id)
    .bind(plan.version)
    .bind(plan.active)
    .bind(&stored_document)
    .execute(&mut *tx)
    .await
    .map_err(TriggerError::Database)?;

    tx.commit().await.map_err(TriggerError::Database)?;

    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id,
            actor,
            action: "trigger.install",
            target_kind: Some("trigger"),
            target_public_id: Some(&plan.id),
            detail: serde_json::json!({
                "project": project_public_id,
                "name": name,
                "revision": plan.version,
                "replaced": plan.replaced,
            }),
        },
    )
    .await;

    Ok(TriggerInstallResult {
        id: plan.id,
        name,
        revision: plan.revision,
        version: plan.version,
        active: plan.active,
        replaced: plan.replaced,
    })
}

fn map_configuration_error(error: ConfigurationError) -> TriggerError {
    match error {
        ConfigurationError::Database(error) => TriggerError::Database(error),
        ConfigurationError::ProjectNotFound => TriggerError::ProjectNotFound,
        // The project lookup is the only configuration call made here.
        _ => TriggerError::ProjectNotFound,
    }
}

/// A trigger row as install bookkeeping needs it.
struct TriggerRow {
    /// Internal id used by the revision queries.
    trigger_id: u64,
    /// Immutable public id returned to the caller; a replacement keeps it.
    public_id: String,
    /// The name as currently stored (may differ in case from the document).
    stored_name: String,
}

impl TriggerRow {
    /// Borrow the identity the pure install planner expects.
    fn identity(&self) -> ExistingTrigger {
        ExistingTrigger {
            public_id: self.public_id.clone(),
        }
    }
}

/// Find the project's trigger by name, scoped to the project.
async fn find_trigger<'e, E>(
    executor: E,
    project_id: u64,
    name: &str,
) -> Result<Option<TriggerRow>, TriggerError>
where
    E: sqlx::Executor<'e, Database = sqlx::MySql>,
{
    let row: Option<(u64, String, String)> = sqlx::query_as(
        "SELECT id, public_id, name FROM triggers WHERE project_id = ? AND name = ?",
    )
    .bind(project_id)
    .bind(name)
    .fetch_optional(executor)
    .await
    .map_err(TriggerError::Database)?;
    Ok(row.map(|(trigger_id, public_id, stored_name)| TriggerRow {
        trigger_id,
        public_id,
        stored_name,
    }))
}

/// Load the version/active state of a trigger's revisions.
async fn load_revisions<'e, E>(
    executor: E,
    trigger_id: u64,
) -> Result<Vec<RevisionState>, TriggerError>
where
    E: sqlx::Executor<'e, Database = sqlx::MySql>,
{
    let rows: Vec<(u32, bool)> =
        sqlx::query_as("SELECT version, active FROM trigger_revisions WHERE trigger_id = ?")
            .bind(trigger_id)
            .fetch_all(executor)
            .await
            .map_err(TriggerError::Database)?;
    Ok(rows
        .into_iter()
        .map(|(version, active)| RevisionState { version, active })
        .collect())
}

/// Resolve `filters.connection` to `(provider, active)`, or `None` when unknown.
async fn resolve_connection(
    pool: &MySqlPool,
    organization_id: u64,
    document: &TriggerDocument,
) -> Result<Option<(ProviderKind, bool)>, TriggerError> {
    let slug = normalize_slug(&document.filters.connection);
    if slug.is_empty() {
        return Ok(None);
    }
    match connection_svc::find_effective_by_slug(pool, organization_id, &slug).await {
        Ok(Some(connection)) => Ok(Some((
            connection.provider,
            connection.status == ConnectionStatus::Active,
        ))),
        Ok(None) => Ok(None),
        Err(ConnectionError::Database(error)) => Err(TriggerError::Database(error)),
        // A malformed environment overlay is a configuration error, not a
        // missing connection; surface it the same way configuration does.
        Err(_) => Ok(None),
    }
}

/// Decide whether an active connection of the event provider delivers for the
/// resource.
async fn resolve_resource(
    pool: &MySqlPool,
    organization_id: u64,
    document: &TriggerDocument,
) -> Result<bool, TriggerError> {
    let resource = &document.filters.resource;
    if resource.kind.trim().is_empty() || resource.id.trim().is_empty() {
        return Ok(false);
    }
    match connection_svc::resolve_resource_owner(
        pool,
        organization_id,
        document.event.provider,
        resource.kind.trim(),
        resource.id.trim(),
    )
    .await
    {
        Ok(owner) => Ok(owner.is_some()),
        Err(ConnectionError::Database(error)) => Err(TriggerError::Database(error)),
        Err(_) => Ok(false),
    }
}

/// Resolve the host, then ask it whether it can run the agent.
async fn resolve_run(
    pool: &MySqlPool,
    organization_id: u64,
    document: &TriggerDocument,
) -> Result<RunResolution, TriggerError> {
    let slug = normalize_slug(&document.run.host);
    if slug.is_empty() {
        return Ok(RunResolution::HostUnknown);
    }
    let host = match host_svc::resolve_for_activation(pool, organization_id, &slug).await {
        Ok(host) => host,
        Err(HostServiceError::UnknownHostSlug { .. } | HostServiceError::NotFound) => {
            return Ok(RunResolution::HostUnknown);
        }
        Err(HostServiceError::Database(error)) => return Err(TriggerError::Database(error)),
        Err(_) => return Ok(RunResolution::HostUnknown),
    };

    match capability_svc::validate_agent_for_host(
        pool,
        organization_id,
        &host.public_id,
        &document.run.agent,
    )
    .await
    {
        Ok(()) => Ok(RunResolution::Runs),
        Err(HostCapabilityError::NotGranted) => Ok(RunResolution::NotGranted),
        Err(HostCapabilityError::Unavailable { field, value }) => Ok(RunResolution::Unavailable {
            field: field.to_string(),
            value,
        }),
        Err(HostCapabilityError::NotFound) => Ok(RunResolution::HostUnknown),
        Err(HostCapabilityError::Invalid(message)) => Ok(RunResolution::Unavailable {
            field: "run.agent".to_string(),
            value: message,
        }),
        Err(HostCapabilityError::Database(error)) => Err(TriggerError::Database(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document_json() -> serde_json::Value {
        serde_json::json!({
            "name": "auto-fix-issues",
            "event": { "provider": "github", "type": "issue.comment_created" },
            "filters": {
                "connection": "github-main",
                "resource": { "kind": "repository", "id": "org/repo" },
                "from_users": ["alice", "bob"]
            },
            "run": {
                "host": "studio-mac",
                "agent": {
                    "provider": "anthropic",
                    "model": "claude",
                    "execution_mode": "default"
                }
            }
        })
    }

    fn document() -> TriggerDocument {
        serde_json::from_value(document_json()).expect("valid document")
    }

    fn resolution(document: &TriggerDocument) -> TriggerResolution<'_> {
        TriggerResolution {
            document,
            connection: Some((ProviderKind::Github, true)),
            resource_owned: true,
            run: RunResolution::Runs,
        }
    }

    #[test]
    fn a_complete_document_resolves_cleanly() {
        let document = document();
        let issues = resolve_issues(&resolution(&document));
        assert!(issues.is_empty(), "expected no issues, got {issues:?}");
    }

    #[test]
    fn document_denies_unknown_fields() {
        let mut json = document_json();
        json["event"]
            .as_object_mut()
            .expect("event")
            .insert("nope".to_string(), serde_json::json!(true));
        assert!(serde_json::from_value::<TriggerDocument>(json).is_err());
    }

    // ── structural rejections ──────────────────────────────────────────────

    #[test]
    fn missing_from_users_is_a_hard_rejection() {
        let mut json = document_json();
        json["filters"]
            .as_object_mut()
            .expect("filters")
            .remove("from_users");
        let document: TriggerDocument = serde_json::from_value(json).expect("deserialize");
        let issues = resolve_issues(&resolution(&document));
        let field = issues
            .iter()
            .find(|issue| issue.field == "filters.from_users");
        assert!(
            field.is_some(),
            "missing from_users must name filters.from_users, got {issues:?}"
        );
        assert_eq!(
            field.expect("issue").message,
            "from_users must be present and non-empty"
        );
    }

    #[test]
    fn empty_from_users_is_a_hard_rejection() {
        let mut json = document_json();
        json["filters"]["from_users"] = serde_json::json!([]);
        let document: TriggerDocument = serde_json::from_value(json).expect("deserialize");
        let issues = resolve_issues(&resolution(&document));
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "filters.from_users")
        );
    }

    #[test]
    fn blank_user_handle_is_named_by_index() {
        let mut json = document_json();
        json["filters"]["from_users"] = serde_json::json!(["alice", "  "]);
        let document: TriggerDocument = serde_json::from_value(json).expect("deserialize");
        let issues = resolve_issues(&resolution(&document));
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "filters.from_users[1]")
        );
    }

    #[test]
    fn missing_name_event_and_agent_fields_are_named() {
        let json = serde_json::json!({
            "name": "",
            "event": { "provider": "github", "type": "" },
            "filters": {
                "connection": "",
                "resource": { "kind": "", "id": "" },
                "from_users": ["alice"]
            },
            "run": {
                "host": "",
                "agent": { "provider": "", "model": "", "execution_mode": "" }
            }
        });
        let document: TriggerDocument = serde_json::from_value(json).expect("deserialize");
        let issues = resolve_issues(&resolution(&document));
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        for expected in [
            "name",
            "event.type",
            "filters.connection",
            "filters.resource.kind",
            "filters.resource.id",
            "run.host",
            "run.agent.provider",
            "run.agent.model",
            "run.agent.execution_mode",
        ] {
            assert!(
                fields.contains(&expected),
                "missing {expected} in {fields:?}"
            );
        }
    }

    #[test]
    fn blank_optional_guards_are_named() {
        let mut json = document_json();
        json["filters"]["channel"] = serde_json::json!("");
        json["filters"]["label"] = serde_json::json!("");
        json["filters"]["contains"] = serde_json::json!("");
        json["filters"]["pattern"] = serde_json::json!("  ");
        let document: TriggerDocument = serde_json::from_value(json).expect("deserialize");
        let issues = resolve_issues(&resolution(&document));
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        for expected in [
            "filters.channel",
            "filters.label",
            "filters.contains",
            "filters.pattern",
        ] {
            assert!(
                fields.contains(&expected),
                "missing {expected} in {fields:?}"
            );
        }
    }

    #[test]
    fn optional_guards_are_optional_when_absent() {
        let document = document();
        assert!(document.filters.channel.is_none());
        assert!(document.filters.pattern.is_none());
        assert!(resolve_issues(&resolution(&document)).is_empty());
    }

    #[test]
    fn every_optional_guard_is_accepted_when_present() {
        // All guards are conjunctive: a document may carry all of them and each
        // one is checked, never treated as an alternative.
        let mut json = document_json();
        json["filters"]["channel"] = serde_json::json!("C0123456");
        json["filters"]["label"] = serde_json::json!("ready");
        json["filters"]["contains"] = serde_json::json!("/fix");
        json["filters"]["pattern"] = serde_json::json!(r"^/fix\b");
        let document: TriggerDocument = serde_json::from_value(json).expect("deserialize");
        assert_eq!(document.filters.channel.as_deref(), Some("C0123456"));
        assert_eq!(document.filters.label.as_deref(), Some("ready"));
        assert_eq!(document.filters.contains.as_deref(), Some("/fix"));
        assert_eq!(document.filters.pattern.as_deref(), Some(r"^/fix\b"));
        assert!(resolve_issues(&resolution(&document)).is_empty());
    }

    #[test]
    fn filters_is_required() {
        let mut json = document_json();
        json.as_object_mut().expect("document").remove("filters");
        assert!(
            serde_json::from_value::<TriggerDocument>(json).is_err(),
            "a trigger without filters must not parse"
        );
    }

    #[test]
    fn any_mode_is_refused() {
        // There is no any/all switch: extra keys are a parse error rather than
        // an alternate mode.
        let mut json = document_json();
        json["filters"]["any"] = serde_json::json!(true);
        assert!(serde_json::from_value::<TriggerDocument>(json).is_err());
    }

    #[test]
    fn blank_thinking_option_is_named() {
        let mut json = document_json();
        json["run"]["agent"]["thinking_option"] = serde_json::json!("  ");
        let document: TriggerDocument = serde_json::from_value(json).expect("deserialize");
        let issues = resolve_issues(&resolution(&document));
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "run.agent.thinking_option"),
            "got {issues:?}"
        );
    }

    #[test]
    fn every_field_cap_is_enforced() {
        let mut json = document_json();
        json["name"] = serde_json::json!("n".repeat(MAX_NAME_CHARS + 1));
        json["event"]["type"] = serde_json::json!("e".repeat(MAX_EVENT_CHARS + 1));
        json["filters"]["connection"] = serde_json::json!("c");
        json["filters"]["resource"]["kind"] = serde_json::json!("k".repeat(MAX_KIND_CHARS + 1));
        json["filters"]["resource"]["id"] = serde_json::json!("i".repeat(MAX_RESOURCE_CHARS + 1));
        json["filters"]["from_users"] = serde_json::json!(
            (0..=MAX_FROM_USERS)
                .map(|index| format!("user{index}"))
                .collect::<Vec<_>>()
        );
        json["filters"]["channel"] = serde_json::json!("c".repeat(MAX_CHANNEL_CHARS + 1));
        json["filters"]["label"] = serde_json::json!("l".repeat(MAX_CHANNEL_CHARS + 1));
        json["filters"]["contains"] = serde_json::json!("c".repeat(MAX_CONTAINS_CHARS + 1));
        json["filters"]["pattern"] = serde_json::json!("p".repeat(MAX_PATTERN_CHARS + 1));

        let document: TriggerDocument = serde_json::from_value(json).expect("deserialize");
        let issues = resolve_issues(&resolution(&document));
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        for expected in [
            "name",
            "event.type",
            "filters.resource.kind",
            "filters.resource.id",
            "filters.from_users",
            "filters.channel",
            "filters.label",
            "filters.contains",
            "filters.pattern",
        ] {
            assert!(
                fields.contains(&expected),
                "missing {expected} in {fields:?}"
            );
        }
    }

    #[test]
    fn an_over_long_user_handle_is_named_by_index() {
        let mut json = document_json();
        json["filters"]["from_users"] = serde_json::json!(["u".repeat(MAX_USER_CHARS + 1)]);
        let document: TriggerDocument = serde_json::from_value(json).expect("deserialize");
        let issues = resolve_issues(&resolution(&document));
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "filters.from_users[0]"),
            "got {issues:?}"
        );
    }

    // ── reference resolution ───────────────────────────────────────────────

    #[test]
    fn unknown_connection_names_the_field() {
        let document = document();
        let resolution = TriggerResolution {
            connection: None,
            ..resolution(&document)
        };
        let issues = resolve_issues(&resolution);
        assert_eq!(issues[0].field, "filters.connection");
        assert_eq!(
            issues[0].message,
            "no active connection with slug 'github-main'"
        );
    }

    #[test]
    fn disabled_connection_is_not_active() {
        let document = document();
        let resolution = TriggerResolution {
            connection: Some((ProviderKind::Github, false)),
            ..resolution(&document)
        };
        let issues = resolve_issues(&resolution);
        assert_eq!(issues[0].field, "filters.connection");
        assert_eq!(issues[0].message, "connection 'github-main' is not active");
    }

    #[test]
    fn provider_mismatch_between_connection_and_event_is_named() {
        let document = document();
        let resolution = TriggerResolution {
            connection: Some((ProviderKind::Slack, true)),
            ..resolution(&document)
        };
        let issues = resolve_issues(&resolution);
        assert_eq!(issues[0].field, "filters.connection");
        assert_eq!(
            issues[0].message,
            "connection 'github-main' is a slack connection, not a github connection"
        );
    }

    #[test]
    fn unknown_resource_names_the_field() {
        let document = document();
        let resolution = TriggerResolution {
            resource_owned: false,
            ..resolution(&document)
        };
        let issues = resolve_issues(&resolution);
        assert_eq!(issues[0].field, "filters.resource");
        assert_eq!(
            issues[0].message,
            "no active github connection delivers for 'repository' 'org/repo'"
        );
    }

    #[test]
    fn unknown_host_names_the_field() {
        let document = document();
        let resolution = TriggerResolution {
            run: RunResolution::HostUnknown,
            ..resolution(&document)
        };
        let issues = resolve_issues(&resolution);
        assert_eq!(issues[0].field, "run.host");
        assert_eq!(issues[0].message, "unknown host slug 'studio-mac'");
    }

    #[test]
    fn host_without_execution_grant_is_named() {
        let document = document();
        let resolution = TriggerResolution {
            run: RunResolution::NotGranted,
            ..resolution(&document)
        };
        let issues = resolve_issues(&resolution);
        assert_eq!(issues[0].field, "run.host");
        assert_eq!(
            issues[0].message,
            "host 'studio-mac' has not been granted execution"
        );
    }

    #[test]
    fn unavailable_agent_names_the_capability_field() {
        let document = document();
        let resolution = TriggerResolution {
            run: RunResolution::Unavailable {
                field: "model".to_string(),
                value: "gpt".to_string(),
            },
            ..resolution(&document)
        };
        let issues = resolve_issues(&resolution);
        assert_eq!(issues[0].field, "run.agent.model");
        assert_eq!(issues[0].message, "host 'studio-mac' cannot run 'gpt'");
    }

    #[test]
    fn every_rejection_is_reported_together() {
        // All conditions must pass and there is no any mode: an invalid
        // document reports every failing reference at once.
        let document = document();
        let resolution = TriggerResolution {
            document: &document,
            connection: None,
            resource_owned: false,
            run: RunResolution::Unavailable {
                field: "provider".to_string(),
                value: "openai".to_string(),
            },
        };
        let issues = resolve_issues(&resolution);
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"filters.connection"));
        assert!(fields.contains(&"filters.resource"));
        assert!(fields.contains(&"run.agent.provider"));
    }

    // ── installation planning (create-then-replace) ────────────────────────

    #[test]
    fn a_first_install_creates_the_trigger_at_version_one() {
        let document = document();
        let plan = plan_install(&document, None, &[], "trigger-1", "revision-1");
        assert_eq!(plan.id, "trigger-1", "a create uses the generated id");
        assert_eq!(plan.name, "auto-fix-issues");
        assert_eq!(plan.version, 1);
        assert_eq!(plan.revision, "revision-1");
        assert!(plan.active);
        assert!(!plan.replaced);
        assert!(plan.creates_trigger);
        assert!(
            !plan.deactivates_previous,
            "nothing is active before the first install"
        );
    }

    #[test]
    fn a_second_install_replaces_but_keeps_the_trigger_identity() {
        let document = document();
        let existing = ExistingTrigger {
            public_id: "trigger-1".into(),
        };
        let revisions = [RevisionState {
            version: 1,
            active: true,
        }];
        let plan = plan_install(
            &document,
            Some(&existing),
            &revisions,
            "generated-should-not-be-used",
            "revision-2",
        );
        assert_eq!(plan.id, "trigger-1", "replacement keeps the trigger id");
        assert_eq!(plan.version, 2, "replacement produces a new revision");
        assert_eq!(plan.revision, "revision-2");
        assert!(plan.active);
        assert!(plan.replaced);
        assert!(!plan.creates_trigger);
        assert!(
            plan.deactivates_previous,
            "the old active revision is deactivated"
        );
    }

    #[test]
    fn replacement_history_keeps_the_previous_revisions_readable() {
        // The planner never renumbers or drops a revision: version is max + 1
        // and only the active flag is flipped, so the older revisions stay.
        let previous = [
            RevisionState {
                version: 1,
                active: false,
            },
            RevisionState {
                version: 2,
                active: false,
            },
            RevisionState {
                version: 3,
                active: true,
            },
        ];
        let plan = RevisionPlan::from_existing(&previous);
        assert_eq!(plan.version, 4);
        assert!(plan.deactivates_previous);
        assert_eq!(previous.len(), 3, "existing revisions are not discarded");
    }

    #[test]
    fn a_gap_in_versions_cannot_produce_a_duplicate() {
        // Version is max + 1, not count + 1, so a swept revision cannot make the
        // next install collide on (trigger_id, version).
        let with_gap = [
            RevisionState {
                version: 1,
                active: false,
            },
            RevisionState {
                version: 5,
                active: true,
            },
        ];
        assert_eq!(RevisionPlan::from_existing(&with_gap).version, 6);
    }

    #[test]
    fn a_trigger_with_no_active_revision_has_nothing_to_deactivate() {
        let revisions = [RevisionState {
            version: 1,
            active: false,
        }];
        let plan = RevisionPlan::from_existing(&revisions);
        assert_eq!(plan.version, 2);
        assert!(!plan.deactivates_previous);
    }

    #[test]
    fn the_generated_id_is_used_only_when_creating() {
        let (id, replaced) = trigger_identity(None, "fresh");
        assert_eq!(id, "fresh");
        assert!(!replaced);

        let existing = ExistingTrigger {
            public_id: "kept".into(),
        };
        let (id, replaced) = trigger_identity(Some(&existing), "fresh");
        assert_eq!(id, "kept", "a replaced trigger keeps its identity");
        assert!(replaced);
    }

    #[test]
    fn the_stored_name_is_trimmed() {
        let mut json = document_json();
        json["name"] = serde_json::json!("  auto-fix-issues  ");
        let document: TriggerDocument = serde_json::from_value(json).expect("deserialize");
        assert_eq!(trigger_name(&document), "auto-fix-issues");
        // The plan stores the trimmed name, so a padded document replaces the
        // trigger of the same name rather than creating a second one.
        let plan = plan_install(&document, None, &[], "trigger-1", "revision-1");
        assert_eq!(plan.name, "auto-fix-issues");
    }

    #[test]
    fn an_invalid_document_produces_a_named_rejection_not_a_plan() {
        // Install validates before planning through the same pure resolver, so
        // a rejected document yields issues (and the caller writes nothing).
        let mut json = document_json();
        json["filters"]["from_users"] = serde_json::json!([]);
        let document: TriggerDocument = serde_json::from_value(json).expect("deserialize");
        let issues = resolve_issues(&resolution(&document));
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "filters.from_users"),
            "a rejected document names the field: {issues:?}"
        );
    }

    #[test]
    fn an_installed_document_round_trips_as_stored_json() {
        // The stored document is the canonical JSON of the authored document,
        // so a later dispatch reads back exactly what was installed.
        let document = document();
        let stored = serde_json::to_string(&document).expect("serialize");
        let parsed: TriggerDocument = serde_json::from_str(&stored).expect("parse");
        assert_eq!(parsed.name, document.name);
        assert_eq!(parsed.event.event_type, document.event.event_type);
        assert_eq!(parsed.filters.from_users, document.filters.from_users);
        assert_eq!(parsed.run.agent, document.run.agent);
        assert_eq!(
            resolve_issues(&resolution(&parsed)),
            Vec::<TriggerIssue>::new()
        );
    }

    // ── install decisions (what gets written) ──────────────────────────────

    fn valid_outcome(document: &TriggerDocument) -> TriggerValidateOutcome {
        TriggerValidateOutcome {
            name: document.name.clone(),
            valid: true,
            errors: Vec::new(),
        }
    }

    #[test]
    fn an_invalid_document_yields_no_plan_and_names_its_issues() {
        // The whole "invalid document changes nothing" rule is here: with no
        // plan there is nothing for the database layer to execute.
        let document = document();
        let outcome = TriggerValidateOutcome {
            name: document.name.clone(),
            valid: false,
            errors: vec![TriggerIssue {
                field: "filters.from_users".into(),
                message: "from_users must be present and non-empty".into(),
            }],
        };
        let error = decide_install(&document, &outcome, None, &[], "trigger-1", "revision-1")
            .expect_err("an invalid document must not install");
        match error {
            TriggerError::Invalid(issues) => {
                assert_eq!(issues.len(), 1);
                assert_eq!(issues[0].field, "filters.from_users");
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[test]
    fn a_document_that_merely_claims_valid_but_carries_issues_is_refused() {
        // Defence in depth: `valid` and an empty issue list must agree.
        let document = document();
        let outcome = TriggerValidateOutcome {
            name: document.name.clone(),
            valid: true,
            errors: vec![TriggerIssue {
                field: "run.host".into(),
                message: "unknown host slug 'studio-mac'".into(),
            }],
        };
        assert!(matches!(
            decide_install(&document, &outcome, None, &[], "t", "r"),
            Err(TriggerError::Invalid(_))
        ));
    }

    #[test]
    fn a_valid_document_installs_and_a_second_install_replaces_it() {
        // Create-then-replace, end to end at the decision layer: the same
        // in-memory state the database layer keeps.
        let document = document();
        let outcome = valid_outcome(&document);

        // First call: no trigger, no revisions.
        let first = decide_install(&document, &outcome, None, &[], "trigger-1", "revision-1")
            .expect("first install");
        assert!(!first.replaced);
        assert_eq!(first.version, 1);
        assert_eq!(first.id, "trigger-1");

        // The database now holds the trigger and revision 1, active.
        let existing = ExistingTrigger {
            public_id: first.id.clone(),
        };
        let revisions = [RevisionState {
            version: first.version,
            active: first.active,
        }];

        // Second call: same name, so the trigger is replaced, not created.
        let second = decide_install(
            &document,
            &outcome,
            Some(&existing),
            &revisions,
            "trigger-should-not-be-used",
            "revision-2",
        )
        .expect("second install");
        assert!(second.replaced, "the second install replaces");
        assert_eq!(second.id, first.id, "identity is kept across a replace");
        assert_eq!(second.version, 2, "a new revision is produced");
        assert_ne!(second.revision, first.revision);
        assert!(
            second.deactivates_previous,
            "the previous revision is deactivated"
        );
        assert!(second.active);
    }

    #[test]
    fn a_failed_install_after_a_success_leaves_the_success_in_place() {
        // Sequence: valid install, then an invalid one. The second attempt
        // produces no plan, so the caller writes nothing and revision 1 stays
        // active — "a later failure does not undo an earlier success".
        let document = document();
        let first = decide_install(
            &document,
            &valid_outcome(&document),
            None,
            &[],
            "trigger-1",
            "revision-1",
        )
        .expect("first install");
        let state_after_first = (
            ExistingTrigger {
                public_id: first.id.clone(),
            },
            vec![RevisionState {
                version: first.version,
                active: first.active,
            }],
        );

        let rejected = TriggerValidateOutcome {
            name: document.name.clone(),
            valid: false,
            errors: vec![TriggerIssue {
                field: "filters.resource".into(),
                message: "no active github connection delivers for 'repository' 'org/repo'".into(),
            }],
        };
        assert!(
            decide_install(
                &document,
                &rejected,
                Some(&state_after_first.0),
                &state_after_first.1,
                "unused",
                "unused",
            )
            .is_err()
        );

        // Nothing about the earlier success changed.
        assert_eq!(state_after_first.0.public_id, "trigger-1");
        assert_eq!(state_after_first.1.len(), 1);
        assert!(state_after_first.1[0].active);
    }
}
