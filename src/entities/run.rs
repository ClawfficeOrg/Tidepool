//! `Run` entity — one workflow run and its ordered steps (task 1.4.6).
//!
//! A run is the durable record of one dispatch: which project and trigger it
//! belongs to, which actor caused it, the stable delivery key used for
//! deduplication, the parsed inputs, the request text and a snapshot of the
//! workflow and environments it evaluates. The snapshot is deliberate: a run
//! keeps evaluating the configuration it was dispatched from even if the
//! project installs a newer revision (task 1.4.7's activity log reads the same
//! record).
//!
//! The pure input parser lives here too. A manual dispatch supplies one input
//! string; `key=value` tokens at its start fill declared inputs and the first
//! token that is not a declared assignment begins the request text:
//!
//! ```text
//! route=bug repo=dev please fix the failing test
//! ^^^^^^^^^^^^^^^^ inputs          ^^^^^^^^^^^^^^^^^^^^ request text
//! ```
//!
//! This module has no database access; [`crate::services::runs`] owns the
//! lifecycle and [`crate::services::dispatch`] owns creation.

use crate::entities::workflow::WorkflowInput;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::TypeInfo as _;
use std::collections::BTreeMap;
use thiserror::Error;

/// Maximum number of characters in a delivery key.
pub const MAX_DELIVERY_KEY_CHARS: usize = 255;
/// Maximum number of characters in a manual dispatch actor.
pub const MAX_ACTOR_CHARS: usize = 255;
/// Maximum number of bytes in a manual dispatch input string.
pub const MAX_INPUT_BYTES: usize = 1_000_000;

/// How a run was started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOrigin {
    /// A provider event matched an installed trigger.
    Event,
    /// A person or key asked for the run explicitly.
    Manual,
}

impl RunOrigin {
    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RunOrigin::Event => "event",
            RunOrigin::Manual => "manual",
        }
    }
}

/// Lifecycle of one run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Created, no step started yet.
    Pending,
    /// At least one step is running.
    Running,
    /// Every step that ran succeeded.
    Succeeded,
    /// A step or the workflow deadline failed the run.
    Failed,
    /// A step was interrupted, or a host reported a terminal error that must
    /// never be silently restarted.
    Interrupted,
}

impl RunStatus {
    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Pending => "pending",
            RunStatus::Running => "running",
            RunStatus::Succeeded => "succeeded",
            RunStatus::Failed => "failed",
            RunStatus::Interrupted => "interrupted",
        }
    }

    /// True when the run can no longer change.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunStatus::Succeeded | RunStatus::Failed | RunStatus::Interrupted
        )
    }
}

/// Lifecycle of one workflow step inside a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    /// Waiting for its turn.
    Pending,
    /// Create intent sent; waiting for the host.
    Running,
    /// Completed successfully.
    Succeeded,
    /// Completed with an error, or missed a deadline.
    Failed,
    /// Its condition was false; the run continues with the next step.
    Skipped,
    /// Interrupted by the host or the pool; never restarted silently.
    Interrupted,
}

impl StepStatus {
    /// Wire/database spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            StepStatus::Pending => "pending",
            StepStatus::Running => "running",
            StepStatus::Succeeded => "succeeded",
            StepStatus::Failed => "failed",
            StepStatus::Skipped => "skipped",
            StepStatus::Interrupted => "interrupted",
        }
    }

    /// True when the step can no longer change.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            StepStatus::Succeeded
                | StepStatus::Failed
                | StepStatus::Skipped
                | StepStatus::Interrupted
        )
    }
}

/// Decode a status enum from the string stored in MySQL.
macro_rules! sqlx_status {
    ($ty:ty, { $($text:literal => $variant:expr),+ $(,)? }) => {
        impl sqlx::Type<sqlx::MySql> for $ty {
            fn type_info() -> sqlx::mysql::MySqlTypeInfo {
                <String as sqlx::Type<sqlx::MySql>>::type_info()
            }

            fn compatible(ty: &sqlx::mysql::MySqlTypeInfo) -> bool {
                <String as sqlx::Type<sqlx::MySql>>::compatible(ty)
                    || ty.name().eq_ignore_ascii_case("ENUM")
            }
        }

        impl<'r> sqlx::Decode<'r, sqlx::MySql> for $ty {
            fn decode(
                value: sqlx::mysql::MySqlValueRef<'r>,
            ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
                let stored = <String as sqlx::Decode<'r, sqlx::MySql>>::decode(value)?;
                match stored.as_str() {
                    $($text => Ok($variant),)+
                    other => Err(format!("unknown {}: {other:?}", stringify!($ty)).into()),
                }
            }
        }
    };
}

sqlx_status!(RunOrigin, {
    "event" => RunOrigin::Event,
    "manual" => RunOrigin::Manual,
});

sqlx_status!(RunStatus, {
    "pending" => RunStatus::Pending,
    "running" => RunStatus::Running,
    "succeeded" => RunStatus::Succeeded,
    "failed" => RunStatus::Failed,
    "interrupted" => RunStatus::Interrupted,
});

sqlx_status!(StepStatus, {
    "pending" => StepStatus::Pending,
    "running" => StepStatus::Running,
    "succeeded" => StepStatus::Succeeded,
    "failed" => StepStatus::Failed,
    "skipped" => StepStatus::Skipped,
    "interrupted" => StepStatus::Interrupted,
});

/// Full run row, mirroring `runs`.
#[derive(Debug, Clone, FromRow)]
pub struct RunRow {
    /// Internal primary key.
    pub id: u64,
    /// Immutable public id.
    pub public_id: String,
    /// Internal organization id.
    pub organization_id: u64,
    /// Internal project id.
    pub project_id: u64,
    /// Project public id, kept for the run view.
    pub project_public_id: String,
    /// Internal trigger id; the dedup key's scope.
    pub trigger_id: u64,
    /// Immutable trigger public id, kept even if the trigger is removed.
    pub trigger_public_id: String,
    /// Trigger name at dispatch time.
    pub trigger_name: String,
    /// Workflow name at dispatch time.
    pub workflow_name: String,
    /// Active configuration revision the run was dispatched from.
    pub revision_id: Option<String>,
    /// How the run was started.
    pub origin: RunOrigin,
    /// Actor recorded for a manual dispatch, or the event actor.
    pub actor: Option<String>,
    /// Stable client/provider delivery key, unique per trigger.
    pub delivery_key: String,
    /// Text the workflow's prompts receive as `request`.
    pub request_text: String,
    /// Parsed inputs, stored as a JSON object.
    pub inputs_json: String,
    /// Workflow snapshot, stored as JSON.
    pub workflow_json: String,
    /// Environment definitions snapshot, stored as JSON.
    pub environments_json: String,
    /// Prompt partial contents snapshot, stored as a JSON object.
    pub partials_json: String,
    /// Lifecycle status.
    pub status: RunStatus,
    /// Wall-clock deadline for the whole run.
    pub deadline_at: Option<DateTime<Utc>>,
    /// When the first step started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the run reached a terminal status.
    pub completed_at: Option<DateTime<Utc>>,
    /// When created.
    pub created_at: DateTime<Utc>,
    /// When last changed.
    pub updated_at: DateTime<Utc>,
}

/// Full run-step row, mirroring `run_steps`.
#[derive(Debug, Clone, FromRow)]
pub struct RunStepRow {
    /// Internal primary key.
    pub id: u64,
    /// Immutable public id.
    pub public_id: String,
    /// Owning run's internal id.
    pub run_id: u64,
    /// Authored step id.
    pub step_id: String,
    /// Authored position, starting at zero.
    pub position: u32,
    /// Lifecycle status.
    pub status: StepStatus,
    /// Condition result, when the step declared one.
    pub condition_met: Option<bool>,
    /// Deterministic execution id used for every create-intent replay.
    pub execution_id: Option<String>,
    /// Selected environment name.
    pub environment: Option<String>,
    /// Host public id the environment resolved to.
    pub host_public_id: Option<String>,
    /// Working directory on that host.
    pub directory: Option<String>,
    /// Rendered worktree branch, when the environment declares one.
    pub branch: Option<String>,
    /// Rendered prompt, stored before the create intent is sent.
    pub prompt: Option<String>,
    /// Final step output.
    pub output: Option<String>,
    /// Failure detail.
    pub error: Option<String>,
    /// Why the step was skipped or the run stopped.
    pub reason: Option<String>,
    /// When the create intent was sent.
    pub started_at: Option<DateTime<Utc>>,
    /// When the step reached a terminal status.
    pub completed_at: Option<DateTime<Utc>>,
    /// When created.
    pub created_at: DateTime<Utc>,
    /// When last changed.
    pub updated_at: DateTime<Utc>,
}

/// Wire projection of one step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStepView {
    /// Immutable step-row id.
    pub id: String,
    /// Authored step id.
    pub step: String,
    /// Authored position.
    pub position: u32,
    /// Lifecycle status.
    pub status: StepStatus,
    /// Condition result, when the step declared one.
    pub condition: Option<bool>,
    /// Stable execution id, once one exists.
    pub execution_id: Option<String>,
    /// Selected environment name.
    pub environment: Option<String>,
    /// Host public id.
    pub host: Option<String>,
    /// Working directory.
    pub directory: Option<String>,
    /// Rendered branch.
    pub branch: Option<String>,
    /// Final output.
    pub output: Option<String>,
    /// Failure detail.
    pub error: Option<String>,
    /// Skip or stop reason.
    pub reason: Option<String>,
    /// When the intent was sent.
    pub started_at: Option<DateTime<Utc>>,
    /// When the step finished.
    pub completed_at: Option<DateTime<Utc>>,
}

impl From<&RunStepRow> for RunStepView {
    fn from(row: &RunStepRow) -> Self {
        Self {
            id: row.public_id.clone(),
            step: row.step_id.clone(),
            position: row.position,
            status: row.status,
            condition: row.condition_met,
            execution_id: row.execution_id.clone(),
            environment: row.environment.clone(),
            host: row.host_public_id.clone(),
            directory: row.directory.clone(),
            branch: row.branch.clone(),
            output: row.output.clone(),
            error: row.error.clone(),
            reason: row.reason.clone(),
            started_at: row.started_at,
            completed_at: row.completed_at,
        }
    }
}

/// Wire projection of a run, including its ordered steps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    /// Immutable run id.
    pub id: String,
    /// Project public id.
    pub project: String,
    /// Trigger public id.
    pub trigger: String,
    /// Trigger name.
    pub trigger_name: String,
    /// Workflow name.
    pub workflow: String,
    /// How the run was started.
    pub origin: RunOrigin,
    /// Recorded actor.
    pub actor: Option<String>,
    /// Stable delivery key.
    pub delivery_key: String,
    /// Lifecycle status.
    pub status: RunStatus,
    /// Request text.
    pub request_text: String,
    /// Parsed inputs.
    pub inputs: BTreeMap<String, String>,
    /// Run deadline.
    pub deadline_at: Option<DateTime<Utc>>,
    /// When the first step started.
    pub started_at: Option<DateTime<Utc>>,
    /// When the run finished.
    pub completed_at: Option<DateTime<Utc>>,
    /// When created.
    pub created_at: DateTime<Utc>,
    /// Ordered steps.
    pub steps: Vec<RunStepView>,
}

/// One manual dispatch body.
///
/// `project`, `trigger`, `actor`, `deliveryKey` and `input` are the documented
/// fields. `revision` is the optional expected active-revision id; a run is
/// refused when the project has moved past it.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ManualDispatchRequest {
    /// Project public id.
    pub project: String,
    /// Installed trigger name.
    pub trigger: String,
    /// Actor that must be allowed by the trigger's `from_users`.
    pub actor: String,
    /// Optional expected active-revision public id.
    #[serde(default, alias = "expected_revision", alias = "expectedRevision")]
    pub revision: Option<String>,
    /// Stable client-supplied deduplication key.
    #[serde(alias = "delivery_key")]
    pub delivery_key: String,
    /// Leading `key=value` tokens plus the request text.
    #[serde(default)]
    pub input: String,
}

impl ManualDispatchRequest {
    /// Validate lengths and required fields before any database access.
    pub fn validate(&self) -> Result<(), InputError> {
        validate_field("project", &self.project, 36)?;
        validate_field("trigger", &self.trigger, MAX_DELIVERY_KEY_CHARS)?;
        validate_field("actor", &self.actor, MAX_ACTOR_CHARS)?;
        validate_field("deliveryKey", &self.delivery_key, MAX_DELIVERY_KEY_CHARS)?;
        if self.delivery_key.chars().any(char::is_whitespace) {
            return Err(InputError::InvalidField {
                field: "deliveryKey",
                message: "must not contain whitespace".into(),
            });
        }
        if self.input.len() > MAX_INPUT_BYTES {
            return Err(InputError::TooLarge {
                field: "input",
                maximum: MAX_INPUT_BYTES,
            });
        }
        if let Some(revision) = self.revision.as_deref() {
            validate_field("revision", revision, 36)?;
        }
        Ok(())
    }
}

fn validate_field(field: &'static str, value: &str, maximum: usize) -> Result<(), InputError> {
    if value.trim().is_empty() {
        return Err(InputError::InvalidField {
            field,
            message: "is required".into(),
        });
    }
    if value.chars().count() > maximum {
        return Err(InputError::TooLarge { field, maximum });
    }
    if value.chars().any(char::is_control) {
        return Err(InputError::InvalidField {
            field,
            message: "must not contain control characters".into(),
        });
    }
    Ok(())
}

/// Result of parsing one manual dispatch input string.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedInput {
    /// Declared inputs filled by leading `key=value` tokens, or by defaults.
    pub inputs: BTreeMap<String, String>,
    /// Everything from the first non-assignment token onwards.
    pub request_text: String,
}

/// Errors parsing or validating manual dispatch input.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum InputError {
    /// A required field is blank or malformed.
    #[error("invalid field '{field}': {message}")]
    InvalidField {
        /// Field name.
        field: &'static str,
        /// Explanation.
        message: String,
    },
    /// A value is longer than its cap.
    #[error("field '{field}' must be at most {maximum} characters")]
    TooLarge {
        /// Field name.
        field: &'static str,
        /// Inclusive maximum.
        maximum: usize,
    },
    /// A declared required input was not supplied.
    #[error("input '{0}' is required")]
    RequiredInput(String),
    /// A supplied value is outside a finite input's choices.
    #[error("input '{input}' value '{value}' is not one of the declared choices")]
    ChoiceNotAllowed {
        /// Input name.
        input: String,
        /// Supplied value.
        value: String,
        /// Allowed values.
        choices: Vec<String>,
    },
}

impl InputError {
    /// Stable problem code for the hub API.
    pub fn code(&self) -> &'static str {
        "validation_failed"
    }
}

/// Parse a manual dispatch input string against the workflow's declared inputs.
///
/// Leading tokens of the form `name=value` fill a declared input exactly once.
/// The first token that is not such an assignment starts the request text, so
/// `please fix it` and `bug=yes please fix it` both keep their whole prose.
/// Defaults are applied to absent inputs before required inputs are checked.
pub fn parse_input_string(
    input: &str,
    declared: &[WorkflowInput],
) -> Result<ParsedInput, InputError> {
    let mut inputs: BTreeMap<String, String> = BTreeMap::new();
    let mut request_start = None;

    for (start, end) in token_ranges(input) {
        let token = &input[start..end];
        let Some((key, value)) = token.split_once('=') else {
            request_start = Some(start);
            break;
        };
        let Some(declared_input) = declared.iter().find(|input| input.name == key) else {
            request_start = Some(start);
            break;
        };
        if inputs.contains_key(key) {
            // A repeat is prose, not a second assignment.
            request_start = Some(start);
            break;
        }
        inputs.insert(declared_input.name.clone(), value.to_string());
    }

    let request_text = match request_start {
        Some(start) => input[start..].trim().to_string(),
        None => String::new(),
    };

    for declared_input in declared {
        let name = declared_input.name.as_str();
        if !inputs.contains_key(name)
            && let Some(default) = &declared_input.default
        {
            inputs.insert(name.to_string(), default.clone());
        }

        match inputs.get(name) {
            None => {
                if declared_input.required {
                    return Err(InputError::RequiredInput(name.to_string()));
                }
            }
            Some(value) => {
                if declared_input.required && value.trim().is_empty() {
                    return Err(InputError::RequiredInput(name.to_string()));
                }
                if !declared_input.choices.is_empty()
                    && !declared_input.choices.iter().any(|choice| choice == value)
                {
                    return Err(InputError::ChoiceNotAllowed {
                        input: name.to_string(),
                        value: value.clone(),
                        choices: declared_input.choices.clone(),
                    });
                }
            }
        }
    }

    Ok(ParsedInput {
        inputs,
        request_text,
    })
}

/// Byte ranges of the whitespace-delimited tokens in `text`.
fn token_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = None;
    for (index, character) in text.char_indices() {
        if character.is_whitespace() {
            if let Some(token_start) = start.take() {
                ranges.push((token_start, index));
            }
        } else if start.is_none() {
            start = Some(index);
        }
    }
    if let Some(token_start) = start {
        ranges.push((token_start, text.len()));
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared() -> Vec<WorkflowInput> {
        vec![
            WorkflowInput {
                name: "route".into(),
                required: false,
                choices: vec!["bug".into(), "docs".into()],
                default: Some("bug".into()),
            },
            WorkflowInput {
                name: "repo".into(),
                required: true,
                choices: Vec::new(),
                default: None,
            },
        ]
    }

    #[test]
    fn leading_assignments_fill_inputs_and_the_rest_is_request_text() {
        let parsed = parse_input_string("route=docs repo=dev please fix the test", &declared())
            .expect("parses");
        assert_eq!(parsed.inputs.get("route").map(String::as_str), Some("docs"));
        assert_eq!(parsed.inputs.get("repo").map(String::as_str), Some("dev"));
        assert_eq!(parsed.request_text, "please fix the test");
    }

    #[test]
    fn parsing_stops_at_the_first_non_assignment_token() {
        let parsed =
            parse_input_string("repo=dev fix route=docs now", &declared()).expect("parses");
        assert_eq!(parsed.inputs.get("route").map(String::as_str), Some("bug"));
        assert_eq!(parsed.inputs.get("repo").map(String::as_str), Some("dev"));
        assert_eq!(parsed.request_text, "fix route=docs now");
    }

    #[test]
    fn an_unknown_key_starts_the_request_text() {
        let parsed =
            parse_input_string("repo=dev priority=high fix it", &declared()).expect("parses");
        assert_eq!(parsed.inputs.get("repo").map(String::as_str), Some("dev"));
        assert_eq!(parsed.request_text, "priority=high fix it");
    }

    #[test]
    fn defaults_apply_and_required_inputs_are_checked() {
        let parsed = parse_input_string("repo=dev fix it", &declared()).expect("parses");
        assert_eq!(parsed.inputs.get("route").map(String::as_str), Some("bug"));
        assert_eq!(parsed.request_text, "fix it");

        let error = parse_input_string("fix it", &declared()).expect_err("repo is required");
        assert_eq!(error, InputError::RequiredInput("repo".into()));
    }

    #[test]
    fn a_value_outside_the_choices_is_refused() {
        let error = parse_input_string("route=ops repo=dev", &declared())
            .expect_err("route must be a declared choice");
        assert!(matches!(error, InputError::ChoiceNotAllowed { .. }));
    }

    #[test]
    fn a_duplicate_assignment_becomes_request_text() {
        let parsed = parse_input_string("repo=dev repo=prod text", &declared()).expect("parses");
        assert_eq!(parsed.inputs.get("repo").map(String::as_str), Some("dev"));
        assert_eq!(parsed.request_text, "repo=prod text");
    }

    #[test]
    fn no_input_string_is_all_prose() {
        let parsed =
            parse_input_string("please fix the test", &declared()).expect_err("repo is required");
        assert_eq!(parsed, InputError::RequiredInput("repo".into()));
    }

    #[test]
    fn blank_required_values_are_refused() {
        let error =
            parse_input_string("repo= fix it", &declared()).expect_err("empty repo is missing");
        assert_eq!(error, InputError::RequiredInput("repo".into()));
    }

    #[test]
    fn manual_request_validation_names_the_field() {
        let request = ManualDispatchRequest {
            project: "project".into(),
            trigger: "trigger".into(),
            actor: "alice".into(),
            revision: None,
            delivery_key: "delivery-1".into(),
            input: "repo=dev fix it".into(),
        };
        request.validate().expect("valid request");

        let mut blank = request.clone();
        blank.actor = "   ".into();
        assert!(matches!(
            blank.validate(),
            Err(InputError::InvalidField { field: "actor", .. })
        ));

        let mut spaced = request.clone();
        spaced.delivery_key = "two words".into();
        assert!(matches!(
            spaced.validate(),
            Err(InputError::InvalidField {
                field: "deliveryKey",
                ..
            })
        ));
    }

    #[test]
    fn statuses_use_snake_case_on_the_wire() {
        assert_eq!(
            serde_json::to_string(&RunStatus::Interrupted).expect("serialize"),
            "\"interrupted\""
        );
        assert_eq!(
            serde_json::to_string(&StepStatus::Skipped).expect("serialize"),
            "\"skipped\""
        );
        assert!(RunStatus::Failed.is_terminal());
        assert!(!RunStatus::Running.is_terminal());
        assert!(StepStatus::Skipped.is_terminal());
    }
}
