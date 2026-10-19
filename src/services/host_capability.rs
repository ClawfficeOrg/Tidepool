//! Host execution permission and capability reports (task 1.2.4).
//!
//! Enrollment and presence do not grant execution. `host.execute` is a separate
//! grant ([`grant_execution`]) defaulting to off; until it is made the host is
//! connected-only. With the grant the host reports a flat list of
//! provider/model/mode/thinking variants.
//!
//! [`validate_agent_for_host`] is the save-time check a trigger uses: an
//! unavailable provider, model, mode or thinking option fails with the field
//! named instead of failing at first dispatch. Only providers in
//! [`GRANTABLE_PROVIDERS`] are offered for pooled execution.

use crate::entities::host::Host;
use crate::entities::host_capability::{
    AgentCapability, AgentSelection, CapabilityReport, HostCapability,
};
use crate::services::api_keys::{self as audit_svc, AuditActor, AuditEntry};
use crate::services::hosts;
use sqlx::MySqlPool;
use thiserror::Error;

/// Providers the pool may offer for pooled execution.
///
/// A host may report anything, but only this set appears in
/// `offered_providers` and may be selected by a trigger.
pub const GRANTABLE_PROVIDERS: &[&str] = &["anthropic", "openai", "google", "openrouter", "local"];

/// Domain errors for host execution/capabilities.
#[derive(Debug, Error)]
pub enum HostCapabilityError {
    /// Host does not exist or is revoked.
    #[error("host not found")]
    NotFound,
    /// `host.execute` has not been granted.
    #[error("host.execute: execution has not been granted for this host")]
    NotGranted,
    /// Invalid report input.
    #[error("invalid input: {0}")]
    Invalid(String),
    /// Selection names a value the host cannot run; field is named.
    #[error("{field}: host cannot run '{value}'")]
    Unavailable {
        /// Field that failed (`provider`, `model`, `execution_mode`,
        /// `thinking_option`).
        field: &'static str,
        /// The rejected value.
        value: String,
    },
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

/// Grant `host.execute`. Execution remains off until this is called.
pub async fn grant_execution(
    pool: &MySqlPool,
    organization_id: u64,
    host_public_id: &str,
    actor: AuditActor,
) -> Result<Host, HostCapabilityError> {
    let result = sqlx::query(
        "UPDATE hosts SET execution_granted = 1 \
         WHERE public_id = ? AND organization_id = ? AND status <> 'revoked'",
    )
    .bind(host_public_id)
    .bind(organization_id)
    .execute(pool)
    .await
    .map_err(HostCapabilityError::Database)?;
    if result.rows_affected() == 0 {
        return Err(HostCapabilityError::NotFound);
    }

    let host = hosts::find_host(pool, organization_id, host_public_id)
        .await
        .map_err(map_hosts_error)?
        .ok_or(HostCapabilityError::NotFound)?;
    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id,
            actor,
            action: "host.execute_grant",
            target_kind: Some("host"),
            target_public_id: Some(&host.public_id),
            detail: serde_json::Value::Null,
        },
    )
    .await;
    Ok(host)
}

/// Revoke `host.execute`. The host remains enrolled and connected, but is no
/// longer an execution target.
pub async fn revoke_execution(
    pool: &MySqlPool,
    organization_id: u64,
    host_public_id: &str,
    actor: AuditActor,
) -> Result<Host, HostCapabilityError> {
    let result = sqlx::query(
        "UPDATE hosts SET execution_granted = 0 \
         WHERE public_id = ? AND organization_id = ? AND status <> 'revoked'",
    )
    .bind(host_public_id)
    .bind(organization_id)
    .execute(pool)
    .await
    .map_err(HostCapabilityError::Database)?;
    if result.rows_affected() == 0 {
        return Err(HostCapabilityError::NotFound);
    }

    let host = hosts::find_host(pool, organization_id, host_public_id)
        .await
        .map_err(map_hosts_error)?
        .ok_or(HostCapabilityError::NotFound)?;
    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id,
            actor,
            action: "host.execute_revoke",
            target_kind: Some("host"),
            target_public_id: Some(&host.public_id),
            detail: serde_json::Value::Null,
        },
    )
    .await;
    Ok(host)
}

/// Replace a host's capability report.
///
/// Requires the execution grant. Empty reports are allowed (the host can run
/// nothing). Providers outside [`GRANTABLE_PROVIDERS`] are stored but never
/// offered for pooled execution.
pub async fn report_capabilities(
    pool: &MySqlPool,
    organization_id: u64,
    host_public_id: &str,
    capabilities: Vec<AgentCapability>,
) -> Result<CapabilityReport, HostCapabilityError> {
    let host = hosts::find_host(pool, organization_id, host_public_id)
        .await
        .map_err(map_hosts_error)?
        .ok_or(HostCapabilityError::NotFound)?;
    if !host.execution_granted {
        return Err(HostCapabilityError::NotGranted);
    }
    for capability in &capabilities {
        validate_capability(capability)?;
    }

    let mut tx = pool.begin().await.map_err(HostCapabilityError::Database)?;
    sqlx::query("DELETE FROM host_capabilities WHERE host_id = ?")
        .bind(host.id)
        .execute(&mut *tx)
        .await
        .map_err(HostCapabilityError::Database)?;

    for capability in &capabilities {
        let options = match &capability.options {
            Some(value) if !value.is_null() => Some(
                serde_json::to_string(value)
                    .map_err(|e| HostCapabilityError::Invalid(format!("options: {e}")))?,
            ),
            _ => None,
        };
        sqlx::query(
            "INSERT INTO host_capabilities \
             (host_id, provider, model, execution_mode, thinking_option, options) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(host.id)
        .bind(&capability.provider)
        .bind(&capability.model)
        .bind(&capability.execution_mode)
        .bind(&capability.thinking_option)
        .bind(options)
        .execute(&mut *tx)
        .await
        .map_err(HostCapabilityError::Database)?;
    }
    tx.commit().await.map_err(HostCapabilityError::Database)?;

    audit_svc::audit(
        pool,
        AuditEntry {
            organization_id,
            actor: AuditActor::System,
            action: "host.capabilities_report",
            target_kind: Some("host"),
            target_public_id: Some(&host.public_id),
            detail: serde_json::json!({ "capabilities": capabilities.len() }),
        },
    )
    .await;

    get_report(pool, organization_id, host_public_id)
        .await?
        .ok_or(HostCapabilityError::NotFound)
}

/// Read the last capability report (empty when the host never reported).
pub async fn get_report(
    pool: &MySqlPool,
    organization_id: u64,
    host_public_id: &str,
) -> Result<Option<CapabilityReport>, HostCapabilityError> {
    let host = hosts::find_host(pool, organization_id, host_public_id)
        .await
        .map_err(map_hosts_error)?
        .ok_or(HostCapabilityError::NotFound)?;

    let rows = sqlx::query_as::<_, HostCapability>(
        "SELECT id, host_id, provider, model, execution_mode, thinking_option, options, \
                reported_at \
         FROM host_capabilities WHERE host_id = ? ORDER BY provider, model, execution_mode",
    )
    .bind(host.id)
    .fetch_all(pool)
    .await
    .map_err(HostCapabilityError::Database)?;

    let capabilities: Vec<AgentCapability> = rows.iter().map(AgentCapability::from).collect();
    let offered_providers = offered_providers(&capabilities);
    let reported_at = rows.iter().map(|row| row.reported_at).max();
    Ok(Some(CapabilityReport {
        host_id: host.public_id,
        reported_at,
        capabilities,
        offered_providers,
    }))
}

/// Validate a trigger's agent selection against the host's report.
///
/// Fails at save time, naming the first unavailable field.
pub async fn validate_agent_for_host(
    pool: &MySqlPool,
    organization_id: u64,
    host_public_id: &str,
    selection: &AgentSelection,
) -> Result<(), HostCapabilityError> {
    let host = hosts::find_host(pool, organization_id, host_public_id)
        .await
        .map_err(map_hosts_error)?
        .ok_or(HostCapabilityError::NotFound)?;
    if !host.execution_granted {
        return Err(HostCapabilityError::NotGranted);
    }
    let report = get_report(pool, organization_id, host_public_id)
        .await?
        .ok_or(HostCapabilityError::NotFound)?;
    validate_selection(&report.capabilities, selection)
}

/// Pure selection check, used by save-time validation and unit tests.
pub fn validate_selection(
    capabilities: &[AgentCapability],
    selection: &AgentSelection,
) -> Result<(), HostCapabilityError> {
    if !GRANTABLE_PROVIDERS.contains(&selection.provider.as_str()) {
        return Err(HostCapabilityError::Unavailable {
            field: "provider",
            value: selection.provider.clone(),
        });
    }

    let provider_rows: Vec<&AgentCapability> = capabilities
        .iter()
        .filter(|capability| capability.provider == selection.provider)
        .collect();
    if provider_rows.is_empty() {
        return Err(HostCapabilityError::Unavailable {
            field: "provider",
            value: selection.provider.clone(),
        });
    }

    let model_rows: Vec<&AgentCapability> = provider_rows
        .into_iter()
        .filter(|capability| capability.model == selection.model)
        .collect();
    if model_rows.is_empty() {
        return Err(HostCapabilityError::Unavailable {
            field: "model",
            value: selection.model.clone(),
        });
    }

    let mode_rows: Vec<&AgentCapability> = model_rows
        .into_iter()
        .filter(|capability| capability.execution_mode == selection.execution_mode)
        .collect();
    if mode_rows.is_empty() {
        return Err(HostCapabilityError::Unavailable {
            field: "execution_mode",
            value: selection.execution_mode.clone(),
        });
    }

    if let Some(thinking) = selection.thinking_option.as_deref()
        && !mode_rows
            .iter()
            .any(|capability| capability.thinking_option.as_deref() == Some(thinking))
    {
        return Err(HostCapabilityError::Unavailable {
            field: "thinking_option",
            value: thinking.to_string(),
        });
    }
    Ok(())
}

/// Distinct providers the pool may offer, filtered to the grantable set.
pub fn offered_providers(capabilities: &[AgentCapability]) -> Vec<String> {
    let mut providers: Vec<String> = capabilities
        .iter()
        .map(|capability| capability.provider.clone())
        .filter(|provider| GRANTABLE_PROVIDERS.contains(&provider.as_str()))
        .collect();
    providers.sort();
    providers.dedup();
    providers
}

fn validate_capability(capability: &AgentCapability) -> Result<(), HostCapabilityError> {
    if capability.provider.trim().is_empty() || capability.provider.len() > 64 {
        return Err(HostCapabilityError::Invalid("provider is required".into()));
    }
    if capability.model.trim().is_empty() || capability.model.len() > 128 {
        return Err(HostCapabilityError::Invalid("model is required".into()));
    }
    if capability.execution_mode.trim().is_empty() || capability.execution_mode.len() > 64 {
        return Err(HostCapabilityError::Invalid(
            "execution_mode is required".into(),
        ));
    }
    if capability
        .thinking_option
        .as_deref()
        .is_some_and(|value| value.len() > 64 || value.trim().is_empty())
    {
        return Err(HostCapabilityError::Invalid(
            "thinking_option is invalid".into(),
        ));
    }
    Ok(())
}

fn map_hosts_error(error: hosts::HostServiceError) -> HostCapabilityError {
    match error {
        hosts::HostServiceError::NotFound => HostCapabilityError::NotFound,
        hosts::HostServiceError::Database(e) => HostCapabilityError::Database(e),
        other => {
            tracing::error!(error = %other, "host capability lookup failed");
            HostCapabilityError::NotFound
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capability(
        provider: &str,
        model: &str,
        mode: &str,
        thinking: Option<&str>,
    ) -> AgentCapability {
        AgentCapability {
            provider: provider.into(),
            model: model.into(),
            execution_mode: mode.into(),
            thinking_option: thinking.map(str::to_string),
            options: None,
        }
    }

    fn selection(
        provider: &str,
        model: &str,
        mode: &str,
        thinking: Option<&str>,
    ) -> AgentSelection {
        AgentSelection {
            provider: provider.into(),
            model: model.into(),
            execution_mode: mode.into(),
            thinking_option: thinking.map(str::to_string),
        }
    }

    #[test]
    fn selection_matches_a_reported_variant() {
        let capabilities = vec![
            capability("anthropic", "claude", "default", None),
            capability("anthropic", "claude", "plan", Some("high")),
        ];
        assert!(
            validate_selection(
                &capabilities,
                &selection("anthropic", "claude", "plan", Some("high"))
            )
            .is_ok()
        );
    }

    #[test]
    fn unavailable_values_name_the_field() {
        let capabilities = vec![capability("anthropic", "claude", "default", None)];

        let wrong_provider =
            validate_selection(&capabilities, &selection("openai", "gpt", "default", None))
                .expect_err("provider");
        assert_eq!(
            wrong_provider.to_string(),
            "provider: host cannot run 'openai'"
        );

        let wrong_model = validate_selection(
            &capabilities,
            &selection("anthropic", "other", "default", None),
        )
        .expect_err("model");
        assert_eq!(wrong_model.to_string(), "model: host cannot run 'other'");

        let wrong_mode = validate_selection(
            &capabilities,
            &selection("anthropic", "claude", "plan", None),
        )
        .expect_err("mode");
        assert_eq!(
            wrong_mode.to_string(),
            "execution_mode: host cannot run 'plan'"
        );

        let wrong_thinking = validate_selection(
            &capabilities,
            &selection("anthropic", "claude", "default", Some("high")),
        )
        .expect_err("thinking");
        assert_eq!(
            wrong_thinking.to_string(),
            "thinking_option: host cannot run 'high'"
        );
    }

    #[test]
    fn only_grantable_providers_are_offered() {
        let capabilities = vec![
            capability("anthropic", "claude", "default", None),
            capability("openai", "gpt", "default", None),
            capability("not-grantable", "x", "default", None),
            capability("anthropic", "claude", "plan", None),
        ];
        assert_eq!(
            offered_providers(&capabilities),
            vec!["anthropic".to_string(), "openai".to_string()]
        );

        let error = validate_selection(
            &capabilities,
            &selection("not-grantable", "x", "default", None),
        )
        .expect_err("not grantable");
        assert_eq!(
            error.to_string(),
            "provider: host cannot run 'not-grantable'"
        );
    }

    #[test]
    fn not_granted_names_the_grant() {
        let error = HostCapabilityError::NotGranted;
        assert_eq!(
            error.to_string(),
            "host.execute: execution has not been granted for this host"
        );
    }
}
