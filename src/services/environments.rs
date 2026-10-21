//! Environment resolution (task 1.4.5).
//!
//! An environment names where a step runs: an enrolled [`Host`], a working
//! directory that must already exist on that machine, and an optional worktree.
//! This service turns authored [`Environment`] documents into dispatch targets
//! and reports every unresolved reference at activation.
//!
//! Two rules shape the module:
//!
//! - **Resolution happens at activation.** [`environment_issues`] checks every
//!   environment against the hosts resolved for the revision, so a host that no
//!   longer exists (or a missing directory declaration) fails activation with a
//!   named field instead of failing at dispatch.
//! - **Environments are never merged.** A selector resolves to exactly one
//!   complete [`Environment`]; there is no overlay path, and the entity refuses
//!   unknown keys.
//!
//! At dispatch, [`resolve_dispatch`] is the pure decision (which environment,
//! which host, which rendered branch) and [`require_for_dispatch`] is the thin
//! database wrapper that asks host presence. An offline host yields
//! [`EnvironmentError::DaemonNotConnected`], the documented
//! `daemon_not_connected` code; nothing is queued.

use crate::entities::environment::{
    DirectoryError, Environment, EnvironmentSelector, FiniteEnvironmentInput, MAX_NAME_CHARS,
    SelectorError, Worktree, WorktreeError, WorktreeMode, render_branch_template,
    validate_base_ref, validate_branch_name, validate_directory,
};
use crate::entities::host::{Host, HostStatus};
use crate::services::host_presence::{self, HostPresenceError};
use crate::services::hosts::{self as host_svc, HostServiceError, normalize_slug};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Execution id used only to validate a branch template at activation.
///
/// A UUID is what the pool will really pass at dispatch, so a template that
/// only works for short ids still fails activation.
pub const SAMPLE_EXECUTION_ID: &str = "00000000-0000-4000-8000-000000000000";

/// One named field problem, without the revision (the caller adds it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentIssue {
    /// Reference path, for example `environments.dev.directory`.
    pub field: String,
    /// Human-readable explanation.
    pub message: String,
}

fn issue(field: impl Into<String>, message: impl Into<String>) -> EnvironmentIssue {
    EnvironmentIssue {
        field: field.into(),
        message: message.into(),
    }
}

/// Resolve every environment reference: name, host, directory and worktree.
///
/// Pure and database-free: the caller passes the set of live host slugs already
/// resolved for the revision. Every failure names the authored field under
/// `environments.<name>`.
pub fn environment_issues(
    environments: &[Environment],
    resolved_hosts: &BTreeSet<String>,
) -> Vec<EnvironmentIssue> {
    let mut issues = Vec::new();
    let mut seen_names = BTreeSet::new();

    for (index, environment) in environments.iter().enumerate() {
        let name = environment.name.trim().to_string();
        let prefix = if name.is_empty() {
            format!("environments[{index}]")
        } else {
            format!("environments.{name}")
        };

        if name.is_empty() {
            issues.push(issue(
                format!("{prefix}.name"),
                "environment name is required",
            ));
        } else {
            if name.chars().count() > MAX_NAME_CHARS {
                issues.push(issue(
                    format!("{prefix}.name"),
                    format!("environment name must be at most {MAX_NAME_CHARS} characters"),
                ));
            }
            if name.chars().any(|character| {
                character.is_whitespace() || character.is_control() || character == '/'
            }) {
                issues.push(issue(
                    format!("{prefix}.name"),
                    "environment name must not contain whitespace, control characters or '/'",
                ));
            }
            if !seen_names.insert(name.clone()) {
                issues.push(issue(
                    format!("{prefix}.name"),
                    format!("duplicate environment name '{name}'"),
                ));
            }
        }

        let host_key = normalize_slug(&environment.host);
        if host_key.is_empty() {
            issues.push(issue(format!("{prefix}.host"), "host slug is required"));
        } else if !resolved_hosts.contains(&host_key) {
            issues.push(issue(
                format!("{prefix}.host"),
                format!("unknown host slug '{}'", environment.host),
            ));
        }

        if let Err(error) = validate_directory(&environment.directory) {
            issues.push(issue(format!("{prefix}.directory"), error.to_string()));
        }

        if let Some(worktree) = &environment.worktree {
            issues.extend(worktree_issues(&prefix, worktree));
        }
    }

    issues
}

/// Structural checks for one worktree declaration.
fn worktree_issues(prefix: &str, worktree: &Worktree) -> Vec<EnvironmentIssue> {
    let mut issues = Vec::new();
    let field = |suffix: &str| format!("{prefix}.worktree.{suffix}");

    match worktree.mode {
        WorktreeMode::BranchOff => {
            if worktree.branch.is_some() {
                issues.push(issue(
                    field("branch"),
                    "branch is only valid with mode 'checkout-branch'",
                ));
            }
            match worktree.new_branch.as_deref() {
                None | Some("") => issues.push(issue(
                    field("newBranch"),
                    "newBranch is required for mode 'branch-off' and must embed the execution id",
                )),
                Some(template) => {
                    if let Err(error) = render_branch_template(template, SAMPLE_EXECUTION_ID) {
                        issues.push(issue(field("newBranch"), error.to_string()));
                    }
                }
            }
            if let Some(base) = worktree.base.as_deref()
                && let Err(error) = validate_base_ref(base)
            {
                issues.push(issue(field("base"), error.to_string()));
            }
        }
        WorktreeMode::CheckoutBranch => {
            if worktree.new_branch.is_some() {
                issues.push(issue(
                    field("newBranch"),
                    "newBranch is only valid with mode 'branch-off'",
                ));
            }
            if worktree.base.is_some() {
                issues.push(issue(
                    field("base"),
                    "base is only valid with mode 'branch-off'",
                ));
            }
            match worktree.branch.as_deref() {
                None | Some("") => issues.push(issue(
                    field("branch"),
                    "branch is required for mode 'checkout-branch'",
                )),
                Some(branch) => {
                    if branch.contains("${{") {
                        issues.push(issue(
                            field("branch"),
                            "branch is literal and takes no template expression",
                        ));
                    } else if let Err(error) = validate_branch_name(branch) {
                        issues.push(issue(field("branch"), error.to_string()));
                    }
                }
            }
        }
    }

    issues
}

/// Activation checks for a step's environment selector.
///
/// Every choice of a finite selector must name a defined environment, so the
/// run can never fail because a possible value was dangling.
pub fn selector_issues(
    selector: &EnvironmentSelector,
    environments: &[Environment],
) -> Vec<EnvironmentIssue> {
    let names: BTreeSet<&str> = environments
        .iter()
        .map(|environment| environment.name.as_str())
        .collect();
    let mut issues = Vec::new();

    match selector {
        EnvironmentSelector::Named(name) => {
            if name.trim().is_empty() {
                issues.push(issue("environment", "environment name is required"));
            } else if !names.contains(name.as_str()) {
                issues.push(issue(
                    "environment",
                    format!("unknown environment '{name}'"),
                ));
            }
        }
        EnvironmentSelector::FiniteInput(FiniteEnvironmentInput { input, choices }) => {
            if input.trim().is_empty() {
                issues.push(issue("environment.input", "input name is required"));
            }
            if choices.is_empty() {
                issues.push(issue(
                    "environment.choices",
                    "at least one choice is required",
                ));
            }
            for (index, choice) in choices.iter().enumerate() {
                if !names.contains(choice.as_str()) {
                    issues.push(issue(
                        format!("environment.choices[{index}]"),
                        format!("unknown environment '{choice}'"),
                    ));
                }
            }
        }
    }

    issues
}

/// Host facts a dispatch decision needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostAvailability {
    /// The host exists and has a live connection.
    Connected {
        /// Immutable host public id.
        public_id: String,
        /// Friendly host slug.
        slug: String,
    },
    /// The host exists but has no live connection.
    Offline {
        /// Friendly host slug.
        slug: String,
    },
    /// No live host has the slug.
    Unknown,
}

/// Facts substituted into [`resolve_dispatch`]; pure and unit-testable.
#[derive(Debug)]
pub struct DispatchFacts<'a> {
    /// Environments defined by the active revision.
    pub environments: &'a [Environment],
    /// Host availability keyed by normalized host slug.
    pub hosts: &'a BTreeMap<String, HostAvailability>,
}

/// A resolved dispatch target: one complete environment and its branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchTarget {
    /// Environment name the step selected.
    pub environment: String,
    /// Immutable host public id the pool addresses.
    pub host_public_id: String,
    /// Friendly host slug, for the audit trail.
    pub host_slug: String,
    /// Working directory on the host, passed through unchanged.
    pub directory: String,
    /// Rendered branch, or `None` when the execution runs in the directory.
    pub branch: Option<String>,
}

/// Decide where one execution runs.
///
/// Pure: the caller supplies host availability and inputs. Selects exactly one
/// complete environment, refuses an unknown or offline host, and renders the
/// worktree branch with this execution's id.
pub fn resolve_dispatch(
    facts: &DispatchFacts<'_>,
    selector: &EnvironmentSelector,
    inputs: &BTreeMap<String, String>,
    execution_id: &str,
) -> Result<DispatchTarget, EnvironmentError> {
    let environment = selector.resolve(facts.environments, inputs)?;
    // Activation already rejected this, but a dispatch target is never built
    // from a non-canonical directory even if a stale caller asks for one.
    validate_directory(&environment.directory)?;
    let host_key = normalize_slug(&environment.host);

    match facts.hosts.get(&host_key) {
        None | Some(HostAvailability::Unknown) => Err(EnvironmentError::UnknownHostSlug {
            slug: environment.host.clone(),
        }),
        Some(HostAvailability::Offline { slug }) => {
            Err(EnvironmentError::DaemonNotConnected { slug: slug.clone() })
        }
        Some(HostAvailability::Connected { public_id, slug }) => {
            let branch = environment.branch_for(execution_id)?;
            Ok(DispatchTarget {
                environment: environment.name.clone(),
                host_public_id: public_id.clone(),
                host_slug: slug.clone(),
                directory: environment.directory.clone(),
                branch,
            })
        }
    }
}

/// Resolve an environment for dispatch and require its host to be connected.
///
/// Gathers the database facts (live host by slug, presence) and then applies
/// [`resolve_dispatch`]. An offline host fails with
/// [`EnvironmentError::DaemonNotConnected`] (`daemon_not_connected`) and
/// nothing is queued.
pub async fn require_for_dispatch(
    pool: &MySqlPool,
    organization_id: u64,
    environments: &[Environment],
    selector: &EnvironmentSelector,
    inputs: &BTreeMap<String, String>,
    execution_id: &str,
) -> Result<DispatchTarget, EnvironmentError> {
    // Resolve the environment first so an unknown name never touches the host.
    let environment = selector.resolve(environments, inputs)?;
    let host_key = normalize_slug(&environment.host);

    let availability = match host_svc::resolve_for_activation(pool, organization_id, &host_key)
        .await
    {
        Ok(host) => presence_for(pool, organization_id, &host).await?,
        Err(HostServiceError::UnknownHostSlug { .. } | HostServiceError::NotFound) => {
            HostAvailability::Unknown
        }
        Err(HostServiceError::Database(error)) => return Err(EnvironmentError::Database(error)),
        Err(_) => HostAvailability::Unknown,
    };

    let mut hosts = BTreeMap::new();
    hosts.insert(host_key, availability);
    resolve_dispatch(
        &DispatchFacts {
            environments,
            hosts: &hosts,
        },
        selector,
        inputs,
        execution_id,
    )
}

/// Ask presence for one host and turn the answer into availability facts.
async fn presence_for(
    pool: &MySqlPool,
    organization_id: u64,
    host: &Host,
) -> Result<HostAvailability, EnvironmentError> {
    if host.status == HostStatus::Revoked {
        return Ok(HostAvailability::Unknown);
    }
    match host_presence::require_connected(pool, organization_id, &host.public_id).await {
        Ok(host) => Ok(HostAvailability::Connected {
            public_id: host.public_id,
            slug: host.slug,
        }),
        Err(HostPresenceError::DaemonNotConnected { slug }) => {
            Ok(HostAvailability::Offline { slug })
        }
        Err(HostPresenceError::NotFound) => Ok(HostAvailability::Unknown),
        Err(HostPresenceError::Database(error)) => Err(EnvironmentError::Database(error)),
    }
}

/// Domain errors resolving an environment.
#[derive(Debug, Error)]
pub enum EnvironmentError {
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
    /// No live host has the environment's slug.
    #[error("unknown host slug '{slug}'")]
    UnknownHostSlug {
        /// Host slug that failed to resolve.
        slug: String,
    },
    /// The environment's host is not connected.
    #[error("daemon_not_connected: host '{slug}' is offline")]
    DaemonNotConnected {
        /// Host slug that is offline.
        slug: String,
    },
    /// The worktree could not be prepared.
    #[error(transparent)]
    Worktree(#[from] WorktreeError),
    /// A working directory declaration is invalid.
    #[error(transparent)]
    Directory(#[from] DirectoryError),
    /// Database failure.
    #[error("database error")]
    Database(#[source] sqlx::Error),
}

impl EnvironmentError {
    /// Stable problem code for the hub API.
    pub fn code(&self) -> &'static str {
        match self {
            EnvironmentError::DaemonNotConnected { .. } => "daemon_not_connected",
            EnvironmentError::Database(_) => "internal_error",
            _ => "validation_failed",
        }
    }
}

impl From<SelectorError> for EnvironmentError {
    fn from(error: SelectorError) -> Self {
        match error {
            SelectorError::UnknownEnvironment(name) => EnvironmentError::UnknownEnvironment(name),
            SelectorError::InputRequired(input) => EnvironmentError::InputRequired(input),
            SelectorError::ChoiceNotAllowed { input, value } => {
                EnvironmentError::ChoiceNotAllowed { input, value }
            }
            SelectorError::NoChoices => EnvironmentError::NoChoices,
            SelectorError::InputNameRequired => EnvironmentError::InputNameRequired,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::environment::{EXECUTION_ID_EXPRESSION, FiniteEnvironmentInput};

    fn hosts() -> BTreeSet<String> {
        ["studio-mac".to_string()].into_iter().collect()
    }

    fn environment(name: &str) -> Environment {
        Environment {
            name: name.into(),
            host: "studio-mac".into(),
            directory: "/Users/you/code/repo".into(),
            worktree: None,
        }
    }

    fn worktree_environment(name: &str) -> Environment {
        Environment {
            name: name.into(),
            host: "studio-mac".into(),
            directory: "/Users/you/code/repo".into(),
            worktree: Some(Worktree {
                mode: WorktreeMode::BranchOff,
                new_branch: Some(format!("trigger-{EXECUTION_ID_EXPRESSION}")),
                base: Some("origin/main".into()),
                branch: None,
            }),
        }
    }

    fn availability(slug: &str) -> BTreeMap<String, HostAvailability> {
        let mut hosts = BTreeMap::new();
        hosts.insert(
            slug.to_string(),
            HostAvailability::Connected {
                public_id: "host-public-id".into(),
                slug: slug.to_string(),
            },
        );
        hosts
    }

    fn named(name: &str) -> EnvironmentSelector {
        EnvironmentSelector::Named(name.into())
    }

    #[test]
    fn a_complete_environment_resolves_cleanly() {
        let issues = environment_issues(&[worktree_environment("dev")], &hosts());
        assert!(issues.is_empty(), "expected no issues, got {issues:?}");
    }

    #[test]
    fn an_unknown_host_fails_activation_at_the_host_field() {
        let mut environment = environment("dev");
        environment.host = "gone-host".into();
        let issues = environment_issues(&[environment], &hosts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "environments.dev.host");
        assert!(issues[0].message.contains("unknown host slug 'gone-host'"));
    }

    #[test]
    fn a_missing_directory_fails_activation_at_the_directory_field() {
        let mut environment = environment("dev");
        environment.directory = String::new();
        let issues = environment_issues(&[environment], &hosts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "environments.dev.directory");
        assert!(issues[0].message.contains("required"));
    }

    #[test]
    fn a_relative_or_traversing_directory_fails_activation() {
        let mut relative = environment("dev");
        relative.directory = "code/repo".into();
        let issues = environment_issues(&[relative], &hosts());
        assert_eq!(issues[0].field, "environments.dev.directory");
        assert!(issues[0].message.contains("absolute"));

        let mut traversing = environment("dev");
        traversing.directory = "/code/../../etc".into();
        let issues = environment_issues(&[traversing], &hosts());
        assert_eq!(issues[0].field, "environments.dev.directory");
        assert!(issues[0].message.contains("'..'"));
    }

    #[test]
    fn duplicate_names_are_refused() {
        let issues = environment_issues(&[environment("dev"), environment("dev")], &hosts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "environments.dev.name");
    }

    #[test]
    fn a_branch_off_template_must_embed_the_execution_id() {
        let mut environment = worktree_environment("dev");
        if let Some(worktree) = environment.worktree.as_mut() {
            worktree.new_branch = Some("trigger".into());
        }
        let issues = environment_issues(&[environment], &hosts());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "environments.dev.worktree.newBranch");
    }

    #[test]
    fn a_branch_off_template_refuses_other_expressions() {
        let mut environment = worktree_environment("dev");
        if let Some(worktree) = environment.worktree.as_mut() {
            worktree.new_branch = Some("trigger-${{ paseo.prompt }}".into());
        }
        let issues = environment_issues(&[environment], &hosts());
        assert_eq!(issues[0].field, "environments.dev.worktree.newBranch");
        assert!(issues[0].message.contains("unsupported"));
    }

    #[test]
    fn a_bad_base_ref_is_named() {
        let mut environment = worktree_environment("dev");
        if let Some(worktree) = environment.worktree.as_mut() {
            worktree.base = Some("origin/../main".into());
        }
        let issues = environment_issues(&[environment], &hosts());
        assert_eq!(issues[0].field, "environments.dev.worktree.base");
    }

    #[test]
    fn a_checkout_branch_is_literal_and_complete() {
        let mut environment = environment("dev");
        environment.worktree = Some(Worktree {
            mode: WorktreeMode::CheckoutBranch,
            new_branch: Some("trigger-${{ paseo.execution.id }}".into()),
            base: Some("origin/main".into()),
            branch: Some("release/1.4".into()),
        });
        let issues = environment_issues(&[environment], &hosts());
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"environments.dev.worktree.newBranch"));
        assert!(fields.contains(&"environments.dev.worktree.base"));
    }

    #[test]
    fn a_selector_must_name_complete_environments() {
        let environments = vec![environment("dev"), worktree_environment("clean")];
        assert!(selector_issues(&named("dev"), &environments).is_empty());
        assert!(!selector_issues(&named("nope"), &environments).is_empty());

        let selector = EnvironmentSelector::FiniteInput(FiniteEnvironmentInput {
            input: "repo".into(),
            choices: vec!["dev".into(), "gone".into()],
        });
        let issues = selector_issues(&selector, &environments);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, "environment.choices[1]");
    }

    #[test]
    fn dispatch_renders_the_execution_id_per_execution() {
        let environments = vec![worktree_environment("dev")];
        let hosts = availability("studio-mac");
        let facts = DispatchFacts {
            environments: &environments,
            hosts: &hosts,
        };

        let first = resolve_dispatch(&facts, &named("dev"), &BTreeMap::new(), "exec-1")
            .expect("first execution");
        let second = resolve_dispatch(&facts, &named("dev"), &BTreeMap::new(), "exec-2")
            .expect("second execution");

        assert_eq!(first.directory, "/Users/you/code/repo");
        assert_eq!(first.branch.as_deref(), Some("trigger-exec-1"));
        assert_eq!(second.branch.as_deref(), Some("trigger-exec-2"));
        assert_ne!(first.branch, second.branch);
        assert_eq!(first.host_public_id, "host-public-id");
    }

    #[test]
    fn dispatch_to_an_offline_host_fails_with_the_documented_code() {
        let environments = vec![worktree_environment("dev")];
        let mut hosts = BTreeMap::new();
        hosts.insert(
            "studio-mac".to_string(),
            HostAvailability::Offline {
                slug: "studio-mac".into(),
            },
        );
        let facts = DispatchFacts {
            environments: &environments,
            hosts: &hosts,
        };

        let error = resolve_dispatch(&facts, &named("dev"), &BTreeMap::new(), "exec-1")
            .expect_err("offline dispatch must fail");
        assert_eq!(error.code(), "daemon_not_connected");
        assert!(error.to_string().contains("daemon_not_connected"));
    }

    #[test]
    fn a_non_canonical_directory_never_reaches_dispatch() {
        let mut environment = environment("dev");
        environment.directory = "/code/../etc".into();
        let environments = vec![environment];
        let hosts = availability("studio-mac");
        let facts = DispatchFacts {
            environments: &environments,
            hosts: &hosts,
        };
        let error = resolve_dispatch(&facts, &named("dev"), &BTreeMap::new(), "exec-1")
            .expect_err("a non-canonical directory must not dispatch");
        assert_eq!(error.code(), "validation_failed");
        assert!(error.to_string().contains("not a canonical path"));
    }

    #[test]
    fn dispatch_to_an_unknown_host_fails_at_activation_style() {
        let environments = vec![worktree_environment("dev")];
        let facts = DispatchFacts {
            environments: &environments,
            hosts: &BTreeMap::new(),
        };
        let error = resolve_dispatch(&facts, &named("dev"), &BTreeMap::new(), "exec-1")
            .expect_err("unknown host must fail");
        assert_eq!(error.code(), "validation_failed");
    }

    #[test]
    fn a_finite_input_selects_one_environment_per_execution() {
        let environments = vec![environment("dev"), worktree_environment("clean")];
        let hosts = availability("studio-mac");
        let facts = DispatchFacts {
            environments: &environments,
            hosts: &hosts,
        };
        let selector = EnvironmentSelector::FiniteInput(FiniteEnvironmentInput {
            input: "repo".into(),
            choices: vec!["dev".into(), "clean".into()],
        });

        let mut dev_input = BTreeMap::new();
        dev_input.insert("repo".to_string(), "dev".to_string());
        let dev = resolve_dispatch(&facts, &selector, &dev_input, "exec-1").expect("dev");
        assert_eq!(dev.environment, "dev");
        assert_eq!(dev.branch, None, "dev has no worktree");

        let mut clean_input = BTreeMap::new();
        clean_input.insert("repo".to_string(), "clean".to_string());
        let clean = resolve_dispatch(&facts, &selector, &clean_input, "exec-1").expect("clean");
        assert_eq!(clean.environment, "clean");
        assert_eq!(clean.branch.as_deref(), Some("trigger-exec-1"));

        let mut bad_input = BTreeMap::new();
        bad_input.insert("repo".to_string(), "prod".to_string());
        let error = resolve_dispatch(&facts, &selector, &bad_input, "exec-1")
            .expect_err("undeclared choice must fail");
        assert_eq!(error.code(), "validation_failed");
    }

    #[test]
    fn environments_are_not_merged_across_selection() {
        let mut clean = worktree_environment("clean");
        if let Some(worktree) = clean.worktree.as_mut() {
            worktree.base = Some("origin/release".into());
        }
        let environments = vec![environment("dev"), clean];
        let hosts = availability("studio-mac");
        let facts = DispatchFacts {
            environments: &environments,
            hosts: &hosts,
        };
        let selector = EnvironmentSelector::FiniteInput(FiniteEnvironmentInput {
            input: "repo".into(),
            choices: vec!["dev".into(), "clean".into()],
        });
        let mut inputs = BTreeMap::new();
        inputs.insert("repo".to_string(), "clean".to_string());

        let target = resolve_dispatch(&facts, &selector, &inputs, "exec-1").expect("clean");
        assert_eq!(target.environment, "clean");
        assert_eq!(
            target.branch.as_deref(),
            Some("trigger-exec-1"),
            "the selected environment contributes its whole worktree"
        );
        assert_eq!(target.directory, "/Users/you/code/repo");
    }

    #[test]
    fn an_offline_host_is_not_queued() {
        // There is no queue type and no retry path: the error is the outcome.
        let error = EnvironmentError::DaemonNotConnected {
            slug: "studio-mac".into(),
        };
        assert_eq!(error.code(), "daemon_not_connected");
        assert_eq!(
            error.to_string(),
            "daemon_not_connected: host 'studio-mac' is offline"
        );
    }
}
