//! `ConfigurationRevision` entity — an immutable authored file set (task 1.4.1).
//!
//! See `migrations/20261021000001_create_projects.up.sql`. A revision records
//! the exact authored files (path + raw content), a per-project `version`, an
//! `active` flag and the validation result. Rows are write-once: there is no
//! update path, and the only field that ever changes is `active`, toggled when
//! a newer revision activates.
//!
//! `validation_errors` is a JSON-encoded array of [`FieldIssue`]; each issue
//! names the field that failed and the revision it belongs to.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Validation outcome of a revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RevisionStatus {
    /// Every reference resolved; the revision may be active.
    Valid,
    /// One or more references failed to resolve.
    Invalid,
}

impl RevisionStatus {
    /// Database/wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RevisionStatus::Valid => "valid",
            RevisionStatus::Invalid => "invalid",
        }
    }
}

use sqlx::TypeInfo as _;

impl sqlx::Type<sqlx::MySql> for RevisionStatus {
    fn type_info() -> sqlx::mysql::MySqlTypeInfo {
        <String as sqlx::Type<sqlx::MySql>>::type_info()
    }
    fn compatible(ty: &sqlx::mysql::MySqlTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::MySql>>::compatible(ty)
            || ty.name().eq_ignore_ascii_case("ENUM")
    }
}

impl<'r> sqlx::Decode<'r, sqlx::MySql> for RevisionStatus {
    fn decode(
        value: sqlx::mysql::MySqlValueRef<'r>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = <String as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?;
        match s.as_str() {
            "valid" => Ok(RevisionStatus::Valid),
            "invalid" => Ok(RevisionStatus::Invalid),
            other => Err(format!("unknown revision status: {other:?}").into()),
        }
    }
}

/// One authored configuration file: an exact path and its verbatim content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFile {
    /// Authored file path.
    pub path: String,
    /// Authored file content, stored exactly.
    pub content: String,
}

/// One named field error produced by reference resolution.
///
/// Both the failing `field` and the `revision` (the version number) it belongs
/// to are named, so a client can point at the exact reference and revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldIssue {
    /// Reference path, e.g. `hosts.studio-mac` or `partials.system.path`.
    pub field: String,
    /// Revision version number the validation belongs to.
    pub revision: u32,
    /// Human-readable explanation.
    pub message: String,
}

/// Full revision row, mirroring `configuration_revisions`.
#[derive(Debug, Clone, FromRow)]
pub struct ConfigurationRevision {
    /// Internal primary key.
    pub id: u64,
    /// Immutable public UUID.
    pub public_id: String,
    /// Internal project id.
    pub project_id: u64,
    /// Per-project, monotonically increasing version number.
    pub version: u32,
    /// Validation outcome.
    pub status: RevisionStatus,
    /// True when this revision is the project's active configuration.
    pub active: bool,
    /// JSON-encoded `Vec<FieldIssue>`; `None` when the revision is valid.
    pub validation_errors: Option<String>,
    /// When the revision was stored.
    pub created_at: DateTime<Utc>,
}

impl ConfigurationRevision {
    /// Decode the stored validation errors.
    pub fn errors(&self) -> Vec<FieldIssue> {
        self.validation_errors
            .as_deref()
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_default()
    }
}

/// One file row of a revision, mirroring `configuration_revision_files`.
#[derive(Debug, Clone, FromRow)]
pub struct ConfigurationRevisionFile {
    /// Internal primary key.
    pub id: u64,
    /// Parent revision.
    pub revision_id: u64,
    /// Authored position (1-based).
    pub file_order: u32,
    /// Authored path.
    pub path: String,
    /// Authored content, verbatim.
    pub content: String,
}

/// A file as projected on the wire: path plus exact content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionFileView {
    /// Authored path.
    pub path: String,
    /// Authored content.
    pub content: String,
}

impl From<&ConfigurationRevisionFile> for RevisionFileView {
    fn from(file: &ConfigurationRevisionFile) -> Self {
        Self {
            path: file.path.clone(),
            content: file.content.clone(),
        }
    }
}

/// Metadata-only projection of a revision; omits file contents and the
/// internal/project ids, and summarizes errors as a count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionMetaView {
    /// Immutable public UUID.
    pub id: String,
    /// Per-project version number.
    pub version: u32,
    /// True when active.
    pub active: bool,
    /// Validation outcome.
    pub status: RevisionStatus,
    /// Number of field issues.
    pub error_count: usize,
    /// When stored.
    pub created_at: DateTime<Utc>,
}

/// Full projection of a revision: metadata, field issues and the exact files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevisionView {
    /// Immutable public UUID.
    pub id: String,
    /// Per-project version number.
    pub version: u32,
    /// True when active.
    pub active: bool,
    /// Validation outcome.
    pub status: RevisionStatus,
    /// Field issues (empty when valid).
    pub errors: Vec<FieldIssue>,
    /// Exact authored files, in authored order.
    pub files: Vec<RevisionFileView>,
    /// When stored.
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ConfigurationRevision {
        ConfigurationRevision {
            id: 1,
            public_id: "22222222-2222-4222-8222-222222222222".into(),
            project_id: 9,
            version: 3,
            status: RevisionStatus::Invalid,
            active: false,
            validation_errors: Some(
                r#"[{"field":"hosts.bad","revision":3,"message":"unknown host slug 'bad'"}]"#
                    .into(),
            ),
            created_at: "2026-10-21T00:00:00Z".parse().expect("ts"),
        }
    }

    #[test]
    fn status_uses_lowercase_wire_spelling() {
        assert_eq!(
            serde_json::to_string(&RevisionStatus::Valid).unwrap(),
            "\"valid\""
        );
        assert_eq!(
            serde_json::to_string(&RevisionStatus::Invalid).unwrap(),
            "\"invalid\""
        );
    }

    #[test]
    fn errors_decode_the_stored_json() {
        let revision = sample();
        let errors = revision.errors();
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].field, "hosts.bad");
        assert_eq!(errors[0].revision, 3);
    }

    #[test]
    fn missing_or_bad_error_json_is_empty() {
        let mut revision = sample();
        revision.validation_errors = None;
        assert!(revision.errors().is_empty());
        revision.validation_errors = Some("not-json".into());
        assert!(revision.errors().is_empty());
    }

    #[test]
    fn field_issue_names_field_and_revision() {
        let issue = FieldIssue {
            field: "partials.system.path".into(),
            revision: 2,
            message: "path 'partials/system.txt' is not in the bundle".into(),
        };
        let json = serde_json::to_string(&issue).expect("serialize");
        assert!(json.contains("\"field\":\"partials.system.path\""));
        assert!(json.contains("\"revision\":2"));
    }
}
