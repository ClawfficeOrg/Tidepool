//! `PoolUser` entity — operator accounts that sign in to a pool.
//!
//! See `migrations/20261019000001_create_pool_identity.up.sql`. Credentials
//! stay on this row only: [`PoolUserView`] never carries `password_hash`.
//!
//! Role defaults to `member`. The first account created by setup or the
//! `TIDEBOOT_*` bootstrap is the organization `owner`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Operator role inside the organization.
///
/// Roles are ordered from most to least privileged (see
/// `docs/HUB_CAPABILITIES.md`): an owner manages members and keys, an admin
/// changes configuration, a member operates, a viewer reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PoolRole {
    /// Owns the organization; manages members and API keys.
    Owner,
    /// May change configuration but not membership.
    Admin,
    /// May operate the pool but not change organization-level settings.
    #[default]
    Member,
    /// Read-only access.
    Viewer,
}

impl PoolRole {
    /// Wire/DB spelling (`owner`, `admin`, `member`, `viewer`).
    pub fn as_str(self) -> &'static str {
        match self {
            PoolRole::Owner => "owner",
            PoolRole::Admin => "admin",
            PoolRole::Member => "member",
            PoolRole::Viewer => "viewer",
        }
    }
}

// MySQL returns ENUM columns as text, so decode via String (same pattern as
// `PlayerStatus` in `src/entities/player.rs`).
use sqlx::TypeInfo as _;

impl sqlx::Type<sqlx::MySql> for PoolRole {
    fn type_info() -> sqlx::mysql::MySqlTypeInfo {
        <String as sqlx::Type<sqlx::MySql>>::type_info()
    }
    fn compatible(ty: &sqlx::mysql::MySqlTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::MySql>>::compatible(ty)
            || ty.name().eq_ignore_ascii_case("ENUM")
    }
}

impl<'r> sqlx::Decode<'r, sqlx::MySql> for PoolRole {
    fn decode(
        value: sqlx::mysql::MySqlValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = <String as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?;
        match s.as_str() {
            "owner" => Ok(PoolRole::Owner),
            "admin" => Ok(PoolRole::Admin),
            "member" => Ok(PoolRole::Member),
            "viewer" => Ok(PoolRole::Viewer),
            other => Err(format!("unknown pool role: {other:?}").into()),
        }
    }
}

/// Full operator row, mirroring `pool_users`.
#[derive(Debug, Clone, FromRow)]
pub struct PoolUser {
    /// Internal primary key; never exposed.
    pub id: u64,
    /// Stable public identifier (UUID); used as the JWT subject.
    pub public_id: String,
    /// Internal organization id; never exposed.
    pub organization_id: u64,
    /// Login email (unique, case-insensitive under the table collation).
    pub email: String,
    /// Name shown in the control plane.
    pub display_name: String,
    /// Argon2id PHC string. Never serialized.
    pub password_hash: String,
    /// Role inside the organization.
    pub role: PoolRole,
    /// True while the account still uses a temporary bootstrap password.
    pub password_change_required: bool,
    /// Last successful sign-in, if any.
    pub last_login_at: Option<DateTime<Utc>>,
    /// When the account was created.
    pub created_at: DateTime<Utc>,
    /// When the account last changed.
    pub updated_at: DateTime<Utc>,
}

/// Public projection of a [`PoolUser`].
///
/// Critically omits the internal id, `organization_id` and `password_hash`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolUserView {
    /// Public UUID of the operator.
    pub id: String,
    /// Login email.
    pub email: String,
    /// Display name.
    pub display_name: String,
    /// Role inside the organization.
    pub role: PoolRole,
    /// True while a password change is required before normal use.
    pub password_change_required: bool,
    /// Last successful sign-in, if any.
    pub last_login_at: Option<DateTime<Utc>>,
    /// When the account was created.
    pub created_at: DateTime<Utc>,
}

impl From<&PoolUser> for PoolUserView {
    fn from(user: &PoolUser) -> Self {
        Self {
            id: user.public_id.clone(),
            email: user.email.clone(),
            display_name: user.display_name.clone(),
            role: user.role,
            password_change_required: user.password_change_required,
            last_login_at: user.last_login_at,
            created_at: user.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PoolUser {
        PoolUser {
            id: 1,
            public_id: "11111111-1111-4111-8111-111111111111".into(),
            organization_id: 7,
            email: "owner@example.com".into(),
            display_name: "Owner".into(),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".into(),
            role: PoolRole::Owner,
            password_change_required: true,
            last_login_at: None,
            created_at: "2026-10-19T12:00:00Z".parse().expect("timestamp"),
            updated_at: "2026-10-19T12:00:00Z".parse().expect("timestamp"),
        }
    }

    #[test]
    fn role_defaults_to_member() {
        assert_eq!(PoolRole::default(), PoolRole::Member);
        assert_eq!(PoolRole::Owner.as_str(), "owner");
        assert_eq!(PoolRole::Viewer.as_str(), "viewer");
    }

    #[test]
    fn view_never_exposes_password_or_internal_ids() {
        let json = serde_json::to_string(&PoolUserView::from(&sample())).expect("serialize");
        assert!(!json.contains("password_hash"));
        assert!(!json.contains("argon2"));
        assert!(!json.contains("organization_id"));
        assert!(!json.contains("\"id\":1"));
        assert!(json.contains("owner@example.com"));
    }
}
