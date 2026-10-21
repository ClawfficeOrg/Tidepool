//! Provider connection registry (task 1.3.1).
//!
//! A connection is one provider account the pool may receive events from.
//! Credentials are **references** to environment variables/secret-store
//! entries (`credential_ref`); the value is resolved only when validating,
//! never stored in the row and never returned.
//!
//! # Provider trait
//!
//! [`ConnectionProvider`] covers credential validation, webhook signature
//! verification and resource resolution. Adding a provider kind means adding
//! an implementation in [`providers`] and one arm in [`provider_for`].
//!
//! # Environment precedence
//!
//! `TIDEPOOL_CONNECTIONS` may hold a JSON array of complete connection
//! definitions. An environment entry wins over a database row with the same
//! slug, is labelled `managed_by = environment`, and cannot be mutated through
//! the API. A database row shadowed by the environment is preserved but not
//! effective.

pub mod github;
pub mod providers;

use crate::entities::connection::{
    Connection, ConnectionResource, ConnectionResourceView, ConnectionStatus, ConnectionView,
    ManagedBy, ProviderKind,
};
use crate::services::api_keys::{self as audit_svc, AuditActor, AuditEntry};
use crate::services::hosts::normalize_slug;
use sqlx::MySqlPool;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Credential values resolved from a secret reference.
#[derive(Debug, Clone, Default)]
pub struct CredentialMaterial {
    /// Field name → value.
    pub values: BTreeMap<String, String>,
}

impl CredentialMaterial {
    /// True when every required key has a non-empty value.
    pub fn has_all(&self, keys: &[&str]) -> bool {
        keys.iter()
            .all(|key| self.values.get(*key).is_some_and(|value| !value.is_empty()))
    }
}

/// A resource resolved by a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedResource {
    /// Provider that owns it.
    pub provider: ProviderKind,
    /// Resource kind.
    pub resource_kind: String,
    /// Provider-side id.
    pub external_id: String,
}

/// One provider implementation.
///
/// Object-safe and synchronous: save-time validation must not make network
/// calls or depend on provider availability.
pub trait ConnectionProvider: Send + Sync {
    /// Provider kind.
    fn kind(&self) -> ProviderKind;
    /// Credential fields the provider requires.
    fn required_credential_keys(&self) -> &'static [&'static str];
    /// Resolve a resource identity for this provider.
    fn resolve_resource(
        &self,
        resource_kind: &str,
        external_id: &str,
    ) -> Result<ResolvedResource, ConnectionError>;

    /// Validate credentials. Default checks required keys.
    fn validate_credentials(&self, material: &CredentialMaterial) -> Result<(), ConnectionError> {
        if material.has_all(self.required_credential_keys()) {
            Ok(())
        } else {
            Err(ConnectionError::CredentialInvalid(format!(
                "{:?} credentials need {}",
                self.kind(),
                self.required_credential_keys().join(", ")
            )))
        }
    }

    /// Verify a webhook signature. Provider tasks implement this; the registry
    /// default refuses until a transport is wired.
    fn verify_webhook(
        &self,
        _secret: &str,
        _headers: &actix_web::http::header::HeaderMap,
        _body: &[u8],
    ) -> Result<(), ConnectionError> {
        Err(ConnectionError::Invalid(
            "webhook verification is not implemented for this provider yet".into(),
        ))
    }
}

static GITHUB: providers::GithubProvider = providers::GithubProvider;
static SLACK: providers::SlackProvider = providers::SlackProvider;
static DISCORD: providers::DiscordProvider = providers::DiscordProvider;

/// Resolve the implementation for a provider kind.
pub fn provider_for(kind: ProviderKind) -> &'static dyn ConnectionProvider {
    match kind {
        ProviderKind::Github => &GITHUB,
        ProviderKind::Slack => &SLACK,
        ProviderKind::Discord => &DISCORD,
    }
}

/// Domain errors for connections.
#[derive(Debug, Error)]
pub enum ConnectionError {
    /// Invalid input.
    #[error("invalid input: {0}")]
    Invalid(String),
    /// No connection with that id/slug.
    #[error("connection not found")]
    NotFound,
    /// Slug already exists.
    #[error("connection slug is already in use: {0}")]
    Duplicate(String),
    /// Environment-managed connection cannot be changed through the API.
    #[error("connection is managed by the environment")]
    ManagedByEnvironment,
    /// Credential reference is absent from the environment.
    #[error("credential reference '{0}' is not set")]
    MissingCredential(String),
    /// Credential value is malformed or missing required fields.
    #[error("invalid credentials: {0}")]
    CredentialInvalid(String),
    /// A provider resource referenced by configuration is unknown.
    #[error("{field}: no connection delivers for '{value}'")]
    UnknownResource {
        /// Field name used by configuration validation.
        field: &'static str,
        /// Rejected resource identity.
        value: String,
    },
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// Effective connection used by provider transports.
#[derive(Debug, Clone)]
pub struct EffectiveConnection {
    /// Public id (`env:<slug>` for environment entries).
    pub public_id: String,
    /// Provider kind.
    pub provider: ProviderKind,
    /// Friendly slug.
    pub slug: String,
    /// Active or disabled.
    pub status: ConnectionStatus,
    /// Secret reference, never exposed through the API.
    pub credential_ref: String,
    /// Resource identities.
    pub resources: Vec<ConnectionResourceView>,
}

/// Input for creating a connection.
#[derive(Debug, Clone)]
pub struct NewConnection {
    /// Provider kind.
    pub provider: ProviderKind,
    /// Friendly slug.
    pub slug: String,
    /// Display name.
    pub display_name: String,
    /// Secret reference (environment variable name).
    pub credential_ref: String,
    /// Resource identities to deliver for.
    pub resources: Vec<NewResource>,
}

/// One resource in a create request.
#[derive(Debug, Clone)]
pub struct NewResource {
    /// Resource kind.
    pub resource_kind: String,
    /// Provider-side id.
    pub external_id: String,
    /// Optional label.
    pub display_name: Option<String>,
}

/// Input for updating a connection.
#[derive(Debug, Clone, Default)]
pub struct UpdateConnection {
    /// New slug.
    pub slug: Option<String>,
    /// New display name.
    pub display_name: Option<String>,
    /// New status.
    pub status: Option<ConnectionStatus>,
}

/// Create a database-managed connection.
pub async fn create_connection(
    pool: &MySqlPool,
    organization_id: u64,
    input: NewConnection,
    actor: AuditActor,
) -> Result<ConnectionView, ConnectionError> {
    let slug = validate_slug(&input.slug)?;
    let display_name = validate_display_name(&input.display_name)?;
    let credential_ref = validate_credential_ref(&input.credential_ref)?;
    let provider = provider_for(input.provider);
    let material = resolve_credentials(&credential_ref)?;
    provider.validate_credentials(&material)?;

    let resources = validate_resources(provider, &input.resources)?;
    let public_id = uuid::Uuid::new_v4().to_string();

    let mut tx = pool.begin().await.map_err(ConnectionError::Database)?;
    let result = sqlx::query(
        "INSERT INTO connections \
         (public_id, organization_id, provider, slug, display_name, credential_ref, status) \
         VALUES (?, ?, ?, ?, ?, ?, 'active')",
    )
    .bind(&public_id)
    .bind(organization_id)
    .bind(input.provider.as_str())
    .bind(&slug)
    .bind(&display_name)
    .bind(&credential_ref)
    .execute(&mut *tx)
    .await;

    let connection_id = match result {
        Ok(result) => result.last_insert_id(),
        Err(error) if is_unique_violation(&error) => {
            return Err(ConnectionError::Duplicate(slug));
        }
        Err(error) => return Err(ConnectionError::Database(error)),
    };

    for resource in &resources {
        sqlx::query(
            "INSERT INTO connection_resources \
             (connection_id, resource_kind, external_id, display_name) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(connection_id)
        .bind(&resource.resource_kind)
        .bind(&resource.external_id)
        .bind(&resource.display_name)
        .execute(&mut *tx)
        .await
        .map_err(ConnectionError::Database)?;
    }
    tx.commit().await.map_err(ConnectionError::Database)?;

    let connection = fetch_connection(pool, organization_id, &public_id)
        .await?
        .ok_or(ConnectionError::NotFound)?;
    let resources = fetch_resources(pool, connection.id).await?;
    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id,
            actor,
            action: "connection.create",
            target_kind: Some("connection"),
            target_public_id: Some(&connection.public_id),
            detail: serde_json::json!({
                "provider": connection.provider.as_str(),
                "slug": connection.slug,
            }),
        },
    )
    .await;
    Ok(view(&connection, &resources, ManagedBy::Database, true))
}

/// List effective connections: environment entries win over database rows.
pub async fn list_connections(
    pool: &MySqlPool,
    organization_id: u64,
) -> Result<Vec<ConnectionView>, ConnectionError> {
    let environment = environment_connections()?;
    let environment_slugs: BTreeSet<&str> = environment
        .iter()
        .map(|entry| entry.slug.as_str())
        .collect();

    let rows = sqlx::query_as::<_, Connection>(
        "SELECT id, public_id, organization_id, provider, slug, display_name, credential_ref, \
                status, created_at, updated_at \
         FROM connections WHERE organization_id = ? ORDER BY slug",
    )
    .bind(organization_id)
    .fetch_all(pool)
    .await
    .map_err(ConnectionError::Database)?;

    let mut effective = Vec::new();
    for connection in &rows {
        if environment_slugs.contains(connection.slug.as_str()) {
            continue;
        }
        let resources = fetch_resources(pool, connection.id).await?;
        effective.push(view(
            connection,
            &resources,
            ManagedBy::Database,
            credential_configured(&connection.credential_ref),
        ));
    }
    effective.extend(environment.iter().map(|entry| entry.view()));
    effective.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(effective)
}

/// Fetch one effective connection by public id (`env:<slug>` for environment).
pub async fn get_connection(
    pool: &MySqlPool,
    organization_id: u64,
    public_id: &str,
) -> Result<ConnectionView, ConnectionError> {
    if let Some(slug) = public_id.strip_prefix("env:") {
        let environment = environment_connections()?;
        return environment
            .into_iter()
            .find(|entry| entry.slug == slug)
            .map(|entry| entry.view())
            .ok_or(ConnectionError::NotFound);
    }

    let connection = fetch_connection(pool, organization_id, public_id)
        .await?
        .ok_or(ConnectionError::NotFound)?;
    let resources = fetch_resources(pool, connection.id).await?;
    Ok(view(
        &connection,
        &resources,
        ManagedBy::Database,
        credential_configured(&connection.credential_ref),
    ))
}

/// Rename / disable a database connection.
pub async fn update_connection(
    pool: &MySqlPool,
    organization_id: u64,
    public_id: &str,
    input: UpdateConnection,
    actor: AuditActor,
) -> Result<ConnectionView, ConnectionError> {
    let connection = fetch_connection(pool, organization_id, public_id)
        .await?
        .ok_or(ConnectionError::NotFound)?;
    refuse_if_environment(&connection.slug)?;

    let slug = match input.slug.as_deref() {
        Some(raw) => validate_slug(raw)?,
        None => connection.slug.clone(),
    };
    let display_name = match input.display_name.as_deref() {
        Some(raw) => validate_display_name(raw)?,
        None => connection.display_name.clone(),
    };
    let status = input.status.unwrap_or(connection.status);

    let result = sqlx::query(
        "UPDATE connections SET slug = ?, display_name = ?, status = ? \
         WHERE id = ? AND organization_id = ?",
    )
    .bind(&slug)
    .bind(&display_name)
    .bind(status.as_str())
    .bind(connection.id)
    .bind(organization_id)
    .execute(pool)
    .await;
    match result {
        Ok(_) => {}
        Err(error) if is_unique_violation(&error) => {
            return Err(ConnectionError::Duplicate(slug));
        }
        Err(error) => return Err(ConnectionError::Database(error)),
    }

    let updated = fetch_connection(pool, organization_id, public_id)
        .await?
        .ok_or(ConnectionError::NotFound)?;
    let resources = fetch_resources(pool, updated.id).await?;
    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id,
            actor,
            action: "connection.update",
            target_kind: Some("connection"),
            target_public_id: Some(&updated.public_id),
            detail: serde_json::json!({ "slug": updated.slug, "status": status.as_str() }),
        },
    )
    .await;
    Ok(view(
        &updated,
        &resources,
        ManagedBy::Database,
        credential_configured(&updated.credential_ref),
    ))
}

/// Delete a database connection. The provider stops delivering immediately.
pub async fn delete_connection(
    pool: &MySqlPool,
    organization_id: u64,
    public_id: &str,
    actor: AuditActor,
) -> Result<(), ConnectionError> {
    let connection = fetch_connection(pool, organization_id, public_id)
        .await?
        .ok_or(ConnectionError::NotFound)?;
    refuse_if_environment(&connection.slug)?;

    sqlx::query("DELETE FROM connections WHERE id = ? AND organization_id = ?")
        .bind(connection.id)
        .bind(organization_id)
        .execute(pool)
        .await
        .map_err(ConnectionError::Database)?;
    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id,
            actor,
            action: "connection.delete",
            target_kind: Some("connection"),
            target_public_id: Some(&connection.public_id),
            detail: serde_json::json!({ "slug": connection.slug }),
        },
    )
    .await;
    Ok(())
}

/// Resolve a resource to the connection that owns it.
///
/// Used by trigger validation; environment entries participate too.
pub async fn resolve_resource_owner(
    pool: &MySqlPool,
    organization_id: u64,
    provider: ProviderKind,
    resource_kind: &str,
    external_id: &str,
) -> Result<Option<ConnectionView>, ConnectionError> {
    let connections = list_connections(pool, organization_id).await?;
    Ok(connections.into_iter().find(|connection| {
        connection.provider == provider
            && connection.status == ConnectionStatus::Active
            && connection.resources.iter().any(|resource| {
                resource.resource_kind == resource_kind && resource.external_id == external_id
            })
    }))
}

/// Fetch an effective connection by slug (database or environment).
pub async fn find_effective_by_slug(
    pool: &MySqlPool,
    organization_id: u64,
    slug: &str,
) -> Result<Option<EffectiveConnection>, ConnectionError> {
    let slug = validate_slug(slug)?;

    if let Some(entry) = environment_connections()?
        .into_iter()
        .find(|entry| entry.slug == slug)
    {
        return Ok(Some(EffectiveConnection {
            public_id: format!("env:{}", entry.slug),
            provider: entry.provider,
            slug: entry.slug,
            status: ConnectionStatus::Active,
            credential_ref: entry.credential_ref,
            resources: entry
                .resources
                .iter()
                .map(|resource| ConnectionResourceView {
                    resource_kind: resource.resource_kind.clone(),
                    external_id: resource.external_id.clone(),
                    display_name: resource.display_name.clone(),
                })
                .collect(),
        }));
    }

    let connection = sqlx::query_as::<_, Connection>(
        "SELECT id, public_id, organization_id, provider, slug, display_name, credential_ref, \
                status, created_at, updated_at \
         FROM connections WHERE organization_id = ? AND slug = ?",
    )
    .bind(organization_id)
    .bind(&slug)
    .fetch_optional(pool)
    .await
    .map_err(ConnectionError::Database)?;

    let Some(connection) = connection else {
        return Ok(None);
    };
    let resources = fetch_resources(pool, connection.id).await?;
    Ok(Some(EffectiveConnection {
        public_id: connection.public_id,
        provider: connection.provider,
        slug: connection.slug,
        status: connection.status,
        credential_ref: connection.credential_ref,
        resources: resources.iter().map(ConnectionResourceView::from).collect(),
    }))
}

/// Resolve credential material for an effective connection.
///
/// Used by provider transports after the connection is located; never exposed
/// through the API.
pub fn credential_material(reference: &str) -> Result<CredentialMaterial, ConnectionError> {
    resolve_credentials(reference)
}

/// Find the active GitHub connection that owns a repository (case-insensitive).
pub async fn resolve_repository_connection(
    pool: &MySqlPool,
    organization_id: u64,
    repository: &str,
) -> Result<Option<ConnectionView>, ConnectionError> {
    let repository = repository.trim().to_ascii_lowercase();
    let connections = list_connections(pool, organization_id).await?;
    Ok(connections.into_iter().find(|connection| {
        connection.provider == ProviderKind::Github
            && connection.status == ConnectionStatus::Active
            && connection.resources.iter().any(|resource| {
                resource.resource_kind == "repository"
                    && resource.external_id.to_ascii_lowercase() == repository
            })
    }))
}

/// Validate that a repository is owned by an active GitHub connection.
///
/// Fails with a named `repository` field so trigger configuration is refused at
/// save time.
pub async fn validate_repository(
    pool: &MySqlPool,
    organization_id: u64,
    repository: &str,
) -> Result<ConnectionView, ConnectionError> {
    resolve_repository_connection(pool, organization_id, repository)
        .await?
        .ok_or_else(|| ConnectionError::UnknownResource {
            field: "repository",
            value: repository.to_string(),
        })
}

// ── environment overlay ───────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
struct EnvironmentEntry {
    provider: ProviderKind,
    slug: String,
    display_name: String,
    credential_ref: String,
    #[serde(default)]
    resources: Vec<EnvironmentResource>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct EnvironmentResource {
    resource_kind: String,
    external_id: String,
    #[serde(default)]
    display_name: Option<String>,
}

struct EnvironmentConnection {
    provider: ProviderKind,
    slug: String,
    display_name: String,
    credential_ref: String,
    resources: Vec<NewResource>,
}

impl EnvironmentConnection {
    fn view(&self) -> ConnectionView {
        ConnectionView {
            id: format!("env:{}", self.slug),
            provider: self.provider,
            slug: self.slug.clone(),
            display_name: self.display_name.clone(),
            status: ConnectionStatus::Active,
            managed_by: ManagedBy::Environment,
            credential_configured: credential_configured(&self.credential_ref),
            resources: self
                .resources
                .iter()
                .map(|resource| ConnectionResourceView {
                    resource_kind: resource.resource_kind.clone(),
                    external_id: resource.external_id.clone(),
                    display_name: resource.display_name.clone(),
                })
                .collect(),
            created_at: chrono::Utc::now(),
        }
    }
}

fn environment_connections() -> Result<Vec<EnvironmentConnection>, ConnectionError> {
    let Ok(raw) = std::env::var("TIDEPOOL_CONNECTIONS") else {
        return Ok(Vec::new());
    };
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let entries: Vec<EnvironmentEntry> = serde_json::from_str(&raw).map_err(|e| {
        ConnectionError::Invalid(format!("TIDEPOOL_CONNECTIONS must be a JSON array: {e}"))
    })?;

    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for entry in entries {
        let slug = validate_slug(&entry.slug)?;
        if !seen.insert(slug.clone()) {
            return Err(ConnectionError::Invalid(format!(
                "duplicate environment connection slug '{slug}'"
            )));
        }
        let display_name = validate_display_name(&entry.display_name)?;
        let credential_ref = validate_credential_ref(&entry.credential_ref)?;
        let provider = provider_for(entry.provider);
        let material = resolve_credentials(&credential_ref)?;
        provider.validate_credentials(&material)?;
        let resources = entry
            .resources
            .into_iter()
            .map(|resource| NewResource {
                resource_kind: resource.resource_kind,
                external_id: resource.external_id,
                display_name: resource.display_name,
            })
            .collect::<Vec<_>>();
        validate_resources(provider, &resources)?;
        result.push(EnvironmentConnection {
            provider: entry.provider,
            slug,
            display_name,
            credential_ref,
            resources,
        });
    }
    Ok(result)
}

fn refuse_if_environment(slug: &str) -> Result<(), ConnectionError> {
    if environment_connections()?
        .iter()
        .any(|entry| entry.slug == slug)
    {
        return Err(ConnectionError::ManagedByEnvironment);
    }
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────

fn resolve_credentials(reference: &str) -> Result<CredentialMaterial, ConnectionError> {
    let raw = std::env::var(reference)
        .map_err(|_| ConnectionError::MissingCredential(reference.to_string()))?;
    providers::parse_credential_material(&raw)
}

fn credential_configured(reference: &str) -> bool {
    std::env::var(reference).is_ok()
}

fn validate_slug(raw: &str) -> Result<String, ConnectionError> {
    let slug = normalize_slug(raw);
    if slug.is_empty() {
        return Err(ConnectionError::Invalid("slug is required".into()));
    }
    Ok(slug)
}

fn validate_display_name(raw: &str) -> Result<String, ConnectionError> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > 128 {
        return Err(ConnectionError::Invalid(
            "display_name must be 1-128 characters".into(),
        ));
    }
    Ok(name.to_string())
}

fn validate_credential_ref(raw: &str) -> Result<String, ConnectionError> {
    let reference = raw.trim();
    if reference.is_empty() || reference.len() > 255 {
        return Err(ConnectionError::Invalid(
            "credential_ref must be 1-255 characters".into(),
        ));
    }
    Ok(reference.to_string())
}

fn validate_resources(
    provider: &dyn ConnectionProvider,
    resources: &[NewResource],
) -> Result<Vec<NewResource>, ConnectionError> {
    let mut seen = BTreeSet::new();
    let mut validated = Vec::new();
    for resource in resources {
        provider.resolve_resource(&resource.resource_kind, &resource.external_id)?;
        if !seen.insert((resource.resource_kind.clone(), resource.external_id.clone())) {
            return Err(ConnectionError::Invalid(
                "duplicate resource identity".into(),
            ));
        }
        validated.push(resource.clone());
    }
    Ok(validated)
}

fn view(
    connection: &Connection,
    resources: &[ConnectionResource],
    managed_by: ManagedBy,
    credential_configured: bool,
) -> ConnectionView {
    ConnectionView {
        id: connection.public_id.clone(),
        provider: connection.provider,
        slug: connection.slug.clone(),
        display_name: connection.display_name.clone(),
        status: connection.status,
        managed_by,
        credential_configured,
        resources: resources.iter().map(ConnectionResourceView::from).collect(),
        created_at: connection.created_at,
    }
}

async fn fetch_connection(
    pool: &MySqlPool,
    organization_id: u64,
    public_id: &str,
) -> Result<Option<Connection>, ConnectionError> {
    sqlx::query_as::<_, Connection>(
        "SELECT id, public_id, organization_id, provider, slug, display_name, credential_ref, \
                status, created_at, updated_at \
         FROM connections WHERE public_id = ? AND organization_id = ?",
    )
    .bind(public_id)
    .bind(organization_id)
    .fetch_optional(pool)
    .await
    .map_err(ConnectionError::Database)
}

async fn fetch_resources(
    pool: &MySqlPool,
    connection_id: u64,
) -> Result<Vec<ConnectionResource>, ConnectionError> {
    sqlx::query_as::<_, ConnectionResource>(
        "SELECT id, connection_id, resource_kind, external_id, display_name \
         FROM connection_resources WHERE connection_id = ? ORDER BY resource_kind, external_id",
    )
    .bind(connection_id)
    .fetch_all(pool)
    .await
    .map_err(ConnectionError::Database)
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.is_unique_violation())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ScriptedProvider;

    impl ConnectionProvider for ScriptedProvider {
        fn kind(&self) -> ProviderKind {
            ProviderKind::Github
        }
        fn required_credential_keys(&self) -> &'static [&'static str] {
            &["token"]
        }
        fn resolve_resource(
            &self,
            resource_kind: &str,
            external_id: &str,
        ) -> Result<ResolvedResource, ConnectionError> {
            if resource_kind != "repository" {
                return Err(ConnectionError::Invalid("bad kind".into()));
            }
            Ok(ResolvedResource {
                provider: ProviderKind::Github,
                resource_kind: resource_kind.into(),
                external_id: external_id.into(),
            })
        }
    }

    #[test]
    fn scripted_provider_validates_credentials_and_resources() {
        let provider = ScriptedProvider;
        let material = CredentialMaterial {
            values: [("token".to_string(), "secret".to_string())]
                .into_iter()
                .collect(),
        };
        provider.validate_credentials(&material).expect("valid");
        assert!(provider.resolve_resource("repository", "org/repo").is_ok());
        assert!(provider.resolve_resource("guild", "1").is_err());
    }

    #[test]
    fn credential_material_requires_non_empty_values() {
        let material = CredentialMaterial {
            values: [("token".to_string(), "".to_string())]
                .into_iter()
                .collect(),
        };
        assert!(!material.has_all(&["token"]));
    }

    #[test]
    fn unknown_credentials_are_invalid_json() {
        assert!(providers::parse_credential_material("not-json").is_err());
        assert!(
            providers::parse_credential_material(r#"{"token":"x"}"#)
                .expect("json")
                .has_all(&["token"])
        );
    }
}
