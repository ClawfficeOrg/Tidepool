//! Built-in provider implementations for the connection registry.
//!
//! Adding a provider kind means adding one [`ConnectionProvider`]
//! implementation here and one arm in [`super::provider_for`]. Provider-specific
//! transports (GitHub webhooks, Slack socket mode, Discord gateway) live in the
//! provider tasks and build on this trait.

use super::{ConnectionError, ConnectionProvider, CredentialMaterial, ResolvedResource};
use crate::entities::connection::ProviderKind;
use std::collections::BTreeMap;

/// GitHub App provider.
pub struct GithubProvider;

impl ConnectionProvider for GithubProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Github
    }

    fn required_credential_keys(&self) -> &'static [&'static str] {
        &["app_id", "private_key", "webhook_secret"]
    }

    fn resolve_resource(
        &self,
        resource_kind: &str,
        external_id: &str,
    ) -> Result<ResolvedResource, ConnectionError> {
        if resource_kind != "repository" {
            return Err(ConnectionError::Invalid(format!(
                "github resource_kind must be 'repository', got '{resource_kind}'"
            )));
        }
        Ok(ResolvedResource {
            provider: ProviderKind::Github,
            resource_kind: resource_kind.to_string(),
            external_id: external_id.to_string(),
        })
    }
}

/// Slack app provider (webhook or socket mode).
pub struct SlackProvider;

impl ConnectionProvider for SlackProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Slack
    }

    fn required_credential_keys(&self) -> &'static [&'static str] {
        &["bot_token", "signing_secret"]
    }

    fn resolve_resource(
        &self,
        resource_kind: &str,
        external_id: &str,
    ) -> Result<ResolvedResource, ConnectionError> {
        if !matches!(resource_kind, "workspace" | "channel") {
            return Err(ConnectionError::Invalid(format!(
                "slack resource_kind must be 'workspace' or 'channel', got '{resource_kind}'"
            )));
        }
        Ok(ResolvedResource {
            provider: ProviderKind::Slack,
            resource_kind: resource_kind.to_string(),
            external_id: external_id.to_string(),
        })
    }
}

/// Discord bot provider.
pub struct DiscordProvider;

impl ConnectionProvider for DiscordProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Discord
    }

    fn required_credential_keys(&self) -> &'static [&'static str] {
        &["bot_token"]
    }

    fn resolve_resource(
        &self,
        resource_kind: &str,
        external_id: &str,
    ) -> Result<ResolvedResource, ConnectionError> {
        if !matches!(resource_kind, "guild" | "channel") {
            return Err(ConnectionError::Invalid(format!(
                "discord resource_kind must be 'guild' or 'channel', got '{resource_kind}'"
            )));
        }
        Ok(ResolvedResource {
            provider: ProviderKind::Discord,
            resource_kind: resource_kind.to_string(),
            external_id: external_id.to_string(),
        })
    }
}

/// Parse a secret-store value: a JSON object of credential fields.
pub fn parse_credential_material(raw: &str) -> Result<CredentialMaterial, ConnectionError> {
    let values: BTreeMap<String, String> = serde_json::from_str(raw).map_err(|e| {
        ConnectionError::CredentialInvalid(format!("credentials must be JSON: {e}"))
    })?;
    Ok(CredentialMaterial { values })
}
