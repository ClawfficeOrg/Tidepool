//! `Organization` entity — the single organization that owns this pool.
//!
//! See `migrations/20261019000001_create_pool_identity.up.sql`. One pool is
//! one organization in v1; the `singleton` column is an internal constraint
//! and is never projected.
//!
//! The internal `id` (BIGINT) is never exposed externally; APIs project
//! `public_id` (UUID) instead.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Full organization row, mirroring the public columns of `organizations`.
#[derive(Debug, Clone, FromRow)]
pub struct Organization {
    /// Internal primary key; never exposed.
    pub id: u64,
    /// Stable public identifier (UUID).
    pub public_id: String,
    /// Human-readable organization name.
    pub name: String,
    /// When the organization was created.
    pub created_at: DateTime<Utc>,
    /// When the organization last changed.
    pub updated_at: DateTime<Utc>,
}

/// Wire projection of an [`Organization`].
///
/// Omits internal ids and the `singleton` constraint column; the organization
/// id is exposed as `id` so clients can reference it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationView {
    /// Public UUID of the organization.
    pub id: String,
    /// Human-readable organization name.
    pub name: String,
    /// When the organization was created.
    pub created_at: DateTime<Utc>,
}

impl From<&Organization> for OrganizationView {
    fn from(org: &Organization) -> Self {
        Self {
            id: org.public_id.clone(),
            name: org.name.clone(),
            created_at: org.created_at,
        }
    }
}
