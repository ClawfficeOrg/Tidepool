//! Run lifecycle (task 1.4.6).
//!
//! A run evaluates its workflow's steps one at a time. For each step the
//! lifecycle decides, from the stored snapshot and the outputs of earlier
//! steps, whether the condition passes (skip without failing the run), whether
//! the workflow deadline has passed (fail and stop), and otherwise sends one
//! create intent to the step's host.
//!
//! **Intent idempotency is the core rule.** The execution id for a step is
//! derived deterministically from the run's public id and the step id, so a
//! lost create response, a reconnect or a pool restart resends the *same*
//! `create_agent` with the *same* execution id. [`decide_create`] turns the
//! stored execution state into one of four decisions:
//!
//! - absent → send;
//! - queued/running → replay the same intent;
//! - succeeded → record the existing completion;
//! - failed/interrupted → record interrupted and never send the prompt again.
//!
//! [`recover_all`] applies that rule to every non-terminal run at startup.
//!
//! The decision functions ([`evaluate_condition`], [`render_prompt`],
//! [`step_deadline_violated`], [`decide_create`]) are pure so the interesting
//! behaviour is unit-tested without MySQL or a live host.

use crate::entities::host_execution::ExecutionStatus;
use crate::entities::run::{
    InputError, ManualDispatchRequest, RunRow, RunStatus, RunStepRow, RunStepView, RunView,
    StepStatus,
};
use crate::entities::workflow::{
    ComparisonOp, Condition, Operand, Predicate, Prompt, PromptPart, Step, Workflow,
};
use crate::services::environments::EnvironmentError;
use crate::services::host_protocol::{self, HostProtocolError};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::MySqlPool;
use std::collections::BTreeMap;
use thiserror::Error;

/// Safety bound on one [`advance_run`] pass.
const MAX_ADVANCE_STEPS: usize = 10_000;

/// Domain errors for the run lifecycle.
#[derive(Debug, Error)]
pub enum RunError {
    /// No run with that public id.
    #[error("run not found")]
    NotFound,
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
    /// A stored snapshot could not be decoded.
    #[error("invalid run snapshot: {0}")]
    InvalidSnapshot(String),
    /// A step produced output that does not match its declared schema.
    #[error("step '{step}' output is not valid JSON: {message}")]
    InvalidOutput {
        /// Authored step id.
        step: String,
        /// Parse failure.
        message: String,
    },
    /// An environment could not be resolved at run time.
    #[error(transparent)]
    Environment(#[from] EnvironmentError),
    /// The host protocol failed.
    #[error(transparent)]
    Protocol(#[from] HostProtocolError),
    /// The run could not make progress.
    #[error("invalid run transition: {0}")]
    InvalidTransition(String),
    /// A manual dispatch request failed validation.
    #[error(transparent)]
    Input(#[from] InputError),
    /// Serialising a snapshot failed.
    #[error("run snapshot could not be encoded: {0}")]
    Encoding(String),
}

impl RunError {
    /// Stable problem code for the hub API.
    pub fn code(&self) -> &'static str {
        match self {
            RunError::NotFound => "not_found",
            RunError::Environment(error) => error.code(),
            RunError::Protocol(
                HostProtocolError::DaemonNotConnected { .. } | HostProtocolError::SocketGone,
            ) => "daemon_not_connected",
            RunError::Protocol(HostProtocolError::NotGranted) => "forbidden",
            RunError::Database(_) | RunError::Encoding(_) | RunError::Protocol(_) => {
                "internal_error"
            }
            RunError::InvalidSnapshot(_)
            | RunError::InvalidOutput { .. }
            | RunError::InvalidTransition(_)
            | RunError::Input(_) => "validation_failed",
        }
    }
}

/// What to do with a step whose execution id already has a ledger row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateDecision {
    /// No execution exists yet; send a new create intent.
    Send,
    /// Re-send the same intent so the host can answer with the existing
    /// execution.
    Replay,
    /// The execution already succeeded; record its completion.
    Complete,
    /// The execution failed, closed or was interrupted; record it as
    /// interrupted and never send the prompt again.
    Interrupt,
}

/// Decide how to treat a stored execution state.
///
/// This is the restart/lost-response rule in one place: a terminal success is
/// replayed from the ledger, and a terminal failure is *never* turned back into
/// a fresh execution.
pub fn decide_create(state: Option<ExecutionStatus>) -> CreateDecision {
    match state {
        None => CreateDecision::Send,
        Some(ExecutionStatus::Queued | ExecutionStatus::Running) => CreateDecision::Replay,
        Some(ExecutionStatus::Succeeded) => CreateDecision::Complete,
        Some(ExecutionStatus::Failed | ExecutionStatus::Interrupted) => CreateDecision::Interrupt,
    }
}

/// Deterministic execution id for one step of one run.
///
/// The pool chooses the id, and this derivation makes it stable across a crash
/// between "record the intent" and "send it". The shape is UUID-like so it can
/// also render into a worktree branch template.
pub fn execution_id_for(run_public_id: &str, step_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"tidepool.execution:");
    hasher.update(run_public_id.as_bytes());
    hasher.update(b":");
    hasher.update(step_id.as_bytes());
    let digest = hex::encode(hasher.finalize());
    let version = "4";
    let variant = match digest.as_bytes()[16] {
        byte @ b'0'..=b'7' => (byte - b'0' + 8) as char,
        byte @ b'8'..=b'9' => (byte - b'0') as char,
        byte @ b'a'..=b'f' => (byte - b'a' + 8) as char,
        _ => '8',
    };
    format!(
        "{}-{}-{}-{}{}{}-{}",
        &digest[0..8],
        &digest[8..12],
        version,
        &digest[13..16],
        variant,
        &digest[17..20],
        &digest[20..32]
    )
}

/// Split one text into a run's wall-clock deadline.
pub fn deadline_at(started_at: DateTime<Utc>, workflow: &Workflow) -> DateTime<Utc> {
    started_at + Duration::seconds(workflow.runtime_seconds() as i64)
}

/// True when `now` has reached the deadline.
pub fn deadline_passed(deadline: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    deadline.is_some_and(|deadline| now >= deadline)
}

/// Whole seconds elapsed between two instants, saturating at zero.
pub fn elapsed_seconds(started_at: DateTime<Utc>, now: DateTime<Utc>) -> u64 {
    let elapsed = now.signed_duration_since(started_at);
    u64::try_from(elapsed.num_seconds()).unwrap_or(0)
}

/// True when a step ran longer than its effective maximum runtime.
///
/// The workflow's remaining time caps the step's declared maximum, exactly as
/// [`Workflow::effective_deadlines`] specifies. Idle timeouts need the
/// timestamps of output publication; the run record does not observe those
/// yet (the activity log, task 1.4.7, does).
pub fn step_deadline_violated(
    workflow: &Workflow,
    step: &Step,
    run_started_at: DateTime<Utc>,
    step_started_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
) -> bool {
    let elapsed = elapsed_seconds(run_started_at, step_started_at);
    let deadlines = workflow.effective_deadlines(step, elapsed);
    let limit = step_started_at + Duration::seconds(deadlines.max_runtime_seconds as i64);
    completed_at > limit
}

/// Evaluate a step condition over inputs and earlier step outputs.
///
/// Missing inputs or absent earlier outputs compare as JSON `null`, so a
/// condition over a skipped step is false and its own step is skipped too.
/// `all([])` is true and `any([])` is false.
pub fn evaluate_condition(
    condition: &Condition,
    inputs: &BTreeMap<String, String>,
    outputs: &BTreeMap<String, Value>,
) -> bool {
    match condition {
        Condition::All(all) => all
            .all
            .iter()
            .all(|condition| evaluate_condition(condition, inputs, outputs)),
        Condition::Any(any) => any
            .any
            .iter()
            .any(|condition| evaluate_condition(condition, inputs, outputs)),
        Condition::Not(not) => not
            .not
            .as_deref()
            .is_some_and(|condition| !evaluate_condition(condition, inputs, outputs)),
        Condition::Predicate(predicate) => evaluate_predicate(predicate, inputs, outputs),
    }
}

fn evaluate_predicate(
    predicate: &Predicate,
    inputs: &BTreeMap<String, String>,
    outputs: &BTreeMap<String, Value>,
) -> bool {
    let left = operand_value(&predicate.left, inputs, outputs);
    match predicate.op {
        ComparisonOp::Exists => !left.is_null(),
        ComparisonOp::NotExists => left.is_null(),
        ComparisonOp::Eq => {
            let right = predicate
                .right
                .as_ref()
                .map(|right| operand_value(right, inputs, outputs))
                .unwrap_or(Value::Null);
            left == right
        }
        ComparisonOp::Ne => {
            let right = predicate
                .right
                .as_ref()
                .map(|right| operand_value(right, inputs, outputs))
                .unwrap_or(Value::Null);
            left != right
        }
        ComparisonOp::In | ComparisonOp::NotIn => {
            let member = predicate
                .right
                .as_ref()
                .map(|right| operand_value(right, inputs, outputs))
                .and_then(|right| match right {
                    Value::Array(values) => Some(values.iter().any(|value| value == &left)),
                    _ => None,
                })
                .unwrap_or(false);
            if predicate.op == ComparisonOp::In {
                member
            } else {
                !member
            }
        }
        ComparisonOp::Contains => {
            let right = predicate
                .right
                .as_ref()
                .map(|right| operand_value(right, inputs, outputs))
                .unwrap_or(Value::Null);
            match (&left, &right) {
                (Value::String(haystack), Value::String(needle)) => haystack.contains(needle),
                (Value::Array(values), value) => values.contains(value),
                _ => false,
            }
        }
        ComparisonOp::StartsWith => {
            let right = predicate
                .right
                .as_ref()
                .map(|right| operand_value(right, inputs, outputs))
                .unwrap_or(Value::Null);
            matches!(
                (&left, &right),
                (Value::String(left), Value::String(right)) if left.starts_with(right)
            )
        }
        ComparisonOp::EndsWith => {
            let right = predicate
                .right
                .as_ref()
                .map(|right| operand_value(right, inputs, outputs))
                .unwrap_or(Value::Null);
            matches!(
                (&left, &right),
                (Value::String(left), Value::String(right)) if left.ends_with(right)
            )
        }
    }
}

fn operand_value(
    operand: &Operand,
    inputs: &BTreeMap<String, String>,
    outputs: &BTreeMap<String, Value>,
) -> Value {
    match operand {
        Operand::Input(input) => inputs
            .get(&input.input)
            .map(|value| Value::String(value.clone()))
            .unwrap_or(Value::Null),
        Operand::Literal(literal) => literal.value.clone(),
        Operand::Step(step) => {
            let output = outputs.get(&step.step).unwrap_or(&Value::Null);
            match step.property.as_deref() {
                None | Some("") => output.clone(),
                Some(property) => property_value(output, property),
            }
        }
    }
}

/// Read a dot-separated property path from an output object.
fn property_value(output: &Value, property: &str) -> Value {
    let mut current = output;
    for segment in property.split('.') {
        match current.get(segment) {
            Some(value) => current = value,
            None => return Value::Null,
        }
    }
    current.clone()
}

/// Parse one step's final output into the value conditions read.
///
/// With a declared schema the output must be a JSON value; without one the
/// text is the value, so `exists` and `contains` still work.
pub fn step_output_value(step: &Step, output: &str) -> Result<Value, RunError> {
    match &step.output {
        Some(_) => serde_json::from_str(output).map_err(|error| RunError::InvalidOutput {
            step: step.id.clone(),
            message: error.to_string(),
        }),
        None => Ok(Value::String(output.to_string())),
    }
}

/// Render a prompt from its ordered parts, partials and the run context.
///
/// Expressions are the documented subset: `inputs.<name>`, `request` (or
/// `request.text`), `steps.<id>` and `steps.<id>.<property>`. Unknown
/// expressions are left verbatim rather than silently dropped.
pub fn render_prompt(
    prompt: &Prompt,
    partials: &BTreeMap<String, String>,
    inputs: &BTreeMap<String, String>,
    outputs: &BTreeMap<String, Value>,
    request_text: &str,
) -> String {
    match prompt {
        Prompt::Text(text) => interpolate(text, inputs, outputs, request_text),
        Prompt::Parts(parts) => parts
            .iter()
            .map(|part| render_part(part, partials, inputs, outputs, request_text))
            .collect(),
        Prompt::Document(document) => document
            .parts
            .iter()
            .map(|part| render_part(part, partials, inputs, outputs, request_text))
            .collect(),
    }
}

fn render_part(
    part: &PromptPart,
    partials: &BTreeMap<String, String>,
    inputs: &BTreeMap<String, String>,
    outputs: &BTreeMap<String, Value>,
    request_text: &str,
) -> String {
    match part {
        PromptPart::Text(text) => interpolate(&text.text, inputs, outputs, request_text),
        PromptPart::Partial(partial) => partials
            .get(&partial.partial)
            .map(|text| interpolate(text, inputs, outputs, request_text))
            .unwrap_or_default(),
    }
}

fn interpolate(
    text: &str,
    inputs: &BTreeMap<String, String>,
    outputs: &BTreeMap<String, Value>,
    request_text: &str,
) -> String {
    let mut rendered = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${{") {
        rendered.push_str(&rest[..start]);
        let after_open = &rest[start + 3..];
        let Some(close) = after_open.find("}}") else {
            rendered.push_str(&rest[start..]);
            return rendered;
        };
        let expression = after_open[..close].trim();
        match resolve_expression(expression, inputs, outputs, request_text) {
            Some(value) => rendered.push_str(&value),
            None => {
                rendered.push_str("${{");
                rendered.push_str(&after_open[..close]);
                rendered.push_str("}}");
            }
        }
        rest = &after_open[close + 2..];
    }
    rendered.push_str(rest);
    rendered
}

fn resolve_expression(
    expression: &str,
    inputs: &BTreeMap<String, String>,
    outputs: &BTreeMap<String, Value>,
    request_text: &str,
) -> Option<String> {
    if expression == "request" || expression == "request.text" {
        return Some(request_text.to_string());
    }
    if let Some(name) = expression.strip_prefix("inputs.") {
        return inputs.get(name).cloned();
    }
    let rest = expression.strip_prefix("steps.")?;
    let (step_id, property) = match rest.split_once('.') {
        Some((step_id, property)) => (step_id, Some(property)),
        None => (rest, None),
    };
    let output = outputs.get(step_id)?;
    let value = match property {
        Some(property) if !property.is_empty() => property_value(output, property),
        _ => output.clone(),
    };
    Some(match value {
        Value::String(text) => text,
        other => other.to_string(),
    })
}

/// One dispatch outcome returned to a caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DispatchOutcome {
    /// The run, whether newly created or deduplicated.
    pub run: RunView,
    /// True when the delivery key already named an existing run.
    pub deduplicated: bool,
}

/// What [`recover_run`] did to one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOutcome {
    /// The run was already terminal.
    Terminal,
    /// A pending step was started or skipped.
    Advanced,
    /// The same create intent was re-sent.
    Replayed,
    /// A stored success was recorded.
    Completed,
    /// A failed/interrupted execution was recorded as interrupted.
    Interrupted,
    /// The host is offline; the intent stays stored for the next recovery.
    Deferred,
}

/// Summary of one startup recovery pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryReport {
    /// Non-terminal runs examined.
    pub scanned: u64,
    /// Runs whose next pending step was started or skipped.
    pub advanced: u64,
    /// Runs whose create intent was re-sent.
    pub replayed: u64,
    /// Runs whose stored success completed the step.
    pub completed: u64,
    /// Runs recorded as interrupted instead of restarted.
    pub interrupted: u64,
    /// Runs left for a later recovery because the host is offline.
    pub deferred: u64,
}

/// Fetch one run by public id, scoped to its organization.
pub async fn find_run(
    pool: &MySqlPool,
    organization_id: u64,
    public_id: &str,
) -> Result<Option<RunRow>, RunError> {
    sqlx::query_as::<_, RunRow>(
        "SELECT id, public_id, organization_id, project_id, project_public_id, trigger_id, \
                trigger_public_id, trigger_name, workflow_name, revision_id, origin, actor, \
                delivery_key, request_text, inputs_json, workflow_json, environments_json, \
                partials_json, status, deadline_at, started_at, completed_at, created_at, \
                updated_at \
         FROM runs WHERE public_id = ? AND organization_id = ?",
    )
    .bind(public_id)
    .bind(organization_id)
    .fetch_optional(pool)
    .await
    .map_err(RunError::Database)
}

async fn load_run(pool: &MySqlPool, public_id: &str) -> Result<RunRow, RunError> {
    find_run_by_public_id(pool, public_id)
        .await?
        .ok_or(RunError::NotFound)
}

async fn find_run_by_public_id(
    pool: &MySqlPool,
    public_id: &str,
) -> Result<Option<RunRow>, RunError> {
    sqlx::query_as::<_, RunRow>(
        "SELECT id, public_id, organization_id, project_id, project_public_id, trigger_id, \
                trigger_public_id, trigger_name, workflow_name, revision_id, origin, actor, \
                delivery_key, request_text, inputs_json, workflow_json, environments_json, \
                partials_json, status, deadline_at, started_at, completed_at, created_at, \
                updated_at \
         FROM runs WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await
    .map_err(RunError::Database)
}

async fn load_steps(pool: &MySqlPool, run_id: u64) -> Result<Vec<RunStepRow>, RunError> {
    sqlx::query_as::<_, RunStepRow>(
        "SELECT id, public_id, run_id, step_id, position, status, condition_met, execution_id, \
                environment, host_public_id, directory, branch, prompt, output, error, reason, \
                started_at, completed_at, created_at, updated_at \
         FROM run_steps WHERE run_id = ? ORDER BY position ASC, id ASC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(RunError::Database)
}

/// Build the wire projection of a run and its steps.
pub async fn run_view(pool: &MySqlPool, run: &RunRow) -> Result<RunView, RunError> {
    let steps = load_steps(pool, run.id).await?;
    let inputs: BTreeMap<String, String> = serde_json::from_str(&run.inputs_json)
        .map_err(|error| RunError::InvalidSnapshot(format!("inputs are not JSON: {error}")))?;
    Ok(RunView {
        id: run.public_id.clone(),
        project: run.project_public_id.clone(),
        trigger: run.trigger_public_id.clone(),
        trigger_name: run.trigger_name.clone(),
        workflow: run.workflow_name.clone(),
        origin: run.origin,
        actor: run.actor.clone(),
        delivery_key: run.delivery_key.clone(),
        status: run.status,
        request_text: run.request_text.clone(),
        inputs,
        deadline_at: run.deadline_at,
        started_at: run.started_at,
        completed_at: run.completed_at,
        created_at: run.created_at,
        steps: steps.iter().map(RunStepView::from).collect(),
    })
}

struct RunSnapshot {
    workflow: Workflow,
    partials: BTreeMap<String, String>,
    inputs: BTreeMap<String, String>,
}

impl RunSnapshot {
    fn from_row(run: &RunRow) -> Result<Self, RunError> {
        let workflow = serde_json::from_str(&run.workflow_json)
            .map_err(|error| RunError::InvalidSnapshot(format!("workflow is not JSON: {error}")))?;
        let partials = serde_json::from_str(&run.partials_json).map_err(|error| {
            RunError::InvalidSnapshot(format!("partials are not JSON: {error}"))
        })?;
        let inputs = serde_json::from_str(&run.inputs_json)
            .map_err(|error| RunError::InvalidSnapshot(format!("inputs are not JSON: {error}")))?;
        Ok(Self {
            workflow,
            partials,
            inputs,
        })
    }
}

fn step_outputs(
    steps: &[RunStepRow],
    workflow: &Workflow,
) -> Result<BTreeMap<String, Value>, RunError> {
    let mut outputs = BTreeMap::new();
    for row in steps {
        if row.status != StepStatus::Succeeded {
            continue;
        }
        let Some(text) = row.output.as_deref() else {
            continue;
        };
        let Some(step) = workflow.step(&row.step_id) else {
            continue;
        };
        outputs.insert(row.step_id.clone(), step_output_value(step, text)?);
    }
    Ok(outputs)
}

/// Advance one run as far as it can go without waiting.
///
/// Skips conditional steps, sends the next eligible create intent, and stops
/// when a step is running. Completing that step calls this function again.
pub async fn advance_run(pool: &MySqlPool, run_public_id: &str) -> Result<RunView, RunError> {
    for _ in 0..MAX_ADVANCE_STEPS {
        let run = load_run(pool, run_public_id).await?;
        if run.status.is_terminal() {
            return run_view(pool, &run).await;
        }

        let snapshot = RunSnapshot::from_row(&run)?;
        let now = Utc::now();
        if deadline_passed(run.deadline_at, now) {
            stop_run(pool, &run, RunStatus::Failed, "run_deadline").await?;
            let run = load_run(pool, run_public_id).await?;
            return run_view(pool, &run).await;
        }

        let steps = load_steps(pool, run.id).await?;
        let Some(current) = steps.iter().find(|step| !step.status.is_terminal()) else {
            finish_run(pool, &run, RunStatus::Succeeded, None).await?;
            let run = load_run(pool, run_public_id).await?;
            return run_view(pool, &run).await;
        };

        if current.status == StepStatus::Running {
            // Waiting for the host. Recovery replays the intent if needed.
            return run_view(pool, &run).await;
        }

        let step = snapshot.workflow.step(&current.step_id).ok_or_else(|| {
            RunError::InvalidSnapshot(format!("unknown step '{}'", current.step_id))
        })?;
        let outputs = step_outputs(&steps, &snapshot.workflow)?;
        let condition_met = step
            .condition
            .as_ref()
            .map(|condition| evaluate_condition(condition, &snapshot.inputs, &outputs))
            .unwrap_or(true);

        if !condition_met {
            sqlx::query(
                "UPDATE run_steps SET status = 'skipped', condition_met = 0, \
                        reason = 'condition_false', completed_at = CURRENT_TIMESTAMP \
                 WHERE id = ? AND status = 'pending'",
            )
            .bind(current.id)
            .execute(pool)
            .await
            .map_err(RunError::Database)?;
            continue;
        }

        let execution_id = current.execution_id.clone().ok_or_else(|| {
            RunError::InvalidSnapshot(format!("step '{}' has no execution id", current.step_id))
        })?;
        let host_public_id = current.host_public_id.clone().ok_or_else(|| {
            RunError::InvalidSnapshot(format!("step '{}' has no host", current.step_id))
        })?;
        let prompt = render_prompt(
            &step.prompt,
            &snapshot.partials,
            &snapshot.inputs,
            &outputs,
            &run.request_text,
        );

        sqlx::query(
            "UPDATE run_steps SET status = 'running', condition_met = 1, prompt = ?, \
                    started_at = CURRENT_TIMESTAMP \
             WHERE id = ? AND status = 'pending'",
        )
        .bind(&prompt)
        .bind(current.id)
        .execute(pool)
        .await
        .map_err(RunError::Database)?;
        sqlx::query(
            "UPDATE runs SET status = 'running', \
                    started_at = COALESCE(started_at, CURRENT_TIMESTAMP) \
             WHERE id = ? AND status = 'pending'",
        )
        .bind(run.id)
        .execute(pool)
        .await
        .map_err(RunError::Database)?;

        match host_protocol::create_execution(
            pool,
            run.organization_id,
            &host_public_id,
            &execution_id,
            &prompt,
        )
        .await
        {
            Ok((execution, _)) => match execution.status {
                ExecutionStatus::Succeeded => {
                    Box::pin(observe_completion(
                        pool,
                        &execution_id,
                        "succeeded",
                        execution.output.as_deref(),
                    ))
                    .await?;
                }
                ExecutionStatus::Failed | ExecutionStatus::Interrupted => {
                    let status = if execution.status == ExecutionStatus::Interrupted {
                        "interrupted"
                    } else {
                        "failed"
                    };
                    Box::pin(observe_completion(
                        pool,
                        &execution_id,
                        status,
                        execution.output.as_deref(),
                    ))
                    .await?;
                }
                ExecutionStatus::Queued | ExecutionStatus::Running => {
                    let run = load_run(pool, run_public_id).await?;
                    return run_view(pool, &run).await;
                }
            },
            Err(HostProtocolError::SocketGone) => {
                // The intent is stored before the frame is sent, so a missing
                // socket leaves a queued execution for recovery, never a lost
                // prompt.
                let run = load_run(pool, run_public_id).await?;
                return run_view(pool, &run).await;
            }
            Err(error) => return Err(RunError::Protocol(error)),
        }
    }

    Err(RunError::InvalidTransition(
        "run did not make progress within the step bound".into(),
    ))
}

/// Observe a host completion and advance the owning run.
///
/// Returns `None` when the execution id does not belong to a run step (for
/// example a bare host-protocol execution), so the host socket can call this
/// unconditionally.
pub async fn observe_completion(
    pool: &MySqlPool,
    execution_id: &str,
    status: &str,
    output: Option<&str>,
) -> Result<Option<RunView>, RunError> {
    let Some(step) = sqlx::query_as::<_, RunStepRow>(
        "SELECT id, public_id, run_id, step_id, position, status, condition_met, execution_id, \
                environment, host_public_id, directory, branch, prompt, output, error, reason, \
                started_at, completed_at, created_at, updated_at \
         FROM run_steps WHERE execution_id = ?",
    )
    .bind(execution_id)
    .fetch_optional(pool)
    .await
    .map_err(RunError::Database)?
    else {
        return Ok(None);
    };

    if step.status.is_terminal() {
        return Ok(None);
    }

    let run = sqlx::query_as::<_, RunRow>(
        "SELECT id, public_id, organization_id, project_id, project_public_id, trigger_id, \
                trigger_public_id, trigger_name, workflow_name, revision_id, origin, actor, \
                delivery_key, request_text, inputs_json, workflow_json, environments_json, \
                partials_json, status, deadline_at, started_at, completed_at, created_at, \
                updated_at \
         FROM runs WHERE id = ?",
    )
    .bind(step.run_id)
    .fetch_one(pool)
    .await
    .map_err(RunError::Database)?;

    if run.status.is_terminal() {
        return Ok(None);
    }

    let snapshot = RunSnapshot::from_row(&run)?;
    let now = Utc::now();

    match status {
        "succeeded" => {
            let Some(workflow_step) = snapshot.workflow.step(&step.step_id) else {
                return Err(RunError::InvalidSnapshot(format!(
                    "unknown step '{}'",
                    step.step_id
                )));
            };
            let started = step.started_at.unwrap_or(run.created_at);
            let run_started = run.started_at.unwrap_or(run.created_at);
            if step_deadline_violated(&snapshot.workflow, workflow_step, run_started, started, now)
            {
                mark_step(
                    pool,
                    step.id,
                    StepStatus::Failed,
                    None,
                    Some("step deadline passed"),
                    Some("step_deadline"),
                    now,
                )
                .await?;
                stop_run(pool, &run, RunStatus::Failed, "step_deadline").await?;
            } else {
                let text = output.unwrap_or_default();
                match step_output_value(workflow_step, text) {
                    Ok(_) => {
                        mark_step(
                            pool,
                            step.id,
                            StepStatus::Succeeded,
                            Some(text),
                            None,
                            None,
                            now,
                        )
                        .await?;
                        advance_run(pool, &run.public_id).await?;
                    }
                    Err(error) => {
                        mark_step(
                            pool,
                            step.id,
                            StepStatus::Failed,
                            Some(text),
                            Some(&error.to_string()),
                            Some("invalid_output"),
                            now,
                        )
                        .await?;
                        stop_run(pool, &run, RunStatus::Failed, "invalid_output").await?;
                    }
                }
            }
        }
        "interrupted" => {
            mark_step(
                pool,
                step.id,
                StepStatus::Interrupted,
                output,
                Some("execution interrupted"),
                Some("interrupted"),
                now,
            )
            .await?;
            stop_run(pool, &run, RunStatus::Interrupted, "interrupted").await?;
        }
        _ => {
            mark_step(
                pool,
                step.id,
                StepStatus::Failed,
                output,
                Some("execution failed"),
                Some("execution_failed"),
                now,
            )
            .await?;
            stop_run(pool, &run, RunStatus::Failed, "execution_failed").await?;
        }
    }

    let run = load_run(pool, &run.public_id).await?;
    Ok(Some(run_view(pool, &run).await?))
}

async fn mark_step(
    pool: &MySqlPool,
    step_id: u64,
    status: StepStatus,
    output: Option<&str>,
    error: Option<&str>,
    reason: Option<&str>,
    completed_at: DateTime<Utc>,
) -> Result<(), RunError> {
    sqlx::query(
        "UPDATE run_steps SET status = ?, output = COALESCE(?, output), \
                error = COALESCE(?, error), reason = COALESCE(?, reason), \
                completed_at = ? \
         WHERE id = ? AND status IN ('pending', 'running')",
    )
    .bind(status.as_str())
    .bind(output)
    .bind(error)
    .bind(reason)
    .bind(completed_at)
    .bind(step_id)
    .execute(pool)
    .await
    .map_err(RunError::Database)?;
    Ok(())
}

async fn finish_run(
    pool: &MySqlPool,
    run: &RunRow,
    status: RunStatus,
    reason: Option<&str>,
) -> Result<(), RunError> {
    sqlx::query(
        "UPDATE runs SET status = ?, completed_at = CURRENT_TIMESTAMP \
         WHERE id = ? AND status IN ('pending', 'running')",
    )
    .bind(status.as_str())
    .bind(run.id)
    .execute(pool)
    .await
    .map_err(RunError::Database)?;
    crate::middleware::metrics::metrics().run_finished(status.as_str());
    if let Some(reason) = reason {
        let running_status = if status == RunStatus::Interrupted {
            StepStatus::Interrupted
        } else {
            StepStatus::Failed
        };
        sqlx::query(
            "UPDATE run_steps SET status = ?, error = COALESCE(error, ?), \
                    reason = COALESCE(reason, ?), completed_at = CURRENT_TIMESTAMP \
             WHERE run_id = ? AND status = 'running'",
        )
        .bind(running_status.as_str())
        .bind(reason)
        .bind(reason)
        .bind(run.id)
        .execute(pool)
        .await
        .map_err(RunError::Database)?;
        sqlx::query(
            "UPDATE run_steps SET status = 'skipped', reason = ? \
             WHERE run_id = ? AND status = 'pending'",
        )
        .bind(reason)
        .bind(run.id)
        .execute(pool)
        .await
        .map_err(RunError::Database)?;
    }
    Ok(())
}

/// Stop a non-terminal run, skipping every step that had not started.
async fn stop_run(
    pool: &MySqlPool,
    run: &RunRow,
    status: RunStatus,
    reason: &str,
) -> Result<(), RunError> {
    finish_run(pool, run, status, Some(reason)).await
}

/// Recover one non-terminal run.
///
/// A pending step is advanced normally. A running step consults the execution
/// ledger and applies [`decide_create`]: queued/running replays the same intent,
/// succeeded records the stored completion, and failed/interrupted is recorded
/// as interrupted without ever starting the prompt again.
pub async fn recover_run(
    pool: &MySqlPool,
    run_public_id: &str,
) -> Result<RecoveryOutcome, RunError> {
    let run = load_run(pool, run_public_id).await?;
    if run.status.is_terminal() {
        return Ok(RecoveryOutcome::Terminal);
    }

    let steps = load_steps(pool, run.id).await?;
    let Some(current) = steps.iter().find(|step| !step.status.is_terminal()) else {
        finish_run(pool, &run, RunStatus::Succeeded, None).await?;
        return Ok(RecoveryOutcome::Completed);
    };

    if current.status == StepStatus::Pending {
        let view = advance_run(pool, run_public_id).await?;
        return Ok(if view.status.is_terminal() {
            RecoveryOutcome::Completed
        } else {
            RecoveryOutcome::Advanced
        });
    }

    let execution_id = current.execution_id.clone().ok_or_else(|| {
        RunError::InvalidSnapshot(format!("step '{}' has no execution id", current.step_id))
    })?;
    let host_public_id = current.host_public_id.clone().ok_or_else(|| {
        RunError::InvalidSnapshot(format!("step '{}' has no host", current.step_id))
    })?;
    let prompt = current.prompt.clone().unwrap_or_default();
    let stored = host_protocol::find_execution(pool, &execution_id).await?;
    let state = stored.as_ref().map(|execution| execution.status);

    match decide_create(state) {
        CreateDecision::Complete => {
            observe_completion(
                pool,
                &execution_id,
                "succeeded",
                stored
                    .as_ref()
                    .and_then(|execution| execution.output.as_deref()),
            )
            .await?;
            Ok(RecoveryOutcome::Completed)
        }
        CreateDecision::Interrupt => {
            observe_completion(
                pool,
                &execution_id,
                "interrupted",
                stored
                    .as_ref()
                    .and_then(|execution| execution.output.as_deref()),
            )
            .await?;
            Ok(RecoveryOutcome::Interrupted)
        }
        CreateDecision::Send | CreateDecision::Replay => {
            match host_protocol::create_execution(
                pool,
                run.organization_id,
                &host_public_id,
                &execution_id,
                &prompt,
            )
            .await
            {
                Ok(_) => Ok(RecoveryOutcome::Replayed),
                Err(
                    HostProtocolError::SocketGone | HostProtocolError::DaemonNotConnected { .. },
                ) => Ok(RecoveryOutcome::Deferred),
                Err(error) => Err(RunError::Protocol(error)),
            }
        }
    }
}

/// Recover every non-terminal run (called once at startup).
pub async fn recover_all(pool: &MySqlPool) -> Result<RecoveryReport, RunError> {
    let public_ids: Vec<String> = sqlx::query_scalar(
        "SELECT public_id FROM runs WHERE status IN ('pending', 'running') ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(RunError::Database)?;

    let mut report = RecoveryReport {
        scanned: public_ids.len() as u64,
        ..RecoveryReport::default()
    };
    for public_id in public_ids {
        match recover_run(pool, &public_id).await {
            Ok(RecoveryOutcome::Terminal) => {}
            Ok(RecoveryOutcome::Advanced) => report.advanced += 1,
            Ok(RecoveryOutcome::Replayed) => report.replayed += 1,
            Ok(RecoveryOutcome::Completed) => report.completed += 1,
            Ok(RecoveryOutcome::Interrupted) => report.interrupted += 1,
            Ok(RecoveryOutcome::Deferred) => report.deferred += 1,
            Err(error) => {
                tracing::warn!(run = %public_id, %error, "run recovery failed");
                report.deferred += 1;
            }
        }
    }
    Ok(report)
}

/// Recover the runs waiting on one host, used when that host reconnects.
///
/// Startup recovery happens while every host is still offline, so this is what
/// turns a stored create intent back into a replayed frame once the socket is
/// live again. It never creates a new execution id.
pub async fn recover_for_host(
    pool: &MySqlPool,
    host_public_id: &str,
) -> Result<RecoveryReport, RunError> {
    let public_ids: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT r.public_id FROM runs r \
         JOIN run_steps s ON s.run_id = r.id \
         WHERE r.status IN ('pending', 'running') AND s.status = 'running' \
           AND s.host_public_id = ? \
         ORDER BY r.id ASC",
    )
    .bind(host_public_id)
    .fetch_all(pool)
    .await
    .map_err(RunError::Database)?;

    let mut report = RecoveryReport {
        scanned: public_ids.len() as u64,
        ..RecoveryReport::default()
    };
    for public_id in public_ids {
        match recover_run(pool, &public_id).await {
            Ok(RecoveryOutcome::Terminal) => {}
            Ok(RecoveryOutcome::Advanced) => report.advanced += 1,
            Ok(RecoveryOutcome::Replayed) => report.replayed += 1,
            Ok(RecoveryOutcome::Completed) => report.completed += 1,
            Ok(RecoveryOutcome::Interrupted) => report.interrupted += 1,
            Ok(RecoveryOutcome::Deferred) => report.deferred += 1,
            Err(error) => {
                tracing::warn!(run = %public_id, %error, "host run recovery failed");
                report.deferred += 1;
            }
        }
    }
    Ok(report)
}

/// Validate a manual dispatch request before it reaches the database.
pub fn validate_manual_request(request: &ManualDispatchRequest) -> Result<(), RunError> {
    request.validate().map_err(RunError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::workflow::{
        AllCondition, AnyCondition, InputOperand, LiteralOperand, NotCondition, Predicate,
        StepOutputOperand, TextPart,
    };
    use serde_json::json;

    fn workflow() -> Workflow {
        serde_json::from_value(json!({
            "name": "triage",
            "trigger": "issue-comment",
            "maxRuntime": 900,
            "inputs": [{ "name": "route", "choices": ["bug", "docs"] }],
            "steps": [
                {
                    "id": "classify",
                    "environment": "default",
                    "agent": "coder",
                    "prompt": "Classify ${{ request }} for ${{ inputs.route }}",
                    "output": { "properties": { "route": { "enum": ["bug", "docs"] } } }
                },
                {
                    "id": "fix",
                    "environment": "default",
                    "agent": "coder",
                    "prompt": "Fix ${{ steps.classify.route }}",
                    "reads": ["classify"]
                }
            ]
        }))
        .expect("workflow")
    }

    fn predicate(left: Operand, op: ComparisonOp, right: Option<Operand>) -> Predicate {
        Predicate { left, op, right }
    }

    fn input(name: &str) -> Operand {
        Operand::Input(InputOperand { input: name.into() })
    }

    fn literal(value: Value) -> Operand {
        Operand::Literal(LiteralOperand { value })
    }

    fn step(name: &str, property: Option<&str>) -> Operand {
        Operand::Step(StepOutputOperand {
            step: name.into(),
            property: property.map(str::to_string),
        })
    }

    #[test]
    fn equality_and_membership_compare_inputs_and_literals() {
        let inputs = BTreeMap::from([("route".to_string(), "bug".to_string())]);
        assert!(evaluate_condition(
            &Condition::Predicate(predicate(
                input("route"),
                ComparisonOp::Eq,
                Some(literal(json!("bug")))
            )),
            &inputs,
            &BTreeMap::new()
        ));
        assert!(evaluate_condition(
            &Condition::Predicate(predicate(
                input("route"),
                ComparisonOp::In,
                Some(literal(json!(["bug", "docs"])))
            )),
            &inputs,
            &BTreeMap::new()
        ));
    }

    #[test]
    fn conditions_read_earlier_step_outputs_by_property() {
        let outputs = BTreeMap::from([("classify".to_string(), json!({ "route": "docs" }))]);
        let condition = Condition::Predicate(predicate(
            input("route"),
            ComparisonOp::Eq,
            Some(step("classify", Some("route"))),
        ));
        let inputs = BTreeMap::from([("route".to_string(), "docs".to_string())]);
        assert!(evaluate_condition(&condition, &inputs, &outputs));
    }

    #[test]
    fn a_missing_earlier_output_makes_a_condition_false() {
        let condition = Condition::Predicate(predicate(
            input("route"),
            ComparisonOp::Eq,
            Some(step("skipped-step", Some("route"))),
        ));
        let inputs = BTreeMap::from([("route".to_string(), "bug".to_string())]);
        assert!(!evaluate_condition(&condition, &inputs, &BTreeMap::new()));
        assert!(evaluate_condition(
            &Condition::Predicate(predicate(
                step("skipped-step", None),
                ComparisonOp::NotExists,
                None
            )),
            &inputs,
            &BTreeMap::new()
        ));
    }

    #[test]
    fn all_any_and_not_compose() {
        let inputs = BTreeMap::from([("route".to_string(), "bug".to_string())]);
        let yes = Condition::Predicate(predicate(
            input("route"),
            ComparisonOp::Eq,
            Some(literal(json!("bug"))),
        ));
        let no = Condition::Predicate(predicate(
            input("route"),
            ComparisonOp::Eq,
            Some(literal(json!("docs"))),
        ));
        assert!(evaluate_condition(
            &Condition::All(AllCondition {
                all: vec![yes.clone()]
            }),
            &inputs,
            &BTreeMap::new()
        ));
        assert!(evaluate_condition(
            &Condition::Any(AnyCondition {
                any: vec![no.clone(), yes.clone()]
            }),
            &inputs,
            &BTreeMap::new()
        ));
        assert!(evaluate_condition(
            &Condition::Not(NotCondition {
                not: Some(Box::new(no))
            }),
            &inputs,
            &BTreeMap::new()
        ));
    }

    #[test]
    fn contains_starts_and_ends_with_work_on_text() {
        let inputs = BTreeMap::from([("route".to_string(), "bug-fix".to_string())]);
        assert!(evaluate_condition(
            &Condition::Predicate(predicate(
                input("route"),
                ComparisonOp::Contains,
                Some(literal(json!("g-f")))
            )),
            &inputs,
            &BTreeMap::new()
        ));
        assert!(evaluate_condition(
            &Condition::Predicate(predicate(
                input("route"),
                ComparisonOp::StartsWith,
                Some(literal(json!("bug")))
            )),
            &inputs,
            &BTreeMap::new()
        ));
        assert!(evaluate_condition(
            &Condition::Predicate(predicate(
                input("route"),
                ComparisonOp::EndsWith,
                Some(literal(json!("fix")))
            )),
            &inputs,
            &BTreeMap::new()
        ));
    }

    #[test]
    fn prompts_render_inputs_request_and_earlier_outputs() {
        let workflow = workflow();
        let inputs = BTreeMap::from([("route".to_string(), "bug".to_string())]);
        let outputs = BTreeMap::from([("classify".to_string(), json!({ "route": "docs" }))]);
        let rendered = render_prompt(
            &workflow.steps[0].prompt,
            &BTreeMap::new(),
            &inputs,
            &outputs,
            "please fix it",
        );
        assert_eq!(rendered, "Classify please fix it for bug");

        let rendered = render_prompt(
            &workflow.steps[1].prompt,
            &BTreeMap::new(),
            &inputs,
            &outputs,
            "",
        );
        assert_eq!(rendered, "Fix docs");
    }

    #[test]
    fn prompt_parts_and_partials_keep_their_order() {
        let prompt = Prompt::Parts(vec![
            PromptPart::Text(TextPart {
                text: "Use ".into(),
            }),
            PromptPart::Partial(crate::entities::workflow::PartialPart {
                partial: "system".into(),
            }),
        ]);
        let partials = BTreeMap::from([(
            "system".to_string(),
            "the rules for ${{ inputs.route }}".to_string(),
        )]);
        let inputs = BTreeMap::from([("route".to_string(), "bug".to_string())]);
        assert_eq!(
            render_prompt(&prompt, &partials, &inputs, &BTreeMap::new(), ""),
            "Use the rules for bug"
        );
    }

    #[test]
    fn unknown_expressions_are_left_verbatim() {
        let prompt = Prompt::Text("keep ${{ secrets.token }}".into());
        assert_eq!(
            render_prompt(
                &prompt,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &BTreeMap::new(),
                ""
            ),
            "keep ${{ secrets.token }}"
        );
    }

    #[test]
    fn structured_output_must_be_json_but_text_output_need_not() {
        let workflow = workflow();
        let classify = &workflow.steps[0];
        assert!(step_output_value(classify, "not json").is_err());
        assert_eq!(
            step_output_value(classify, r#"{"route":"bug"}"#).expect("json"),
            json!({ "route": "bug" })
        );
        let fix = &workflow.steps[1];
        assert_eq!(
            step_output_value(fix, "plain text").expect("text"),
            json!("plain text")
        );
    }

    #[test]
    fn execution_ids_are_stable_and_distinct() {
        let first = execution_id_for("run-1", "classify");
        let again = execution_id_for("run-1", "classify");
        let other_step = execution_id_for("run-1", "fix");
        let other_run = execution_id_for("run-2", "classify");
        assert_eq!(first, again);
        assert_ne!(first, other_step);
        assert_ne!(first, other_run);
        assert_eq!(first.len(), 36);
    }

    #[test]
    fn create_decisions_never_restart_a_terminal_failure() {
        assert_eq!(decide_create(None), CreateDecision::Send);
        assert_eq!(
            decide_create(Some(ExecutionStatus::Queued)),
            CreateDecision::Replay
        );
        assert_eq!(
            decide_create(Some(ExecutionStatus::Running)),
            CreateDecision::Replay
        );
        assert_eq!(
            decide_create(Some(ExecutionStatus::Succeeded)),
            CreateDecision::Complete
        );
        assert_eq!(
            decide_create(Some(ExecutionStatus::Failed)),
            CreateDecision::Interrupt
        );
        assert_eq!(
            decide_create(Some(ExecutionStatus::Interrupted)),
            CreateDecision::Interrupt
        );
    }

    #[test]
    fn deadlines_compose_and_stop_late_steps() {
        let workflow = workflow();
        let started: DateTime<Utc> = "2026-10-21T00:00:00Z".parse().expect("timestamp");
        let deadline = deadline_at(started, &workflow);
        assert_eq!(deadline, started + Duration::seconds(900));
        assert!(!deadline_passed(Some(deadline), started));
        assert!(deadline_passed(
            Some(deadline),
            started + Duration::seconds(900)
        ));

        assert!(!step_deadline_violated(
            &workflow,
            &workflow.steps[0],
            started,
            started,
            started + Duration::seconds(299)
        ));
        assert!(step_deadline_violated(
            &workflow,
            &workflow.steps[0],
            started,
            started,
            started + Duration::seconds(901)
        ));
    }

    #[test]
    fn a_step_is_capped_by_the_workflow_time_remaining() {
        let mut workflow = workflow();
        workflow.max_runtime_seconds = Some(300);
        let started: DateTime<Utc> = "2026-10-21T00:00:00Z".parse().expect("timestamp");
        // The second step starts after 200s, so only 100s remain.
        let step_start = started + Duration::seconds(200);
        assert!(step_deadline_violated(
            &workflow,
            &workflow.steps[1],
            started,
            step_start,
            step_start + Duration::seconds(101)
        ));
        assert!(!step_deadline_violated(
            &workflow,
            &workflow.steps[1],
            started,
            step_start,
            step_start + Duration::seconds(100)
        ));
    }

    #[test]
    fn run_error_codes_are_documented() {
        assert_eq!(RunError::NotFound.code(), "not_found");
        assert_eq!(
            RunError::Protocol(HostProtocolError::DaemonNotConnected {
                slug: "studio-mac".into()
            })
            .code(),
            "daemon_not_connected"
        );
        assert_eq!(
            RunError::Protocol(HostProtocolError::NotGranted).code(),
            "forbidden"
        );
        assert_eq!(
            RunError::Input(InputError::RequiredInput("repo".into())).code(),
            "validation_failed"
        );
    }
}
