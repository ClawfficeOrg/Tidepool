//! Dispatch (task 1.4.6).
//!
//! Dispatch turns one matched provider event, or one manual request, into a
//! durable run. It owns the three rules the run lifecycle depends on:
//!
//! - **Deduplication by delivery key.** One trigger never has two runs with the
//!   same key. An event retry and a manual resubmission return the existing run
//!   (`deduplicated: true`) instead of starting a second one. The key is the
//!   trigger's, not the pool's, so two triggers may see the same provider
//!   delivery without interfering.
//! - **Manual actor and revision checks.** The actor must be in the trigger's
//!   `from_users` allowlist, and an expected revision is refused once the
//!   project has moved past it.
//! - **Preflight before insert.** Every step's environment is resolved, and
//!   every host must be connected, *before* the run row exists. An offline host
//!   fails with the documented `daemon_not_connected` code and queues nothing.
//!
//! Creation snapshots the workflow, environments and prompt partials onto the
//! run, so restart recovery evaluates exactly what was dispatched.
//!
//! A manual dispatch deliberately promises deduplication, not exactly-once
//! delivery: the response reports which run a key names, and a caller that
//! loses the response can retry the same key safely.

use crate::entities::activity::{
    ContinuationPlan, ReceiptStatus, conversation_key_for, output_limits, plan_continuation,
};
use crate::entities::configuration_revision::ConfigFile;
use crate::entities::connection::ProviderKind;
use crate::entities::environment::Environment;
use crate::entities::project::Project;
use crate::entities::run::{InputError, ManualDispatchRequest, RunOrigin, RunStatus, RunView};
use crate::entities::workflow::{Workflow, WorkflowInput};
use crate::services::activity as activity_svc;
use crate::services::configuration as configuration_svc;
use crate::services::environments::{self, EnvironmentError};
use crate::services::hosts::normalize_slug;
use crate::services::projects::{self as project_svc, ProjectServiceError};
use crate::services::runs::{self, RunError};
use crate::services::triggers::{TriggerDocument, TriggerFilters};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use std::collections::BTreeMap;
use thiserror::Error;

/// Manifest file that names the bundle's prompt partials.
const MANIFEST_FILE: &str = "tidepool.json";

/// Domain errors for dispatch.
#[derive(Debug, Error)]
pub enum DispatchError {
    /// No project with that id in this organization.
    #[error("project not found")]
    ProjectNotFound,
    /// No installed, active trigger with that name.
    #[error("trigger not found")]
    TriggerNotFound,
    /// The actor is not in the trigger's allowlist.
    #[error("actor '{0}' is not allowed by the trigger")]
    ActorNotAllowed(String),
    /// The caller expected a revision that is no longer active.
    #[error("revision '{expected}' is no longer active")]
    RevisionNotActive {
        /// Revision the caller expected.
        expected: String,
    },
    /// The active revision has no workflow for the trigger.
    #[error("no workflow in the active revision starts trigger '{0}'")]
    NoWorkflow(String),
    /// The event or request is malformed.
    #[error("invalid dispatch: {0}")]
    Invalid(String),
    /// The manual input string is invalid.
    #[error(transparent)]
    Input(#[from] InputError),
    /// A run snapshot could not be encoded or decoded.
    #[error("run snapshot error: {0}")]
    Snapshot(String),
    /// An environment could not be resolved.
    #[error(transparent)]
    Environment(#[from] EnvironmentError),
    /// The run lifecycle failed.
    #[error(transparent)]
    Run(#[from] RunError),
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

impl DispatchError {
    /// Stable problem code for the hub API.
    pub fn code(&self) -> &'static str {
        match self {
            DispatchError::ProjectNotFound | DispatchError::TriggerNotFound => "not_found",
            DispatchError::ActorNotAllowed(_) => "forbidden",
            DispatchError::RevisionNotActive { .. } => "conflict",
            DispatchError::NoWorkflow(_) => "configuration_invalid",
            DispatchError::Invalid(_) | DispatchError::Input(_) => "validation_failed",
            DispatchError::Environment(error) => error.code(),
            DispatchError::Run(error) => error.code(),
            DispatchError::Snapshot(_) | DispatchError::Database(_) => "internal_error",
        }
    }
}

/// One normalized provider event ready for matching.
#[derive(Debug, Clone)]
pub struct InboundEvent {
    /// Provider that delivered the event.
    pub provider: ProviderKind,
    /// Internal event type, for example `issue.comment_created`.
    pub event_type: String,
    /// Connection slug the event arrived through.
    pub connection: String,
    /// Provider resource kind.
    pub resource_kind: String,
    /// Provider resource id (repository, workspace, guild, channel).
    pub resource_id: String,
    /// Human handle of the user who caused the event.
    pub actor: String,
    /// Channel or thread, when the provider has one.
    pub channel: Option<String>,
    /// Label, when the event carries one.
    pub label: Option<String>,
    /// Text the filters inspect.
    pub content: String,
    /// Text the workflow receives as `request`.
    pub text: String,
    /// Stable provider delivery id; the run's dedup key.
    pub delivery_key: String,
}

impl InboundEvent {
    /// Validate the fields dispatch relies on.
    pub fn validate(&self) -> Result<(), DispatchError> {
        if self.event_type.trim().is_empty() {
            return Err(DispatchError::Invalid("event type is required".into()));
        }
        if self.delivery_key.trim().is_empty() {
            return Err(DispatchError::Invalid("delivery key is required".into()));
        }
        if self.delivery_key.chars().count() > 255 {
            return Err(DispatchError::Invalid(
                "delivery key must be at most 255 characters".into(),
            ));
        }
        Ok(())
    }
}

/// One installed trigger's active document.
#[derive(Debug, Clone)]
struct ActiveTrigger {
    id: u64,
    public_id: String,
    name: String,
    document: TriggerDocument,
}

/// Everything one run needs at creation.
struct NewRun<'a> {
    organization_id: u64,
    project: &'a Project,
    trigger: &'a ActiveTrigger,
    revision_id: Option<&'a str>,
    workflow: &'a Workflow,
    environments: &'a [Environment],
    partials: &'a BTreeMap<String, String>,
    inputs: &'a BTreeMap<String, String>,
    request_text: &'a str,
    origin: RunOrigin,
    actor: Option<&'a str>,
    delivery_key: &'a str,
    provider: Option<&'a str>,
    receipt_public_id: Option<&'a str>,
    conversation_key: Option<&'a str>,
    continuation: ContinuationPlan,
}

/// One step resolved before the run row exists.
#[derive(Debug, Clone)]
struct PreparedStep {
    step_id: String,
    position: u32,
    execution_id: String,
    environment: String,
    host_public_id: String,
    directory: String,
    branch: Option<String>,
}

/// Dispatch one manual request.
pub async fn dispatch_manual(
    pool: &MySqlPool,
    organization_id: u64,
    request: &ManualDispatchRequest,
) -> Result<runs::DispatchOutcome, DispatchError> {
    request.validate()?;
    let project = find_project(pool, organization_id, &request.project).await?;
    let trigger = find_active_trigger(pool, project.id, &request.trigger)
        .await?
        .ok_or(DispatchError::TriggerNotFound)?;
    if !actor_allowed(&trigger.document.filters, &request.actor) {
        return Err(DispatchError::ActorNotAllowed(request.actor.clone()));
    }
    // A retry with a known key returns its run before input or revision
    // validation: deduplication must survive a configuration change.
    if let Some(outcome) =
        existing_run(pool, organization_id, trigger.id, &request.delivery_key).await?
    {
        return Ok(outcome);
    }

    let revision = configuration_svc::get_active_revision(pool, organization_id, &request.project)
        .await
        .map_err(map_configuration_error)?
        .ok_or_else(|| DispatchError::RevisionNotActive {
            expected: request
                .revision
                .clone()
                .unwrap_or_else(|| "(active revision)".into()),
        })?;
    if let Some(expected) = request.revision.as_deref()
        && expected != revision.id
    {
        return Err(DispatchError::RevisionNotActive {
            expected: expected.to_string(),
        });
    }

    let files = config_files(&revision.files);
    let workflows = configuration_svc::parse_workflows(&files).map_err(map_configuration_error)?;
    let environments =
        configuration_svc::parse_environments(&files).map_err(map_configuration_error)?;
    let partials = partial_texts(&files)?;
    let workflow = workflows
        .into_iter()
        .find(|workflow| workflow.trigger == trigger.name)
        .ok_or_else(|| DispatchError::NoWorkflow(trigger.name.clone()))?;
    let parsed = crate::entities::run::parse_input_string(&request.input, &workflow.inputs)?;

    create_run(
        pool,
        NewRun {
            organization_id,
            project: &project,
            trigger: &trigger,
            revision_id: Some(&revision.id),
            workflow: &workflow,
            environments: &environments,
            partials: &partials,
            inputs: &parsed.inputs,
            request_text: &parsed.request_text,
            origin: RunOrigin::Manual,
            actor: Some(request.actor.trim()),
            delivery_key: &request.delivery_key,
            provider: None,
            receipt_public_id: None,
            conversation_key: None,
            continuation: ContinuationPlan::created(),
        },
    )
    .await
}

/// Dispatch one provider event to every trigger in the project it matches.
///
/// A receipt is written before matching, so a delivery that matches nothing
/// still appears in the activity log. No match is an empty result, not an
/// error. A duplicate delivery key returns the runs the first delivery
/// produced and writes nothing else.
pub async fn dispatch_event(
    pool: &MySqlPool,
    organization_id: u64,
    project_public_id: &str,
    event: &InboundEvent,
) -> Result<Vec<runs::DispatchOutcome>, DispatchError> {
    event.validate()?;
    let project = find_project(pool, organization_id, project_public_id).await?;
    let delivery = activity_svc::DeliveryFacts {
        provider: event.provider,
        connection: &event.connection,
        delivery_key: &event.delivery_key,
        event_type: &event.event_type,
        action: None,
        resource_kind: &event.resource_kind,
        resource_id: &event.resource_id,
        actor: &event.actor,
        channel: event.channel.as_deref(),
    };
    let receipt = activity_svc::begin_receipt(
        pool,
        organization_id,
        Some(project.id),
        Some(&project.public_id),
        &delivery,
    )
    .await
    .map_err(map_activity_error)?;

    // A receipt still `received` means an earlier attempt crashed before matching
    // finished. Re-match it: create_run dedupes on (trigger, delivery_key), so
    // runs that already exist are returned instead of being created twice.
    if receipt.duplicate && !matches!(receipt.receipt.status, ReceiptStatus::Received) {
        let mut outcomes = Vec::new();
        for public_id in
            activity_svc::receipt_run_ids(&receipt.receipt).map_err(map_activity_error)?
        {
            if let Some(run) = runs::find_run(pool, organization_id, &public_id).await? {
                outcomes.push(runs::DispatchOutcome {
                    run: runs::run_view(pool, &run).await?,
                    deduplicated: true,
                });
            }
        }
        return Ok(outcomes);
    }

    let receipt_public_id = receipt.receipt.public_id.clone();
    match dispatch_event_matched(pool, organization_id, &project, event, &receipt_public_id).await {
        Ok(outcomes) => {
            let run_ids: Vec<String> = outcomes
                .iter()
                .map(|outcome| outcome.run.id.clone())
                .collect();
            let status = if run_ids.is_empty() {
                ReceiptStatus::Unmatched
            } else {
                ReceiptStatus::Matched
            };
            activity_svc::finish_receipt(
                pool,
                organization_id,
                &receipt_public_id,
                status,
                run_ids.len(),
                &run_ids,
            )
            .await
            .map_err(map_activity_error)?;
            Ok(outcomes)
        }
        Err(error) => {
            if let Err(mark) = activity_svc::finish_receipt(
                pool,
                organization_id,
                &receipt_public_id,
                ReceiptStatus::Rejected,
                0,
                &[],
            )
            .await
            {
                tracing::warn!(%mark, "failed to mark delivery receipt rejected");
            }
            Err(error)
        }
    }
}

async fn dispatch_event_matched(
    pool: &MySqlPool,
    organization_id: u64,
    project: &Project,
    event: &InboundEvent,
    receipt_public_id: &str,
) -> Result<Vec<runs::DispatchOutcome>, DispatchError> {
    let triggers = list_active_triggers(pool, project.id).await?;
    let matching: Vec<ActiveTrigger> = triggers
        .into_iter()
        .filter(|trigger| trigger_matches(&trigger.document, event))
        .collect();
    if matching.is_empty() {
        return Ok(Vec::new());
    }

    let revision =
        configuration_svc::get_active_revision(pool, organization_id, &project.public_id)
            .await
            .map_err(map_configuration_error)?
            .ok_or_else(|| DispatchError::NoWorkflow(event.event_type.clone()))?;
    let files = config_files(&revision.files);
    let workflows = configuration_svc::parse_workflows(&files).map_err(map_configuration_error)?;
    let environments =
        configuration_svc::parse_environments(&files).map_err(map_configuration_error)?;
    let partials = partial_texts(&files)?;
    let conversation_key = conversation_key_for(
        event.provider,
        &event.resource_kind,
        &event.resource_id,
        event.channel.as_deref(),
    );

    let mut outcomes = Vec::with_capacity(matching.len());
    for trigger in matching {
        let workflow = workflows
            .iter()
            .find(|workflow| workflow.trigger == trigger.name)
            .ok_or_else(|| DispatchError::NoWorkflow(trigger.name.clone()))?;
        let parsed = crate::entities::run::parse_input_string("", &workflow.inputs)?;
        let continuation = match conversation_key.as_deref() {
            Some(key) => {
                let existing =
                    activity_svc::find_conversation(pool, organization_id, trigger.id, key)
                        .await
                        .map_err(map_activity_error)?;
                plan_continuation(existing.as_ref())
            }
            None => ContinuationPlan::created(),
        };
        outcomes.push(
            create_run(
                pool,
                NewRun {
                    organization_id,
                    project,
                    trigger: &trigger,
                    revision_id: Some(&revision.id),
                    workflow,
                    environments: &environments,
                    partials: &partials,
                    inputs: &parsed.inputs,
                    request_text: &event.text,
                    origin: RunOrigin::Event,
                    actor: Some(event.actor.trim()),
                    delivery_key: &event.delivery_key,
                    provider: Some(event.provider.as_str()),
                    receipt_public_id: Some(receipt_public_id),
                    conversation_key: conversation_key.as_deref(),
                    continuation,
                },
            )
            .await?,
        );
    }
    Ok(outcomes)
}

fn map_activity_error(error: activity_svc::ActivityError) -> DispatchError {
    match error {
        activity_svc::ActivityError::Database(error) => DispatchError::Database(error),
        activity_svc::ActivityError::Filter(error) => DispatchError::Invalid(error.to_string()),
        activity_svc::ActivityError::NotFound => DispatchError::ProjectNotFound,
        activity_svc::ActivityError::Invalid(message) => DispatchError::Snapshot(message),
        activity_svc::ActivityError::Run(error) => DispatchError::Run(error),
    }
}

async fn find_project(
    pool: &MySqlPool,
    organization_id: u64,
    public_id: &str,
) -> Result<Project, DispatchError> {
    project_svc::find_project(pool, organization_id, public_id)
        .await
        .map_err(|error| match error {
            ProjectServiceError::NotFound => DispatchError::ProjectNotFound,
            ProjectServiceError::Database(error) => DispatchError::Database(error),
            ProjectServiceError::Invalid(message) => DispatchError::Invalid(message),
            ProjectServiceError::SlugConflict(_) => DispatchError::ProjectNotFound,
        })?
        .ok_or(DispatchError::ProjectNotFound)
}

async fn find_active_trigger(
    pool: &MySqlPool,
    project_id: u64,
    name: &str,
) -> Result<Option<ActiveTrigger>, DispatchError> {
    let row: Option<(u64, String, String, String)> = sqlx::query_as(
        "SELECT t.id, t.public_id, t.name, r.document \
         FROM triggers t \
         JOIN trigger_revisions r ON r.trigger_id = t.id AND r.active = 1 \
         WHERE t.project_id = ? AND t.name = ? \
         ORDER BY r.version DESC LIMIT 1",
    )
    .bind(project_id)
    .bind(name.trim())
    .fetch_optional(pool)
    .await
    .map_err(DispatchError::Database)?;
    row.map(parse_trigger_row).transpose()
}

async fn list_active_triggers(
    pool: &MySqlPool,
    project_id: u64,
) -> Result<Vec<ActiveTrigger>, DispatchError> {
    let rows: Vec<(u64, String, String, String)> = sqlx::query_as(
        "SELECT t.id, t.public_id, t.name, r.document \
         FROM triggers t \
         JOIN trigger_revisions r ON r.trigger_id = t.id AND r.active = 1 \
         WHERE t.project_id = ? \
         ORDER BY t.id ASC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(DispatchError::Database)?;
    rows.into_iter().map(parse_trigger_row).collect()
}

fn parse_trigger_row(row: (u64, String, String, String)) -> Result<ActiveTrigger, DispatchError> {
    let (id, public_id, name, document) = row;
    let document = serde_json::from_str(&document).map_err(|error| {
        DispatchError::Snapshot(format!("trigger document is not JSON: {error}"))
    })?;
    Ok(ActiveTrigger {
        id,
        public_id,
        name,
        document,
    })
}

fn config_files(
    files: &[crate::entities::configuration_revision::RevisionFileView],
) -> Vec<ConfigFile> {
    files
        .iter()
        .map(|file| ConfigFile {
            path: file.path.clone(),
            content: file.content.clone(),
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct PartialManifest {
    #[serde(default)]
    partials: Vec<PartialDeclaration>,
}

#[derive(Debug, Deserialize)]
struct PartialDeclaration {
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
}

/// Map declared partial names to their authored text from the bundle.
fn partial_texts(files: &[ConfigFile]) -> Result<BTreeMap<String, String>, DispatchError> {
    let Some(manifest) = files.iter().find(|file| file.path == MANIFEST_FILE) else {
        return Err(DispatchError::Snapshot(format!(
            "configuration bundle is missing '{MANIFEST_FILE}'"
        )));
    };
    let manifest: PartialManifest = serde_json::from_str(&manifest.content)
        .map_err(|error| DispatchError::Snapshot(format!("manifest is not JSON: {error}")))?;
    let mut partials = BTreeMap::new();
    for partial in manifest.partials {
        if let Some(file) = files.iter().find(|file| file.path == partial.path) {
            partials.insert(partial.name, file.content.clone());
        }
    }
    Ok(partials)
}

async fn existing_run(
    pool: &MySqlPool,
    organization_id: u64,
    trigger_id: u64,
    delivery_key: &str,
) -> Result<Option<runs::DispatchOutcome>, DispatchError> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT public_id FROM runs WHERE trigger_id = ? AND delivery_key = ?")
            .bind(trigger_id)
            .bind(delivery_key)
            .fetch_optional(pool)
            .await
            .map_err(DispatchError::Database)?;
    let Some(public_id) = existing else {
        return Ok(None);
    };
    let run = runs::find_run(pool, organization_id, &public_id)
        .await?
        .ok_or(DispatchError::ProjectNotFound)?;
    Ok(Some(runs::DispatchOutcome {
        run: runs::run_view(pool, &run).await?,
        deduplicated: true,
    }))
}

async fn create_run(
    pool: &MySqlPool,
    new: NewRun<'_>,
) -> Result<runs::DispatchOutcome, DispatchError> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT public_id FROM runs WHERE trigger_id = ? AND delivery_key = ?")
            .bind(new.trigger.id)
            .bind(new.delivery_key)
            .fetch_optional(pool)
            .await
            .map_err(DispatchError::Database)?;
    if let Some(public_id) = existing {
        let run = runs::find_run(pool, new.organization_id, &public_id)
            .await?
            .ok_or(DispatchError::ProjectNotFound)?;
        return Ok(runs::DispatchOutcome {
            run: runs::run_view(pool, &run).await?,
            deduplicated: true,
        });
    }

    let run_public_id = uuid::Uuid::new_v4().to_string();
    let prepared = prepare_steps(
        pool,
        new.organization_id,
        new.environments,
        new.workflow,
        new.inputs,
        &run_public_id,
    )
    .await?;

    let workflow_json = serde_json::to_string(new.workflow)
        .map_err(|error| DispatchError::Snapshot(error.to_string()))?;
    let environments_json = serde_json::to_string(new.environments)
        .map_err(|error| DispatchError::Snapshot(error.to_string()))?;
    let partials_json = serde_json::to_string(new.partials)
        .map_err(|error| DispatchError::Snapshot(error.to_string()))?;
    let inputs_json = serde_json::to_string(new.inputs)
        .map_err(|error| DispatchError::Snapshot(error.to_string()))?;
    let output_limits =
        output_limits(new.workflow).map_err(|error| DispatchError::Invalid(error.to_string()))?;
    let output_limits_json = serde_json::to_string(&output_limits)
        .map_err(|error| DispatchError::Snapshot(error.to_string()))?;
    let now = Utc::now();
    let deadline = runs::deadline_at(now, new.workflow);

    let mut tx = pool.begin().await.map_err(DispatchError::Database)?;
    let insert_result = sqlx::query(
        "INSERT INTO runs (public_id, organization_id, project_id, project_public_id, \
                           trigger_id, trigger_public_id, trigger_name, workflow_name, \
                           revision_id, provider, receipt_public_id, conversation_key, \
                           continuation, steered, output_limits_json, \
                           origin, actor, delivery_key, request_text, inputs_json, \
                           workflow_json, environments_json, partials_json, status, deadline_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
                 'pending', ?)",
    )
    .bind(&run_public_id)
    .bind(new.organization_id)
    .bind(new.project.id)
    .bind(&new.project.public_id)
    .bind(new.trigger.id)
    .bind(&new.trigger.public_id)
    .bind(&new.trigger.name)
    .bind(&new.workflow.name)
    .bind(new.revision_id)
    .bind(new.provider)
    .bind(new.receipt_public_id)
    .bind(new.conversation_key)
    .bind(new.continuation.mode.as_str())
    .bind(if new.continuation.steer { 1_i32 } else { 0_i32 })
    .bind(&output_limits_json)
    .bind(new.origin.as_str())
    .bind(new.actor)
    .bind(new.delivery_key)
    .bind(new.request_text)
    .bind(&inputs_json)
    .bind(&workflow_json)
    .bind(&environments_json)
    .bind(&partials_json)
    .bind(deadline)
    .execute(&mut *tx)
    .await;
    if let Err(error) = insert_result {
        // Two dispatches of the same key can both pass the read above; the
        // unique key is the real arbiter, and the loser returns the winner.
        if error
            .as_database_error()
            .is_some_and(|error| error.is_unique_violation())
        {
            drop(tx);
            if let Some(outcome) =
                existing_run(pool, new.organization_id, new.trigger.id, new.delivery_key).await?
            {
                return Ok(outcome);
            }
        }
        return Err(DispatchError::Database(error));
    }
    let run_id: u64 = sqlx::query_scalar("SELECT id FROM runs WHERE public_id = ?")
        .bind(&run_public_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(DispatchError::Database)?;

    for step in &prepared {
        sqlx::query(
            "INSERT INTO run_steps (public_id, run_id, step_id, position, status, \
                                    execution_id, environment, host_public_id, directory, branch) \
             VALUES (?, ?, ?, ?, 'pending', ?, ?, ?, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(run_id)
        .bind(&step.step_id)
        .bind(step.position)
        .bind(&step.execution_id)
        .bind(&step.environment)
        .bind(&step.host_public_id)
        .bind(&step.directory)
        .bind(&step.branch)
        .execute(&mut *tx)
        .await
        .map_err(DispatchError::Database)?;
    }
    tx.commit().await.map_err(DispatchError::Database)?;

    if let Some(conversation_key) = new.conversation_key {
        // Track this arrival's own execution. Each run creates fresh executions,
        // so pointing at the reused id would pin the conversation to arrival 1.
        let agent_execution_id = prepared.first().map(|step| step.execution_id.as_str());
        activity_svc::record_conversation(
            pool,
            new.organization_id,
            new.project.id,
            new.trigger.id,
            conversation_key,
            agent_execution_id,
            &run_public_id,
            true,
            &output_limits_json,
        )
        .await
        .map_err(map_activity_error)?;
    }

    match runs::advance_run(pool, &run_public_id).await {
        Ok(_) => {}
        Err(error) if error.code() == "daemon_not_connected" => {
            // Preflight made every host's presence explicit. If the host left
            // between preflight and the first intent, remove the run so the
            // documented "offline queues nothing" rule still holds.
            sqlx::query("DELETE FROM runs WHERE public_id = ?")
                .bind(&run_public_id)
                .execute(pool)
                .await
                .map_err(DispatchError::Database)?;
            return Err(DispatchError::Environment(
                EnvironmentError::DaemonNotConnected {
                    slug: "host went offline before dispatch".into(),
                },
            ));
        }
        Err(error) => return Err(DispatchError::Run(error)),
    }

    let run = runs::find_run(pool, new.organization_id, &run_public_id)
        .await?
        .ok_or(DispatchError::ProjectNotFound)?;
    Ok(runs::DispatchOutcome {
        run: runs::run_view(pool, &run).await?,
        deduplicated: false,
    })
}

/// Resolve every step's environment before any row is written.
async fn prepare_steps(
    pool: &MySqlPool,
    organization_id: u64,
    environments: &[Environment],
    workflow: &Workflow,
    inputs: &BTreeMap<String, String>,
    run_public_id: &str,
) -> Result<Vec<PreparedStep>, DispatchError> {
    let mut prepared = Vec::with_capacity(workflow.steps.len());
    for (position, step) in workflow.steps.iter().enumerate() {
        let selector = step.environment.as_ref().ok_or_else(|| {
            DispatchError::Invalid(format!("step '{}' has no environment", step.id))
        })?;
        let execution_id = runs::execution_id_for(run_public_id, &step.id);
        let target = environments::require_for_dispatch(
            pool,
            organization_id,
            environments,
            selector,
            inputs,
            &execution_id,
        )
        .await?;
        prepared.push(PreparedStep {
            step_id: step.id.clone(),
            position: position as u32,
            execution_id,
            environment: target.environment,
            host_public_id: target.host_public_id,
            directory: target.directory,
            branch: target.branch,
        });
    }
    Ok(prepared)
}

/// True when every contributing predicate of the trigger allows `event`.
pub fn trigger_matches(document: &TriggerDocument, event: &InboundEvent) -> bool {
    if document.event.provider != event.provider || document.event.event_type != event.event_type {
        return false;
    }
    let filters = &document.filters;
    if normalize_slug(&filters.connection) != normalize_slug(&event.connection) {
        return false;
    }
    if normalize_slug(&filters.resource.kind) != normalize_slug(&event.resource_kind)
        || !filters
            .resource
            .id
            .trim()
            .eq_ignore_ascii_case(event.resource_id.trim())
    {
        return false;
    }
    if !actor_allowed(filters, &event.actor) {
        return false;
    }
    if let Some(channel) = filters.channel.as_deref()
        && !event
            .channel
            .as_deref()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(channel))
    {
        return false;
    }
    if let Some(label) = filters.label.as_deref()
        && !event
            .label
            .as_deref()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(label))
    {
        return false;
    }
    if let Some(contains) = filters.contains.as_deref()
        && !event.content.contains(contains)
    {
        return false;
    }
    if let Some(pattern) = filters.pattern.as_deref()
        && !glob_matches(pattern, &event.content)
    {
        return false;
    }
    true
}

/// True when `actor` is in the trigger's allowlist (case-insensitive).
pub fn actor_allowed(filters: &TriggerFilters, actor: &str) -> bool {
    let actor = actor.trim();
    !actor.is_empty()
        && filters
            .from_users
            .iter()
            .any(|allowed| allowed.trim().eq_ignore_ascii_case(actor))
}

/// Match a `*`/`?` pattern against text.
///
/// The trigger schema has no regex dependency; the documented guard language
/// is a glob: `*` matches any run of characters and `?` exactly one. Everything
/// else is literal, so an authored pattern cannot become a denial-of-service
/// regex.
pub fn glob_matches(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut pattern_at, mut text_at) = (0usize, 0usize);
    let (mut star, mut star_text) = (None, 0usize);

    while text_at < text.len() {
        if pattern_at < pattern.len()
            && (pattern[pattern_at] == '?' || pattern[pattern_at] == text[text_at])
        {
            pattern_at += 1;
            text_at += 1;
        } else if pattern_at < pattern.len() && pattern[pattern_at] == '*' {
            star = Some(pattern_at);
            pattern_at += 1;
            star_text = text_at;
        } else if let Some(star_at) = star {
            pattern_at = star_at + 1;
            star_text += 1;
            text_at = star_text;
        } else {
            return false;
        }
    }
    while pattern_at < pattern.len() && pattern[pattern_at] == '*' {
        pattern_at += 1;
    }
    pattern_at == pattern.len()
}

/// Validate that a step's declared inputs are satisfiable without an input
/// string. Exposed so event dispatch can report it before creating anything.
pub fn default_inputs(inputs: &[WorkflowInput]) -> Result<BTreeMap<String, String>, InputError> {
    Ok(crate::entities::run::parse_input_string("", inputs)?.inputs)
}

fn map_configuration_error(error: configuration_svc::ConfigurationError) -> DispatchError {
    match error {
        configuration_svc::ConfigurationError::Database(error) => DispatchError::Database(error),
        configuration_svc::ConfigurationError::ProjectNotFound => DispatchError::ProjectNotFound,
        other => DispatchError::Snapshot(other.to_string()),
    }
}

/// A manual dispatch response body.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualDispatchResponse {
    /// The run the key names.
    pub run: RunView,
    /// True when the key already named this run.
    pub deduplicated: bool,
}

impl From<runs::DispatchOutcome> for ManualDispatchResponse {
    fn from(outcome: runs::DispatchOutcome) -> Self {
        Self {
            run: outcome.run,
            deduplicated: outcome.deduplicated,
        }
    }
}

/// True when the run reached a terminal status (small helper for callers).
pub fn run_finished(run: &RunView) -> bool {
    matches!(
        run.status,
        RunStatus::Succeeded | RunStatus::Failed | RunStatus::Interrupted
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::connection::ProviderKind;
    use crate::entities::host_capability::AgentSelection;
    use crate::services::triggers::{ResourceFilter, TriggerEvent, TriggerRun};
    use serde_json::json;

    fn document(filters: TriggerFilters) -> TriggerDocument {
        TriggerDocument {
            name: "auto-fix-issues".into(),
            event: TriggerEvent {
                provider: ProviderKind::Github,
                event_type: "issue.comment_created".into(),
            },
            filters,
            run: TriggerRun {
                host: "studio-mac".into(),
                agent: AgentSelection {
                    provider: "anthropic".into(),
                    model: "claude".into(),
                    execution_mode: "default".into(),
                    thinking_option: None,
                },
            },
        }
    }

    fn filters() -> TriggerFilters {
        TriggerFilters {
            connection: "github-main".into(),
            resource: ResourceFilter {
                kind: "repository".into(),
                id: "Org/Repo".into(),
            },
            from_users: vec!["Alice".into(), "bob".into()],
            channel: None,
            label: None,
            contains: None,
            pattern: None,
        }
    }

    fn event() -> InboundEvent {
        InboundEvent {
            provider: ProviderKind::Github,
            event_type: "issue.comment_created".into(),
            connection: "github-main".into(),
            resource_kind: "repository".into(),
            resource_id: "org/repo".into(),
            actor: "alice".into(),
            channel: None,
            label: None,
            content: "please fix the failing test".into(),
            text: "please fix the failing test".into(),
            delivery_key: "delivery-1".into(),
        }
    }

    #[test]
    fn every_filter_is_a_conjunction() {
        let trigger = document(filters());
        assert!(trigger_matches(&trigger, &event()));

        let mut wrong_actor = event();
        wrong_actor.actor = "mallory".into();
        assert!(!trigger_matches(&trigger, &wrong_actor));

        let mut wrong_repo = event();
        wrong_repo.resource_id = "org/other".into();
        assert!(!trigger_matches(&trigger, &wrong_repo));

        let mut wrong_event = event();
        wrong_event.event_type = "issue.opened".into();
        assert!(!trigger_matches(&trigger, &wrong_event));

        let mut wrong_connection = event();
        wrong_connection.connection = "github-other".into();
        assert!(!trigger_matches(&trigger, &wrong_connection));
    }

    #[test]
    fn optional_channel_label_contains_and_pattern_all_must_pass() {
        let mut filters = filters();
        filters.channel = Some("C123".into());
        filters.label = Some("needs-fix".into());
        filters.contains = Some("failing".into());
        filters.pattern = Some("please*test".into());
        let trigger = document(filters);

        let mut event = event();
        event.channel = Some("c123".into());
        event.label = Some("NEEDS-FIX".into());
        assert!(trigger_matches(&trigger, &event));

        event.label = Some("other".into());
        assert!(!trigger_matches(&trigger, &event));
    }

    #[test]
    fn the_allowlist_is_case_insensitive_and_never_empty_in_practice() {
        let filters = filters();
        assert!(actor_allowed(&filters, "alice"));
        assert!(actor_allowed(&filters, "BOB"));
        assert!(!actor_allowed(&filters, ""));
        assert!(!actor_allowed(&filters, "mallory"));
    }

    #[test]
    fn glob_patterns_are_literal_except_for_star_and_question() {
        assert!(glob_matches("please*test", "please fix the failing test"));
        assert!(glob_matches("bug?", "bugs"));
        assert!(!glob_matches("bug?", "bug"));
        assert!(glob_matches("*", "anything"));
        assert!(glob_matches(".*", ".*"));
        assert!(!glob_matches("a*b", "ac"));
        assert!(glob_matches("a*b*", "axxbxx"));
    }

    #[test]
    fn event_validation_names_a_missing_key() {
        let mut event = event();
        event.delivery_key = "  ".into();
        assert!(matches!(event.validate(), Err(DispatchError::Invalid(_))));
        event.delivery_key = "x".repeat(256);
        assert!(matches!(event.validate(), Err(DispatchError::Invalid(_))));
    }

    #[test]
    fn event_dispatch_defaults_are_applied_to_declared_inputs() {
        let inputs = vec![WorkflowInput {
            name: "route".into(),
            required: false,
            choices: vec!["bug".into()],
            default: Some("bug".into()),
        }];
        let defaults = default_inputs(&inputs).expect("defaults");
        assert_eq!(defaults.get("route").map(String::as_str), Some("bug"));
    }

    #[test]
    fn dispatch_error_codes_are_documented() {
        assert_eq!(DispatchError::TriggerNotFound.code(), "not_found");
        assert_eq!(
            DispatchError::ActorNotAllowed("mallory".into()).code(),
            "forbidden"
        );
        assert_eq!(
            DispatchError::RevisionNotActive {
                expected: "r1".into()
            }
            .code(),
            "conflict"
        );
        assert_eq!(
            DispatchError::Environment(EnvironmentError::DaemonNotConnected {
                slug: "studio-mac".into()
            })
            .code(),
            "daemon_not_connected"
        );
    }

    #[test]
    fn a_manual_response_reports_deduplication() {
        let response = ManualDispatchResponse {
            run: RunView {
                id: "run-1".into(),
                project: "project-1".into(),
                trigger: "trigger-1".into(),
                trigger_name: "auto-fix-issues".into(),
                workflow: "triage".into(),
                origin: RunOrigin::Manual,
                actor: Some("alice".into()),
                delivery_key: "delivery-1".into(),
                status: RunStatus::Running,
                request_text: "fix it".into(),
                inputs: BTreeMap::new(),
                deadline_at: None,
                started_at: None,
                completed_at: None,
                created_at: "2026-10-21T00:00:00Z".parse().expect("timestamp"),
                steps: Vec::new(),
            },
            deduplicated: true,
        };
        let json = serde_json::to_value(response).expect("serialize");
        assert_eq!(json["deduplicated"], true);
        assert_eq!(json["run"]["deliveryKey"], "delivery-1");
        assert_eq!(json["run"]["triggerName"], "auto-fix-issues");
    }

    #[test]
    fn the_manifest_partials_map_names_to_files() {
        let files = vec![
            ConfigFile {
                path: MANIFEST_FILE.into(),
                content: json!({
                    "partials": [{ "name": "system", "path": "prompts/system.md" }]
                })
                .to_string(),
            },
            ConfigFile {
                path: "prompts/system.md".into(),
                content: "Be careful.".into(),
            },
        ];
        let partials = partial_texts(&files).expect("partials");
        assert_eq!(
            partials.get("system").map(String::as_str),
            Some("Be careful.")
        );
    }
}
