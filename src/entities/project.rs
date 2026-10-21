//! `Project` entity — one hub project holding a configuration (task 1.4.1).
//!
//! See `migrations/20261021000001_create_projects.up.sql`. A project has a
//! stable `public_id`, a mutable friendly `slug` and a `name`. Projects are
//! scoped to the organization the credential belongs to; the internal
//! `organization_id` never leaves the server.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Full project row, mirroring `projects`.
#[derive(Debug, Clone, FromRow)]
pub struct Project {
    /// Internal primary key.
    pub id: u64,
    /// Immutable public UUID.
    pub public_id: String,
    /// Internal organization id.
    pub organization_id: u64,
    /// Mutable friendly slug.
    pub slug: String,
    /// Human-readable name.
    pub name: String,
    /// When the project was created.
    pub created_at: DateTime<Utc>,
    /// When the project last changed.
    pub updated_at: DateTime<Utc>,
}

/// Wire projection of a [`Project`]; omits the internal and organization ids.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectView {
    /// Immutable public UUID.
    pub id: String,
    /// Friendly slug.
    pub slug: String,
    /// Human-readable name.
    pub name: String,
    /// When the project was created.
    pub created_at: DateTime<Utc>,
    /// When the project last changed.
    pub updated_at: DateTime<Utc>,
}

impl From<&Project> for ProjectView {
    fn from(project: &Project) -> Self {
        Self {
            id: project.public_id.clone(),
            slug: project.slug.clone(),
            name: project.name.clone(),
            created_at: project.created_at,
            updated_at: project.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Project {
        Project {
            id: 1,
            public_id: "11111111-1111-4111-8111-111111111111".into(),
            organization_id: 7,
            slug: "nightly-builds".into(),
            name: "Nightly Builds".into(),
            created_at: "2026-10-21T00:00:00Z".parse().expect("ts"),
            updated_at: "2026-10-21T00:00:00Z".parse().expect("ts"),
        }
    }

    #[test]
    fn view_never_exposes_internal_ids() {
        let json = serde_json::to_string(&ProjectView::from(&sample())).expect("serialize");
        assert!(!json.contains("organization_id"));
        assert!(!json.contains("\"id\":1"));
        assert!(json.contains("nightly-builds"));
    }
}
