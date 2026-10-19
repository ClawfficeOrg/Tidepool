//! `HostExecution` entity — a create-agent intent tracked by the pool
//! (task 1.2.5).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Execution lifecycle reported by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// Intent stored, not yet acknowledged by the host.
    Queued,
    /// Host acknowledged and is running.
    Running,
    /// Completed successfully.
    Succeeded,
    /// Completed with a failure.
    Failed,
    /// Interrupted by the pool or the host.
    Interrupted,
}

use sqlx::TypeInfo as _;

impl sqlx::Type<sqlx::MySql> for ExecutionStatus {
    fn type_info() -> sqlx::mysql::MySqlTypeInfo {
        <String as sqlx::Type<sqlx::MySql>>::type_info()
    }
    fn compatible(ty: &sqlx::mysql::MySqlTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::MySql>>::compatible(ty)
            || ty.name().eq_ignore_ascii_case("ENUM")
    }
}

impl<'r> sqlx::Decode<'r, sqlx::MySql> for ExecutionStatus {
    fn decode(
        value: sqlx::mysql::MySqlValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = <String as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?;
        match s.as_str() {
            "queued" => Ok(ExecutionStatus::Queued),
            "running" => Ok(ExecutionStatus::Running),
            "succeeded" => Ok(ExecutionStatus::Succeeded),
            "failed" => Ok(ExecutionStatus::Failed),
            "interrupted" => Ok(ExecutionStatus::Interrupted),
            other => Err(format!("unknown execution status: {other:?}").into()),
        }
    }
}

/// Full execution row.
#[derive(Debug, Clone, FromRow)]
pub struct HostExecution {
    /// Internal primary key.
    pub id: u64,
    /// Stable pool-supplied execution id (idempotency key).
    pub public_id: String,
    /// Internal host id.
    pub host_id: u64,
    /// Prompt text sent to the host.
    pub prompt: String,
    /// Lifecycle status.
    pub status: ExecutionStatus,
    /// Observed output.
    pub output: Option<String>,
    /// When created.
    pub created_at: DateTime<Utc>,
    /// When last changed.
    pub updated_at: DateTime<Utc>,
    /// When completed.
    pub completed_at: Option<DateTime<Utc>>,
}

/// Wire projection of an execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostExecutionView {
    /// Stable execution id.
    pub id: String,
    /// Lifecycle status.
    pub status: ExecutionStatus,
    /// Observed output.
    pub output: Option<String>,
    /// When completed.
    pub completed_at: Option<DateTime<Utc>>,
}

impl From<&HostExecution> for HostExecutionView {
    fn from(execution: &HostExecution) -> Self {
        Self {
            id: execution.public_id.clone(),
            status: execution.status,
            output: execution.output.clone(),
            completed_at: execution.completed_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_uses_snake_case() {
        assert_eq!(
            serde_json::to_string(&ExecutionStatus::Succeeded).unwrap(),
            "\"succeeded\""
        );
        assert_eq!(
            serde_json::to_string(&ExecutionStatus::Interrupted).unwrap(),
            "\"interrupted\""
        );
    }
}
