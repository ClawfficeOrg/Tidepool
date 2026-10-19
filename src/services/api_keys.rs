//! API key service — creation, authentication, revocation and the hub audit
//! trail (task 1.1.3).
//!
//! Keys are organization-scoped and carry explicit [`ApiKeyScope`]s. The full
//! key is `<prefix>.<secret>`; only the lookup prefix and an Argon2id hash of
//! the secret are stored, so a database read cannot reconstruct a usable key.
//!
//! Revocation is a status update, and [`authenticate`] only selects active
//! rows: the next request with a revoked key fails with 401. Routes that need a
//! capability call `require_scope`, which answers 403 (authenticated but not
//! allowed), never 401.
//!
//! Every hub mutation calls [`audit`] with the acting member, API key or the
//! system. The audit row is not part of the mutation transaction in v1; a
//! failure is logged rather than hidden, and later phases may tighten it.

use crate::entities::api_key::{ApiKey, ApiKeyScope};
use crate::services::auth;
use sqlx::MySqlPool;
use thiserror::Error;

/// Prefix that identifies a Tidepool API key at a glance.
pub const KEY_PREFIX_MARKER: &str = "tpk_";

/// Domain errors for API key operations.
#[derive(Debug, Error)]
pub enum ApiKeyServiceError {
    /// Invalid name, empty scope list, duplicate scope, ...
    #[error("invalid input: {0}")]
    Invalid(String),
    /// No key with that public id in this organization.
    #[error("API key not found")]
    NotFound,
    /// Unknown prefix, wrong secret, or revoked key.
    #[error("invalid API key")]
    InvalidCredentials,
    /// Password-hash operation failed.
    #[error("credential hashing failed: {0}")]
    PasswordHash(String),
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// The principal recorded in an audit row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditActor {
    /// A signed-in operator.
    Member {
        /// Public UUID of the member.
        public_id: String,
        /// Email or display name, for humans reading the log.
        label: String,
    },
    /// An organization API key.
    ApiKey {
        /// Public UUID of the key.
        public_id: String,
        /// Display prefix.
        label: String,
    },
    /// First-run setup or bootstrap, before any member exists.
    System,
}

impl AuditActor {
    /// Build a member actor from a resolved operator.
    pub fn member(public_id: &str, label: &str) -> Self {
        Self::Member {
            public_id: public_id.to_string(),
            label: label.to_string(),
        }
    }

    /// Build an API-key actor.
    pub fn api_key(public_id: &str, label: &str) -> Self {
        Self::ApiKey {
            public_id: public_id.to_string(),
            label: label.to_string(),
        }
    }

    fn kind_and_id(&self) -> (&'static str, Option<&str>, Option<&str>) {
        match self {
            AuditActor::Member { public_id, label } => {
                ("member", Some(public_id.as_str()), Some(label.as_str()))
            }
            AuditActor::ApiKey { public_id, label } => {
                ("api_key", Some(public_id.as_str()), Some(label.as_str()))
            }
            AuditActor::System => ("system", None, None),
        }
    }
}

/// One audit row to append.
#[derive(Debug, Clone)]
pub struct AuditEntry<'a> {
    /// Organization the mutation belongs to.
    pub organization_id: u64,
    /// Who performed it.
    pub actor: AuditActor,
    /// Stable action name, e.g. `member.role_change`.
    pub action: &'a str,
    /// Target kind, e.g. `member`, `invitation`, `api_key`.
    pub target_kind: Option<&'a str>,
    /// Target public id.
    pub target_public_id: Option<&'a str>,
    /// Extra non-secret context.
    pub detail: serde_json::Value,
}

/// Append an audit row.
pub async fn record_audit(
    pool: &MySqlPool,
    entry: AuditEntry<'_>,
) -> Result<(), ApiKeyServiceError> {
    let (kind, actor_id, label) = entry.actor.kind_and_id();
    let detail = if entry.detail.is_null() {
        None
    } else {
        Some(serde_json::to_string(&entry.detail).map_err(|e| {
            ApiKeyServiceError::Invalid(format!("audit detail is not serializable: {e}"))
        })?)
    };

    sqlx::query(
        "INSERT INTO audit_events \
         (organization_id, actor_kind, actor_public_id, actor_label, action, \
          target_kind, target_public_id, detail) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.organization_id)
    .bind(kind)
    .bind(actor_id)
    .bind(label)
    .bind(entry.action)
    .bind(entry.target_kind)
    .bind(entry.target_public_id)
    .bind(detail)
    .execute(pool)
    .await
    .map_err(ApiKeyServiceError::Database)?;

    Ok(())
}

/// Best-effort audit write: log and continue if the row cannot be stored.
pub async fn audit(pool: &MySqlPool, entry: AuditEntry<'_>) {
    if let Err(error) = record_audit(pool, entry).await {
        tracing::error!(error = %error, "failed to write hub audit event");
    }
}

/// Create a key and return it with the full one-time token.
///
/// The token is `"<prefix>.<secret>"`; only the prefix and the secret's
/// Argon2id hash are stored.
pub async fn create_api_key(
    pool: &MySqlPool,
    organization_id: u64,
    created_by: Option<u64>,
    name: &str,
    scopes: &[ApiKeyScope],
) -> Result<(ApiKey, String), ApiKeyServiceError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 128 {
        return Err(ApiKeyServiceError::Invalid(
            "name must be 1-128 characters".into(),
        ));
    }
    validate_scopes(scopes)?;

    let prefix = format!(
        "{KEY_PREFIX_MARKER}{}",
        &uuid::Uuid::new_v4().simple().to_string()[..12]
    );
    let secret = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let secret_hash = auth::hash_password(&secret)
        .map_err(|e| ApiKeyServiceError::PasswordHash(e.to_string()))?;
    let scopes_json = serde_json::to_string(scopes)
        .map_err(|e| ApiKeyServiceError::Invalid(format!("scopes: {e}")))?;
    let public_id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO api_keys \
         (public_id, organization_id, name, prefix, secret_hash, scopes, created_by) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(name)
    .bind(&prefix)
    .bind(&secret_hash)
    .bind(&scopes_json)
    .bind(created_by)
    .execute(pool)
    .await
    .map_err(ApiKeyServiceError::Database)?;

    let key = sqlx::query_as::<_, ApiKey>(
        "SELECT id, public_id, organization_id, name, prefix, secret_hash, scopes, \
                status, created_by, last_used_at, revoked_at, created_at, updated_at \
         FROM api_keys WHERE public_id = ?",
    )
    .bind(&public_id)
    .fetch_one(pool)
    .await
    .map_err(ApiKeyServiceError::Database)?;

    Ok((key, format!("{prefix}.{secret}")))
}

/// List an organization's keys, newest first. Never returns secrets.
pub async fn list_api_keys(
    pool: &MySqlPool,
    organization_id: u64,
) -> Result<Vec<ApiKey>, ApiKeyServiceError> {
    sqlx::query_as::<_, ApiKey>(
        "SELECT id, public_id, organization_id, name, prefix, secret_hash, scopes, \
                status, created_by, last_used_at, revoked_at, created_at, updated_at \
         FROM api_keys WHERE organization_id = ? ORDER BY created_at DESC, id DESC",
    )
    .bind(organization_id)
    .fetch_all(pool)
    .await
    .map_err(ApiKeyServiceError::Database)
}

/// Authenticate a raw `<prefix>.<secret>` token.
///
/// Only active keys are selected, so revocation takes effect immediately. A
/// malformed, unknown or revoked key all yield
/// [`ApiKeyServiceError::InvalidCredentials`] (401) to avoid oracles.
pub async fn authenticate(pool: &MySqlPool, raw_token: &str) -> Result<ApiKey, ApiKeyServiceError> {
    let (prefix, secret) =
        parse_key_token(raw_token).ok_or(ApiKeyServiceError::InvalidCredentials)?;

    let key = sqlx::query_as::<_, ApiKey>(
        "SELECT id, public_id, organization_id, name, prefix, secret_hash, scopes, \
                status, created_by, last_used_at, revoked_at, created_at, updated_at \
         FROM api_keys WHERE prefix = ? AND status = 'active'",
    )
    .bind(prefix)
    .fetch_optional(pool)
    .await
    .map_err(ApiKeyServiceError::Database)?
    .ok_or(ApiKeyServiceError::InvalidCredentials)?;

    let matches = auth::verify_password(secret, &key.secret_hash).map_err(|e| {
        tracing::error!(
            error = %e,
            api_key = %key.public_id,
            "API key secret verification failed"
        );
        ApiKeyServiceError::PasswordHash("verification failed".into())
    })?;
    if !matches {
        return Err(ApiKeyServiceError::InvalidCredentials);
    }

    sqlx::query("UPDATE api_keys SET last_used_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(key.id)
        .execute(pool)
        .await
        .map_err(ApiKeyServiceError::Database)?;

    Ok(key)
}

/// Revoke a key. Effective on the next authentication because only active rows
/// are selected.
///
/// Daemon enrollment tokens do not exist yet (phase 1.4); when they land they
/// must be considered consumed/dead whenever their issuing key is not active.
pub async fn revoke_api_key(
    pool: &MySqlPool,
    organization_id: u64,
    public_id: &str,
) -> Result<ApiKey, ApiKeyServiceError> {
    let result = sqlx::query(
        "UPDATE api_keys SET status = 'revoked', revoked_at = CURRENT_TIMESTAMP \
         WHERE public_id = ? AND organization_id = ? AND status = 'active'",
    )
    .bind(public_id)
    .bind(organization_id)
    .execute(pool)
    .await
    .map_err(ApiKeyServiceError::Database)?;

    if result.rows_affected() == 0 {
        return Err(ApiKeyServiceError::NotFound);
    }

    sqlx::query_as::<_, ApiKey>(
        "SELECT id, public_id, organization_id, name, prefix, secret_hash, scopes, \
                status, created_by, last_used_at, revoked_at, created_at, updated_at \
         FROM api_keys WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_one(pool)
    .await
    .map_err(ApiKeyServiceError::Database)
}

/// Validate a scope selection: non-empty, unique and only known scopes.
pub fn validate_scopes(scopes: &[ApiKeyScope]) -> Result<(), ApiKeyServiceError> {
    if scopes.is_empty() {
        return Err(ApiKeyServiceError::Invalid(
            "select at least one scope".into(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for scope in scopes {
        if !seen.insert(*scope) {
            return Err(ApiKeyServiceError::Invalid(format!(
                "duplicate scope {}",
                scope.as_str()
            )));
        }
    }
    Ok(())
}

fn parse_key_token(raw: &str) -> Option<(&str, &str)> {
    let (prefix, secret) = raw.split_once('.')?;
    if !prefix.starts_with(KEY_PREFIX_MARKER) || secret.is_empty() {
        return None;
    }
    Some((prefix, secret))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::api_key::ApiKeyStatus;

    #[test]
    fn key_token_requires_the_marker_and_a_secret() {
        assert_eq!(
            parse_key_token("tpk_abc.secret"),
            Some(("tpk_abc", "secret"))
        );
        assert_eq!(parse_key_token("jwt.token.here"), None);
        assert_eq!(parse_key_token("tpk_abc."), None);
        assert_eq!(parse_key_token("no-dot"), None);
    }

    #[test]
    fn scope_selection_rejects_empty_and_duplicates() {
        assert!(validate_scopes(&[]).is_err());
        assert!(validate_scopes(&[ApiKeyScope::ProjectsRead, ApiKeyScope::ProjectsRead]).is_err());
        assert!(validate_scopes(&[ApiKeyScope::ProjectsRead, ApiKeyScope::RunsDispatch]).is_ok());
    }

    #[test]
    fn audit_actor_fields_are_stable() {
        assert_eq!(
            AuditActor::member("id-1", "owner@example.com").kind_and_id(),
            ("member", Some("id-1"), Some("owner@example.com"))
        );
        assert_eq!(
            AuditActor::api_key("key-1", "tpk_abc").kind_and_id(),
            ("api_key", Some("key-1"), Some("tpk_abc"))
        );
        assert_eq!(AuditActor::System.kind_and_id(), ("system", None, None));
    }

    #[test]
    fn status_round_trips() {
        assert_eq!(
            serde_json::to_string(&ApiKeyStatus::Active).unwrap(),
            "\"active\""
        );
        assert_eq!(
            serde_json::to_string(&ApiKeyStatus::Revoked).unwrap(),
            "\"revoked\""
        );
    }
}
