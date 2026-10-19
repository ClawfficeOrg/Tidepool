//! `ApiKey` entity — organization-scoped automation credentials (task 1.1.3).
//!
//! See `migrations/20261019000003_create_api_keys_and_audit.up.sql`. The full
//! key is `<prefix>.<secret>` and is returned once; only the prefix and an
//! Argon2id hash of the secret are stored. [`ApiKeyView`] never carries either
//! credential half.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::str::FromStr;

/// Lifecycle state of an API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyStatus {
    /// Usable for authentication.
    Active,
    /// Rejected immediately; kept for audit.
    Revoked,
}

// MySQL ENUM decoding follows the same pattern as `PlayerStatus`.
use sqlx::TypeInfo as _;

impl sqlx::Type<sqlx::MySql> for ApiKeyStatus {
    fn type_info() -> sqlx::mysql::MySqlTypeInfo {
        <String as sqlx::Type<sqlx::MySql>>::type_info()
    }
    fn compatible(ty: &sqlx::mysql::MySqlTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::MySql>>::compatible(ty)
            || ty.name().eq_ignore_ascii_case("ENUM")
    }
}

impl<'r> sqlx::Decode<'r, sqlx::MySql> for ApiKeyStatus {
    fn decode(
        value: sqlx::mysql::MySqlValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = <String as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?;
        match s.as_str() {
            "active" => Ok(ApiKeyStatus::Active),
            "revoked" => Ok(ApiKeyStatus::Revoked),
            other => Err(format!("unknown API key status: {other:?}").into()),
        }
    }
}

/// The capabilities an API key may carry.
///
/// These are conservative, named capabilities rather than wildcard admin: a
/// key gets exactly what the automation needs. Unknown strings are refused at
/// creation and when loading a stored key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiKeyScope {
    /// Read projects and their resources.
    #[serde(rename = "projects:read")]
    ProjectsRead,
    /// Validate configuration without installing it.
    #[serde(rename = "configuration:validate")]
    ConfigurationValidate,
    /// Install configuration into a project.
    #[serde(rename = "configuration:install")]
    ConfigurationInstall,
    /// Dispatch runs.
    #[serde(rename = "runs:dispatch")]
    RunsDispatch,
    /// Enroll daemons/hosts.
    #[serde(rename = "daemons:enroll")]
    DaemonsEnroll,
}

impl ApiKeyScope {
    /// Every scope the service accepts, in a stable order.
    pub const ALL: [ApiKeyScope; 5] = [
        ApiKeyScope::ProjectsRead,
        ApiKeyScope::ConfigurationValidate,
        ApiKeyScope::ConfigurationInstall,
        ApiKeyScope::RunsDispatch,
        ApiKeyScope::DaemonsEnroll,
    ];

    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ApiKeyScope::ProjectsRead => "projects:read",
            ApiKeyScope::ConfigurationValidate => "configuration:validate",
            ApiKeyScope::ConfigurationInstall => "configuration:install",
            ApiKeyScope::RunsDispatch => "runs:dispatch",
            ApiKeyScope::DaemonsEnroll => "daemons:enroll",
        }
    }
}

impl FromStr for ApiKeyScope {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|scope| scope.as_str() == value)
            .ok_or(())
    }
}

/// Full API key row, mirroring `api_keys`.
#[derive(Debug, Clone, FromRow)]
pub struct ApiKey {
    /// Internal primary key.
    pub id: u64,
    /// Public UUID; the id exposed to members.
    pub public_id: String,
    /// Internal organization id.
    pub organization_id: u64,
    /// Human-readable key name.
    pub name: String,
    /// Key prefix (`tpk_...`), safe to display.
    pub prefix: String,
    /// Argon2id PHC string for the secret half.
    pub secret_hash: String,
    /// JSON array of scope strings as stored.
    pub scopes: String,
    /// Active or revoked.
    pub status: ApiKeyStatus,
    /// Member who created the key, if still present.
    pub created_by: Option<u64>,
    /// Last successful authentication.
    pub last_used_at: Option<DateTime<Utc>>,
    /// When the key was revoked.
    pub revoked_at: Option<DateTime<Utc>>,
    /// When the key was created.
    pub created_at: DateTime<Utc>,
    /// When the key last changed.
    pub updated_at: DateTime<Utc>,
}

impl ApiKey {
    /// Parse the stored scope JSON.
    ///
    /// Unknown entries are ignored defensively; `require_scope` still refuses
    /// anything not explicitly granted.
    pub fn scope_set(&self) -> Vec<ApiKeyScope> {
        serde_json::from_str::<Vec<ApiKeyScope>>(&self.scopes).unwrap_or_default()
    }

    /// True when this key carries `scope`.
    pub fn has_scope(&self, scope: ApiKeyScope) -> bool {
        self.scope_set().into_iter().any(|granted| granted == scope)
    }
}

/// Wire projection of an [`ApiKey`]; the secret and its hash never appear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKeyView {
    /// Public UUID.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Display prefix only.
    pub prefix: String,
    /// Granted scopes.
    pub scopes: Vec<ApiKeyScope>,
    /// Active or revoked.
    pub status: ApiKeyStatus,
    /// Last successful authentication.
    pub last_used_at: Option<DateTime<Utc>>,
    /// When revoked.
    pub revoked_at: Option<DateTime<Utc>>,
    /// When created.
    pub created_at: DateTime<Utc>,
}

impl From<&ApiKey> for ApiKeyView {
    fn from(key: &ApiKey) -> Self {
        Self {
            id: key.public_id.clone(),
            name: key.name.clone(),
            prefix: key.prefix.clone(),
            scopes: key.scope_set(),
            status: key.status,
            last_used_at: key.last_used_at,
            revoked_at: key.revoked_at,
            created_at: key.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ApiKey {
        ApiKey {
            id: 1,
            public_id: "11111111-1111-4111-8111-111111111111".into(),
            organization_id: 7,
            name: "CI".into(),
            prefix: "tpk_11111111".into(),
            secret_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".into(),
            scopes: r#"["projects:read","runs:dispatch"]"#.into(),
            status: ApiKeyStatus::Active,
            created_by: Some(2),
            last_used_at: None,
            revoked_at: None,
            created_at: "2026-10-19T12:00:00Z".parse().expect("ts"),
            updated_at: "2026-10-19T12:00:00Z".parse().expect("ts"),
        }
    }

    #[test]
    fn scopes_round_trip_the_wire_spelling() {
        let json = serde_json::to_string(&ApiKeyScope::ConfigurationValidate).expect("serialize");
        assert_eq!(json, r#""configuration:validate""#);
        assert_eq!(
            "configuration:validate".parse::<ApiKeyScope>(),
            Ok(ApiKeyScope::ConfigurationValidate)
        );
        assert!("admin:*".parse::<ApiKeyScope>().is_err());
    }

    #[test]
    fn scope_set_reads_stored_json_and_has_scope_is_exact() {
        let key = sample();
        assert!(key.has_scope(ApiKeyScope::ProjectsRead));
        assert!(key.has_scope(ApiKeyScope::RunsDispatch));
        assert!(!key.has_scope(ApiKeyScope::DaemonsEnroll));
    }

    #[test]
    fn view_never_exposes_either_credential_half() {
        let json = serde_json::to_string(&ApiKeyView::from(&sample())).expect("serialize");
        assert!(!json.contains("secret"));
        assert!(!json.contains("argon2"));
        assert!(json.contains("tpk_11111111"));
        assert!(json.contains("projects:read"));
    }
}
