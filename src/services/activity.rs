//! Activity log, receipts and conversations (task 1.4.7).
//!
//! Every provider delivery gets one [`event_receipts`] row before matching
//! starts. The row's unique key is the delivery identity, so a provider retry
//! increments `duplicate_count` on the same receipt and never creates a second
//! run. Matching then finishes the receipt as `matched`, `unmatched` or
//! `rejected`, recording the runs it produced.
//!
//! The read side combines receipts, runs, run steps and executions into one
//! ordered page. Every branch is scoped to the caller's organization and the
//! optional project filter is applied to the union, so filters can be combined
//! without one project's rows leaking into another's page.
//!
//! [`activity_run_detail`] adds the arrival record to a run: whether it created,
//! continued or restored an agent, whether it steered a busy agent, and the
//! output limits that belong to that arrival alone.

use crate::entities::activity::{
    ActivityCursor, ActivityEntry, ActivityFilters, ActivityKind, ActivityPage, ActivityRunDetail,
    ArrivalDetail, ContinuationMode, ConversationState, FilterError, MAX_CONVERSATION_KEY_CHARS,
    OutputLimit, ReceiptAction, ReceiptKey, ReceiptRow, ReceiptStatus, decide_receipt, receipt_key,
    step_output_limits,
};
use crate::services::runs::{self, RunError};
use chrono::{DateTime, Utc};
use sqlx::{FromRow, MySqlPool};
use std::collections::BTreeMap;
use thiserror::Error;

/// Activity service errors.
#[derive(Debug, Error)]
pub enum ActivityError {
    /// A query filter is invalid.
    #[error(transparent)]
    Filter(#[from] FilterError),
    /// No run with that public id in this organization.
    #[error("run not found")]
    NotFound,
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
    /// A stored activity projection could not be decoded.
    #[error("invalid activity record: {0}")]
    Invalid(String),
    /// Reading the run and its steps failed.
    #[error(transparent)]
    Run(#[from] RunError),
}

impl ActivityError {
    /// Stable problem code for the hub API.
    pub fn code(&self) -> &'static str {
        match self {
            ActivityError::Filter(_) => "validation_failed",
            ActivityError::NotFound => "not_found",
            ActivityError::Invalid(_) | ActivityError::Run(_) => "internal_error",
            ActivityError::Database(_) => "internal_error",
        }
    }
}

/// A receipt write outcome.
#[derive(Debug, Clone)]
pub struct ReceiptOutcome {
    /// The receipt row, newly inserted or already present.
    pub receipt: ReceiptRow,
    /// True when this delivery key was already recorded.
    pub duplicate: bool,
}

/// One delivery's receipt fields.
#[derive(Debug, Clone)]
pub struct DeliveryFacts<'a> {
    /// Provider that delivered the event.
    pub provider: crate::entities::connection::ProviderKind,
    /// Connection slug the event arrived through.
    pub connection: &'a str,
    /// Stable provider delivery id.
    pub delivery_key: &'a str,
    /// Internal event type.
    pub event_type: &'a str,
    /// Provider action, when the delivery has one.
    pub action: Option<&'a str>,
    /// Provider resource kind.
    pub resource_kind: &'a str,
    /// Provider resource id.
    pub resource_id: &'a str,
    /// Actor that caused the event.
    pub actor: &'a str,
    /// Channel or thread, when the provider has one.
    pub channel: Option<&'a str>,
}

/// Record one delivery receipt before matching.
///
/// `INSERT IGNORE` and the unique delivery key are the concurrency arbiter: a
/// retry finds the existing receipt, counts itself on `duplicate_count`, and
/// returns the original runs through [`receipt_run_ids`].
pub async fn begin_receipt(
    pool: &MySqlPool,
    organization_id: u64,
    project_id: Option<u64>,
    project_public_id: Option<&str>,
    delivery: &DeliveryFacts<'_>,
) -> Result<ReceiptOutcome, ActivityError> {
    let key = receipt_key(
        delivery.provider,
        delivery.connection,
        delivery.delivery_key,
    )?;
    let public_id = uuid::Uuid::new_v4().to_string();
    let result = sqlx::query(
        "INSERT IGNORE INTO event_receipts \
         (public_id, organization_id, project_id, project_public_id, provider, connection, \
          delivery_key, event_type, action, resource_kind, resource_id, actor, channel, status) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'received')",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(project_id)
    .bind(project_public_id)
    .bind(&key.provider)
    .bind(&key.connection)
    .bind(&key.delivery_key)
    .bind(delivery.event_type)
    .bind(delivery.action)
    .bind(delivery.resource_kind)
    .bind(delivery.resource_id)
    .bind(delivery.actor)
    .bind(delivery.channel)
    .execute(pool)
    .await
    .map_err(ActivityError::Database)?;

    let duplicate = decide_receipt(result.rows_affected() == 0) == ReceiptAction::Duplicate;
    if duplicate {
        sqlx::query(
            "UPDATE event_receipts SET duplicate_count = duplicate_count + 1 \
             WHERE organization_id = ? AND provider = ? AND connection = ? AND delivery_key = ?",
        )
        .bind(organization_id)
        .bind(&key.provider)
        .bind(&key.connection)
        .bind(&key.delivery_key)
        .execute(pool)
        .await
        .map_err(ActivityError::Database)?;
    }

    let receipt = fetch_receipt(pool, organization_id, &key).await?;
    Ok(ReceiptOutcome { receipt, duplicate })
}

/// Finish a receipt's matching result.
pub async fn finish_receipt(
    pool: &MySqlPool,
    organization_id: u64,
    receipt_public_id: &str,
    status: ReceiptStatus,
    matched_triggers: usize,
    run_public_ids: &[String],
) -> Result<(), ActivityError> {
    let runs_json = if run_public_ids.is_empty() {
        None
    } else {
        Some(
            serde_json::to_string(run_public_ids)
                .map_err(|error| ActivityError::Invalid(error.to_string()))?,
        )
    };
    sqlx::query(
        "UPDATE event_receipts SET status = ?, matched_triggers = ?, run_public_ids = ? \
         WHERE public_id = ? AND organization_id = ?",
    )
    .bind(status.as_str())
    .bind(matched_triggers as u32)
    .bind(runs_json.as_deref())
    .bind(receipt_public_id)
    .bind(organization_id)
    .execute(pool)
    .await
    .map_err(ActivityError::Database)?;
    Ok(())
}

/// Runs a receipt recorded, newest values first.
pub fn receipt_run_ids(receipt: &ReceiptRow) -> Result<Vec<String>, ActivityError> {
    let Some(json) = receipt.run_public_ids.as_deref() else {
        return Ok(Vec::new());
    };
    serde_json::from_str(json)
        .map_err(|error| ActivityError::Invalid(format!("receipt runs are not JSON: {error}")))
}

async fn fetch_receipt(
    pool: &MySqlPool,
    organization_id: u64,
    key: &ReceiptKey,
) -> Result<ReceiptRow, ActivityError> {
    sqlx::query_as::<_, ReceiptRow>(
        "SELECT id, public_id, organization_id, project_id, project_public_id, provider, \
                connection, delivery_key, event_type, action, resource_kind, resource_id, actor, \
                channel, status, matched_triggers, run_public_ids, duplicate_count, received_at, \
                created_at, updated_at \
         FROM event_receipts \
         WHERE organization_id = ? AND provider = ? AND connection = ? AND delivery_key = ?",
    )
    .bind(organization_id)
    .bind(&key.provider)
    .bind(&key.connection)
    .bind(&key.delivery_key)
    .fetch_one(pool)
    .await
    .map_err(ActivityError::Database)
}

/// Look up the agent serving one trigger's conversation.
pub async fn find_conversation(
    pool: &MySqlPool,
    organization_id: u64,
    trigger_id: u64,
    conversation_key: &str,
) -> Result<Option<ConversationState>, ActivityError> {
    let row: Option<(Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT c.agent_execution_id, e.public_id, e.status \
         FROM agent_conversations c \
         LEFT JOIN host_executions e ON e.public_id = c.agent_execution_id \
         WHERE c.organization_id = ? AND c.trigger_id = ? AND c.conversation_key = ?",
    )
    .bind(organization_id)
    .bind(trigger_id)
    .bind(conversation_key)
    .fetch_optional(pool)
    .await
    .map_err(ActivityError::Database)?;
    Ok(
        row.map(|(agent_execution_id, execution_public_id, status)| {
            // The LEFT JOIN keeps the row when the execution is deleted, so a missing
            // execution id means the agent is gone and the conversation must restore.
            ConversationState {
                agent_execution_id: execution_public_id.and(agent_execution_id),
                busy: matches!(status.as_deref(), Some("queued" | "running")),
            }
        }),
    )
}

/// Record which agent serves a trigger's conversation after dispatch.
#[allow(clippy::too_many_arguments)]
pub async fn record_conversation(
    pool: &MySqlPool,
    organization_id: u64,
    project_id: u64,
    trigger_id: u64,
    conversation_key: &str,
    agent_execution_id: Option<&str>,
    run_public_id: &str,
    busy: bool,
    output_limits_json: &str,
) -> Result<(), ActivityError> {
    if conversation_key.trim().is_empty()
        || conversation_key.chars().count() > MAX_CONVERSATION_KEY_CHARS
    {
        return Err(ActivityError::Invalid(
            "conversation key must be 1-512 characters".into(),
        ));
    }
    sqlx::query(
        "INSERT INTO agent_conversations \
         (public_id, organization_id, project_id, trigger_id, conversation_key, \
          agent_execution_id, run_public_id, busy, output_limits_json) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON DUPLICATE KEY UPDATE agent_execution_id = VALUES(agent_execution_id), \
             run_public_id = VALUES(run_public_id), busy = VALUES(busy), \
             output_limits_json = VALUES(output_limits_json), updated_at = CURRENT_TIMESTAMP",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(organization_id)
    .bind(project_id)
    .bind(trigger_id)
    .bind(conversation_key)
    .bind(agent_execution_id)
    .bind(run_public_id)
    .bind(busy)
    .bind(output_limits_json)
    .execute(pool)
    .await
    .map_err(ActivityError::Database)?;
    Ok(())
}

/// Read one page of combined activity history.
pub async fn list_activity(
    pool: &MySqlPool,
    organization_id: u64,
    filters: &ActivityFilters,
) -> Result<ActivityPage, ActivityError> {
    let plan = plan_activity_sql(filters);
    let mut query = sqlx::query_as::<_, ActivityRow>(&plan.sql);
    for bind in &plan.binds {
        query = match bind {
            SqlBind::Organization => query.bind(organization_id),
            SqlBind::Kind(kind) => query.bind(kind.as_str()),
            SqlBind::Project(project) => query.bind(project.as_str()),
            SqlBind::Provider(provider) => query.bind(provider.as_str()),
            SqlBind::Trigger(trigger) => query.bind(trigger.as_str()),
            SqlBind::Status(status) => query.bind(status.as_str()),
            SqlBind::CursorAt(at) => query.bind(*at),
            SqlBind::CursorId(id) => query.bind(id.as_str()),
            SqlBind::Limit(limit) => query.bind(*limit),
        };
    }
    let rows = query
        .fetch_all(pool)
        .await
        .map_err(ActivityError::Database)?;

    let has_more = rows.len() > filters.limit as usize;
    let mut items = Vec::with_capacity(rows.len().min(filters.limit as usize));
    for row in rows.into_iter().take(filters.limit as usize) {
        items.push(row.into_entry()?);
    }
    let next_cursor = if has_more {
        items.last().map(|item| {
            ActivityCursor {
                at: item.at,
                id: item.id.clone(),
            }
            .encode()
        })
    } else {
        None
    };
    Ok(ActivityPage {
        items,
        next_cursor,
        has_more,
    })
}

/// Read one run plus the arrival record that explains how it started.
pub async fn activity_run_detail(
    pool: &MySqlPool,
    organization_id: u64,
    run_public_id: &str,
) -> Result<ActivityRunDetail, ActivityError> {
    let run = runs::find_run(pool, organization_id, run_public_id)
        .await?
        .ok_or(ActivityError::NotFound)?;
    let row = sqlx::query_as::<_, RunActivityRow>(
        "SELECT provider, receipt_public_id, conversation_key, CAST(continuation AS CHAR) \
                AS continuation, steered, reused_execution_id, output_limits_json \
         FROM runs WHERE id = ? AND organization_id = ?",
    )
    .bind(run.id)
    .bind(organization_id)
    .fetch_one(pool)
    .await
    .map_err(ActivityError::Database)?;

    let mode = match row.continuation.as_str() {
        "created" => ContinuationMode::Created,
        "continued" => ContinuationMode::Continued,
        "restored" => ContinuationMode::Restored,
        other => {
            return Err(ActivityError::Invalid(format!(
                "unknown continuation mode: {other:?}"
            )));
        }
    };
    let limits: BTreeMap<String, Vec<OutputLimit>> = match row.output_limits_json.as_deref() {
        Some(json) => serde_json::from_str(json).map_err(|error| {
            ActivityError::Invalid(format!("output limits are not JSON: {error}"))
        })?,
        None => BTreeMap::new(),
    };

    Ok(ActivityRunDetail {
        run: runs::run_view(pool, &run).await?,
        arrival: ArrivalDetail {
            mode,
            provider: row.provider,
            conversation_key: row.conversation_key,
            steered: row.steered,
            reused_execution: row.reused_execution_id,
            receipt: row.receipt_public_id,
            output_limits: step_output_limits(&limits),
        },
    })
}

/// Every row the activity union can produce.
#[derive(Debug, Clone, FromRow)]
struct ActivityRow {
    kind: String,
    id: String,
    project: Option<String>,
    provider: Option<String>,
    trigger_public_id: Option<String>,
    trigger_name: Option<String>,
    run_public_id: Option<String>,
    step_id: Option<String>,
    execution_id: Option<String>,
    status: String,
    at: DateTime<Utc>,
    event_type: Option<String>,
    actor: Option<String>,
    matched: Option<bool>,
    duplicates: Option<u32>,
    output: Option<String>,
    error: Option<String>,
    continuation: Option<String>,
}

impl ActivityRow {
    fn into_entry(self) -> Result<ActivityEntry, ActivityError> {
        let kind = ActivityKind::parse(&self.kind).ok_or_else(|| {
            ActivityError::Invalid(format!("unknown activity kind: {}", self.kind))
        })?;
        Ok(ActivityEntry {
            kind,
            id: self.id,
            project: self.project,
            provider: self.provider,
            trigger: self.trigger_public_id,
            trigger_name: self.trigger_name,
            run: self.run_public_id,
            step: self.step_id,
            execution: self.execution_id,
            status: self.status,
            event_type: self.event_type,
            actor: self.actor,
            matched: self.matched,
            duplicates: self.duplicates,
            output: self.output,
            error: self.error,
            continuation: self.continuation,
            at: self.at,
        })
    }
}

/// Run columns the arrival detail reads.
#[derive(Debug, Clone, FromRow)]
struct RunActivityRow {
    provider: Option<String>,
    receipt_public_id: Option<String>,
    conversation_key: Option<String>,
    continuation: String,
    steered: bool,
    reused_execution_id: Option<String>,
    output_limits_json: Option<String>,
}

/// One value the activity SQL needs to bind.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SqlBind {
    Organization,
    Kind(String),
    Project(String),
    Provider(String),
    Trigger(String),
    Status(String),
    CursorAt(DateTime<Utc>),
    CursorId(String),
    Limit(i64),
}

/// A built activity query and its binds, in the order the placeholders appear.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ActivitySqlPlan {
    sql: String,
    binds: Vec<SqlBind>,
}

/// The union of receipts, runs, steps and executions.
///
/// Each branch is independently organization-scoped. The project, provider,
/// trigger, status, kind and cursor filters are applied to the union as a
/// whole, which is what stops a filter from reaching across projects.
const ACTIVITY_UNION: &str = "\
SELECT kind, id, project, provider, trigger_public_id, trigger_name, run_public_id, \
       step_id, execution_id, status, at, event_type, actor, matched, duplicates, output, \
       error, continuation \
FROM ( \
    SELECT 'run' AS kind, r.public_id AS id, r.project_public_id AS project, \
           r.provider AS provider, r.trigger_public_id AS trigger_public_id, \
           r.trigger_name AS trigger_name, r.public_id AS run_public_id, \
           NULL AS step_id, NULL AS execution_id, CAST(r.status AS CHAR) AS status, \
           COALESCE(r.completed_at, r.started_at, r.created_at) AS at, \
           NULL AS event_type, r.actor AS actor, NULL AS matched, NULL AS duplicates, \
           NULL AS output, NULL AS error, CAST(r.continuation AS CHAR) AS continuation \
    FROM runs r WHERE r.organization_id = ? \
    UNION ALL \
    SELECT 'step', s.public_id, r.project_public_id, r.provider, r.trigger_public_id, \
           r.trigger_name, r.public_id, s.step_id, s.execution_id, CAST(s.status AS CHAR), \
           COALESCE(s.completed_at, s.started_at, s.created_at), \
           NULL, r.actor, NULL, NULL, s.output, s.error, NULL \
    FROM run_steps s JOIN runs r ON r.id = s.run_id WHERE r.organization_id = ? \
    UNION ALL \
    SELECT 'execution', e.public_id, r.project_public_id, r.provider, r.trigger_public_id, \
           r.trigger_name, r.public_id, s.step_id, e.public_id, CAST(e.status AS CHAR), \
           COALESCE(e.completed_at, e.updated_at, e.created_at), \
           NULL, r.actor, NULL, NULL, e.output, NULL, NULL \
    FROM host_executions e \
    JOIN run_steps s ON s.execution_id = e.public_id \
    JOIN runs r ON r.id = s.run_id WHERE r.organization_id = ? \
    UNION ALL \
    SELECT 'receipt', er.public_id, er.project_public_id, er.provider, NULL, NULL, NULL, \
           NULL, NULL, CAST(er.status AS CHAR), er.received_at, er.event_type, er.actor, \
           CASE WHEN er.status = 'matched' THEN 1 ELSE 0 END, er.duplicate_count, \
           NULL, NULL, NULL \
    FROM event_receipts er WHERE er.organization_id = ? \
) activity";

fn plan_activity_sql(filters: &ActivityFilters) -> ActivitySqlPlan {
    let mut clauses: Vec<&'static str> = Vec::new();
    let mut binds = vec![
        SqlBind::Organization,
        SqlBind::Organization,
        SqlBind::Organization,
        SqlBind::Organization,
    ];

    if let Some(kind) = filters.kind {
        clauses.push("kind = ?");
        binds.push(SqlBind::Kind(kind.as_str().to_string()));
    }
    if let Some(project) = &filters.project {
        clauses.push("project = ?");
        binds.push(SqlBind::Project(project.clone()));
    }
    if let Some(provider) = &filters.provider {
        clauses.push("provider = ?");
        binds.push(SqlBind::Provider(provider.clone()));
    }
    if let Some(trigger) = &filters.trigger {
        clauses.push("(trigger_public_id = ? OR trigger_name = ?)");
        binds.push(SqlBind::Trigger(trigger.clone()));
        binds.push(SqlBind::Trigger(trigger.clone()));
    }
    if let Some(status) = &filters.status {
        clauses.push("status = ?");
        binds.push(SqlBind::Status(status.clone()));
    }
    if let Some(cursor) = &filters.cursor {
        clauses.push("(at < ? OR (at = ? AND id < ?))");
        binds.push(SqlBind::CursorAt(cursor.at));
        binds.push(SqlBind::CursorAt(cursor.at));
        binds.push(SqlBind::CursorId(cursor.id.clone()));
    }

    let mut sql = String::from(ACTIVITY_UNION);
    if !clauses.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&clauses.join(" AND "));
    }
    sql.push_str(" ORDER BY at DESC, id DESC LIMIT ?");
    binds.push(SqlBind::Limit(i64::from(filters.limit) + 1));

    ActivitySqlPlan { sql, binds }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    #[test]
    fn every_branch_is_organization_scoped() {
        let plan = plan_activity_sql(&ActivityFilters::default());
        assert_eq!(count(&plan.sql, "organization_id = ?"), 4);
        assert_eq!(
            plan.binds,
            vec![
                SqlBind::Organization,
                SqlBind::Organization,
                SqlBind::Organization,
                SqlBind::Organization,
                SqlBind::Limit(i64::from(ActivityFilters::default().limit) + 1),
            ]
        );
    }

    #[test]
    fn filters_combine_in_a_stable_bind_order() {
        let filters = ActivityFilters::parse(
            Some("project-1".into()),
            Some("github".into()),
            Some("auto-fix".into()),
            Some("succeeded".into()),
            Some("run".into()),
            Some("0:run-9".into()),
            Some(10),
        )
        .expect("filters");
        let plan = plan_activity_sql(&filters);

        for predicate in [
            "kind = ?",
            "project = ?",
            "provider = ?",
            "(trigger_public_id = ? OR trigger_name = ?)",
            "status = ?",
            "(at < ? OR (at = ? AND id < ?))",
        ] {
            assert!(
                plan.sql.contains(predicate),
                "missing predicate {predicate}: {}",
                plan.sql
            );
        }
        assert_eq!(count(&plan.sql, "organization_id = ?"), 4);
        assert!(plan.sql.ends_with("ORDER BY at DESC, id DESC LIMIT ?"));

        let at: DateTime<Utc> = "1970-01-01T00:00:00Z".parse().expect("time");
        assert_eq!(
            plan.binds,
            vec![
                SqlBind::Organization,
                SqlBind::Organization,
                SqlBind::Organization,
                SqlBind::Organization,
                SqlBind::Kind("run".into()),
                SqlBind::Project("project-1".into()),
                SqlBind::Provider("github".into()),
                SqlBind::Trigger("auto-fix".into()),
                SqlBind::Trigger("auto-fix".into()),
                SqlBind::Status("succeeded".into()),
                SqlBind::CursorAt(at),
                SqlBind::CursorAt(at),
                SqlBind::CursorId("run-9".into()),
                SqlBind::Limit(11),
            ]
        );
    }

    #[test]
    fn a_project_filter_is_one_predicate_across_every_kind() {
        let filters =
            ActivityFilters::parse(Some("project-1".into()), None, None, None, None, None, None)
                .expect("filters");
        let plan = plan_activity_sql(&filters);
        assert_eq!(count(&plan.sql, "project = ?"), 1);
        assert_eq!(count(&plan.sql, "organization_id = ?"), 4);
        assert!(!plan.sql.contains("provider = ?"));
    }
}
