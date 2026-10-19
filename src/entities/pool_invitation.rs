//! `PoolInvitation` entity — pending team invitations (task 1.1.2).
//!
//! See `migrations/20261019000002_create_pool_invitations.up.sql`. The raw
//! invitation token is returned once by the service and never stored; only an
//! Argon2id PHC hash of its secret half lives in `token_hash`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::entities::pool_user::PoolRole;

/// Full invitation row, mirroring `pool_invitations`.
#[derive(Debug, Clone, FromRow)]
pub struct PoolInvitation {
    /// Internal primary key; never exposed.
    pub id: u64,
    /// Public identifier, also the lookup half of the invitation token.
    pub public_id: String,
    /// Internal organization id.
    pub organization_id: u64,
    /// Invitee email (normalized to lowercase).
    pub email: String,
    /// Role granted on acceptance.
    pub role: PoolRole,
    /// Argon2id PHC string for the token's secret half.
    pub token_hash: String,
    /// Operator who created the invitation, if still present.
    pub invited_by: Option<u64>,
    /// Expiry instant.
    pub expires_at: DateTime<Utc>,
    /// Set once the invitation has been accepted (single use).
    pub accepted_at: Option<DateTime<Utc>>,
    /// Set when an owner revokes a pending invitation.
    pub revoked_at: Option<DateTime<Utc>>,
    /// When the invitation was created.
    pub created_at: DateTime<Utc>,
    /// When the invitation last changed.
    pub updated_at: DateTime<Utc>,
}

impl PoolInvitation {
    /// True when the invitation can still be accepted at `now`.
    pub fn is_pending(&self, now: DateTime<Utc>) -> bool {
        self.accepted_at.is_none() && self.revoked_at.is_none() && self.expires_at > now
    }
}

/// Wire projection of a [`PoolInvitation`]; never carries `token_hash`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolInvitationView {
    /// Public UUID of the invitation.
    pub id: String,
    /// Invitee email.
    pub email: String,
    /// Role granted on acceptance.
    pub role: PoolRole,
    /// Expiry instant.
    pub expires_at: DateTime<Utc>,
    /// Set once accepted.
    pub accepted_at: Option<DateTime<Utc>>,
    /// Set once revoked.
    pub revoked_at: Option<DateTime<Utc>>,
    /// When the invitation was created.
    pub created_at: DateTime<Utc>,
}

impl From<&PoolInvitation> for PoolInvitationView {
    fn from(invitation: &PoolInvitation) -> Self {
        Self {
            id: invitation.public_id.clone(),
            email: invitation.email.clone(),
            role: invitation.role,
            expires_at: invitation.expires_at,
            accepted_at: invitation.accepted_at,
            revoked_at: invitation.revoked_at,
            created_at: invitation.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(expires_at: DateTime<Utc>) -> PoolInvitation {
        PoolInvitation {
            id: 1,
            public_id: "11111111-1111-4111-8111-111111111111".into(),
            organization_id: 7,
            email: "member@example.com".into(),
            role: PoolRole::Member,
            token_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".into(),
            invited_by: Some(2),
            expires_at,
            accepted_at: None,
            revoked_at: None,
            created_at: "2026-10-19T12:00:00Z".parse().expect("ts"),
            updated_at: "2026-10-19T12:00:00Z".parse().expect("ts"),
        }
    }

    #[test]
    fn pending_requires_unused_unrevoked_and_unexpired() {
        let now = "2026-10-19T12:00:00Z".parse().expect("ts");
        let future = "2026-10-26T12:00:00Z".parse().expect("ts");
        let past = "2026-10-18T12:00:00Z".parse().expect("ts");

        assert!(sample(future).is_pending(now));
        assert!(!sample(past).is_pending(now));

        let mut accepted = sample(future);
        accepted.accepted_at = Some(now);
        assert!(!accepted.is_pending(now));

        let mut revoked = sample(future);
        revoked.revoked_at = Some(now);
        assert!(!revoked.is_pending(now));
    }

    #[test]
    fn view_never_exposes_the_token_hash() {
        let view = PoolInvitationView::from(&sample("2026-10-26T12:00:00Z".parse().expect("ts")));
        let json = serde_json::to_string(&view).expect("serialize");
        assert!(!json.contains("argon2"));
        assert!(!json.contains("token_hash"));
    }
}
