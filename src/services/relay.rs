//! Session relay service (task 1.5.5).
//!
//! A session publishes events only after someone opts in, and publication
//! stops the moment it is stopped: the publisher checks the relay row on every
//! event rather than trusting a cached flag.
//!
//! Events are a small JSON object with a documented `kind`, a monotonic
//! `sequence` per session and no file contents. [`validate_payload`] refuses
//! keys that name credentials or artefacts, caps the payload size and rejects
//! nesting that could smuggle a blob through.
//!
//! Subscribers read with `sequence > after`, so a client that falls behind
//! backfills exactly once: the cursor is exclusive and a repeated read returns
//! the same window.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::{FromRow, MySqlPool};
use thiserror::Error;

/// Event kinds the relay accepts.
pub const RELAY_KINDS: [&str; 6] = ["started", "status", "output", "tool", "message", "stopped"];

/// Largest accepted payload, in bytes.
pub const MAX_PAYLOAD_BYTES: usize = 8 * 1024;

/// Keys a relayed payload may never contain, matched case-insensitively at any
/// depth.
const FORBIDDEN_KEYS: [&str; 12] = [
    "content",
    "contents",
    "credential",
    "credentials",
    "env",
    "environment",
    "file",
    "files",
    "password",
    "privatekey",
    "secret",
    "token",
];

/// Default page size for a backfill.
pub const DEFAULT_BACKFILL_LIMIT: u32 = 100;
/// Largest accepted backfill page.
pub const MAX_BACKFILL_LIMIT: u32 = 500;

/// Relay errors.
#[derive(Debug, Error)]
pub enum RelayError {
    /// Publication is off, or was stopped.
    #[error("relay is not enabled for this session")]
    NotEnabled,
    /// No such session in this organization.
    #[error("session not found")]
    NotFound,
    /// A payload violates the relay contract.
    #[error("invalid relay payload: {0}")]
    Invalid(String),
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// A relay's opt-in state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayState {
    /// The session id.
    pub session: String,
    /// True while events may be published.
    pub enabled: bool,
    /// When publication started.
    pub started_at: Option<DateTime<Utc>>,
    /// When publication was stopped; once set it never reopens.
    pub stopped_at: Option<DateTime<Utc>>,
}

/// One relayed event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayEvent {
    /// Immutable public id.
    pub id: String,
    /// Session the event belongs to.
    pub session: String,
    /// Monotonic per-session sequence.
    pub sequence: u64,
    /// Event kind from [`RELAY_KINDS`].
    pub kind: String,
    /// Small JSON object.
    pub payload: Value,
    /// When the event was published.
    pub created_at: DateTime<Utc>,
}

/// One backfill page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayPage {
    /// Events in sequence order.
    pub events: Vec<RelayEvent>,
    /// Cursor for the next read; absent when this is the newest page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<u64>,
}

#[derive(Debug, Clone, FromRow)]
struct RelaySessionRow {
    session_id: String,
    enabled: i8,
    started_at: Option<DateTime<Utc>>,
    stopped_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
struct RelayEventRow {
    public_id: String,
    session_id: String,
    sequence: u64,
    kind: String,
    payload_json: String,
    created_at: DateTime<Utc>,
}

fn state_from(row: RelaySessionRow) -> RelayState {
    RelayState {
        session: row.session_id,
        enabled: row.enabled == 1,
        started_at: row.started_at,
        stopped_at: row.stopped_at,
    }
}

/// Refuse payloads that could carry credentials or file contents.
pub fn validate_payload(payload: &Value) -> Result<(), RelayError> {
    let object = payload
        .as_object()
        .ok_or_else(|| RelayError::Invalid("payload must be a JSON object".to_string()))?;
    let encoded = serde_json::to_vec(payload)
        .map_err(|error| RelayError::Invalid(format!("payload is not serializable: {error}")))?;
    if encoded.len() > MAX_PAYLOAD_BYTES {
        return Err(RelayError::Invalid(format!(
            "payload must be at most {MAX_PAYLOAD_BYTES} bytes"
        )));
    }
    check_keys(object, 0)
}

fn check_keys(object: &Map<String, Value>, depth: usize) -> Result<(), RelayError> {
    if depth > 3 {
        return Err(RelayError::Invalid(
            "payload may nest at most three levels".to_string(),
        ));
    }
    for (key, value) in object {
        let normalized: String = key
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect();
        if FORBIDDEN_KEYS.contains(&normalized.as_str()) {
            return Err(RelayError::Invalid(format!(
                "payload may not carry {key:?}; the relay never carries credentials or file contents"
            )));
        }
        match value {
            Value::Object(nested) => check_keys(nested, depth + 1)?,
            Value::Array(items) => {
                for item in items {
                    if let Value::Object(nested) = item {
                        check_keys(nested, depth + 1)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validate an event kind.
pub fn validate_kind(kind: &str) -> Result<(), RelayError> {
    if RELAY_KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(RelayError::Invalid(format!(
            "unknown relay event kind: {kind:?}"
        )))
    }
}

async fn session_exists(
    pool: &MySqlPool,
    organization_id: u64,
    session: &str,
) -> Result<(), RelayError> {
    let found: Option<(u64,)> = sqlx::query_as(
        "SELECT e.id FROM host_executions e JOIN hosts h ON h.id = e.host_id \
         WHERE e.public_id = ? AND h.organization_id = ?",
    )
    .bind(session)
    .bind(organization_id)
    .fetch_optional(pool)
    .await
    .map_err(RelayError::Database)?;
    found.map(|_| ()).ok_or(RelayError::NotFound)
}

/// The relay state of a session; publishing is off when no row exists.
pub async fn state(
    pool: &MySqlPool,
    organization_id: u64,
    session: &str,
) -> Result<RelayState, RelayError> {
    session_exists(pool, organization_id, session).await?;
    let row: Option<RelaySessionRow> = sqlx::query_as(
        "SELECT session_id, enabled, started_at, stopped_at FROM relay_sessions \
         WHERE organization_id = ? AND session_id = ?",
    )
    .bind(organization_id)
    .bind(session)
    .fetch_optional(pool)
    .await
    .map_err(RelayError::Database)?;

    Ok(match row {
        Some(row) => state_from(row),
        None => RelayState {
            session: session.to_string(),
            enabled: false,
            started_at: None,
            stopped_at: None,
        },
    })
}

/// Opt a session in. Re-enabling a stopped relay is refused.
pub async fn enable(
    pool: &MySqlPool,
    organization_id: u64,
    session: &str,
) -> Result<RelayState, RelayError> {
    session_exists(pool, organization_id, session).await?;
    let existing: Option<RelaySessionRow> = sqlx::query_as(
        "SELECT session_id, enabled, started_at, stopped_at FROM relay_sessions \
         WHERE organization_id = ? AND session_id = ?",
    )
    .bind(organization_id)
    .bind(session)
    .fetch_optional(pool)
    .await
    .map_err(RelayError::Database)?;

    if existing
        .as_ref()
        .is_some_and(|row| row.stopped_at.is_some())
    {
        return Err(RelayError::Invalid(
            "publication for this session was stopped and cannot be resumed".to_string(),
        ));
    }

    sqlx::query(
        "INSERT INTO relay_sessions (organization_id, session_id, enabled, started_at) \
         VALUES (?, ?, 1, CURRENT_TIMESTAMP) \
         ON DUPLICATE KEY UPDATE enabled = 1, \
             started_at = COALESCE(started_at, CURRENT_TIMESTAMP)",
    )
    .bind(organization_id)
    .bind(session)
    .execute(pool)
    .await
    .map_err(RelayError::Database)?;

    state(pool, organization_id, session).await
}

/// Stop publication immediately.
pub async fn stop(
    pool: &MySqlPool,
    organization_id: u64,
    session: &str,
) -> Result<RelayState, RelayError> {
    session_exists(pool, organization_id, session).await?;
    sqlx::query(
        "INSERT INTO relay_sessions (organization_id, session_id, enabled, started_at, stopped_at) \
         VALUES (?, ?, 0, NULL, CURRENT_TIMESTAMP) \
         ON DUPLICATE KEY UPDATE enabled = 0, stopped_at = CURRENT_TIMESTAMP",
    )
    .bind(organization_id)
    .bind(session)
    .execute(pool)
    .await
    .map_err(RelayError::Database)?;

    state(pool, organization_id, session).await
}

/// Publish one event. Refused unless the relay is currently enabled.
pub async fn publish(
    pool: &MySqlPool,
    organization_id: u64,
    session: &str,
    kind: &str,
    payload: &Value,
) -> Result<RelayEvent, RelayError> {
    validate_kind(kind)?;
    validate_payload(payload)?;

    let current = state(pool, organization_id, session).await?;
    if !current.enabled {
        return Err(RelayError::NotEnabled);
    }

    let public_id = uuid::Uuid::new_v4().to_string();
    let payload_json = serde_json::to_string(payload)
        .map_err(|error| RelayError::Invalid(format!("payload is not JSON: {error}")))?;

    let mut tx = pool.begin().await.map_err(RelayError::Database)?;
    let next: Option<(u64,)> = sqlx::query_as(
        "SELECT CAST(COALESCE(MAX(sequence), 0) + 1 AS UNSIGNED) FROM relay_events \
         WHERE organization_id = ? AND session_id = ? FOR UPDATE",
    )
    .bind(organization_id)
    .bind(session)
    .fetch_optional(&mut *tx)
    .await
    .map_err(RelayError::Database)?;
    let sequence = next.map(|(value,)| value).unwrap_or(1);

    sqlx::query(
        "INSERT INTO relay_events (public_id, organization_id, session_id, sequence, kind, \
                                   payload_json) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(session)
    .bind(sequence)
    .bind(kind)
    .bind(&payload_json)
    .execute(&mut *tx)
    .await
    .map_err(RelayError::Database)?;
    tx.commit().await.map_err(RelayError::Database)?;

    let row: RelayEventRow = sqlx::query_as(
        "SELECT public_id, session_id, sequence, kind, payload_json, created_at \
         FROM relay_events WHERE public_id = ?",
    )
    .bind(&public_id)
    .fetch_one(pool)
    .await
    .map_err(RelayError::Database)?;

    event_from(row)
}

fn event_from(row: RelayEventRow) -> Result<RelayEvent, RelayError> {
    let payload: Value = serde_json::from_str(&row.payload_json)
        .map_err(|error| RelayError::Invalid(format!("stored payload is not JSON: {error}")))?;
    Ok(RelayEvent {
        id: row.public_id,
        session: row.session_id,
        sequence: row.sequence,
        kind: row.kind,
        payload,
        created_at: row.created_at,
    })
}

/// Read one backfill page after a cursor. Available whether or not the relay
/// still publishes, so a client can always finish reading what was published.
pub async fn backfill(
    pool: &MySqlPool,
    organization_id: u64,
    session: &str,
    after: Option<u64>,
    limit: Option<u32>,
) -> Result<RelayPage, RelayError> {
    session_exists(pool, organization_id, session).await?;
    let limit = limit
        .unwrap_or(DEFAULT_BACKFILL_LIMIT)
        .clamp(1, MAX_BACKFILL_LIMIT);

    let rows: Vec<RelayEventRow> = sqlx::query_as(
        "SELECT public_id, session_id, sequence, kind, payload_json, created_at \
         FROM relay_events WHERE organization_id = ? AND session_id = ? AND sequence > ? \
         ORDER BY sequence ASC LIMIT ?",
    )
    .bind(organization_id)
    .bind(session)
    .bind(after.unwrap_or(0))
    .bind(limit + 1)
    .fetch_all(pool)
    .await
    .map_err(RelayError::Database)?;

    let has_more = rows.len() as u32 > limit;
    let page_rows = if has_more {
        &rows[..limit as usize]
    } else {
        &rows[..]
    };
    let mut events = Vec::with_capacity(page_rows.len());
    for row in page_rows {
        events.push(event_from(row.clone())?);
    }
    let next = if has_more {
        events.last().map(|event| event.sequence)
    } else {
        None
    };
    Ok(RelayPage { events, next })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn payloads_with_credentials_or_files_are_refused() {
        for payload in [
            json!({"token": "abc"}),
            json!({"nested": {"private_key": "x"}}),
            json!({"files": [{"path": "a"}]}),
            json!({"Content": "block"}),
            json!({"env": {}}),
        ] {
            assert!(
                validate_payload(&payload).is_err(),
                "should refuse {payload}"
            );
        }
        assert!(validate_payload(&json!({"status": "running", "step": 2})).is_ok());
        assert!(validate_payload(&json!(["not", "an", "object"])).is_err());
    }

    #[test]
    fn payloads_are_bounded() {
        let big = json!({"status": "a".repeat(MAX_PAYLOAD_BYTES)});
        assert!(validate_payload(&big).is_err());
    }

    #[test]
    fn only_documented_kinds_are_accepted() {
        for kind in RELAY_KINDS {
            assert!(validate_kind(kind).is_ok());
        }
        assert!(validate_kind("screenshot").is_err());
    }
}
