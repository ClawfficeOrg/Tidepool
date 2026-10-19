//! Host registry service (task 1.2.1).
//!
//! A host is registered before it is enrolled. Registration creates a stable
//! [`crate::entities::host::Host`] with an immutable `public_id` and a friendly
//! `slug` derived from the hostname. The slug is mutable, de-duplicated with a
//! numeric suffix, and normalized to lowercase ASCII kebab-case.
//!
//! Revoked hosts are filtered from every lookup and listed as absent, so they
//! cannot be renamed, re-read or selected as a dispatch target. The host keeps
//! running its own local work; revocation only ends the pool relationship.

use crate::entities::host::Host;
use crate::services::api_keys::{self as audit_svc, AuditActor, AuditEntry};
use sqlx::MySqlPool;
use thiserror::Error;

/// Maximum slug length accepted by the schema.
pub const MAX_SLUG_LEN: usize = 63;

/// Domain errors for host operations.
#[derive(Debug, Error)]
pub enum HostServiceError {
    /// Invalid hostname, slug or display name.
    #[error("invalid input: {0}")]
    Invalid(String),
    /// No live host with that id in this organization.
    #[error("host not found")]
    NotFound,
    /// An explicit rename would collide with another host.
    #[error("slug is already in use: {0}")]
    SlugConflict(String),
    /// A configuration referenced a slug that does not resolve.
    #[error("host_slug: unknown host slug '{slug}'")]
    UnknownHostSlug {
        /// Field name used by configuration validation.
        field: &'static str,
        /// The normalized slug that failed to resolve.
        slug: String,
    },
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// Input for registering a host.
#[derive(Debug, Clone)]
pub struct NewHost {
    /// Hostname reported by the machine.
    pub hostname: String,
    /// Display name; defaults to the hostname when absent.
    pub display_name: Option<String>,
}

/// Input for renaming a host. At least one field must be present.
#[derive(Debug, Clone, Default)]
pub struct RenameHost {
    /// New friendly slug (normalized before use).
    pub slug: Option<String>,
    /// New display name.
    pub display_name: Option<String>,
}

/// Register a host in `pending_approval` state.
///
/// The slug is derived from the hostname and made unique by appending `-2`,
/// `-3`, ... until the insert succeeds. A concurrent registration cannot take
/// the same slug because the database unique key is the final arbiter.
pub async fn register_host(
    pool: &MySqlPool,
    organization_id: u64,
    input: NewHost,
    actor: AuditActor,
) -> Result<Host, HostServiceError> {
    let hostname = validate_hostname(&input.hostname)?;
    let display_name = match input.display_name.as_deref() {
        Some(name) => validate_display_name(name)?,
        None => hostname.clone(),
    };
    let base = normalize_slug(&hostname);

    for attempt in 1..=50 {
        let slug = slug_candidate(&base, attempt);
        let public_id = uuid::Uuid::new_v4().to_string();
        let result = sqlx::query(
            "INSERT INTO hosts (public_id, organization_id, slug, display_name, hostname, status) \
             VALUES (?, ?, ?, ?, ?, 'pending_approval')",
        )
        .bind(&public_id)
        .bind(organization_id)
        .bind(&slug)
        .bind(&display_name)
        .bind(&hostname)
        .execute(pool)
        .await;

        match result {
            Ok(_) => {
                let host = find_host(pool, organization_id, &public_id)
                    .await?
                    .ok_or(HostServiceError::NotFound)?;
                audit_svc::audit(
                    pool,
                    AuditEntry {
                        organization_id,
                        actor,
                        action: "host.register",
                        target_kind: Some("host"),
                        target_public_id: Some(&host.public_id),
                        detail: serde_json::json!({
                            "slug": host.slug,
                            "hostname": host.hostname,
                        }),
                    },
                )
                .await;
                return Ok(host);
            }
            Err(error) if is_unique_violation(&error) => continue,
            Err(error) => return Err(HostServiceError::Database(error)),
        }
    }

    Err(HostServiceError::SlugConflict(base))
}

/// List live hosts (revoked hosts are absent).
pub async fn list_hosts(
    pool: &MySqlPool,
    organization_id: u64,
) -> Result<Vec<Host>, HostServiceError> {
    sqlx::query_as::<_, Host>(
        "SELECT id, public_id, organization_id, slug, display_name, hostname, status, \
                revoked_at, created_at, updated_at \
         FROM hosts WHERE organization_id = ? AND status <> 'revoked' \
         ORDER BY slug",
    )
    .bind(organization_id)
    .fetch_all(pool)
    .await
    .map_err(HostServiceError::Database)
}

/// Fetch a live host by public id; revoked hosts are treated as not found.
pub async fn find_host(
    pool: &MySqlPool,
    organization_id: u64,
    public_id: &str,
) -> Result<Option<Host>, HostServiceError> {
    sqlx::query_as::<_, Host>(
        "SELECT id, public_id, organization_id, slug, display_name, hostname, status, \
                revoked_at, created_at, updated_at \
         FROM hosts WHERE public_id = ? AND organization_id = ? AND status <> 'revoked'",
    )
    .bind(public_id)
    .bind(organization_id)
    .fetch_optional(pool)
    .await
    .map_err(HostServiceError::Database)
}

/// Rename the friendly slug and/or display name without changing the id.
pub async fn rename_host(
    pool: &MySqlPool,
    organization_id: u64,
    public_id: &str,
    input: RenameHost,
    actor: AuditActor,
) -> Result<Host, HostServiceError> {
    if input.slug.is_none() && input.display_name.is_none() {
        return Err(HostServiceError::Invalid(
            "provide a slug or display_name".into(),
        ));
    }

    let existing = find_host(pool, organization_id, public_id)
        .await?
        .ok_or(HostServiceError::NotFound)?;

    let display_name = match input.display_name.as_deref() {
        Some(name) => validate_display_name(name)?,
        None => existing.display_name.clone(),
    };
    let slug = match input.slug.as_deref() {
        Some(raw) => {
            let slug = normalize_slug(raw);
            if slug.is_empty() {
                return Err(HostServiceError::Invalid("slug is required".into()));
            }
            slug
        }
        None => existing.slug.clone(),
    };

    let result = sqlx::query(
        "UPDATE hosts SET slug = ?, display_name = ? \
         WHERE public_id = ? AND organization_id = ? AND status <> 'revoked'",
    )
    .bind(&slug)
    .bind(&display_name)
    .bind(public_id)
    .bind(organization_id)
    .execute(pool)
    .await;

    match result {
        Ok(_) => {}
        Err(error) if is_unique_violation(&error) => {
            return Err(HostServiceError::SlugConflict(slug));
        }
        Err(error) => return Err(HostServiceError::Database(error)),
    }

    let host = find_host(pool, organization_id, public_id)
        .await?
        .ok_or(HostServiceError::NotFound)?;
    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id,
            actor,
            action: "host.rename",
            target_kind: Some("host"),
            target_public_id: Some(&host.public_id),
            detail: serde_json::json!({
                "slug": host.slug,
                "previous_slug": existing.slug,
            }),
        },
    )
    .await;
    Ok(host)
}

/// Revoke a host. The row is kept for audit but filtered from every lookup.
pub async fn revoke_host(
    pool: &MySqlPool,
    organization_id: u64,
    public_id: &str,
    actor: AuditActor,
) -> Result<(), HostServiceError> {
    let result = sqlx::query(
        "UPDATE hosts SET status = 'revoked', revoked_at = CURRENT_TIMESTAMP \
         WHERE public_id = ? AND organization_id = ? AND status <> 'revoked'",
    )
    .bind(public_id)
    .bind(organization_id)
    .execute(pool)
    .await
    .map_err(HostServiceError::Database)?;

    if result.rows_affected() == 0 {
        return Err(HostServiceError::NotFound);
    }

    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id,
            actor,
            action: "host.revoke",
            target_kind: Some("host"),
            target_public_id: Some(public_id),
            detail: serde_json::Value::Null,
        },
    )
    .await;
    Ok(())
}

/// Resolve a host slug for configuration activation.
///
/// Returns a named `host_slug` field error when the slug is unknown or revoked,
/// so activation can fail at save time rather than at first dispatch.
pub async fn resolve_for_activation(
    pool: &MySqlPool,
    organization_id: u64,
    slug: &str,
) -> Result<Host, HostServiceError> {
    let slug = normalize_slug(slug);
    let host = sqlx::query_as::<_, Host>(
        "SELECT id, public_id, organization_id, slug, display_name, hostname, status, \
                revoked_at, created_at, updated_at \
         FROM hosts WHERE organization_id = ? AND slug = ? AND status <> 'revoked'",
    )
    .bind(organization_id)
    .bind(&slug)
    .fetch_optional(pool)
    .await
    .map_err(HostServiceError::Database)?
    .ok_or_else(|| HostServiceError::UnknownHostSlug {
        field: "host_slug",
        slug: slug.clone(),
    })?;
    Ok(host)
}

/// Normalize a hostname or user-supplied slug to lowercase kebab-case.
///
/// Non-ASCII-alphanumeric runs become one `-`; leading/trailing hyphens are
/// removed and the result is capped at [`MAX_SLUG_LEN`]. An empty result means
/// the caller supplied nothing usable.
pub fn normalize_slug(raw: &str) -> String {
    let mut slug = String::with_capacity(raw.len());
    let mut pending_hyphen = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_hyphen && !slug.is_empty() {
                slug.push('-');
            }
            pending_hyphen = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            pending_hyphen = true;
        }
    }
    let slug = slug.trim_matches('-');
    let truncated: String = slug.chars().take(MAX_SLUG_LEN).collect();
    truncated.trim_end_matches('-').to_string()
}

/// Collision suffix: attempt 1 is the base, attempt 2 is `-2`, ...
pub fn slug_candidate(base: &str, attempt: u32) -> String {
    if attempt <= 1 {
        return base.to_string();
    }
    let suffix = format!("-{attempt}");
    let keep = MAX_SLUG_LEN.saturating_sub(suffix.len());
    let prefix: String = base.chars().take(keep).collect();
    let prefix = prefix.trim_end_matches('-');
    format!("{prefix}{suffix}")
}

fn validate_hostname(hostname: &str) -> Result<String, HostServiceError> {
    let hostname = hostname.trim();
    if hostname.is_empty() || hostname.chars().count() > 255 {
        return Err(HostServiceError::Invalid(
            "hostname must be 1-255 characters".into(),
        ));
    }
    Ok(hostname.to_string())
}

fn validate_display_name(name: &str) -> Result<String, HostServiceError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 128 {
        return Err(HostServiceError::Invalid(
            "display_name must be 1-128 characters".into(),
        ));
    }
    Ok(name.to_string())
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.is_unique_violation())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_normalizes_case_and_separators() {
        assert_eq!(normalize_slug("Studio-Mac.local"), "studio-mac-local");
        assert_eq!(normalize_slug("  My   Host!! "), "my-host");
        assert_eq!(normalize_slug("HELLO__WORLD"), "hello-world");
        assert_eq!(normalize_slug("---"), "");
    }

    #[test]
    fn slug_is_capped_and_has_no_trailing_hyphen() {
        let long = "a".repeat(80);
        let slug = normalize_slug(&long);
        assert_eq!(slug.len(), MAX_SLUG_LEN);
        let with_break = format!("{}-tail", "a".repeat(62));
        assert!(!normalize_slug(&with_break).ends_with('-'));
    }

    #[test]
    fn collision_candidates_are_unique_and_fit() {
        let base = "studio-mac";
        assert_eq!(slug_candidate(base, 1), "studio-mac");
        assert_eq!(slug_candidate(base, 2), "studio-mac-2");
        let long = "a".repeat(MAX_SLUG_LEN);
        let second = slug_candidate(&long, 2);
        assert_eq!(second.len(), MAX_SLUG_LEN);
        assert!(second.ends_with("-2"));
        assert_ne!(second, long);
    }

    #[test]
    fn unknown_slug_error_names_the_field() {
        let error = HostServiceError::UnknownHostSlug {
            field: "host_slug",
            slug: "missing".into(),
        };
        assert_eq!(error.to_string(), "host_slug: unknown host slug 'missing'");
    }
}
