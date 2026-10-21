//! `Environment` entity — where a workflow step runs (task 1.4.5).
//!
//! An environment is a **complete named object** with three parts:
//!
//! - an enrolled **host**, named by its mutable friendly slug and resolved to
//!   the host at activation;
//! - a **working directory** on that machine. The pool never clones, creates or
//!   rewrites it: the directory must already exist on the host;
//! - an optional **worktree**: a mode, a new branch and a base ref. The new
//!   branch embeds the execution id (`${{ paseo.execution.id }}`), so every
//!   execution gets its own branch and keeps it across a retry or recovery.
//!
//! Environments are never merged, inherited or partially overridden. The
//! document uses `deny_unknown_fields`, so an `extends`-style key is a parse
//! error rather than a silent overlay, and a step selects exactly one complete
//! environment by name or through a finite input whose every choice names a
//! complete environment ([`EnvironmentSelector`]).
//!
//! This module is pure: parsing, template rendering and reference selection.
//! Database resolution and activation issues live in
//! [`crate::services::environments`].

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;

/// The only expression a worktree branch template may embed.
pub const EXECUTION_ID_EXPRESSION: &str = "${{ paseo.execution.id }}";
/// Tidepool-spelled synonym for [`EXECUTION_ID_EXPRESSION`].
pub const EXECUTION_ID_EXPRESSION_TIDEPOOL: &str = "${{ tidepool.execution.id }}";
/// Maximum number of characters in an environment name.
pub const MAX_NAME_CHARS: usize = 255;
/// Maximum number of characters in a working directory.
pub const MAX_DIRECTORY_CHARS: usize = 4096;
/// Maximum number of characters in a rendered branch name or base ref.
pub const MAX_REF_CHARS: usize = 255;
/// Maximum number of characters in an execution id.
pub const MAX_EXECUTION_ID_CHARS: usize = 64;

/// A complete named environment.
///
/// Every field is optional at parse time so activation can name the missing
/// one (for example `environments.dev.directory`) instead of rejecting the
/// whole bundle as malformed JSON. `deny_unknown_fields` is what makes the
/// object complete: there is no key an overlay could ride in on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Environment {
    /// Environment name a step selects.
    #[serde(default)]
    pub name: String,
    /// Enrolled host slug the step runs on.
    #[serde(default, alias = "daemon")]
    pub host: String,
    /// Absolute working directory on the host; it must already exist.
    #[serde(default, alias = "cwd")]
    pub directory: String,
    /// Optional isolation for the execution.
    #[serde(default)]
    pub worktree: Option<Worktree>,
}

/// The worktree an environment asks the host to use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Worktree {
    /// What the host should do before starting the agent.
    pub mode: WorktreeMode,
    /// Branch template for `branch-off`; embeds the execution id.
    #[serde(default, rename = "newBranch")]
    pub new_branch: Option<String>,
    /// Optional base ref for `branch-off` (default: the host's current HEAD).
    #[serde(default)]
    pub base: Option<String>,
    /// Literal branch for `checkout-branch`.
    #[serde(default)]
    pub branch: Option<String>,
}

/// Worktree modes the pool understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorktreeMode {
    /// Branch off `base` onto a per-execution branch rendered from `newBranch`.
    BranchOff,
    /// Check out an existing literal branch; the pool never clones.
    CheckoutBranch,
}

impl WorktreeMode {
    /// Wire spelling used in authored configuration.
    pub fn as_str(self) -> &'static str {
        match self {
            WorktreeMode::BranchOff => "branch-off",
            WorktreeMode::CheckoutBranch => "checkout-branch",
        }
    }
}

impl Environment {
    /// The branch this execution uses, or `None` when the environment runs in
    /// its working directory unchanged.
    ///
    /// `branch-off` renders the execution id into `newBranch`; `checkout-branch`
    /// returns its literal branch. The pool never clones a repository.
    pub fn branch_for(&self, execution_id: &str) -> Result<Option<String>, WorktreeError> {
        match &self.worktree {
            None => Ok(None),
            Some(worktree) => worktree.branch_for(execution_id).map(Some),
        }
    }
}

impl Worktree {
    /// Render the branch for one execution.
    pub fn branch_for(&self, execution_id: &str) -> Result<String, WorktreeError> {
        match self.mode {
            WorktreeMode::BranchOff => {
                let template = self
                    .new_branch
                    .as_deref()
                    .ok_or(WorktreeError::BranchRequired)?;
                render_branch_template(template, execution_id)
            }
            WorktreeMode::CheckoutBranch => {
                let branch = self
                    .branch
                    .as_deref()
                    .ok_or(WorktreeError::BranchRequired)?;
                if branch.contains("${{") {
                    return Err(WorktreeError::UnsupportedExpression(branch.to_string()));
                }
                validate_branch_name(branch)?;
                Ok(branch.to_string())
            }
        }
    }
}

/// A complete named environment chosen by a step.
///
/// A literal is one name. A finite input is an input whose declared choices are
/// all environment names; every choice must resolve at activation, and the
/// chosen value selects one complete environment. There is deliberately no
/// variant that layers, patches or merges environments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EnvironmentSelector {
    /// Exactly one named environment.
    Named(String),
    /// A finite input chooses one of the declared environment names.
    FiniteInput(FiniteEnvironmentInput),
}

/// A finite input selector: the input name plus every allowed environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FiniteEnvironmentInput {
    /// Input whose value selects the environment.
    pub input: String,
    /// Complete environment names the input may take.
    pub choices: Vec<String>,
}

/// Errors selecting or rendering an environment.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SelectorError {
    /// No environment with that name.
    #[error("environment '{0}' is not defined")]
    UnknownEnvironment(String),
    /// A finite selector's input has no value.
    #[error("input '{0}' is required to select an environment")]
    InputRequired(String),
    /// The input value is outside the declared choices.
    #[error("input '{input}' value '{value}' is not one of the declared choices")]
    ChoiceNotAllowed {
        /// Input name.
        input: String,
        /// Supplied value.
        value: String,
    },
    /// A finite selector declared no choices.
    #[error("environment selector must declare at least one choice")]
    NoChoices,
    /// A finite selector declared no input name.
    #[error("environment selector input name is required")]
    InputNameRequired,
}

impl EnvironmentSelector {
    /// Resolve the selector to exactly one complete environment.
    ///
    /// `inputs` holds the resolved finite input values for the run. Resolution
    /// returns a borrow of the whole environment, never a merged view.
    pub fn resolve<'a>(
        &self,
        environments: &'a [Environment],
        inputs: &BTreeMap<String, String>,
    ) -> Result<&'a Environment, SelectorError> {
        let name = match self {
            EnvironmentSelector::Named(name) => name.as_str(),
            EnvironmentSelector::FiniteInput(spec) => {
                if spec.input.trim().is_empty() {
                    return Err(SelectorError::InputNameRequired);
                }
                if spec.choices.is_empty() {
                    return Err(SelectorError::NoChoices);
                }
                let value = inputs
                    .get(&spec.input)
                    .ok_or_else(|| SelectorError::InputRequired(spec.input.clone()))?;
                if !spec.choices.iter().any(|choice| choice == value) {
                    return Err(SelectorError::ChoiceNotAllowed {
                        input: spec.input.clone(),
                        value: value.clone(),
                    });
                }
                value.as_str()
            }
        };
        environments
            .iter()
            .find(|environment| environment.name == name)
            .ok_or_else(|| SelectorError::UnknownEnvironment(name.to_string()))
    }
}

/// Errors rendering or validating a worktree branch.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorktreeError {
    /// The mode's branch field is absent.
    #[error("worktree branch is required")]
    BranchRequired,
    /// The template does not embed the execution id.
    #[error(
        "worktree branch must embed {EXECUTION_ID_EXPRESSION} so every execution gets its own branch"
    )]
    MissingExecutionId,
    /// The template contains an expression other than the execution id.
    #[error("unsupported template expression '{0}' in worktree branch")]
    UnsupportedExpression(String),
    /// The template opened `${{` without closing it.
    #[error("unclosed template expression in worktree branch")]
    UnclosedExpression,
    /// The execution id is not safe to place in a Git ref.
    #[error("execution id must be 1-{0} characters of [A-Za-z0-9._-] and start alphanumeric")]
    InvalidExecutionId(usize),
    /// The rendered branch is not a valid Git ref.
    #[error("invalid branch name '{0}': {1}")]
    InvalidBranch(String, &'static str),
    /// The base ref is not a valid Git ref.
    #[error("invalid base ref '{0}': {1}")]
    InvalidBaseRef(String, &'static str),
}

/// Errors validating a working directory.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DirectoryError {
    /// The environment declared no directory.
    #[error("working directory is required; the pool never creates it")]
    Missing,
    /// A relative path cannot be resolved on a host.
    #[error("working directory must be an absolute path on the host: '{0}'")]
    NotAbsolute(String),
    /// The path exceeds [`MAX_DIRECTORY_CHARS`].
    #[error("working directory must be at most {0} characters")]
    TooLong(usize),
    /// The path is not canonical.
    #[error("working directory '{0}' is not a canonical path ({1})")]
    NotCanonical(String, &'static str),
}

/// Validate a working directory declaration.
///
/// This validates shape only: the pool never touches the host's filesystem, so
/// existence is the host's fact. It rejects an absent, relative, over-long or
/// non-canonical path (`..`, `.`, repeated or trailing separators) so a broken
/// declaration fails activation instead of reaching dispatch.
pub fn validate_directory(path: &str) -> Result<(), DirectoryError> {
    if path.trim().is_empty() {
        return Err(DirectoryError::Missing);
    }
    if path.chars().count() > MAX_DIRECTORY_CHARS {
        return Err(DirectoryError::TooLong(MAX_DIRECTORY_CHARS));
    }
    if !is_absolute_directory(path) {
        return Err(DirectoryError::NotAbsolute(path.to_string()));
    }
    if path.chars().any(char::is_control) {
        return Err(DirectoryError::NotCanonical(
            path.to_string(),
            "contains a control character",
        ));
    }
    let windows_style = path.as_bytes().get(1) == Some(&b':');
    let body = if windows_style { &path[2..] } else { path };
    let normalized = body.replace('\\', "/");
    if normalized == "/" {
        return Ok(());
    }
    let parts: Vec<&str> = normalized.split('/').collect();
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            if index == 0 {
                continue;
            }
            return Err(DirectoryError::NotCanonical(
                path.to_string(),
                if index + 1 == parts.len() {
                    "ends with a separator"
                } else {
                    "contains a repeated separator"
                },
            ));
        }
        if *part == "." {
            return Err(DirectoryError::NotCanonical(
                path.to_string(),
                "contains a '.' component",
            ));
        }
        if *part == ".." {
            return Err(DirectoryError::NotCanonical(
                path.to_string(),
                "contains a '..' component",
            ));
        }
    }
    Ok(())
}

/// Whether a path is absolute, in POSIX or Windows form.
pub fn is_absolute_directory(path: &str) -> bool {
    if path.starts_with('/') {
        return true;
    }
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}

/// Render a `branch-off` template for one execution.
///
/// The template must embed [`EXECUTION_ID_EXPRESSION`] (or its Tidepool
/// spelling) at least once; every other `${{ ... }}` expression is refused, and
/// the rendered result must be a valid Git ref.
pub fn render_branch_template(template: &str, execution_id: &str) -> Result<String, WorktreeError> {
    if template.trim().is_empty() {
        return Err(WorktreeError::BranchRequired);
    }
    validate_execution_id(execution_id)?;
    let spans = expression_spans(template)?;
    if spans.is_empty() {
        return Err(WorktreeError::MissingExecutionId);
    }

    let mut rendered = String::with_capacity(template.len() + execution_id.len());
    let mut cursor = 0;
    for (start, end) in spans {
        rendered.push_str(&template[cursor..start]);
        rendered.push_str(execution_id);
        cursor = end;
    }
    rendered.push_str(&template[cursor..]);

    validate_branch_name(&rendered)?;
    Ok(rendered)
}

/// Validate an execution id before it is placed in a ref.
pub fn validate_execution_id(execution_id: &str) -> Result<(), WorktreeError> {
    let length = execution_id.chars().count();
    let Some(first) = execution_id.chars().next() else {
        return Err(WorktreeError::InvalidExecutionId(MAX_EXECUTION_ID_CHARS));
    };
    if length > MAX_EXECUTION_ID_CHARS
        || !first.is_ascii_alphanumeric()
        || !execution_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(WorktreeError::InvalidExecutionId(MAX_EXECUTION_ID_CHARS));
    }
    Ok(())
}

/// Validate a rendered branch name.
pub fn validate_branch_name(branch: &str) -> Result<(), WorktreeError> {
    ref_problem(branch).map_or(Ok(()), |reason| {
        Err(WorktreeError::InvalidBranch(branch.to_string(), reason))
    })
}

/// Validate a `base` ref.
pub fn validate_base_ref(base: &str) -> Result<(), WorktreeError> {
    ref_problem(base).map_or(Ok(()), |reason| {
        Err(WorktreeError::InvalidBaseRef(base.to_string(), reason))
    })
}

/// Find every execution-id expression span in the template.
///
/// Returns byte ranges covering `${{ ... }}` delimiters. Any expression whose
/// name is not the execution id is refused.
fn expression_spans(template: &str) -> Result<Vec<(usize, usize)>, WorktreeError> {
    let mut spans = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = template[cursor..].find("${{") {
        let start = cursor + relative;
        let inner_start = start + 3;
        let Some(close) = template[inner_start..].find("}}") else {
            return Err(WorktreeError::UnclosedExpression);
        };
        let inner_end = inner_start + close;
        let inner = template[inner_start..inner_end].trim();
        if inner != "paseo.execution.id" && inner != "tidepool.execution.id" {
            return Err(WorktreeError::UnsupportedExpression(inner.to_string()));
        }
        spans.push((start, inner_end + 2));
        cursor = inner_end + 2;
    }
    Ok(spans)
}

/// Shared Git ref shape rules; returns the failing reason when invalid.
fn ref_problem(value: &str) -> Option<&'static str> {
    if value.is_empty() {
        Some("must not be empty")
    } else if value.chars().count() > MAX_REF_CHARS {
        Some("is too long")
    } else if value.starts_with('-') {
        Some("must not start with '-'")
    } else if value.starts_with('/') {
        Some("must not start with '/'")
    } else if value.ends_with('/') {
        Some("must not end with '/'")
    } else if value.ends_with('.') {
        Some("must not end with '.'")
    } else if value.ends_with(".lock") {
        Some("must not end with '.lock'")
    } else if value.contains("..") {
        Some("must not contain '..'")
    } else if value.contains("@{") {
        Some("must not contain '@{'")
    } else if value.contains("//") {
        Some("must not contain '//'")
    } else if value
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        Some("must not contain an empty or '.' component")
    } else if value
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        Some("must not contain whitespace or control characters")
    } else if value
        .chars()
        .any(|character| matches!(character, '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
    {
        Some("must not contain '~', '^', ':', '?', '*', '[' or '\\'")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branch_off() -> Worktree {
        Worktree {
            mode: WorktreeMode::BranchOff,
            new_branch: Some("trigger-${{ paseo.execution.id }}".into()),
            base: Some("origin/main".into()),
            branch: None,
        }
    }

    #[test]
    fn branch_off_renders_the_execution_id() {
        let worktree = branch_off();
        let branch = worktree.branch_for("exec-123").expect("render");
        assert_eq!(branch, "trigger-exec-123");
        let other = worktree.branch_for("exec-456").expect("render");
        assert_ne!(branch, other, "each execution gets its own branch");
    }

    #[test]
    fn the_tidepool_spelling_also_renders() {
        let worktree = Worktree {
            mode: WorktreeMode::BranchOff,
            new_branch: Some("tp/${{ tidepool.execution.id }}".into()),
            base: None,
            branch: None,
        };
        assert_eq!(
            worktree.branch_for("abc").expect("render"),
            "tp/abc",
            "both spellings name the same expression"
        );
    }

    #[test]
    fn a_template_without_the_execution_id_is_refused() {
        assert_eq!(
            render_branch_template("trigger", "exec-1"),
            Err(WorktreeError::MissingExecutionId)
        );
    }

    #[test]
    fn other_expressions_are_refused() {
        assert_eq!(
            render_branch_template("trigger-${{ paseo.prompt }}", "exec-1"),
            Err(WorktreeError::UnsupportedExpression("paseo.prompt".into()))
        );
        assert_eq!(
            render_branch_template("trigger-${{ paseo.execution.id", "exec-1"),
            Err(WorktreeError::UnclosedExpression)
        );
    }

    #[test]
    fn an_unsafe_execution_id_is_refused() {
        let too_long = "a".repeat(MAX_EXECUTION_ID_CHARS + 1);
        for bad in [
            "",
            "../evil",
            "exec/1",
            "-exec",
            ".exec",
            "a b",
            too_long.as_str(),
        ] {
            assert!(
                validate_execution_id(bad).is_err(),
                "'{bad}' must not be trusted in a ref"
            );
        }
        assert!(validate_execution_id("exec-1_2.3").is_ok());
    }

    #[test]
    fn an_invalid_rendered_branch_is_refused() {
        assert!(render_branch_template("bad branch-${{ paseo.execution.id }}", "exec-1").is_err());
        assert!(render_branch_template("bad..branch-${{ paseo.execution.id }}", "exec-1").is_err());
        assert!(render_branch_template("${{ paseo.execution.id }}.lock", "exec-1").is_err());
    }

    #[test]
    fn checkout_branch_is_literal() {
        let worktree = Worktree {
            mode: WorktreeMode::CheckoutBranch,
            new_branch: None,
            base: None,
            branch: Some("release/1.4".into()),
        };
        assert_eq!(
            worktree.branch_for("exec-1").expect("literal"),
            "release/1.4"
        );
    }

    #[test]
    fn checkout_branch_rejects_a_template() {
        let worktree = Worktree {
            mode: WorktreeMode::CheckoutBranch,
            new_branch: None,
            base: None,
            branch: Some("release/${{ paseo.execution.id }}".into()),
        };
        assert!(matches!(
            worktree.branch_for("exec-1"),
            Err(WorktreeError::UnsupportedExpression(_))
        ));
    }

    #[test]
    fn directory_shape_is_validated() {
        assert!(validate_directory("/Users/you/code/repo").is_ok());
        assert!(validate_directory("C:/code/repo").is_ok());
        assert_eq!(validate_directory("   "), Err(DirectoryError::Missing));
        assert!(matches!(
            validate_directory("code/repo"),
            Err(DirectoryError::NotAbsolute(_))
        ));
        assert!(matches!(
            validate_directory("/code/../repo"),
            Err(DirectoryError::NotCanonical(_, "contains a '..' component"))
        ));
        assert!(matches!(
            validate_directory("/code/./repo"),
            Err(DirectoryError::NotCanonical(_, "contains a '.' component"))
        ));
        assert!(matches!(
            validate_directory("/code/repo/"),
            Err(DirectoryError::NotCanonical(_, "ends with a separator"))
        ));
        assert!(matches!(
            validate_directory("/code//repo"),
            Err(DirectoryError::NotCanonical(
                _,
                "contains a repeated separator"
            ))
        ));
    }

    #[test]
    fn unknown_fields_are_not_merged_into_an_environment() {
        let parsed = serde_json::from_str::<Environment>(
            r#"{"name":"dev","host":"mac","directory":"/code","extends":"prod"}"#,
        );
        assert!(
            parsed.is_err(),
            "an environment is complete: 'extends' must not overlay another"
        );
        let parsed = serde_json::from_str::<Environment>(
            r#"{"name":"dev","host":"mac","directory":"/code","worktree":{"mode":"branch-off","newBranch":"x-${{ paseo.execution.id }}","overrides":"prod"}}"#,
        );
        assert!(parsed.is_err(), "a worktree is complete too");
    }

    #[test]
    fn daemon_and_cwd_are_accepted_as_aliases() {
        let parsed =
            serde_json::from_str::<Environment>(r#"{"name":"dev","daemon":"mac","cwd":"/code"}"#)
                .expect("aliases parse");
        assert_eq!(parsed.host, "mac");
        assert_eq!(parsed.directory, "/code");
    }

    #[test]
    fn finite_input_selects_one_complete_environment() {
        let environments = vec![
            Environment {
                name: "dev".into(),
                host: "mac".into(),
                directory: "/code".into(),
                worktree: None,
            },
            Environment {
                name: "clean".into(),
                host: "mac".into(),
                directory: "/code".into(),
                worktree: Some(branch_off()),
            },
        ];
        let selector = EnvironmentSelector::FiniteInput(FiniteEnvironmentInput {
            input: "repo".into(),
            choices: vec!["dev".into(), "clean".into()],
        });
        let mut inputs = BTreeMap::new();
        inputs.insert("repo".to_string(), "clean".to_string());
        let selected = selector.resolve(&environments, &inputs).expect("select");
        assert_eq!(selected.name, "clean");
        assert!(selected.worktree.is_some(), "the chosen object is complete");
        assert_eq!(selected.directory, "/code");
    }

    #[test]
    fn finite_input_rejects_an_undeclared_value() {
        let environments = vec![Environment {
            name: "dev".into(),
            host: "mac".into(),
            directory: "/code".into(),
            worktree: None,
        }];
        let selector = EnvironmentSelector::FiniteInput(FiniteEnvironmentInput {
            input: "repo".into(),
            choices: vec!["dev".into()],
        });
        let mut inputs = BTreeMap::new();
        inputs.insert("repo".to_string(), "prod".to_string());
        assert_eq!(
            selector.resolve(&environments, &inputs),
            Err(SelectorError::ChoiceNotAllowed {
                input: "repo".into(),
                value: "prod".into()
            })
        );
        assert_eq!(
            selector.resolve(&environments, &BTreeMap::new()),
            Err(SelectorError::InputRequired("repo".into()))
        );
    }
}
