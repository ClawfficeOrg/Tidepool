//! Host enrollment service (task 1.2.2).
//!
//! Two-step exchange:
//!
//! 1. [`mint_enrollment_token`] creates a ten-minute token bound to one
//!    registered host. The raw token is returned exactly once.
//! 2. [`enroll_host`] exchanges that token (single use) for a host-generated
//!    long-lived credential. Only the credential's Argon2id hash is stored.
//!
//! [`authenticate_host`] verifies the relationship credential for later
//! presence/protocol work. There is no API path that returns either secret.
//!
//! API-key revocation expires every unconsumed token issued by that key; see
//! `services::api_keys::revoke_api_key`.

use crate::entities::enrollment_token::EnrollmentToken;
use crate::entities::host::Host;
use crate::services::api_keys::{self as audit_svc, AuditActor, AuditEntry};
use crate::services::auth;
use chrono::{Duration, Utc};
use sqlx::MySqlPool;
use thiserror::Error;

/// Enrollment tokens live for ten minutes.
pub const ENROLLMENT_TTL_SECS: i64 = 10 * 60;

/// Minimum host-generated credential length.
pub const MIN_CREDENTIAL_LEN: usize = 32;

/// Maximum accepted credential length.
pub const MAX_CREDENTIAL_LEN: usize = 1024;

/// Domain errors for enrollment.
#[derive(Debug, Error)]
pub enum EnrollmentError {
    /// Invalid credential or request input.
    #[error("invalid input: {0}")]
    Invalid(String),
    /// Host does not exist or is revoked.
    #[error("host not found")]
    HostNotFound,
    /// The host already has a relationship credential.
    #[error("host is already enrolled")]
    AlreadyEnrolled,
    /// Token is unknown or the secret half does not verify.
    #[error("enrollment token is invalid")]
    TokenInvalid,
    /// Token has expired.
    #[error("enrollment token has expired")]
    TokenExpired,
    /// Token has already been consumed.
    #[error("enrollment token has already been used")]
    TokenUsed,
    /// Token was cancelled, including by API-key revocation.
    #[error("enrollment token has been revoked")]
    TokenRevoked,
    /// Relationship credential does not verify.
    #[error("invalid host credential")]
    InvalidCredentials,
    /// Credential hashing failed.
    #[error("credential hashing failed: {0}")]
    PasswordHash(String),
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// Mint a token bound to a registered host.
///
/// Refuses a host that already has a relationship credential. `issued_by`
/// records an API key so revocation can expire the token.
pub async fn mint_enrollment_token(
    pool: &MySqlPool,
    organization_id: u64,
    host_public_id: &str,
    issued_by_api_key_id: Option<u64>,
    actor: AuditActor,
) -> Result<(EnrollmentToken, String), EnrollmentError> {
    let host = sqlx::query_as::<_, Host>(
        "SELECT id, public_id, organization_id, slug, display_name, hostname, status, execution_granted, \
                revoked_at, created_at, updated_at \
         FROM hosts WHERE public_id = ? AND organization_id = ? AND status <> 'revoked'",
    )
    .bind(host_public_id)
    .bind(organization_id)
    .fetch_optional(pool)
    .await
    .map_err(EnrollmentError::Database)?
    .ok_or(EnrollmentError::HostNotFound)?;

    let enrolled: Option<u64> = sqlx::query_scalar(
        "SELECT id FROM host_credentials WHERE host_id = ? AND revoked_at IS NULL",
    )
    .bind(host.id)
    .fetch_optional(pool)
    .await
    .map_err(EnrollmentError::Database)?;
    if enrolled.is_some() {
        return Err(EnrollmentError::AlreadyEnrolled);
    }

    let public_id = uuid::Uuid::new_v4().to_string();
    let secret = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let token_hash =
        auth::hash_password(&secret).map_err(|e| EnrollmentError::PasswordHash(e.to_string()))?;
    let expires_at = Utc::now() + Duration::seconds(ENROLLMENT_TTL_SECS);

    sqlx::query(
        "INSERT INTO enrollment_tokens \
         (public_id, organization_id, host_id, token_hash, issued_by_api_key_id, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(host.id)
    .bind(&token_hash)
    .bind(issued_by_api_key_id)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(EnrollmentError::Database)?;

    let token = sqlx::query_as::<_, EnrollmentToken>(
        "SELECT id, public_id, organization_id, host_id, token_hash, issued_by_api_key_id, \
                expires_at, consumed_at, revoked_at, created_at \
         FROM enrollment_tokens WHERE public_id = ?",
    )
    .bind(&public_id)
    .fetch_one(pool)
    .await
    .map_err(EnrollmentError::Database)?;

    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id,
            actor,
            action: "host.enrollment_token",
            target_kind: Some("host"),
            target_public_id: Some(&host.public_id),
            detail: serde_json::json!({ "expires_at": expires_at }),
        },
    )
    .await;

    Ok((token, format!("{public_id}.{secret}")))
}

/// Exchange a one-time token for a host relationship credential.
///
/// The host supplies the credential it generated; only its hash is stored.
/// Single use and expiry are checked under `FOR UPDATE`.
pub async fn enroll_host(
    pool: &MySqlPool,
    raw_token: &str,
    credential: &str,
) -> Result<Host, EnrollmentError> {
    if credential.len() < MIN_CREDENTIAL_LEN || credential.len() > MAX_CREDENTIAL_LEN {
        return Err(EnrollmentError::Invalid(format!(
            "credential must be {MIN_CREDENTIAL_LEN}-{MAX_CREDENTIAL_LEN} characters"
        )));
    }
    let (public_id, secret) = parse_token(raw_token).ok_or(EnrollmentError::TokenInvalid)?;

    let token = sqlx::query_as::<_, EnrollmentToken>(
        "SELECT id, public_id, organization_id, host_id, token_hash, issued_by_api_key_id, \
                expires_at, consumed_at, revoked_at, created_at \
         FROM enrollment_tokens WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await
    .map_err(EnrollmentError::Database)?
    .ok_or(EnrollmentError::TokenInvalid)?;

    let secret_matches = auth::verify_password(secret, &token.token_hash)
        .map_err(|e| EnrollmentError::PasswordHash(format!("token verify failed: {e}")))?;
    if !secret_matches {
        return Err(EnrollmentError::TokenInvalid);
    }

    let credential_hash = auth::hash_password(credential)
        .map_err(|e| EnrollmentError::PasswordHash(e.to_string()))?;

    let mut tx = pool.begin().await.map_err(EnrollmentError::Database)?;

    let token = sqlx::query_as::<_, EnrollmentToken>(
        "SELECT id, public_id, organization_id, host_id, token_hash, issued_by_api_key_id, \
                expires_at, consumed_at, revoked_at, created_at \
         FROM enrollment_tokens WHERE id = ? FOR UPDATE",
    )
    .bind(token.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(EnrollmentError::Database)?;

    let now = Utc::now();
    if token.consumed_at.is_some() {
        return Err(EnrollmentError::TokenUsed);
    }
    if token.revoked_at.is_some() {
        return Err(EnrollmentError::TokenRevoked);
    }
    if token.expires_at <= now {
        return Err(EnrollmentError::TokenExpired);
    }

    let host = sqlx::query_as::<_, Host>(
        "SELECT id, public_id, organization_id, slug, display_name, hostname, status, execution_granted, \
                revoked_at, created_at, updated_at \
         FROM hosts WHERE id = ? AND status <> 'revoked' FOR UPDATE",
    )
    .bind(token.host_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(EnrollmentError::Database)?
    .ok_or(EnrollmentError::HostNotFound)?;

    let existing: Option<u64> =
        sqlx::query_scalar("SELECT id FROM host_credentials WHERE host_id = ?")
            .bind(host.id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(EnrollmentError::Database)?;
    if existing.is_some() {
        return Err(EnrollmentError::AlreadyEnrolled);
    }

    sqlx::query("INSERT INTO host_credentials (host_id, credential_hash) VALUES (?, ?)")
        .bind(host.id)
        .bind(&credential_hash)
        .execute(&mut *tx)
        .await
        .map_err(EnrollmentError::Database)?;

    sqlx::query("UPDATE hosts SET status = 'offline' WHERE id = ?")
        .bind(host.id)
        .execute(&mut *tx)
        .await
        .map_err(EnrollmentError::Database)?;

    sqlx::query("UPDATE enrollment_tokens SET consumed_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(token.id)
        .execute(&mut *tx)
        .await
        .map_err(EnrollmentError::Database)?;

    tx.commit().await.map_err(EnrollmentError::Database)?;

    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id: token.organization_id,
            actor: AuditActor::System,
            action: "host.enroll",
            target_kind: Some("host"),
            target_public_id: Some(&host.public_id),
            detail: serde_json::json!({ "slug": host.slug }),
        },
    )
    .await;

    sqlx::query_as::<_, Host>(
        "SELECT id, public_id, organization_id, slug, display_name, hostname, status, execution_granted, \
                revoked_at, created_at, updated_at \
         FROM hosts WHERE id = ?",
    )
    .bind(host.id)
    .fetch_one(pool)
    .await
    .map_err(EnrollmentError::Database)
}

/// Verify a host relationship credential for future presence/protocol calls.
pub async fn authenticate_host(
    pool: &MySqlPool,
    host_public_id: &str,
    credential: &str,
) -> Result<Host, EnrollmentError> {
    let host = sqlx::query_as::<_, Host>(
        "SELECT h.id, h.public_id, h.organization_id, h.slug, h.display_name, h.hostname, \
                h.status, h.execution_granted, h.revoked_at, h.created_at, h.updated_at \
         FROM hosts h JOIN host_credentials c ON c.host_id = h.id \
         WHERE h.public_id = ? AND h.status <> 'revoked' \
           AND c.revoked_at IS NULL",
    )
    .bind(host_public_id)
    .fetch_optional(pool)
    .await
    .map_err(EnrollmentError::Database)?
    .ok_or(EnrollmentError::InvalidCredentials)?;

    let hash: String = sqlx::query_scalar(
        "SELECT credential_hash FROM host_credentials \
         WHERE host_id = ? AND revoked_at IS NULL",
    )
    .bind(host.id)
    .fetch_one(pool)
    .await
    .map_err(EnrollmentError::Database)?;
    let matches = auth::verify_password(credential, &hash).map_err(|e| {
        tracing::error!(error = %e, host = %host.public_id, "host credential verification failed");
        EnrollmentError::PasswordHash("verification failed".into())
    })?;
    if !matches {
        return Err(EnrollmentError::InvalidCredentials);
    }

    sqlx::query("UPDATE host_credentials SET last_used_at = CURRENT_TIMESTAMP WHERE host_id = ?")
        .bind(host.id)
        .execute(pool)
        .await
        .map_err(EnrollmentError::Database)?;

    Ok(host)
}

/// Expire every unconsumed token minted by an API key.
pub async fn revoke_tokens_for_api_key(
    pool: &MySqlPool,
    api_key_id: u64,
) -> Result<u64, EnrollmentError> {
    let result = sqlx::query(
        "UPDATE enrollment_tokens SET revoked_at = CURRENT_TIMESTAMP \
         WHERE issued_by_api_key_id = ? AND consumed_at IS NULL AND revoked_at IS NULL",
    )
    .bind(api_key_id)
    .execute(pool)
    .await
    .map_err(EnrollmentError::Database)?;
    Ok(result.rows_affected())
}

fn parse_token(raw: &str) -> Option<(&str, &str)> {
    let (public_id, secret) = raw.split_once('.')?;
    if public_id.is_empty() || secret.is_empty() {
        return None;
    }
    Some((public_id, secret))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enrollment_token_is_ten_minutes() {
        assert_eq!(ENROLLMENT_TTL_SECS, 600);
    }

    #[test]
    fn token_parsing_requires_both_halves() {
        assert_eq!(parse_token("id.secret"), Some(("id", "secret")));
        assert_eq!(parse_token("id."), None);
        assert_eq!(parse_token(".secret"), None);
        assert_eq!(parse_token("no-dot"), None);
    }
}
