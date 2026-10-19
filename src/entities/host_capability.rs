//! `HostCapability` entity — what an enrolled host can run (task 1.2.4).
//!
//! A capability is one provider/model/execution-mode/thinking-option variant.
//! `options` is the provider-native option object supplied by the host; the
//! pool treats it as opaque. Re-reporting replaces the whole set.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// One reported capability variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentCapability {
    /// Provider id (for example `anthropic`, `openai`).
    pub provider: String,
    /// Provider-native model id.
    pub model: String,
    /// Execution mode (for example `default`, `plan`).
    pub execution_mode: String,
    /// Thinking option, when the mode offers one.
    #[serde(default)]
    pub thinking_option: Option<String>,
    /// Provider-native options this variant accepts.
    #[serde(default)]
    pub options: Option<serde_json::Value>,
}

/// Full capability row, mirroring `host_capabilities`.
#[derive(Debug, Clone, FromRow)]
pub struct HostCapability {
    /// Internal primary key.
    pub id: u64,
    /// Internal host id.
    pub host_id: u64,
    /// Provider id.
    pub provider: String,
    /// Model id.
    pub model: String,
    /// Execution mode.
    pub execution_mode: String,
    /// Thinking option.
    pub thinking_option: Option<String>,
    /// Provider-native options as stored JSON.
    pub options: Option<String>,
    /// When the host last reported this row.
    pub reported_at: DateTime<Utc>,
}

impl From<&HostCapability> for AgentCapability {
    fn from(row: &HostCapability) -> Self {
        Self {
            provider: row.provider.clone(),
            model: row.model.clone(),
            execution_mode: row.execution_mode.clone(),
            thinking_option: row.thinking_option.clone(),
            options: row
                .options
                .as_deref()
                .and_then(|raw| serde_json::from_str(raw).ok()),
        }
    }
}

/// A host's capability report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapabilityReport {
    /// Public id of the reporting host.
    pub host_id: String,
    /// When the report was stored; `None` when the host never reported.
    pub reported_at: Option<DateTime<Utc>>,
    /// Reported variants.
    pub capabilities: Vec<AgentCapability>,
    /// Providers the pool may offer for pooled execution, derived from the
    /// report and the grantable allow-list.
    pub offered_providers: Vec<String>,
}

/// Agent selection a trigger asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSelection {
    /// Provider id.
    pub provider: String,
    /// Model id.
    pub model: String,
    /// Execution mode.
    pub execution_mode: String,
    /// Thinking option.
    #[serde(default)]
    pub thinking_option: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_round_trips_options() {
        let capability = AgentCapability {
            provider: "anthropic".into(),
            model: "claude".into(),
            execution_mode: "default".into(),
            thinking_option: Some("high".into()),
            options: Some(serde_json::json!({"max_tokens": 8192})),
        };
        let json = serde_json::to_value(&capability).expect("serialize");
        assert_eq!(json["provider"], "anthropic");
        assert_eq!(json["thinking_option"], "high");
        assert_eq!(json["options"]["max_tokens"], 8192);
    }
}
