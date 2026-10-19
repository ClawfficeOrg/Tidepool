//! `EnrollmentToken` entity — short-lived host enrollment tokens (task 1.2.2).
//!
//! The raw token is `<public_id>.<secret>` and is returned once. Only an
//! Argon2id hash of the secret is stored. A token is valid only while it is
//! unconsumed, unrevoked and before `expires_at`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Full enrollment token row.
#[derive(Debug, Clone, FromRow)]
pub struct EnrollmentToken {
    /// Internal primary key.
    pub id: u64,
    /// Public lookup half (UUID).
    pub public_id: String,
    /// Internal organization id.
    pub organization_id: u64,
    /// Internal host id the token is bound to.
    pub host_id: u64,
    /// Argon2id PHC string for the token's secret half.
    pub token_hash: String,
    /// API key that minted the token, if any.
    pub issued_by_api_key_id: Option<u64>,
    /// Expiry instant.
    pub expires_at: DateTime<Utc>,
    /// Set once enrollment succeeds (single use).
    pub consumed_at: Option<DateTime<Utc>>,
    /// Set when the issuing key is revoked or the token is cancelled.
    pub revoked_at: Option<DateTime<Utc>>,
    /// When the token was minted.
    pub created_at: DateTime<Utc>,
}

impl EnrollmentToken {
    /// True when the token can still be exchanged at `now`.
    pub fn is_pending(&self, now: DateTime<Utc>) -> bool {
        self.consumed_at.is_none() && self.revoked_at.is_none() && self.expires_at > now
    }
}

/// Wire projection of an [`EnrollmentToken`]; never carries the hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrollmentTokenView {
    /// Public UUID of the token.
    pub id: String,
    /// Expiry instant.
    pub expires_at: DateTime<Utc>,
    /// Set once consumed.
    pub consumed_at: Option<DateTime<Utc>>,
    /// Set once revoked.
    pub revoked_at: Option<DateTime<Utc>>,
    /// When minted.
    pub created_at: DateTime<Utc>,
}

impl From<&EnrollmentToken> for EnrollmentTokenView {
    fn from(token: &EnrollmentToken) -> Self {
        Self {
            id: token.public_id.clone(),
            expires_at: token.expires_at,
            consumed_at: token.consumed_at,
            revoked_at: token.revoked_at,
            created_at: token.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(expires_at: DateTime<Utc>) -> EnrollmentToken {
        EnrollmentToken {
            id: 1,
            public_id: "11111111-1111-4111-8111-111111111111".into(),
            organization_id: 7,
            host_id: 3,
            token_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".into(),
            issued_by_api_key_id: Some(2),
            expires_at,
            consumed_at: None,
            revoked_at: None,
            created_at: "2026-10-20T00:00:00Z".parse().expect("ts"),
        }
    }

    #[test]
    fn pending_requires_unused_unrevoked_and_unexpired() {
        let now = "2026-10-20T00:00:00Z".parse().expect("ts");
        let future = "2026-10-20T00:10:00Z".parse().expect("ts");
        let past = "2026-10-19T23:59:00Z".parse().expect("ts");
        assert!(sample(future).is_pending(now));
        assert!(!sample(past).is_pending(now));

        let mut used = sample(future);
        used.consumed_at = Some(now);
        assert!(!used.is_pending(now));

        let mut revoked = sample(future);
        revoked.revoked_at = Some(now);
        assert!(!revoked.is_pending(now));
    }

    #[test]
    fn view_never_exposes_the_hash() {
        let view = EnrollmentTokenView::from(&sample("2026-10-20T00:10:00Z".parse().expect("ts")));
        let json = serde_json::to_string(&view).expect("serialize");
        assert!(!json.contains("argon2"));
        assert!(!json.contains("token_hash"));
    }
}
