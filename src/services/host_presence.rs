//! Host presence and heartbeat (task 1.2.3).
//!
//! Presence is derived, never client-declared:
//!
//! - a host is **connected** while it holds at least one host WebSocket and its
//!   last heartbeat is within [`HEARTBEAT_TIMEOUT_SECS`];
//! - dropping the last connection makes it **offline**;
//! - a missed heartbeat makes it offline even if the socket is still open;
//! - `revoked` wins over everything (the host row is filtered from lookups).
//!
//! The in-memory registry is authoritative for the live process; `hosts.status`
//! is the durable projection used by the API. On startup the process marks any
//! leftover `connected` rows offline, because no connection survived the
//! restart.
//!
//! Dispatch asks [`require_connected`]. An offline host fails immediately with
//! `daemon_not_connected`, so the trigger must fire again instead of the event
//! being queued for later.

use crate::entities::host::Host;
use crate::middleware::metrics::metrics;
use crate::services::hosts;
use chrono::{DateTime, Duration, Utc};
use once_cell::sync::Lazy;
use sqlx::MySqlPool;
use std::collections::HashMap;
use std::sync::Mutex;
use thiserror::Error;

/// A host is offline when no heartbeat arrives within this window.
pub const HEARTBEAT_TIMEOUT_SECS: i64 = 90;

/// Domain errors for presence/dispatch.
#[derive(Debug, Error)]
pub enum HostPresenceError {
    /// No live host with that id.
    #[error("host not found")]
    NotFound,
    /// The host is enrolled but not connected / heartbeat is stale.
    #[error("daemon_not_connected: host '{slug}' is offline")]
    DaemonNotConnected {
        /// Host slug for the error detail.
        slug: String,
    },
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

impl HostPresenceError {
    /// Stable problem code for dispatch-time failures.
    pub fn code(&self) -> &'static str {
        match self {
            HostPresenceError::DaemonNotConnected { .. } => "daemon_not_connected",
            HostPresenceError::NotFound => "not_found",
            HostPresenceError::Database(_) => "internal_error",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ConnState {
    connections: usize,
    last_heartbeat: DateTime<Utc>,
}

/// In-memory connection/heartbeat registry. Methods take `now` so tests can
/// advance a fake clock without sleeping.
#[derive(Default)]
pub struct HostPresenceRegistry {
    conns: Mutex<HashMap<String, ConnState>>,
}

impl HostPresenceRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a connection. Returns true when this is the first connection
    /// (online transition).
    pub fn connect(&self, host_public_id: &str, now: DateTime<Utc>) -> bool {
        let mut conns = self.conns.lock().unwrap_or_else(|e| e.into_inner());
        let entry = conns
            .entry(host_public_id.to_string())
            .or_insert(ConnState {
                connections: 0,
                last_heartbeat: now,
            });
        let was_offline = entry.connections == 0;
        entry.connections += 1;
        entry.last_heartbeat = now;
        was_offline
    }

    /// Unregister a connection. Returns true when the last connection left
    /// (offline transition).
    pub fn disconnect(&self, host_public_id: &str) -> bool {
        let mut conns = self.conns.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = conns.get_mut(host_public_id) else {
            return false;
        };
        entry.connections = entry.connections.saturating_sub(1);
        if entry.connections == 0 {
            conns.remove(host_public_id);
            true
        } else {
            false
        }
    }

    /// Record a heartbeat. Returns false if the host has no live connection.
    pub fn heartbeat(&self, host_public_id: &str, now: DateTime<Utc>) -> bool {
        let mut conns = self.conns.lock().unwrap_or_else(|e| e.into_inner());
        match conns.get_mut(host_public_id) {
            Some(entry) if entry.connections > 0 => {
                entry.last_heartbeat = now;
                true
            }
            _ => false,
        }
    }

    /// True when the host has a connection and a fresh heartbeat at `now`.
    pub fn is_online(&self, host_public_id: &str, now: DateTime<Utc>) -> bool {
        let conns = self.conns.lock().unwrap_or_else(|e| e.into_inner());
        conns.get(host_public_id).is_some_and(|entry| {
            entry.connections > 0
                && now - entry.last_heartbeat <= Duration::seconds(HEARTBEAT_TIMEOUT_SECS)
        })
    }

    /// Remove every connection for a host (revocation). Returns true when the
    /// host was online.
    pub fn forget(&self, host_public_id: &str) -> bool {
        let mut conns = self.conns.lock().unwrap_or_else(|e| e.into_inner());
        conns
            .remove(host_public_id)
            .is_some_and(|entry| entry.connections > 0)
    }

    /// Number of hosts online at `now`.
    pub fn online_count(&self, now: DateTime<Utc>) -> usize {
        let conns = self.conns.lock().unwrap_or_else(|e| e.into_inner());
        conns
            .values()
            .filter(|entry| {
                entry.connections > 0
                    && now - entry.last_heartbeat <= Duration::seconds(HEARTBEAT_TIMEOUT_SECS)
            })
            .count()
    }
}

/// Process-wide presence registry.
static REGISTRY: Lazy<HostPresenceRegistry> = Lazy::new(HostPresenceRegistry::new);

/// Access the process-wide registry (tests use fixed clocks).
pub fn registry() -> &'static HostPresenceRegistry {
    &REGISTRY
}

/// Register a host connection and project `connected` into the database.
pub async fn host_connected(
    pool: &MySqlPool,
    organization_id: u64,
    host_public_id: &str,
) -> Result<Host, HostPresenceError> {
    let host = hosts::find_host(pool, organization_id, host_public_id)
        .await
        .map_err(map_hosts_error)?
        .ok_or(HostPresenceError::NotFound)?;

    if registry().connect(&host.public_id, Utc::now()) {
        metrics().host_connected();
    }
    sqlx::query("UPDATE hosts SET status = 'connected' WHERE id = ? AND status <> 'revoked'")
        .bind(host.id)
        .execute(pool)
        .await
        .map_err(HostPresenceError::Database)?;

    Ok(host)
}

/// Drop a host connection; the last drop projects `offline`.
pub async fn host_disconnected(
    pool: &MySqlPool,
    host_public_id: &str,
) -> Result<(), HostPresenceError> {
    if registry().disconnect(host_public_id) {
        metrics().host_disconnected();
        sqlx::query(
            "UPDATE hosts SET status = 'offline' \
             WHERE public_id = ? AND status = 'connected'",
        )
        .bind(host_public_id)
        .execute(pool)
        .await
        .map_err(HostPresenceError::Database)?;
    }
    Ok(())
}

/// Record a heartbeat. A heartbeat from a host with no connection is ignored,
/// so a client cannot declare itself present.
pub async fn heartbeat(pool: &MySqlPool, host_public_id: &str) -> Result<bool, HostPresenceError> {
    let now = Utc::now();
    if !registry().heartbeat(host_public_id, now) {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE hosts SET status = 'connected' \
         WHERE public_id = ? AND status = 'offline'",
    )
    .bind(host_public_id)
    .execute(pool)
    .await
    .map_err(HostPresenceError::Database)?;
    Ok(true)
}

/// Forget a revoked host and drop it from the connected gauge immediately.
pub async fn forget_host(pool: &MySqlPool, host_public_id: &str) -> Result<(), HostPresenceError> {
    if registry().forget(host_public_id) {
        metrics().host_disconnected();
    }
    sqlx::query(
        "UPDATE hosts SET status = 'revoked'          WHERE public_id = ? AND status <> 'revoked'",
    )
    .bind(host_public_id)
    .execute(pool)
    .await
    .map_err(HostPresenceError::Database)?;
    Ok(())
}

/// Mark connected hosts whose heartbeat is stale as offline, and resync the
/// gauge. Returns how many rows changed.
pub async fn refresh_stale(pool: &MySqlPool, now: DateTime<Utc>) -> Result<u64, HostPresenceError> {
    let rows: Vec<(u64, String)> =
        sqlx::query_as("SELECT id, public_id FROM hosts WHERE status = 'connected'")
            .fetch_all(pool)
            .await
            .map_err(HostPresenceError::Database)?;

    let mut changed = 0u64;
    for (id, public_id) in rows {
        if !registry().is_online(&public_id, now) {
            let result = sqlx::query(
                "UPDATE hosts SET status = 'offline' WHERE id = ? AND status = 'connected'",
            )
            .bind(id)
            .execute(pool)
            .await
            .map_err(HostPresenceError::Database)?;
            changed += result.rows_affected();
        }
    }
    metrics().hosts_connected_set(registry().online_count(now) as i64);
    Ok(changed)
}

/// Require a host to be connected before dispatch.
///
/// Rejects an offline or stale host immediately with
/// [`HostPresenceError::DaemonNotConnected`]; there is no queue for later.
pub async fn require_connected(
    pool: &MySqlPool,
    organization_id: u64,
    host_public_id: &str,
) -> Result<Host, HostPresenceError> {
    let now = Utc::now();
    let host = hosts::find_host(pool, organization_id, host_public_id)
        .await
        .map_err(map_hosts_error)?
        .ok_or(HostPresenceError::NotFound)?;

    if !registry().is_online(&host.public_id, now) {
        sqlx::query(
            "UPDATE hosts SET status = 'offline' \
             WHERE id = ? AND status = 'connected'",
        )
        .bind(host.id)
        .execute(pool)
        .await
        .map_err(HostPresenceError::Database)?;
        metrics().hosts_connected_set(registry().online_count(now) as i64);
        return Err(HostPresenceError::DaemonNotConnected { slug: host.slug });
    }
    Ok(host)
}

/// Startup projection: no connection survives a process restart.
pub async fn mark_all_offline(pool: &MySqlPool) -> Result<u64, HostPresenceError> {
    let result = sqlx::query("UPDATE hosts SET status = 'offline' WHERE status = 'connected'")
        .execute(pool)
        .await
        .map_err(HostPresenceError::Database)?;
    metrics().hosts_connected_set(0);
    Ok(result.rows_affected())
}

fn map_hosts_error(error: hosts::HostServiceError) -> HostPresenceError {
    match error {
        hosts::HostServiceError::NotFound => HostPresenceError::NotFound,
        hosts::HostServiceError::Database(e) => HostPresenceError::Database(e),
        other => {
            tracing::error!(error = %other, "host presence lookup failed");
            HostPresenceError::NotFound
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        "2026-10-20T00:00:00Z".parse::<DateTime<Utc>>().expect("ts") + Duration::seconds(seconds)
    }

    #[test]
    fn connection_lifecycle_moves_online_and_offline() {
        let registry = HostPresenceRegistry::new();
        assert!(!registry.is_online("host-1", at(0)));

        assert!(registry.connect("host-1", at(0)));
        assert!(registry.is_online("host-1", at(0)));
        assert!(!registry.connect("host-1", at(10))); // second connection
        assert!(!registry.disconnect("host-1"));
        assert!(registry.is_online("host-1", at(20)));
        assert!(registry.disconnect("host-1"));
        assert!(!registry.is_online("host-1", at(20)));
    }

    #[test]
    fn stale_heartbeat_marks_offline_within_the_window() {
        let registry = HostPresenceRegistry::new();
        registry.connect("host-1", at(0));
        assert!(registry.is_online("host-1", at(HEARTBEAT_TIMEOUT_SECS)));
        assert!(!registry.is_online("host-1", at(HEARTBEAT_TIMEOUT_SECS + 1)));

        assert!(registry.heartbeat("host-1", at(HEARTBEAT_TIMEOUT_SECS + 1)));
        assert!(registry.is_online("host-1", at(HEARTBEAT_TIMEOUT_SECS + 1)));
    }

    #[test]
    fn heartbeat_without_connection_is_ignored() {
        let registry = HostPresenceRegistry::new();
        assert!(!registry.heartbeat("host-1", at(0)));
        assert!(!registry.is_online("host-1", at(0)));
    }

    #[test]
    fn online_count_filters_stale_entries() {
        let registry = HostPresenceRegistry::new();
        registry.connect("host-1", at(0));
        registry.connect("host-2", at(0));
        assert_eq!(registry.online_count(at(0)), 2);
        registry.disconnect("host-2");
        assert_eq!(registry.online_count(at(0)), 1);
        assert_eq!(registry.online_count(at(HEARTBEAT_TIMEOUT_SECS + 1)), 0);
    }

    #[test]
    fn forget_removes_every_connection() {
        let registry = HostPresenceRegistry::new();
        registry.connect("host-1", at(0));
        registry.connect("host-1", at(1));
        assert_eq!(registry.online_count(at(1)), 1);
        assert!(registry.forget("host-1"));
        assert!(!registry.is_online("host-1", at(1)));
        assert!(!registry.forget("host-1"));
    }

    #[test]
    fn offline_dispatch_uses_the_documented_code() {
        let error = HostPresenceError::DaemonNotConnected {
            slug: "studio-mac".into(),
        };
        assert_eq!(error.code(), "daemon_not_connected");
        assert_eq!(
            error.to_string(),
            "daemon_not_connected: host 'studio-mac' is offline"
        );
    }
}
