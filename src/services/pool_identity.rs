//! Pool identity service — organization creation, operator accounts and the
//! first-run bootstrap.
//!
//! The plan is single-tenant in v1: one `organizations` row per pool. That is
//! enforced twice, deliberately — a `UNIQUE` key on the constant `singleton`
//! column, and [`setup_pool`]/[`bootstrap_from_env`] checking for an existing
//! organization first. No endpoint can create a second organization.
//!
//! # Bootstrap
//!
//! [`bootstrap_from_env`] is called once at startup when `TIDEBOOT_*` is
//! configured. It only ever inserts; it never updates an existing operator.
//! Consequently a restart cannot re-apply the temporary password, and once the
//! operator changes it the old password stops verifying because the stored
//! Argon2id hash was replaced.
//!
//! The bootstrap password is a privileged secret: it is never logged, and this
//! module stores only the PHC hash.

use crate::config::PoolBootstrapConfig;
use crate::entities::organization::Organization;
use crate::entities::pool_user::PoolUser;
use crate::services::auth;
use sqlx::MySqlPool;
use thiserror::Error;

/// Minimum length for an operator password (browser setup, bootstrap and
/// password change all use the same floor).
pub const MIN_OPERATOR_PASSWORD_LEN: usize = 12;

/// Maximum accepted plaintext password length.
pub const MAX_OPERATOR_PASSWORD_LEN: usize = 1024;

/// Domain errors for pool identity operations.
#[derive(Debug, Error)]
pub enum PoolIdentityError {
    /// An organization already exists; the pool is initialized.
    #[error("pool is already initialized")]
    AlreadyInitialized,
    /// Bootstrap ran after the organization had already been created.
    #[error("pool was already bootstrapped")]
    AlreadyBootstrapped,
    /// Invalid input (empty name, malformed email, ...).
    #[error("invalid input: {0}")]
    Invalid(String),
    /// Password is shorter than [`MIN_OPERATOR_PASSWORD_LEN`].
    #[error("password must be at least {MIN_OPERATOR_PASSWORD_LEN} characters")]
    WeakPassword,
    /// Wrong email/password, or a stale password-change attempt.
    #[error("invalid credentials")]
    InvalidCredentials,
    /// No operator with that public id.
    #[error("operator not found")]
    NotFound,
    /// Password hashing/verification failed.
    #[error("password hashing failed: {0}")]
    PasswordHash(String),
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// Result of a first-run bootstrap attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BootstrapOutcome {
    /// `TIDEBOOT_*` was not configured.
    Disabled,
    /// A new organization and owner were created.
    Created {
        /// Organization name written to the database.
        organization: String,
        /// Owner email (not a credential).
        owner_email: String,
    },
    /// An organization already existed; nothing was changed.
    AlreadyBootstrapped,
}

/// State of the first-run setup surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupStatus {
    /// True when no organization exists yet, so browser setup is allowed.
    pub needs_setup: bool,
    /// True when `TIDEBOOT_*` is configured for an unattended install.
    pub bootstrap_configured: bool,
}

/// Input for browser setup (`POST /v1/setup`).
#[derive(Debug, Clone)]
pub struct NewOrganization {
    /// Organization display name.
    pub organization_name: String,
    /// Owner email.
    pub email: String,
    /// Owner display name.
    pub display_name: String,
    /// Plaintext password chosen during setup (at least 12 characters).
    pub password: String,
}

/// Report whether the pool still needs first-run setup.
pub async fn setup_status(
    pool: &MySqlPool,
    bootstrap_configured: bool,
) -> Result<SetupStatus, PoolIdentityError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM organizations")
        .fetch_one(pool)
        .await
        .map_err(PoolIdentityError::Database)?;
    Ok(SetupStatus {
        needs_setup: count == 0,
        bootstrap_configured,
    })
}

/// Create the organization and its first owner from the browser setup form.
///
/// Fails with [`PoolIdentityError::AlreadyInitialized`] if any organization
/// already exists, so this cannot be used to create a second tenant.
pub async fn setup_pool(
    pool: &MySqlPool,
    input: NewOrganization,
) -> Result<(Organization, PoolUser), PoolIdentityError> {
    let org_name = validate_organization_name(&input.organization_name)?;
    let display_name = validate_display_name(&input.display_name)?;
    let email = normalize_email(&input.email)?;
    validate_password(&input.password)?;

    ensure_uninitialized(pool).await?;
    let password_hash = hash_password(&input.password)?;

    let mut tx = pool.begin().await.map_err(PoolIdentityError::Database)?;

    let org_insert = sqlx::query(
        "INSERT INTO organizations \
         (public_id, name, singleton, bootstrap_source, bootstrap_completed_at) \
         VALUES (?, ?, 1, 'browser', CURRENT_TIMESTAMP)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&org_name)
    .execute(&mut *tx)
    .await;

    let org_id = match org_insert {
        Ok(result) => result.last_insert_id(),
        Err(e) if is_unique_violation(&e) => {
            return Err(PoolIdentityError::AlreadyInitialized);
        }
        Err(e) => return Err(PoolIdentityError::Database(e)),
    };

    let user_insert = sqlx::query(
        "INSERT INTO pool_users \
         (public_id, organization_id, email, display_name, password_hash, role, \
          password_change_required) \
         VALUES (?, ?, ?, ?, ?, 'owner', 0)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(org_id)
    .bind(&email)
    .bind(&display_name)
    .bind(&password_hash)
    .execute(&mut *tx)
    .await;

    if let Err(e) = user_insert {
        return Err(if is_unique_violation(&e) {
            PoolIdentityError::Invalid("email already registered".into())
        } else {
            PoolIdentityError::Database(e)
        });
    }

    let organization = fetch_organization_by_internal_id(&mut tx, org_id).await?;
    let user = fetch_owner_by_organization(&mut tx, org_id).await?;
    tx.commit().await.map_err(PoolIdentityError::Database)?;

    Ok((organization, user))
}

/// Insert the organization and owner from `TIDEBOOT_*` settings.
///
/// Idempotent across restarts: if an organization exists (including one
/// created by an earlier bootstrap), returns
/// [`BootstrapOutcome::AlreadyBootstrapped`] and changes nothing.
pub async fn bootstrap_from_env(
    pool: &MySqlPool,
    config: &PoolBootstrapConfig,
) -> Result<BootstrapOutcome, PoolIdentityError> {
    if config.is_empty() {
        return Ok(BootstrapOutcome::Disabled);
    }

    let org_name = validate_organization_name(&config.organization_name)?;
    let display_name = validate_display_name(&config.owner_display_name)?;
    let email = normalize_email(&config.owner_email)?;
    validate_password(&config.owner_password)?;

    if !setup_status(pool, true).await?.needs_setup {
        return Ok(BootstrapOutcome::AlreadyBootstrapped);
    }

    let password_hash = hash_password(&config.owner_password)?;
    let mut tx = pool.begin().await.map_err(PoolIdentityError::Database)?;

    let org_insert = sqlx::query(
        "INSERT INTO organizations \
         (public_id, name, singleton, bootstrap_source, bootstrap_completed_at) \
         VALUES (?, ?, 1, 'environment', CURRENT_TIMESTAMP)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&org_name)
    .execute(&mut *tx)
    .await;

    let org_id = match org_insert {
        Ok(result) => result.last_insert_id(),
        // A concurrent startup won the race; that is still "already done".
        Err(e) if is_unique_violation(&e) => {
            return Ok(BootstrapOutcome::AlreadyBootstrapped);
        }
        Err(e) => return Err(PoolIdentityError::Database(e)),
    };

    let user_insert = sqlx::query(
        "INSERT INTO pool_users \
         (public_id, organization_id, email, display_name, password_hash, role, \
          password_change_required) \
         VALUES (?, ?, ?, ?, ?, 'owner', 1)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(org_id)
    .bind(&email)
    .bind(&display_name)
    .bind(&password_hash)
    .execute(&mut *tx)
    .await;

    if let Err(e) = user_insert {
        return Err(if is_unique_violation(&e) {
            // Another pool used this email; the whole transaction rolls back,
            // so the organization is not left without an owner.
            PoolIdentityError::Invalid("owner email already registered".into())
        } else {
            PoolIdentityError::Database(e)
        });
    }

    tx.commit().await.map_err(PoolIdentityError::Database)?;

    Ok(BootstrapOutcome::Created {
        organization: org_name,
        owner_email: email,
    })
}

/// Verify an operator's email and password.
///
/// Returns the operator on success. The temporary bootstrap password still
/// authenticates until [`change_password`] replaces the stored hash; the
/// caller must surface `password_change_required` and allow only the password
/// change route.
pub async fn authenticate(
    pool: &MySqlPool,
    email: &str,
    password: &str,
) -> Result<PoolUser, PoolIdentityError> {
    let email = normalize_email(email)?;
    let user = sqlx::query_as::<_, PoolUser>(
        "SELECT id, public_id, organization_id, email, display_name, password_hash, \
                role, password_change_required, last_login_at, created_at, updated_at \
         FROM pool_users WHERE email = ?",
    )
    .bind(&email)
    .fetch_optional(pool)
    .await
    .map_err(PoolIdentityError::Database)?
    .ok_or(PoolIdentityError::InvalidCredentials)?;

    let matches = auth::verify_password(password, &user.password_hash).map_err(|e| {
        tracing::error!(
            error = %e,
            operator = %user.public_id,
            "operator password verification failed"
        );
        PoolIdentityError::PasswordHash("verification failed".into())
    })?;
    if !matches {
        return Err(PoolIdentityError::InvalidCredentials);
    }

    sqlx::query("UPDATE pool_users SET last_login_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(user.id)
        .execute(pool)
        .await
        .map_err(PoolIdentityError::Database)?;

    Ok(user)
}

/// Fetch one operator by public id (the JWT subject).
pub async fn find_by_public_id(
    pool: &MySqlPool,
    public_id: &str,
) -> Result<Option<PoolUser>, PoolIdentityError> {
    sqlx::query_as::<_, PoolUser>(
        "SELECT id, public_id, organization_id, email, display_name, password_hash, \
                role, password_change_required, last_login_at, created_at, updated_at \
         FROM pool_users WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await
    .map_err(PoolIdentityError::Database)
}

/// Fetch the organization an operator belongs to.
pub async fn find_organization(
    pool: &MySqlPool,
    organization_id: u64,
) -> Result<Option<Organization>, PoolIdentityError> {
    sqlx::query_as::<_, Organization>(
        "SELECT id, public_id, name, created_at, updated_at \
         FROM organizations WHERE id = ?",
    )
    .bind(organization_id)
    .fetch_optional(pool)
    .await
    .map_err(PoolIdentityError::Database)
}

/// Replace an operator's password after verifying the current one.
///
/// Clears `password_change_required`. The update is conditional on the old
/// hash still being current, so two concurrent changes cannot both win. Once
/// this succeeds the bootstrap/temporary password no longer authenticates.
pub async fn change_password(
    pool: &MySqlPool,
    public_id: &str,
    current_password: &str,
    new_password: &str,
) -> Result<(), PoolIdentityError> {
    validate_password(new_password)?;

    let user = find_by_public_id(pool, public_id)
        .await?
        .ok_or(PoolIdentityError::NotFound)?;

    let current_matches = auth::verify_password(current_password, &user.password_hash)
        .map_err(|e| PoolIdentityError::PasswordHash(format!("verify failed: {e}")))?;
    if !current_matches {
        return Err(PoolIdentityError::InvalidCredentials);
    }
    if current_password == new_password {
        return Err(PoolIdentityError::Invalid(
            "new password must differ from the current password".into(),
        ));
    }

    let new_hash = hash_password(new_password)?;
    let result = sqlx::query(
        "UPDATE pool_users \
         SET password_hash = ?, password_change_required = 0 \
         WHERE public_id = ? AND password_hash = ?",
    )
    .bind(&new_hash)
    .bind(public_id)
    .bind(&user.password_hash)
    .execute(pool)
    .await
    .map_err(PoolIdentityError::Database)?;

    if result.rows_affected() == 0 {
        // The hash changed underneath us (another concurrent change).
        return Err(PoolIdentityError::InvalidCredentials);
    }
    Ok(())
}

// ── helpers ────────────────────────────────────────────────────────────────

async fn ensure_uninitialized(pool: &MySqlPool) -> Result<(), PoolIdentityError> {
    if !setup_status(pool, false).await?.needs_setup {
        return Err(PoolIdentityError::AlreadyInitialized);
    }
    Ok(())
}

fn hash_password(plain: &str) -> Result<String, PoolIdentityError> {
    auth::hash_password(plain).map_err(|e| PoolIdentityError::PasswordHash(e.to_string()))
}

fn validate_password(password: &str) -> Result<(), PoolIdentityError> {
    if password.len() < MIN_OPERATOR_PASSWORD_LEN {
        return Err(PoolIdentityError::WeakPassword);
    }
    if password.len() > MAX_OPERATOR_PASSWORD_LEN {
        return Err(PoolIdentityError::Invalid(format!(
            "password must be at most {MAX_OPERATOR_PASSWORD_LEN} characters"
        )));
    }
    Ok(())
}

fn validate_organization_name(name: &str) -> Result<String, PoolIdentityError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 128 {
        return Err(PoolIdentityError::Invalid(
            "organization name must be 1-128 characters".into(),
        ));
    }
    Ok(name.to_string())
}

fn validate_display_name(name: &str) -> Result<String, PoolIdentityError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 128 {
        return Err(PoolIdentityError::Invalid(
            "display name must be 1-128 characters".into(),
        ));
    }
    Ok(name.to_string())
}

fn normalize_email(email: &str) -> Result<String, PoolIdentityError> {
    let email = email.trim().to_lowercase();
    if !crate::config::is_valid_email(&email) {
        return Err(PoolIdentityError::Invalid("valid email required".into()));
    }
    Ok(email)
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.is_unique_violation())
}

async fn fetch_organization_by_internal_id(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    id: u64,
) -> Result<Organization, PoolIdentityError> {
    sqlx::query_as::<_, Organization>(
        "SELECT id, public_id, name, created_at, updated_at \
         FROM organizations WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&mut **tx)
    .await
    .map_err(PoolIdentityError::Database)
}

async fn fetch_owner_by_organization(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    organization_id: u64,
) -> Result<PoolUser, PoolIdentityError> {
    sqlx::query_as::<_, PoolUser>(
        "SELECT id, public_id, organization_id, email, display_name, password_hash, \
                role, password_change_required, last_login_at, created_at, updated_at \
         FROM pool_users WHERE organization_id = ? AND role = 'owner' LIMIT 1",
    )
    .bind(organization_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(PoolIdentityError::Database)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::organization::OrganizationView;
    use crate::entities::pool_user::{PoolRole, PoolUserView};

    #[test]
    fn short_password_is_rejected() {
        let err = validate_password("short").expect_err("must reject");
        assert!(matches!(err, PoolIdentityError::WeakPassword));
        assert_eq!(MIN_OPERATOR_PASSWORD_LEN, 12);
        assert!(validate_password("twelve-chars!").is_ok());
    }

    #[test]
    fn invalid_email_is_rejected() {
        assert!(matches!(
            normalize_email("not-an-email"),
            Err(PoolIdentityError::Invalid(_))
        ));
        assert_eq!(
            normalize_email(" Owner@Example.COM ").expect("valid"),
            "owner@example.com"
        );
    }

    #[test]
    fn organization_names_are_trimmed_and_bounded() {
        assert_eq!(
            validate_organization_name("  Rock Pool  ").expect("valid"),
            "Rock Pool"
        );
        assert!(validate_organization_name("   ").is_err());
        assert!(validate_organization_name(&"x".repeat(129)).is_err());
    }

    #[test]
    fn bootstrap_outcome_reports_created_and_already() {
        let created = BootstrapOutcome::Created {
            organization: "Pool".into(),
            owner_email: "owner@example.com".into(),
        };
        assert_ne!(created, BootstrapOutcome::AlreadyBootstrapped);
        assert_ne!(BootstrapOutcome::Disabled, created);
    }

    #[test]
    fn views_are_credential_free() {
        let view = PoolUserView {
            id: "11111111-1111-4111-8111-111111111111".into(),
            email: "owner@example.com".into(),
            display_name: "Owner".into(),
            role: PoolRole::Owner,
            password_change_required: true,
            last_login_at: None,
            created_at: "2026-10-19T12:00:00Z".parse().expect("ts"),
        };
        let json = serde_json::to_string(&view).expect("serialize");
        assert!(!json.contains("argon2"));
        assert!(!json.contains("password_hash"));
        assert!(!json.contains("argon2"));
        let _ = OrganizationView {
            id: "22222222-2222-4222-8222-222222222222".into(),
            name: "Pool".into(),
            created_at: "2026-10-19T12:00:00Z".parse().expect("ts"),
        };
    }
}
