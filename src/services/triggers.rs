//! Trigger schema and validation (task 1.4.2).
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

use crate::entities::connection::{ConnectionStatus, ProviderKind};
use crate::entities::host_capability::AgentSelection;
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
#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerEvent {
    /// Provider kind (`github`, `slack`, `discord`).
    pub provider: ProviderKind,
    /// Internal event type (for example `issue.comment_created`).
    #[serde(rename = "type")]
    pub event_type: String,
}

/// Filters that gate which events may start the run.
#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceFilter {
    /// Resource kind (`repository`, `workspace`, `guild`, `channel`).
    pub kind: String,
    /// Provider-side id (for example `org/repo` or a workspace id).
    pub id: String,
}

/// The run an allowed event starts: a host and the agent to run on it.
#[derive(Debug, Clone, Deserialize)]
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

/// Domain errors for trigger validation.
#[derive(Debug, Error)]
pub enum TriggerError {
    /// Database failure while resolving a reference.
    #[error("database error")]
    Database(#[source] sqlx::Error),
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
}
