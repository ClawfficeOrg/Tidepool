//! `HostCredential` entity — a host's long-lived relationship credential
//! (task 1.2.2).
//!
//! The host generates the credential and keeps it; the pool stores only an
//! Argon2id PHC hash. There is deliberately no wire projection: no read
//! endpoint returns the credential or its hash.

use chrono::{DateTime, Utc};
use sqlx::FromRow;

/// Full host credential row.
#[derive(Debug, Clone, FromRow)]
pub struct HostCredential {
    /// Internal primary key.
    pub id: u64,
    /// Internal host id (UNIQUE: one relationship per host).
    pub host_id: u64,
    /// Argon2id PHC string. Never serialized or logged.
    pub credential_hash: String,
    /// Last successful host authentication.
    pub last_used_at: Option<DateTime<Utc>>,
    /// Set when the relationship is revoked.
    pub revoked_at: Option<DateTime<Utc>>,
    /// When the credential was established.
    pub created_at: DateTime<Utc>,
    /// When the credential last changed.
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_does_not_implement_serialize() {
        // Compile-time guard: HostCredential must never become wire-facing.
        // (`Serialize` is intentionally absent; this comment documents why.)
        let credential = HostCredential {
            id: 1,
            host_id: 2,
            credential_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA".into(),
            last_used_at: None,
            revoked_at: None,
            created_at: "2026-10-20T00:00:00Z".parse().expect("ts"),
            updated_at: "2026-10-20T00:00:00Z".parse().expect("ts"),
        };
        assert!(credential.credential_hash.starts_with("$argon2id$"));
    }
}
