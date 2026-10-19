//! `Host` entity — one enrolled machine in the pool (task 1.2.1).
//!
//! See `migrations/20261020000001_create_hosts.up.sql`. `public_id` is
//! immutable; `slug` is a friendly handle that may be renamed. Internal ids and
//! the organization id never leave the server.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Lifecycle state of a host.
///
/// `PendingApproval` is the state after registration and before a completed
/// enrollment/approval. Presence (1.2.3) moves an enrolled host between
/// `Connected` and `Offline`; `Revoked` is terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostStatus {
    /// Registered but not yet approved/enrolled.
    PendingApproval,
    /// Enrolled and currently connected; accepting dispatch.
    Connected,
    /// Enrolled but not currently connected.
    Offline,
    /// Relationship ended; refused everywhere.
    Revoked,
}

// MySQL returns ENUM columns as text; decode via String like PlayerStatus.
use sqlx::TypeInfo as _;

impl sqlx::Type<sqlx::MySql> for HostStatus {
    fn type_info() -> sqlx::mysql::MySqlTypeInfo {
        <String as sqlx::Type<sqlx::MySql>>::type_info()
    }
    fn compatible(ty: &sqlx::mysql::MySqlTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::MySql>>::compatible(ty)
            || ty.name().eq_ignore_ascii_case("ENUM")
    }
}

impl<'r> sqlx::Decode<'r, sqlx::MySql> for HostStatus {
    fn decode(
        value: sqlx::mysql::MySqlValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = <String as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?;
        match s.as_str() {
            "pending_approval" => Ok(HostStatus::PendingApproval),
            "connected" => Ok(HostStatus::Connected),
            "offline" => Ok(HostStatus::Offline),
            "revoked" => Ok(HostStatus::Revoked),
            other => Err(format!("unknown host status: {other:?}").into()),
        }
    }
}

/// Full host row, mirroring `hosts`.
#[derive(Debug, Clone, FromRow)]
pub struct Host {
    /// Internal primary key.
    pub id: u64,
    /// Immutable public UUID.
    pub public_id: String,
    /// Internal organization id.
    pub organization_id: u64,
    /// Mutable friendly handle.
    pub slug: String,
    /// Name shown in the control plane.
    pub display_name: String,
    /// Hostname reported at registration.
    pub hostname: String,
    /// Lifecycle state.
    pub status: HostStatus,
    /// Whether the separate `host.execute` grant has been made.
    pub execution_granted: bool,
    /// When the relationship was revoked.
    pub revoked_at: Option<DateTime<Utc>>,
    /// When the host was registered.
    pub created_at: DateTime<Utc>,
    /// When the host last changed.
    pub updated_at: DateTime<Utc>,
}

impl Host {
    /// Revoked hosts are refused by every endpoint and dispatch lookup.
    pub fn is_revoked(&self) -> bool {
        self.status == HostStatus::Revoked
    }
}

/// Wire projection of a [`Host`]; omits internal and organization ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostView {
    /// Immutable public UUID.
    pub id: String,
    /// Mutable friendly slug.
    pub slug: String,
    /// Display name.
    pub display_name: String,
    /// Hostname.
    pub hostname: String,
    /// Lifecycle state.
    pub status: HostStatus,
    /// True when the host may execute agents (`host.execute` grant).
    pub execution_enabled: bool,
    /// When revoked, if it is.
    pub revoked_at: Option<DateTime<Utc>>,
    /// When registered.
    pub created_at: DateTime<Utc>,
}

impl From<&Host> for HostView {
    fn from(host: &Host) -> Self {
        Self {
            id: host.public_id.clone(),
            slug: host.slug.clone(),
            display_name: host.display_name.clone(),
            hostname: host.hostname.clone(),
            status: host.status,
            execution_enabled: host.execution_granted,
            revoked_at: host.revoked_at,
            created_at: host.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Host {
        Host {
            id: 1,
            public_id: "11111111-1111-4111-8111-111111111111".into(),
            organization_id: 7,
            slug: "studio-mac".into(),
            display_name: "Studio Mac".into(),
            hostname: "Studio-Mac.local".into(),
            status: HostStatus::PendingApproval,
            execution_granted: false,
            revoked_at: None,
            created_at: "2026-10-20T00:00:00Z".parse().expect("ts"),
            updated_at: "2026-10-20T00:00:00Z".parse().expect("ts"),
        }
    }

    #[test]
    fn status_uses_snake_case() {
        assert_eq!(
            serde_json::to_string(&HostStatus::PendingApproval).unwrap(),
            "\"pending_approval\""
        );
        assert_eq!(
            serde_json::to_string(&HostStatus::Connected).unwrap(),
            "\"connected\""
        );
    }

    #[test]
    fn view_never_exposes_internal_ids() {
        let json = serde_json::to_string(&HostView::from(&sample())).expect("serialize");
        assert!(!json.contains("organization_id"));
        assert!(!json.contains("\"id\":1"));
        assert!(json.contains("studio-mac"));
    }

    #[test]
    fn execution_grant_is_separate_from_enrollment() {
        let host = sample();
        assert!(!host.execution_granted);
        let view = HostView::from(&host);
        assert!(!view.execution_enabled);
    }

    #[test]
    fn revoked_is_terminal_state() {
        let mut host = sample();
        assert!(!host.is_revoked());
        host.status = HostStatus::Revoked;
        assert!(host.is_revoked());
    }
}
