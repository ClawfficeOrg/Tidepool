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
use crate::entities::pool_invitation::PoolInvitation;
use crate::entities::pool_user::{PoolRole, PoolUser};
use crate::services::api_keys::{self as audit_svc, AuditActor, AuditEntry};
use crate::services::auth;
use chrono::{Duration, Utc};
use sqlx::MySqlPool;
use thiserror::Error;

/// Minimum length for an operator password (browser setup, bootstrap and
/// password change all use the same floor).
pub const MIN_OPERATOR_PASSWORD_LEN: usize = 12;

/// Maximum accepted plaintext password length.
pub const MAX_OPERATOR_PASSWORD_LEN: usize = 1024;

/// Default invitation lifetime: seven days.
pub const INVITATION_TTL_SECS: i64 = 7 * 24 * 60 * 60;

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
    /// No operator with that public id (or not in this organization).
    #[error("operator not found")]
    NotFound,
    /// The caller's role may not perform this operation.
    #[error("forbidden: {0}")]
    Forbidden(String),
    /// Removing or demoting this owner would leave the organization ownerless.
    #[error("the last owner cannot be removed or demoted")]
    LastOwner,
    /// Invitation is unknown or the secret half does not verify.
    #[error("invitation is invalid")]
    InvitationInvalid,
    /// Invitation has already been accepted.
    #[error("invitation has already been used")]
    InvitationUsed,
    /// Invitation was revoked by an owner.
    #[error("invitation has been revoked")]
    InvitationRevoked,
    /// Invitation is past its expiry.
    #[error("invitation has expired")]
    InvitationExpired,
    /// Invitation is unknown in this organization.
    #[error("invitation not found")]
    InvitationNotFound,
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

    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id: organization.id,
            actor: AuditActor::System,
            action: "pool.setup",
            target_kind: Some("organization"),
            target_public_id: Some(&organization.public_id),
            detail: serde_json::json!({ "name": organization.name }),
        },
    )
    .await;

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

    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id: org_id,
            actor: AuditActor::System,
            action: "pool.bootstrap",
            target_kind: Some("organization"),
            target_public_id: None,
            detail: serde_json::json!({ "name": org_name }),
        },
    )
    .await;

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

    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id: user.organization_id,
            actor: AuditActor::member(&user.public_id, &user.email),
            action: "member.password_change",
            target_kind: Some("member"),
            target_public_id: Some(&user.public_id),
            detail: serde_json::Value::Null,
        },
    )
    .await;

    Ok(())
}

// ── team members and invitations (task 1.1.2) ─────────────────────────────

/// List the operators in an organization (any role may read).
pub async fn list_members(
    pool: &MySqlPool,
    organization_id: u64,
) -> Result<Vec<PoolUser>, PoolIdentityError> {
    sqlx::query_as::<_, PoolUser>(
        "SELECT id, public_id, organization_id, email, display_name, password_hash, \
                role, password_change_required, last_login_at, created_at, updated_at \
         FROM pool_users WHERE organization_id = ? ORDER BY created_at, id",
    )
    .bind(organization_id)
    .fetch_all(pool)
    .await
    .map_err(PoolIdentityError::Database)
}

/// Create an invitation and return it with the raw one-time token.
///
/// The raw token is `"<public_id>.<secret>"`; only an Argon2id hash of the
/// secret is stored. Any still-pending invitation for the same email is
/// revoked first, so only the newest link is valid.
pub async fn create_invitation(
    pool: &MySqlPool,
    organization_id: u64,
    invited_by: Option<u64>,
    email: &str,
    role: PoolRole,
    ttl_secs: i64,
) -> Result<(PoolInvitation, String), PoolIdentityError> {
    let email = normalize_email(email)?;

    if find_by_email(pool, &email).await?.is_some() {
        return Err(PoolIdentityError::Invalid(
            "that email already belongs to a member".into(),
        ));
    }

    let public_id = uuid::Uuid::new_v4().to_string();
    let secret = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let token_hash = hash_password(&secret)?;
    let raw_token = format!("{public_id}.{secret}");
    let expires_at = Utc::now() + Duration::seconds(ttl_secs);

    let mut tx = pool.begin().await.map_err(PoolIdentityError::Database)?;

    sqlx::query(
        "UPDATE pool_invitations SET revoked_at = CURRENT_TIMESTAMP \
         WHERE organization_id = ? AND email = ? \
           AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(organization_id)
    .bind(&email)
    .execute(&mut *tx)
    .await
    .map_err(PoolIdentityError::Database)?;

    sqlx::query(
        "INSERT INTO pool_invitations \
         (public_id, organization_id, email, role, token_hash, invited_by, expires_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(&email)
    .bind(role.as_str())
    .bind(&token_hash)
    .bind(invited_by)
    .bind(expires_at)
    .execute(&mut *tx)
    .await
    .map_err(PoolIdentityError::Database)?;

    let invitation = sqlx::query_as::<_, PoolInvitation>(
        "SELECT id, public_id, organization_id, email, role, token_hash, invited_by, \
                expires_at, accepted_at, revoked_at, created_at, updated_at \
         FROM pool_invitations WHERE public_id = ?",
    )
    .bind(&public_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(PoolIdentityError::Database)?;

    tx.commit().await.map_err(PoolIdentityError::Database)?;
    Ok((invitation, raw_token))
}

/// List invitations that can still be accepted.
pub async fn list_pending_invitations(
    pool: &MySqlPool,
    organization_id: u64,
) -> Result<Vec<PoolInvitation>, PoolIdentityError> {
    sqlx::query_as::<_, PoolInvitation>(
        "SELECT id, public_id, organization_id, email, role, token_hash, invited_by, \
                expires_at, accepted_at, revoked_at, created_at, updated_at \
         FROM pool_invitations \
         WHERE organization_id = ? AND accepted_at IS NULL AND revoked_at IS NULL \
           AND expires_at > CURRENT_TIMESTAMP \
         ORDER BY created_at, id",
    )
    .bind(organization_id)
    .fetch_all(pool)
    .await
    .map_err(PoolIdentityError::Database)
}

/// Revoke a pending invitation.
pub async fn revoke_invitation(
    pool: &MySqlPool,
    organization_id: u64,
    invitation_public_id: &str,
) -> Result<(), PoolIdentityError> {
    let result = sqlx::query(
        "UPDATE pool_invitations SET revoked_at = CURRENT_TIMESTAMP \
         WHERE public_id = ? AND organization_id = ? \
           AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(invitation_public_id)
    .bind(organization_id)
    .execute(pool)
    .await
    .map_err(PoolIdentityError::Database)?;

    if result.rows_affected() == 0 {
        return Err(PoolIdentityError::InvitationNotFound);
    }
    Ok(())
}

/// Accept an invitation, creating the member account.
///
/// Single use is enforced inside a transaction with `FOR UPDATE`: a second
/// concurrent acceptance sees `accepted_at` and fails with
/// [`PoolIdentityError::InvitationUsed`]. Expired and revoked invitations fail
/// before the user row is created.
pub async fn accept_invitation(
    pool: &MySqlPool,
    raw_token: &str,
    display_name: &str,
    password: &str,
) -> Result<PoolUser, PoolIdentityError> {
    let display_name = validate_display_name(display_name)?;
    validate_password(password)?;

    let (public_id, secret) =
        parse_invitation_token(raw_token).ok_or(PoolIdentityError::InvitationInvalid)?;

    let invitation = sqlx::query_as::<_, PoolInvitation>(
        "SELECT id, public_id, organization_id, email, role, token_hash, invited_by, \
                expires_at, accepted_at, revoked_at, created_at, updated_at \
         FROM pool_invitations WHERE public_id = ?",
    )
    .bind(public_id)
    .fetch_optional(pool)
    .await
    .map_err(PoolIdentityError::Database)?
    .ok_or(PoolIdentityError::InvitationInvalid)?;

    let secret_matches = auth::verify_password(secret, &invitation.token_hash)
        .map_err(|e| PoolIdentityError::PasswordHash(format!("invitation verify failed: {e}")))?;
    if !secret_matches {
        return Err(PoolIdentityError::InvitationInvalid);
    }

    let password_hash = hash_password(password)?;
    let mut tx = pool.begin().await.map_err(PoolIdentityError::Database)?;

    let invitation = sqlx::query_as::<_, PoolInvitation>(
        "SELECT id, public_id, organization_id, email, role, token_hash, invited_by, \
                expires_at, accepted_at, revoked_at, created_at, updated_at \
         FROM pool_invitations WHERE id = ? FOR UPDATE",
    )
    .bind(invitation.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(PoolIdentityError::Database)?;

    let now = Utc::now();
    if invitation.accepted_at.is_some() {
        return Err(PoolIdentityError::InvitationUsed);
    }
    if invitation.revoked_at.is_some() {
        return Err(PoolIdentityError::InvitationRevoked);
    }
    if invitation.expires_at <= now {
        return Err(PoolIdentityError::InvitationExpired);
    }

    let user_public_id = uuid::Uuid::new_v4().to_string();
    let insert = sqlx::query(
        "INSERT INTO pool_users \
         (public_id, organization_id, email, display_name, password_hash, role, \
          password_change_required) \
         VALUES (?, ?, ?, ?, ?, ?, 0)",
    )
    .bind(&user_public_id)
    .bind(invitation.organization_id)
    .bind(&invitation.email)
    .bind(&display_name)
    .bind(&password_hash)
    .bind(invitation.role.as_str())
    .execute(&mut *tx)
    .await;

    if let Err(e) = insert {
        return Err(if is_unique_violation(&e) {
            PoolIdentityError::Invalid("that email already belongs to a member".into())
        } else {
            PoolIdentityError::Database(e)
        });
    }

    sqlx::query("UPDATE pool_invitations SET accepted_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind(invitation.id)
        .execute(&mut *tx)
        .await
        .map_err(PoolIdentityError::Database)?;

    let user = sqlx::query_as::<_, PoolUser>(
        "SELECT id, public_id, organization_id, email, display_name, password_hash, \
                role, password_change_required, last_login_at, created_at, updated_at \
         FROM pool_users WHERE public_id = ?",
    )
    .bind(&user_public_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(PoolIdentityError::Database)?;

    tx.commit().await.map_err(PoolIdentityError::Database)?;

    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id: invitation.organization_id,
            actor: AuditActor::member(&user.public_id, &user.email),
            action: "invitation.accept",
            target_kind: Some("invitation"),
            target_public_id: Some(&invitation.public_id),
            detail: serde_json::json!({ "role": user.role.as_str() }),
        },
    )
    .await;

    Ok(user)
}

/// Change a member's role, refusing to demote the last owner.
pub async fn change_member_role(
    pool: &MySqlPool,
    organization_id: u64,
    member_public_id: &str,
    new_role: PoolRole,
) -> Result<PoolUser, PoolIdentityError> {
    let mut tx = pool.begin().await.map_err(PoolIdentityError::Database)?;

    // Lock every member of the organization in id order. A consistent order
    // avoids deadlocks when two owner actions overlap, and the full set gives
    // the last-owner count in the same snapshot.
    let members = lock_organization_members(&mut tx, organization_id).await?;
    let target = members
        .iter()
        .find(|member| member.public_id == member_public_id)
        .cloned()
        .ok_or(PoolIdentityError::NotFound)?;

    if target.role == PoolRole::Owner
        && new_role != PoolRole::Owner
        && members
            .iter()
            .filter(|member| member.role == PoolRole::Owner)
            .count()
            <= 1
    {
        return Err(PoolIdentityError::LastOwner);
    }

    sqlx::query("UPDATE pool_users SET role = ? WHERE id = ?")
        .bind(new_role.as_str())
        .bind(target.id)
        .execute(&mut *tx)
        .await
        .map_err(PoolIdentityError::Database)?;

    let updated = sqlx::query_as::<_, PoolUser>(
        "SELECT id, public_id, organization_id, email, display_name, password_hash, \
                role, password_change_required, last_login_at, created_at, updated_at \
         FROM pool_users WHERE id = ?",
    )
    .bind(target.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(PoolIdentityError::Database)?;

    tx.commit().await.map_err(PoolIdentityError::Database)?;
    Ok(updated)
}

/// Remove a member, refusing to remove the last owner.
///
/// The row is deleted, so any JWT whose subject is that member stops resolving
/// on the next request (the operator extractor always loads the row).
pub async fn remove_member(
    pool: &MySqlPool,
    organization_id: u64,
    member_public_id: &str,
) -> Result<(), PoolIdentityError> {
    let mut tx = pool.begin().await.map_err(PoolIdentityError::Database)?;

    let members = lock_organization_members(&mut tx, organization_id).await?;
    let target = members
        .iter()
        .find(|member| member.public_id == member_public_id)
        .cloned()
        .ok_or(PoolIdentityError::NotFound)?;

    if target.role == PoolRole::Owner
        && members
            .iter()
            .filter(|member| member.role == PoolRole::Owner)
            .count()
            <= 1
    {
        return Err(PoolIdentityError::LastOwner);
    }

    sqlx::query("DELETE FROM pool_users WHERE id = ?")
        .bind(target.id)
        .execute(&mut *tx)
        .await
        .map_err(PoolIdentityError::Database)?;

    tx.commit().await.map_err(PoolIdentityError::Database)?;
    Ok(())
}

async fn lock_organization_members(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    organization_id: u64,
) -> Result<Vec<PoolUser>, PoolIdentityError> {
    sqlx::query_as::<_, PoolUser>(
        "SELECT id, public_id, organization_id, email, display_name, password_hash, \
                role, password_change_required, last_login_at, created_at, updated_at \
         FROM pool_users WHERE organization_id = ? ORDER BY id FOR UPDATE",
    )
    .bind(organization_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(PoolIdentityError::Database)
}

async fn find_by_email(
    pool: &MySqlPool,
    email: &str,
) -> Result<Option<PoolUser>, PoolIdentityError> {
    sqlx::query_as::<_, PoolUser>(
        "SELECT id, public_id, organization_id, email, display_name, password_hash, \
                role, password_change_required, last_login_at, created_at, updated_at \
         FROM pool_users WHERE email = ?",
    )
    .bind(email)
    .fetch_optional(pool)
    .await
    .map_err(PoolIdentityError::Database)
}

fn parse_invitation_token(raw: &str) -> Option<(&str, &str)> {
    let (public_id, secret) = raw.split_once('.')?;
    if public_id.is_empty() || secret.is_empty() {
        return None;
    }
    Some((public_id, secret))
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
    fn invitation_token_round_trips_its_public_id() {
        let token = "11111111-1111-4111-8111-111111111111.abcdef";
        assert_eq!(
            parse_invitation_token(token),
            Some(("11111111-1111-4111-8111-111111111111", "abcdef"))
        );
        assert_eq!(parse_invitation_token("no-dot"), None);
        assert_eq!(parse_invitation_token(".secret"), None);
        assert_eq!(parse_invitation_token("id."), None);
    }

    #[test]
    fn invitation_ttl_is_seven_days() {
        assert_eq!(INVITATION_TTL_SECS, 604_800);
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
