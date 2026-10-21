//! Activity log, event receipts and conversation continuation (task 1.4.7).
//!
//! The activity log is the durable answer to three questions: what arrived,
//! what it matched and what ran. It is built from records that already exist —
//! [`crate::entities::run`] runs, their steps and the execution ledger — plus
//! two new shapes here:
//!
//! - a **receipt** for every provider delivery, matched or not, deduplicated
//!   by `(organization, provider, connection, delivery key)`;
//! - a **conversation** row per trigger and thread/issue/pull request, so a
//!   later arrival can continue the same agent or restore it when it is gone.
//!
//! Continuation is decided by [`plan_continuation`], a pure function of the
//! conversation's current agent state. A busy agent is *continued and steered*;
//! an idle one is continued; a conversation whose agent no longer exists is
//! *restored*. The plan is recorded on the run, together with the output
//! limits that belong to that arrival alone, so history never rewrites an
//! earlier arrival's limits or completion status.
//!
//! This module is pure: parsing, grouping, cursor arithmetic and output-limit
//! projection. Database access lives in [`crate::services::activity`].

use crate::entities::connection::ProviderKind;
use crate::entities::host_execution::ExecutionStatus;
use crate::entities::run::{RunStatus, StepStatus};
use crate::entities::workflow::Workflow;
use crate::services::outputs::OutputError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// Page size used when an activity read names no limit.
pub const DEFAULT_PAGE_SIZE: u32 = 25;
/// Largest page an activity read may ask for.
pub const MAX_PAGE_SIZE: u32 = 100;
/// Maximum number of characters in a provider delivery key.
pub const MAX_DELIVERY_KEY_CHARS: usize = 255;
/// Maximum number of characters in a conversation key.
pub const MAX_CONVERSATION_KEY_CHARS: usize = 512;

/// Lifecycle of one delivery receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    /// Recorded, matching not finished yet.
    Received,
    /// At least one trigger matched and a run was created.
    Matched,
    /// The delivery was valid but matched no trigger.
    Unmatched,
    /// Matching or dispatch failed; the receipt keeps the failure for history.
    Rejected,
}

impl ReceiptStatus {
    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ReceiptStatus::Received => "received",
            ReceiptStatus::Matched => "matched",
            ReceiptStatus::Unmatched => "unmatched",
            ReceiptStatus::Rejected => "rejected",
        }
    }
}

impl sqlx::Type<sqlx::MySql> for ReceiptStatus {
    fn type_info() -> sqlx::mysql::MySqlTypeInfo {
        <String as sqlx::Type<sqlx::MySql>>::type_info()
    }

    fn compatible(ty: &sqlx::mysql::MySqlTypeInfo) -> bool {
        use sqlx::TypeInfo as _;
        <String as sqlx::Type<sqlx::MySql>>::compatible(ty)
            || ty.name().eq_ignore_ascii_case("ENUM")
    }
}

impl<'r> sqlx::Decode<'r, sqlx::MySql> for ReceiptStatus {
    fn decode(
        value: sqlx::mysql::MySqlValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let stored = <String as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?;
        match stored.as_str() {
            "received" => Ok(ReceiptStatus::Received),
            "matched" => Ok(ReceiptStatus::Matched),
            "unmatched" => Ok(ReceiptStatus::Unmatched),
            "rejected" => Ok(ReceiptStatus::Rejected),
            other => Err(format!("unknown receipt status: {other:?}").into()),
        }
    }
}

/// Full receipt row, mirroring `event_receipts`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReceiptRow {
    /// Internal primary key.
    pub id: u64,
    /// Immutable public id.
    pub public_id: String,
    /// Internal organization id.
    pub organization_id: u64,
    /// Internal project id, when the delivery could be attributed.
    pub project_id: Option<u64>,
    /// Project public id, when the delivery could be attributed.
    pub project_public_id: Option<String>,
    /// Provider kind.
    pub provider: String,
    /// Connection slug the delivery arrived through.
    pub connection: String,
    /// Provider delivery id.
    pub delivery_key: String,
    /// Internal event type.
    pub event_type: String,
    /// Provider action.
    pub action: Option<String>,
    /// Provider resource kind.
    pub resource_kind: Option<String>,
    /// Provider resource id.
    pub resource_id: Option<String>,
    /// Actor that caused the event.
    pub actor: Option<String>,
    /// Channel or thread.
    pub channel: Option<String>,
    /// Matching outcome.
    pub status: ReceiptStatus,
    /// Number of triggers this delivery matched.
    pub matched_triggers: u32,
    /// JSON array of run public ids the delivery produced.
    pub run_public_ids: Option<String>,
    /// Number of duplicate deliveries seen for this key.
    pub duplicate_count: u32,
    /// When the first delivery arrived.
    pub received_at: DateTime<Utc>,
    /// When created.
    pub created_at: DateTime<Utc>,
    /// When last updated.
    pub updated_at: DateTime<Utc>,
}

/// How this arrival related to the agent already serving its conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationMode {
    /// No conversation existed; a new agent was created.
    Created,
    /// The same agent was reused for this thread/issue/pull request.
    Continued,
    /// The conversation existed but its agent was gone; a new agent restored it.
    Restored,
}

impl ContinuationMode {
    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ContinuationMode::Created => "created",
            ContinuationMode::Continued => "continued",
            ContinuationMode::Restored => "restored",
        }
    }
}

/// What is known about a conversation's current agent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ConversationState {
    /// Execution id of the agent currently serving the conversation.
    pub agent_execution_id: Option<String>,
    /// True while that agent is queued or running.
    pub busy: bool,
}

/// The continuation decision recorded on a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuationPlan {
    /// Created, continued or restored.
    pub mode: ContinuationMode,
    /// True when the existing agent is busy and this arrival steers it.
    pub steer: bool,
    /// Existing agent execution to reuse, when there is one.
    pub reuse_execution_id: Option<String>,
}

impl Default for ContinuationPlan {
    fn default() -> Self {
        Self {
            mode: ContinuationMode::Created,
            steer: false,
            reuse_execution_id: None,
        }
    }
}

impl ContinuationPlan {
    /// Convenience constructor for a brand-new conversation.
    pub fn created() -> Self {
        Self::default()
    }
}

/// Decide how an arrival continues its conversation.
///
/// An existing agent is always reused. It is steered when it is busy, and the
/// conversation is restored when the previous agent no longer exists.
pub fn plan_continuation(existing: Option<&ConversationState>) -> ContinuationPlan {
    match existing {
        None => ContinuationPlan::created(),
        Some(state) => match state.agent_execution_id.as_deref() {
            Some(agent_execution_id) => ContinuationPlan {
                mode: ContinuationMode::Continued,
                steer: state.busy,
                reuse_execution_id: Some(agent_execution_id.to_string()),
            },
            None => ContinuationPlan {
                mode: ContinuationMode::Restored,
                steer: false,
                reuse_execution_id: None,
            },
        },
    }
}

/// Stable conversation key for one thread, issue or pull request.
///
/// Slack and Discord place the thread id in `channel`; GitHub issue and pull
/// request events carry the issue/PR conversation there too. Without a
/// conversation id there is nothing to continue, so this returns `None` and
/// the arrival creates a fresh agent.
pub fn conversation_key_for(
    provider: ProviderKind,
    resource_kind: &str,
    resource_id: &str,
    channel: Option<&str>,
) -> Option<String> {
    let channel = channel.map(str::trim).filter(|value| !value.is_empty())?;
    if resource_kind.trim().is_empty() || resource_id.trim().is_empty() {
        return None;
    }
    Some(format!(
        "{}:{}:{}:{}",
        provider.as_str(),
        resource_kind.trim().to_ascii_lowercase(),
        resource_id.trim().to_ascii_lowercase(),
        channel.to_ascii_lowercase()
    ))
}

/// Normalized identity of one provider delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptKey {
    /// Provider kind, lowercase.
    pub provider: String,
    /// Connection slug, lowercase.
    pub connection: String,
    /// Provider delivery id, trimmed.
    pub delivery_key: String,
}

/// Normalize the three fields the receipt's unique key is built from.
pub fn receipt_key(
    provider: ProviderKind,
    connection: &str,
    delivery_key: &str,
) -> Result<ReceiptKey, FilterError> {
    let connection = connection.trim().to_ascii_lowercase();
    let delivery_key = delivery_key.trim().to_string();
    if connection.is_empty() {
        return Err(FilterError::Invalid {
            field: "connection",
            message: "is required".into(),
        });
    }
    if connection.chars().count() > 64 {
        return Err(FilterError::Invalid {
            field: "connection",
            message: "must be at most 64 characters".into(),
        });
    }
    if delivery_key.is_empty() {
        return Err(FilterError::Invalid {
            field: "deliveryKey",
            message: "is required".into(),
        });
    }
    if delivery_key.chars().count() > MAX_DELIVERY_KEY_CHARS {
        return Err(FilterError::Invalid {
            field: "deliveryKey",
            message: "must be at most 255 characters".into(),
        });
    }
    Ok(ReceiptKey {
        provider: provider.as_str().to_string(),
        connection,
        delivery_key,
    })
}

/// What to do with a delivery whose receipt key is already present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiptAction {
    /// No receipt exists; insert one and continue matching.
    Insert,
    /// The delivery was already recorded; return its runs and change nothing.
    Duplicate,
}

/// Decide how to treat an existing receipt.
pub fn decide_receipt(existing: bool) -> ReceiptAction {
    if existing {
        ReceiptAction::Duplicate
    } else {
        ReceiptAction::Insert
    }
}

/// One declared output limit, kept per arrival.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputLimit {
    /// Registered capability name.
    pub capability: String,
    /// Maximum emissions for this step.
    pub maximum: u32,
    /// Whether the step must emit it before finishing.
    pub required: bool,
}

/// Resolve every step's declared output limits for one arrival.
///
/// The run stores this projection rather than reading the live configuration
/// later, so a later revision cannot change what an earlier arrival allowed.
pub fn output_limits(
    workflow: &Workflow,
) -> Result<BTreeMap<String, Vec<OutputLimit>>, OutputError> {
    let mut limits = BTreeMap::new();
    for step in &workflow.steps {
        let mut resolved = Vec::with_capacity(step.outputs.len());
        for declaration in &step.outputs {
            let output = declaration.resolve()?;
            resolved.push(OutputLimit {
                capability: output.spec.name.to_string(),
                maximum: output.maximum,
                required: output.required,
            });
        }
        limits.insert(step.id.clone(), resolved);
    }
    Ok(limits)
}

/// A declared output limit as an activity caller reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepOutputLimits {
    /// Authored step id.
    pub step: String,
    /// Declared capabilities for the step.
    pub limits: Vec<OutputLimit>,
}

/// Project one stored output-limit map onto the wire shape.
pub fn step_output_limits(limits: &BTreeMap<String, Vec<OutputLimit>>) -> Vec<StepOutputLimits> {
    limits
        .iter()
        .map(|(step, limits)| StepOutputLimits {
            step: step.clone(),
            limits: limits.clone(),
        })
        .collect()
}

/// What kind of record one activity entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    /// One provider delivery receipt.
    Receipt,
    /// One trigger run.
    Run,
    /// One workflow step.
    Step,
    /// One host execution.
    Execution,
}

impl ActivityKind {
    /// Wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ActivityKind::Receipt => "receipt",
            ActivityKind::Run => "run",
            ActivityKind::Step => "step",
            ActivityKind::Execution => "execution",
        }
    }

    /// Parse a wire spelling.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "receipt" => Some(ActivityKind::Receipt),
            "run" => Some(ActivityKind::Run),
            "step" => Some(ActivityKind::Step),
            "execution" => Some(ActivityKind::Execution),
            _ => None,
        }
    }
}

/// Opaque position in an activity page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityCursor {
    /// Timestamp of the last row on the previous page.
    pub at: DateTime<Utc>,
    /// Row id, the tie-breaker inside one timestamp.
    pub id: String,
}

impl ActivityCursor {
    /// Encode a cursor as `<micros>:<id>`.
    pub fn encode(&self) -> String {
        format!("{}:{}", self.at.timestamp_micros(), self.id)
    }

    /// Decode a cursor produced by [`ActivityCursor::encode`].
    pub fn decode(value: &str) -> Result<Self, FilterError> {
        let (micros, id) = value.split_once(':').ok_or(FilterError::InvalidCursor)?;
        let micros: i64 = micros.parse().map_err(|_| FilterError::InvalidCursor)?;
        let at =
            DateTime::<Utc>::from_timestamp_micros(micros).ok_or(FilterError::InvalidCursor)?;
        let id = id.trim();
        if id.is_empty() || id.chars().count() > 64 {
            return Err(FilterError::InvalidCursor);
        }
        Ok(Self {
            at,
            id: id.to_string(),
        })
    }
}

/// A query error from an activity read.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FilterError {
    /// A query field cannot be used.
    #[error("invalid {field}: {message}")]
    Invalid {
        /// Query field name.
        field: &'static str,
        /// Explanation.
        message: String,
    },
    /// The cursor is not one this API produced.
    #[error("cursor is malformed")]
    InvalidCursor,
}

/// Every filter an activity read may combine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityFilters {
    /// Project public id.
    pub project: Option<String>,
    /// Provider kind, lowercase.
    pub provider: Option<String>,
    /// Trigger public id or trigger name.
    pub trigger: Option<String>,
    /// Record status, kind-specific.
    pub status: Option<String>,
    /// Record kind.
    pub kind: Option<ActivityKind>,
    /// Position after which to read.
    pub cursor: Option<ActivityCursor>,
    /// Page size, 1..=100.
    pub limit: u32,
}

impl Default for ActivityFilters {
    fn default() -> Self {
        Self {
            project: None,
            provider: None,
            trigger: None,
            status: None,
            kind: None,
            cursor: None,
            limit: DEFAULT_PAGE_SIZE,
        }
    }
}

impl ActivityFilters {
    /// Validate and normalize one activity read.
    #[allow(clippy::too_many_arguments)]
    pub fn parse(
        project: Option<String>,
        provider: Option<String>,
        trigger: Option<String>,
        status: Option<String>,
        kind: Option<String>,
        cursor: Option<String>,
        limit: Option<u32>,
    ) -> Result<Self, FilterError> {
        let project = normalize_filter(project, "project", 36)?;
        let trigger = normalize_filter(trigger, "trigger", 255)?;
        let provider = match normalize_filter(provider, "provider", 16)? {
            Some(value) => {
                let provider = value.to_ascii_lowercase();
                if !ProviderKind::ALL
                    .iter()
                    .any(|kind| kind.as_str() == provider)
                {
                    return Err(FilterError::Invalid {
                        field: "provider",
                        message: "must be github, slack or discord".into(),
                    });
                }
                Some(provider)
            }
            None => None,
        };
        let status = match normalize_filter(status, "status", 64)? {
            Some(value) => {
                let status = value.to_ascii_lowercase();
                if !known_status(&status) {
                    return Err(FilterError::Invalid {
                        field: "status",
                        message: "is not a known activity status".into(),
                    });
                }
                Some(status)
            }
            None => None,
        };
        let kind = match normalize_filter(kind, "kind", 16)? {
            Some(value) => Some(ActivityKind::parse(&value).ok_or(FilterError::Invalid {
                field: "kind",
                message: "must be receipt, run, step or execution".into(),
            })?),
            None => None,
        };
        let cursor = match normalize_filter(cursor, "cursor", 128)? {
            Some(value) => Some(ActivityCursor::decode(&value)?),
            None => None,
        };
        let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE).clamp(1, MAX_PAGE_SIZE);
        Ok(Self {
            project,
            provider,
            trigger,
            status,
            kind,
            cursor,
            limit,
        })
    }
}

fn normalize_filter(
    value: Option<String>,
    field: &'static str,
    maximum: usize,
) -> Result<Option<String>, FilterError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > maximum {
        return Err(FilterError::Invalid {
            field,
            message: format!("must be at most {maximum} characters"),
        });
    }
    if value.chars().any(char::is_control) {
        return Err(FilterError::Invalid {
            field,
            message: "must not contain control characters".into(),
        });
    }
    Ok(Some(value))
}

/// True when `status` names a run, step, execution or receipt status.
pub fn known_status(status: &str) -> bool {
    matches!(
        status,
        "pending"
            | "running"
            | "succeeded"
            | "failed"
            | "skipped"
            | "interrupted"
            | "received"
            | "matched"
            | "unmatched"
            | "rejected"
            | "queued"
    )
}

/// One activity read's query fields.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ActivityQuery {
    /// Project public id.
    #[serde(default)]
    pub project: Option<String>,
    /// Provider kind.
    #[serde(default)]
    pub provider: Option<String>,
    /// Trigger public id or name.
    #[serde(default)]
    pub trigger: Option<String>,
    /// Record status.
    #[serde(default)]
    pub status: Option<String>,
    /// Record kind.
    #[serde(default)]
    pub kind: Option<String>,
    /// Opaque page cursor.
    #[serde(default)]
    pub cursor: Option<String>,
    /// Page size.
    #[serde(default)]
    pub limit: Option<u32>,
}

impl ActivityQuery {
    /// Validate this query into filters.
    pub fn filters(&self) -> Result<ActivityFilters, FilterError> {
        ActivityFilters::parse(
            self.project.clone(),
            self.provider.clone(),
            self.trigger.clone(),
            self.status.clone(),
            self.kind.clone(),
            self.cursor.clone(),
            self.limit,
        )
    }
}

/// One row in the combined activity history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    /// Kind of record.
    pub kind: ActivityKind,
    /// Stable id of the record.
    pub id: String,
    /// Project public id, when known.
    pub project: Option<String>,
    /// Provider kind, when known.
    pub provider: Option<String>,
    /// Trigger public id.
    pub trigger: Option<String>,
    /// Trigger name.
    pub trigger_name: Option<String>,
    /// Owning run public id.
    pub run: Option<String>,
    /// Authored step id.
    pub step: Option<String>,
    /// Execution id.
    pub execution: Option<String>,
    /// Kind-specific status.
    pub status: String,
    /// Event type for a receipt.
    pub event_type: Option<String>,
    /// Actor for a run or receipt.
    pub actor: Option<String>,
    /// Whether a receipt matched a trigger.
    pub matched: Option<bool>,
    /// How many duplicate deliveries this receipt has seen.
    pub duplicates: Option<u32>,
    /// Step or execution output.
    pub output: Option<String>,
    /// Step failure detail.
    pub error: Option<String>,
    /// How a run continued its conversation.
    pub continuation: Option<String>,
    /// Observation time used for ordering.
    pub at: DateTime<Utc>,
}

/// One page of activity history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPage {
    /// Entries, newest first.
    pub items: Vec<ActivityEntry>,
    /// Cursor for the next page, when one exists.
    pub next_cursor: Option<String>,
    /// True when another page exists.
    pub has_more: bool,
}

/// How one run related to the agent serving its conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrivalDetail {
    /// Created, continued or restored.
    pub mode: ContinuationMode,
    /// Provider the arrival came from, when it was an event.
    pub provider: Option<String>,
    /// Conversation the arrival belonged to.
    pub conversation_key: Option<String>,
    /// True when this arrival steered a busy agent.
    pub steered: bool,
    /// Existing agent execution reused by this arrival.
    pub reused_execution: Option<String>,
    /// Receipt that recorded the arrival.
    pub receipt: Option<String>,
    /// Output limits that apply to this arrival alone.
    pub output_limits: Vec<StepOutputLimits>,
}

/// One run plus its arrival metadata.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRunDetail {
    /// The run and its ordered steps.
    pub run: crate::entities::run::RunView,
    /// How the run was created, continued or restored.
    pub arrival: ArrivalDetail,
}

/// True when an execution state means the agent is still working.
pub fn execution_busy(status: ExecutionStatus) -> bool {
    matches!(status, ExecutionStatus::Queued | ExecutionStatus::Running)
}

/// True when a step state means the step is still working.
pub fn step_running(status: StepStatus) -> bool {
    matches!(status, StepStatus::Pending | StepStatus::Running)
}

/// True when a run state means the run is still working.
pub fn run_running(status: RunStatus) -> bool {
    matches!(status, RunStatus::Pending | RunStatus::Running)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn receipt_keys_normalize_the_unique_identity() {
        let key = receipt_key(ProviderKind::Github, "  GitHub-Main ", " delivery-1 ").expect("key");
        assert_eq!(key.provider, "github");
        assert_eq!(key.connection, "github-main");
        assert_eq!(key.delivery_key, "delivery-1");

        assert!(receipt_key(ProviderKind::Github, " ", "delivery-1").is_err());
        assert!(
            receipt_key(ProviderKind::Github, "github-main", " ").is_err(),
            "a blank delivery key is refused"
        );
        assert!(
            receipt_key(ProviderKind::Github, "github-main", &"x".repeat(256)).is_err(),
            "an oversized delivery key is refused"
        );
    }

    #[test]
    fn an_existing_receipt_is_a_duplicate_not_a_second_run() {
        assert_eq!(decide_receipt(false), ReceiptAction::Insert);
        assert_eq!(decide_receipt(true), ReceiptAction::Duplicate);
    }

    #[test]
    fn conversation_keys_group_one_thread_and_separate_others() {
        let first = conversation_key_for(ProviderKind::Slack, "channel", "C123", Some("thread-7"))
            .expect("key");
        let same = conversation_key_for(ProviderKind::Slack, "channel", "c123", Some("THREAD-7"))
            .expect("key");
        let other_thread =
            conversation_key_for(ProviderKind::Slack, "channel", "C123", Some("thread-8"))
                .expect("key");
        let other_provider =
            conversation_key_for(ProviderKind::Discord, "channel", "C123", Some("thread-7"))
                .expect("key");

        assert_eq!(first, same);
        assert_ne!(first, other_thread);
        assert_ne!(first, other_provider);
        assert_eq!(
            conversation_key_for(ProviderKind::Github, "repository", "org/repo", None),
            None,
            "a delivery with no conversation id creates a fresh agent"
        );
    }

    #[test]
    fn continuation_plans_create_continue_steer_and_restore() {
        assert_eq!(
            plan_continuation(None),
            ContinuationPlan {
                mode: ContinuationMode::Created,
                steer: false,
                reuse_execution_id: None,
            }
        );

        let idle = ConversationState {
            agent_execution_id: Some("agent-1".into()),
            busy: false,
        };
        assert_eq!(
            plan_continuation(Some(&idle)),
            ContinuationPlan {
                mode: ContinuationMode::Continued,
                steer: false,
                reuse_execution_id: Some("agent-1".into()),
            }
        );

        let busy = ConversationState {
            agent_execution_id: Some("agent-1".into()),
            busy: true,
        };
        assert_eq!(
            plan_continuation(Some(&busy)),
            ContinuationPlan {
                mode: ContinuationMode::Continued,
                steer: true,
                reuse_execution_id: Some("agent-1".into()),
            }
        );

        let gone = ConversationState {
            agent_execution_id: None,
            busy: false,
        };
        assert_eq!(
            plan_continuation(Some(&gone)),
            ContinuationPlan {
                mode: ContinuationMode::Restored,
                steer: false,
                reuse_execution_id: None,
            }
        );
    }

    #[test]
    fn output_limits_are_resolved_per_step_for_one_arrival() {
        let workflow: Workflow = serde_json::from_value(json!({
            "name": "triage",
            "trigger": "issue-comment",
            "steps": [{
                "id": "reply",
                "outputs": [
                    { "capability": "slack.reply", "maximum": 2, "required": true }
                ]
            }]
        }))
        .expect("workflow");
        let limits = output_limits(&workflow).expect("limits");
        assert_eq!(
            limits.get("reply"),
            Some(&vec![OutputLimit {
                capability: "slack.reply".into(),
                maximum: 2,
                required: true,
            }])
        );
        let wire = step_output_limits(&limits);
        assert_eq!(wire[0].step, "reply");
        assert_eq!(wire[0].limits[0].maximum, 2);
    }

    #[test]
    fn cursors_round_trip_and_reject_junk() {
        let at: DateTime<Utc> = "2026-10-21T00:00:00.000001Z".parse().expect("time");
        let cursor = ActivityCursor {
            at,
            id: "run-1".into(),
        };
        let encoded = cursor.encode();
        assert_eq!(ActivityCursor::decode(&encoded).expect("decode"), cursor);

        assert!(ActivityCursor::decode("nope").is_err());
        assert!(ActivityCursor::decode("123:").is_err());
        assert!(ActivityCursor::decode("0:run-1").is_ok());
    }

    #[test]
    fn filters_validate_and_clamp_without_accepting_unknown_values() {
        let filters = ActivityFilters::parse(
            Some(" project-1 ".into()),
            Some("GitHub".into()),
            Some("auto-fix".into()),
            Some("Succeeded".into()),
            Some("Run".into()),
            None,
            Some(500),
        )
        .expect("filters");
        assert_eq!(filters.project.as_deref(), Some("project-1"));
        assert_eq!(filters.provider.as_deref(), Some("github"));
        assert_eq!(filters.trigger.as_deref(), Some("auto-fix"));
        assert_eq!(filters.status.as_deref(), Some("succeeded"));
        assert_eq!(filters.kind, Some(ActivityKind::Run));
        assert_eq!(filters.limit, MAX_PAGE_SIZE);

        assert!(
            ActivityFilters::parse(None, Some("gitlab".into()), None, None, None, None, None)
                .is_err()
        );
        assert!(
            ActivityFilters::parse(None, None, None, Some("exploded".into()), None, None, None)
                .is_err()
        );
        assert!(
            ActivityFilters::parse(None, None, None, None, Some("daemon".into()), None, None)
                .is_err()
        );
    }

    #[test]
    fn the_query_shape_is_camel_case_and_denies_unknown_fields() {
        let query: ActivityQuery = serde_json::from_value(json!({
            "project": "project-1",
            "provider": "github",
            "trigger": "auto-fix",
            "status": "succeeded",
            "kind": "run",
            "cursor": "0:run-1",
            "limit": 10
        }))
        .expect("query");
        let filters = query.filters().expect("filters");
        assert_eq!(filters.limit, 10);
        assert!(serde_json::from_value::<ActivityQuery>(json!({ "nope": true })).is_err());
    }

    #[test]
    fn statuses_and_kinds_use_snake_case_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&ContinuationMode::Restored).expect("serialize"),
            "\"restored\""
        );
        assert_eq!(
            serde_json::to_string(&ActivityKind::Execution).expect("serialize"),
            "\"execution\""
        );
        assert_eq!(ReceiptStatus::Matched.as_str(), "matched");
        assert!(known_status("queued"));
        assert!(!known_status("exploded"));
    }

    #[test]
    fn running_helpers_follow_the_lifecycle() {
        assert!(execution_busy(ExecutionStatus::Running));
        assert!(!execution_busy(ExecutionStatus::Succeeded));
        assert!(step_running(StepStatus::Pending));
        assert!(!step_running(StepStatus::Skipped));
        assert!(run_running(RunStatus::Running));
        assert!(!run_running(RunStatus::Failed));
    }
}
