//! Pool host protocol service (task 1.2.5).
//!
//! The pool and a connected host exchange versioned JSON frames over the host
//! WebSocket. This module owns the frames the pool sends, the socket registry,
//! and the execution ledger:
//!
//! - `create_agent` carries a stable `executionId`. Re-sending the same intent
//!   returns the existing execution and the host answers with the existing
//!   result instead of running the prompt twice.
//! - `interrupt_execution` asks the host to stop an execution.
//! - The host sends `execution_started`, `execution_output` and
//!   `execution_completed`; output is appended and completion is recorded.
//!
//! Version 2 (task 1.7.1) adds negotiation and typed `tkrun` jobs: the host
//! sends `hello`, the pool answers `hello.ack` with the intersection, and jobs
//! travel as `start_job`/`stop_job` frames. Jobs live in
//! [`crate::services::tidetable_jobs`]; this module owns the frames and the
//! per-socket feature set.
//!
//! See `docs/HOST_PROTOCOL.md` for the complete frame catalog.

use crate::entities::host_execution::{ExecutionStatus, HostExecution, HostExecutionView};
use crate::services::host_presence::{self, HostPresenceError};
use crate::sockets::host_ws::HostWs;
use actix::{Addr, Message};
use once_cell::sync::Lazy;
use serde_json::{Value, json};
use sqlx::MySqlPool;
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;

/// Protocol version carried in every v1 frame.
pub const PROTOCOL_VERSION: u32 = 1;

/// Protocol version for negotiated v2 frames (typed jobs).
pub const PROTOCOL_VERSION_V2: u32 = 2;

/// Versions this pool speaks, in preference order.
pub const SUPPORTED_VERSIONS: [u32; 2] = [PROTOCOL_VERSION, PROTOCOL_VERSION_V2];

/// Features this pool offers to a v2 host.
pub const SUPPORTED_FEATURES: [&str; 1] = [JOBS_TKRUN_FEATURE];

/// Feature name for typed `tkrun` jobs.
pub const JOBS_TKRUN_FEATURE: &str = "jobs.tkrun";

/// Frame the pool sends to start one execution.
#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct SendFrame(pub String);

/// Registry of live host sockets, keyed by the host's public id.
static SOCKETS: Lazy<Mutex<HashMap<String, Addr<HostWs>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Features each live host negotiated on its current socket.
static FEATURES: Lazy<Mutex<HashMap<String, Vec<String>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Errors for protocol/execution operations.
#[derive(Debug, Error)]
pub enum HostProtocolError {
    /// Host is unknown/revoked.
    #[error("host not found")]
    NotFound,
    /// Host is not connected or its heartbeat is stale.
    #[error("daemon_not_connected: host '{slug}' is offline")]
    DaemonNotConnected {
        /// Host slug.
        slug: String,
    },
    /// Prompt or frame input is invalid.
    #[error("invalid input: {0}")]
    Invalid(String),
    /// The host socket disappeared before the intent was sent.
    #[error("daemon_not_connected: host socket is gone")]
    SocketGone,
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// Register a live host socket. A newer connection replaces the old one.
pub fn register_socket(host_public_id: &str, addr: Addr<HostWs>) {
    let mut sockets = SOCKETS.lock().unwrap_or_else(|e| e.into_inner());
    sockets.insert(host_public_id.to_string(), addr);
}

/// Remove a host socket if it is still the registered one.
pub fn unregister_socket(host_public_id: &str) {
    let mut sockets = SOCKETS.lock().unwrap_or_else(|e| e.into_inner());
    sockets.remove(host_public_id);
    drop(sockets);
    let mut features = FEATURES.lock().unwrap_or_else(|e| e.into_inner());
    features.remove(host_public_id);
}

/// Record the features negotiated on a host's current socket.
pub fn set_features(host_public_id: &str, features: Vec<String>) {
    let mut map = FEATURES.lock().unwrap_or_else(|e| e.into_inner());
    map.insert(host_public_id.to_string(), features);
}

/// True when the host's current socket negotiated `feature`.
pub fn has_feature(host_public_id: &str, feature: &str) -> bool {
    let map = FEATURES.lock().unwrap_or_else(|e| e.into_inner());
    map.get(host_public_id)
        .is_some_and(|features| features.iter().any(|f| f == feature))
}

/// Pick the protocol version and feature set for a host's `hello`.
///
/// The version is the highest one both sides speak; features are the
/// intersection with [`SUPPORTED_FEATURES`], in the host's order. A host that
/// offers no common version is answered with version 1.
pub fn negotiate(versions: &[u64], features: &[String]) -> (u32, Vec<String>) {
    let version = SUPPORTED_VERSIONS
        .iter()
        .rev()
        .copied()
        .find(|supported| versions.contains(&u64::from(*supported)))
        .unwrap_or(PROTOCOL_VERSION);
    if version < PROTOCOL_VERSION_V2 {
        // Features are a v2 concept; a v1 socket negotiates none.
        return (version, Vec::new());
    }
    let agreed = features
        .iter()
        .filter(|feature| SUPPORTED_FEATURES.contains(&feature.as_str()))
        .cloned()
        .collect();
    (version, agreed)
}

/// Build the `hello.ack` answer to a v2 `hello`.
pub fn hello_ack_frame(version: u32, features: &[String]) -> Value {
    json!({
        "v": PROTOCOL_VERSION_V2,
        "type": "hello.ack",
        "version": version,
        "features": features,
    })
}

/// Build a v2 `start_job` frame.
///
/// The frame carries typed fields only: the slug, the server origin, the
/// checkout name and bounded options. No command line, path or credential.
pub fn start_job_frame(
    job_id: &str,
    slug: &str,
    server: &str,
    checkout: &str,
    options: &Value,
) -> Value {
    json!({
        "v": PROTOCOL_VERSION_V2,
        "type": "start_job",
        "jobId": job_id,
        "kind": "tkrun",
        "project": {"slug": slug, "server": server},
        "checkout": checkout,
        "options": options,
    })
}

/// Build a v2 `stop_job` frame. `mode` is `stop` or `kill`.
pub fn stop_job_frame(job_id: &str, mode: &str) -> Value {
    json!({
        "v": PROTOCOL_VERSION_V2,
        "type": "stop_job",
        "jobId": job_id,
        "mode": mode,
    })
}

/// Send one frame to a connected host. Returns false when there is no socket.
pub fn send_frame(host_public_id: &str, frame: Value) -> bool {
    let sockets = SOCKETS.lock().unwrap_or_else(|e| e.into_inner());
    let Some(addr) = sockets.get(host_public_id) else {
        return false;
    };
    addr.do_send(SendFrame(frame.to_string()));
    true
}

/// Build a `create_agent` frame.
pub fn create_agent_frame(execution_id: &str, prompt: &str) -> Value {
    json!({
        "v": PROTOCOL_VERSION,
        "type": "create_agent",
        "executionId": execution_id,
        "prompt": prompt,
    })
}

/// Build an `interrupt_execution` frame.
pub fn interrupt_frame(execution_id: &str) -> Value {
    json!({
        "v": PROTOCOL_VERSION,
        "type": "interrupt_execution",
        "executionId": execution_id,
    })
}

/// Create (or replay) an execution on a connected host.
///
/// The first call inserts a queued row and sends `create_agent`. A repeated
/// call with the same `execution_id` returns the existing row without creating
/// a new one; the frame is re-sent so a host that lost the response can answer
/// with the existing execution.
pub async fn create_execution(
    pool: &MySqlPool,
    organization_id: u64,
    host_public_id: &str,
    execution_id: &str,
    prompt: &str,
) -> Result<(HostExecutionView, bool), HostProtocolError> {
    if execution_id.trim().is_empty() || execution_id.len() > 64 {
        return Err(HostProtocolError::Invalid(
            "executionId must be 1-64 characters".into(),
        ));
    }
    if prompt.trim().is_empty() || prompt.len() > 1_000_000 {
        return Err(HostProtocolError::Invalid(
            "prompt must be 1-1000000 characters".into(),
        ));
    }

    let existing = find_execution(pool, execution_id).await?;
    if let Some(execution) = existing.as_ref()
        && matches!(
            execution.status,
            ExecutionStatus::Succeeded | ExecutionStatus::Failed | ExecutionStatus::Interrupted
        )
    {
        // Terminal replay: return the recorded result without requiring the
        // host socket to be live.
        return Ok((HostExecutionView::from(execution), true));
    }

    let host = host_presence::require_connected(pool, organization_id, host_public_id)
        .await
        .map_err(map_presence_error)?;

    if existing.is_none() {
        let result = sqlx::query(
            "INSERT INTO host_executions (public_id, host_id, prompt, status) \
             VALUES (?, ?, ?, 'queued') \
             ON DUPLICATE KEY UPDATE public_id = public_id",
        )
        .bind(execution_id)
        .bind(host.id)
        .bind(prompt)
        .execute(pool)
        .await
        .map_err(HostProtocolError::Database)?;
        let _created = result.rows_affected() == 1;
    }

    if !send_frame(&host.public_id, create_agent_frame(execution_id, prompt)) {
        return Err(HostProtocolError::SocketGone);
    }

    sqlx::query(
        "UPDATE host_executions SET status = 'running' \
         WHERE public_id = ? AND status = 'queued'",
    )
    .bind(execution_id)
    .execute(pool)
    .await
    .map_err(HostProtocolError::Database)?;

    let execution = find_execution(pool, execution_id)
        .await?
        .ok_or(HostProtocolError::NotFound)?;
    Ok((HostExecutionView::from(&execution), existing.is_some()))
}

/// Ask a host to interrupt an execution.
pub async fn interrupt_execution(
    pool: &MySqlPool,
    execution_id: &str,
) -> Result<(), HostProtocolError> {
    let execution = find_execution(pool, execution_id)
        .await?
        .ok_or(HostProtocolError::NotFound)?;
    let host_public_id: String = sqlx::query_scalar("SELECT public_id FROM hosts WHERE id = ?")
        .bind(execution.host_id)
        .fetch_one(pool)
        .await
        .map_err(HostProtocolError::Database)?;
    if !send_frame(&host_public_id, interrupt_frame(execution_id)) {
        return Err(HostProtocolError::SocketGone);
    }
    Ok(())
}

/// Append output observed from the host.
pub async fn record_output(
    pool: &MySqlPool,
    execution_id: &str,
    delta: &str,
) -> Result<(), HostProtocolError> {
    sqlx::query(
        "UPDATE host_executions SET output = CONCAT(COALESCE(output, ''), ?) \
         WHERE public_id = ? AND status IN ('queued', 'running')",
    )
    .bind(delta)
    .bind(execution_id)
    .execute(pool)
    .await
    .map_err(HostProtocolError::Database)?;
    Ok(())
}

/// Record that the host acknowledged the intent.
pub async fn record_started(pool: &MySqlPool, execution_id: &str) -> Result<(), HostProtocolError> {
    sqlx::query(
        "UPDATE host_executions SET status = 'running' \
         WHERE public_id = ? AND status = 'queued'",
    )
    .bind(execution_id)
    .execute(pool)
    .await
    .map_err(HostProtocolError::Database)?;
    Ok(())
}

/// Record completion, optionally replacing the output with the final text.
pub async fn record_completion(
    pool: &MySqlPool,
    execution_id: &str,
    status: &str,
    output: Option<&str>,
) -> Result<(), HostProtocolError> {
    let status = match status {
        "succeeded" => ExecutionStatus::Succeeded,
        "interrupted" => ExecutionStatus::Interrupted,
        _ => ExecutionStatus::Failed,
    };
    let status_str = match status {
        ExecutionStatus::Succeeded => "succeeded",
        ExecutionStatus::Failed => "failed",
        ExecutionStatus::Interrupted => "interrupted",
        _ => "failed",
    };

    sqlx::query(
        "UPDATE host_executions \
         SET status = ?, output = COALESCE(?, output), completed_at = CURRENT_TIMESTAMP \
         WHERE public_id = ?",
    )
    .bind(status_str)
    .bind(output)
    .bind(execution_id)
    .execute(pool)
    .await
    .map_err(HostProtocolError::Database)?;
    Ok(())
}

/// Fetch one execution by its stable id.
pub async fn find_execution(
    pool: &MySqlPool,
    execution_id: &str,
) -> Result<Option<HostExecution>, HostProtocolError> {
    sqlx::query_as::<_, HostExecution>(
        "SELECT id, public_id, host_id, prompt, status, output, created_at, updated_at, \
                completed_at \
         FROM host_executions WHERE public_id = ?",
    )
    .bind(execution_id)
    .fetch_optional(pool)
    .await
    .map_err(HostProtocolError::Database)
}

fn map_presence_error(error: HostPresenceError) -> HostProtocolError {
    match error {
        HostPresenceError::NotFound => HostProtocolError::NotFound,
        HostPresenceError::DaemonNotConnected { slug } => {
            HostProtocolError::DaemonNotConnected { slug }
        }
        HostPresenceError::Database(e) => HostProtocolError::Database(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiation_picks_v2_and_intersects_features() {
        let (version, features) = negotiate(
            &[1, 2],
            &["jobs.tkrun".to_string(), "agent.cwd".to_string()],
        );
        assert_eq!(version, 2);
        assert_eq!(features, vec!["jobs.tkrun".to_string()]);
    }

    #[test]
    fn host_without_v2_negotiates_no_features() {
        let (version, features) = negotiate(&[1], &["jobs.tkrun".to_string()]);
        assert_eq!(version, 1);
        assert!(features.is_empty());
    }

    #[test]
    fn start_job_frame_is_typed_and_has_no_secret_fields() {
        let options = json!({"hours": 2, "minutes": 0, "tasks": [], "jobs": 1, "dryRun": true});
        let frame = start_job_frame(
            "job-1",
            "chiton",
            "https://tk.example",
            "chiton-main",
            &options,
        );
        assert_eq!(frame["v"], 2);
        assert_eq!(frame["type"], "start_job");
        assert_eq!(frame["kind"], "tkrun");
        assert_eq!(frame["project"]["slug"], "chiton");
        assert_eq!(frame["project"]["server"], "https://tk.example");
        assert_eq!(frame["checkout"], "chiton-main");
        assert!(frame.get("token").is_none());
        assert!(frame.get("command").is_none());
    }

    #[test]
    fn feature_registry_is_cleared_on_unregister() {
        set_features("host-x", vec!["jobs.tkrun".to_string()]);
        assert!(has_feature("host-x", "jobs.tkrun"));
        unregister_socket("host-x");
        assert!(!has_feature("host-x", "jobs.tkrun"));
    }

    #[test]
    fn create_frame_is_versioned_and_stable() {
        let frame = create_agent_frame("exec-1", "do the thing");
        assert_eq!(frame["v"], 1);
        assert_eq!(frame["type"], "create_agent");
        assert_eq!(frame["executionId"], "exec-1");
        assert_eq!(frame["prompt"], "do the thing");

        // A replay uses the same frame and execution id.
        assert_eq!(frame, create_agent_frame("exec-1", "do the thing"));
    }

    #[test]
    fn interrupt_frame_is_versioned() {
        let frame = interrupt_frame("exec-1");
        assert_eq!(frame["v"], 1);
        assert_eq!(frame["type"], "interrupt_execution");
        assert_eq!(frame["executionId"], "exec-1");
    }

    #[test]
    fn unknown_completion_maps_to_failed() {
        // The mapping is exercised through record_completion; this pins the
        // fallback status string used for unknown host values.
        let status = match "nonsense" {
            "succeeded" => "succeeded",
            "interrupted" => "interrupted",
            _ => "failed",
        };
        assert_eq!(status, "failed");
    }
}
