//! `Connection` entity — a provider account the pool receives events from
//! (task 1.3.1).
//!
//! Credentials are secret references (`credential_ref`), never inline values.
//! The reference is not projected on the wire; [`ConnectionView`] only says
//! whether a credential is configured.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Supported provider kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    /// GitHub App.
    Github,
    /// Slack app (webhook or socket mode).
    Slack,
    /// Discord bot gateway.
    Discord,
}

impl ProviderKind {
    /// Every supported kind.
    pub const ALL: [ProviderKind; 3] = [
        ProviderKind::Github,
        ProviderKind::Slack,
        ProviderKind::Discord,
    ];

    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::Github => "github",
            ProviderKind::Slack => "slack",
            ProviderKind::Discord => "discord",
        }
    }
}

use sqlx::TypeInfo as _;

impl sqlx::Type<sqlx::MySql> for ProviderKind {
    fn type_info() -> sqlx::mysql::MySqlTypeInfo {
        <String as sqlx::Type<sqlx::MySql>>::type_info()
    }
    fn compatible(ty: &sqlx::mysql::MySqlTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::MySql>>::compatible(ty)
            || ty.name().eq_ignore_ascii_case("ENUM")
    }
}

impl<'r> sqlx::Decode<'r, sqlx::MySql> for ProviderKind {
    fn decode(
        value: sqlx::mysql::MySqlValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = <String as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?;
        match s.as_str() {
            "github" => Ok(ProviderKind::Github),
            "slack" => Ok(ProviderKind::Slack),
            "discord" => Ok(ProviderKind::Discord),
            other => Err(format!("unknown provider kind: {other:?}").into()),
        }
    }
}

/// Connection lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    /// Delivering events.
    Active,
    /// Kept but no longer delivering.
    Disabled,
}

impl ConnectionStatus {
    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectionStatus::Active => "active",
            ConnectionStatus::Disabled => "disabled",
        }
    }
}

impl sqlx::Type<sqlx::MySql> for ConnectionStatus {
    fn type_info() -> sqlx::mysql::MySqlTypeInfo {
        <String as sqlx::Type<sqlx::MySql>>::type_info()
    }
    fn compatible(ty: &sqlx::mysql::MySqlTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::MySql>>::compatible(ty)
            || ty.name().eq_ignore_ascii_case("ENUM")
    }
}

impl<'r> sqlx::Decode<'r, sqlx::MySql> for ConnectionStatus {
    fn decode(
        value: sqlx::mysql::MySqlValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = <String as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?;
        match s.as_str() {
            "active" => Ok(ConnectionStatus::Active),
            "disabled" => Ok(ConnectionStatus::Disabled),
            other => Err(format!("unknown connection status: {other:?}").into()),
        }
    }
}

/// Who owns the configuration of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManagedBy {
    /// Stored in the database and editable through the API.
    Database,
    /// Supplied by `TIDEPOOL_CONNECTIONS`; wins over a database row.
    Environment,
}

/// Full connection row, mirroring `connections`.
#[derive(Debug, Clone, FromRow)]
pub struct Connection {
    /// Internal primary key.
    pub id: u64,
    /// Public UUID (or `env:<slug>` for environment-managed entries).
    pub public_id: String,
    /// Internal organization id.
    pub organization_id: u64,
    /// Provider kind.
    pub provider: ProviderKind,
    /// Mutable friendly handle.
    pub slug: String,
    /// Display name.
    pub display_name: String,
    /// Secret reference; never serialized.
    pub credential_ref: String,
    /// Active or disabled.
    pub status: ConnectionStatus,
    /// When created.
    pub created_at: DateTime<Utc>,
    /// When last changed.
    pub updated_at: DateTime<Utc>,
}

/// One provider resource identity a connection may deliver for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct ConnectionResource {
    /// Internal primary key.
    pub id: u64,
    /// Parent connection.
    pub connection_id: u64,
    /// Resource kind (`repository`, `workspace`, `guild`, ...).
    pub resource_kind: String,
    /// Provider-side id.
    pub external_id: String,
    /// Optional human label.
    pub display_name: Option<String>,
}

/// Wire projection of a resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionResourceView {
    /// Resource kind.
    pub resource_kind: String,
    /// Provider-side id.
    pub external_id: String,
    /// Optional human label.
    pub display_name: Option<String>,
}

impl From<&ConnectionResource> for ConnectionResourceView {
    fn from(resource: &ConnectionResource) -> Self {
        Self {
            resource_kind: resource.resource_kind.clone(),
            external_id: resource.external_id.clone(),
            display_name: resource.display_name.clone(),
        }
    }
}

/// Wire projection of a connection; never carries `credential_ref`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionView {
    /// Public id, or `env:<slug>` for environment-managed entries.
    pub id: String,
    /// Provider kind.
    pub provider: ProviderKind,
    /// Friendly slug.
    pub slug: String,
    /// Display name.
    pub display_name: String,
    /// Active or disabled.
    pub status: ConnectionStatus,
    /// Database or environment managed.
    pub managed_by: ManagedBy,
    /// True when a credential reference resolves.
    pub credential_configured: bool,
    /// Resources delivered for.
    pub resources: Vec<ConnectionResourceView>,
    /// When created.
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_kinds_use_the_wire_spelling() {
        assert_eq!(ProviderKind::Github.as_str(), "github");
        assert_eq!(
            serde_json::to_string(&ProviderKind::Slack).unwrap(),
            "\"slack\""
        );
        assert_eq!(ProviderKind::ALL.len(), 3);
    }

    #[test]
    fn managed_by_is_lowercase() {
        assert_eq!(
            serde_json::to_string(&ManagedBy::Environment).unwrap(),
            "\"environment\""
        );
    }
}
