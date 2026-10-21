//! Workflow validation (task 1.4.4).
//!
//! A workflow is validated against the rest of one configuration revision:
//! its environments (task 1.4.5), named agents, prompt partials and its own
//! declared inputs. [`workflow_issues`] is pure and database-free: the caller
//! passes the resolved name sets, so every rule is unit-testable without
//! MySQL.
//!
//! The rules that matter:
//!
//! - **Steps run in file order and may read only earlier steps.** A `reads`
//!   entry, a condition operand and a `${{ steps.<id>... }}` prompt expression
//!   must name a step that appears before the reader. A later step (or the step
//!   itself) is rejected.
//! - **Step ids are unique** and stable.
//! - **An agent is a name or one complete static inline configuration.** A
//!   dynamic inline object is named and refused.
//! - **A finite output schema may bound a later choice; an unbounded one may
//!   not.** When a condition compares a declared input to an earlier step's
//!   finite output property, every declared input choice must be in the finite
//!   set. An unbounded property leaves the choice unconstrained.
//! - **Deadlines are positive and bounded.** The workflow's remaining runtime
//!   is applied later by [`Workflow::effective_deadlines`].
//!
//! The bundle caps (100 files, 512-character paths, 1,000,000 characters per
//! file and 5,000,000 bytes of prompt partials) belong to
//! [`crate::services::configuration`], which owns the authored file set.

use crate::entities::environment::Environment;
use crate::entities::workflow::{
    AgentRef, ComparisonOp, MAX_STEP_ID_CHARS, MAX_STEP_TIMEOUT_SECONDS,
    MAX_WORKFLOW_RUNTIME_SECONDS, Operand, OutputSchema, PromptPart, Step, Workflow,
    step_references,
};
use crate::services::environments as environment_svc;
use crate::services::outputs::OutputLedger;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// One named workflow validation failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowIssue {
    /// Reference path, for example `workflows.triage.steps[1].reads[0]`.
    pub field: String,
    /// Human-readable explanation.
    pub message: String,
}

fn issue(field: impl Into<String>, message: impl Into<String>) -> WorkflowIssue {
    WorkflowIssue {
        field: field.into(),
        message: message.into(),
    }
}

/// Validate every workflow in a revision.
///
/// `environments` are the revision's complete environment objects,
/// `agent_names` the named agents and `partial_names` the declared prompt
/// partials. Every failure names the authored field; the caller adds the
/// revision number.
pub fn workflow_issues(
    workflows: &[Workflow],
    environments: &[Environment],
    agent_names: &BTreeSet<String>,
    partial_names: &BTreeSet<String>,
) -> Vec<WorkflowIssue> {
    workflow_issues_with_partial_texts(
        workflows,
        environments,
        agent_names,
        partial_names,
        &BTreeMap::new(),
    )
}

/// Validate every workflow, also scanning the text of referenced partials for
/// step-output expressions.
///
/// A partial is a literal file, but it is still part of a step's prompt: a
/// `${{ steps.<id>... }}` expression inside it must name an earlier step just
/// like one written inline. `partial_texts` maps a declared partial name to
/// its authored content; a name with no text is checked by name only.
pub fn workflow_issues_with_partial_texts(
    workflows: &[Workflow],
    environments: &[Environment],
    agent_names: &BTreeSet<String>,
    partial_names: &BTreeSet<String>,
    partial_texts: &BTreeMap<String, String>,
) -> Vec<WorkflowIssue> {
    let mut issues = Vec::new();
    let mut seen_workflows = BTreeSet::new();

    for (workflow_index, workflow) in workflows.iter().enumerate() {
        let name = workflow.name.trim();
        let prefix = if name.is_empty() {
            format!("workflows[{workflow_index}]")
        } else {
            format!("workflows.{name}")
        };

        if name.is_empty() {
            issues.push(issue(format!("{prefix}.name"), "workflow name is required"));
        } else if !seen_workflows.insert(name.to_string()) {
            issues.push(issue(
                format!("{prefix}.name"),
                format!("duplicate workflow name '{name}'"),
            ));
        }

        if workflow.trigger.trim().is_empty() {
            issues.push(issue(
                format!("{prefix}.trigger"),
                "a workflow must name exactly one trigger",
            ));
        }

        if let Some(runtime) = workflow.max_runtime_seconds
            && (runtime == 0 || runtime > MAX_WORKFLOW_RUNTIME_SECONDS)
        {
            issues.push(issue(
                format!("{prefix}.maxRuntime"),
                format!(
                    "workflow maximum runtime must be 1-{MAX_WORKFLOW_RUNTIME_SECONDS} seconds, got {runtime}"
                ),
            ));
        }

        validate_inputs(workflow, &prefix, &mut issues);

        if workflow.steps.is_empty() {
            issues.push(issue(
                format!("{prefix}.steps"),
                "a workflow must declare at least one step",
            ));
            continue;
        }

        let mut seen_steps = BTreeSet::new();
        for (step_index, step) in workflow.steps.iter().enumerate() {
            let step_prefix = format!("{prefix}.steps[{step_index}]");
            validate_step(
                workflow,
                step,
                step_index,
                &step_prefix,
                environments,
                agent_names,
                partial_names,
                partial_texts,
                &mut seen_steps,
                &mut issues,
            );
        }
    }

    issues
}

fn validate_inputs(workflow: &Workflow, prefix: &str, issues: &mut Vec<WorkflowIssue>) {
    let mut seen = BTreeSet::new();
    for (index, input) in workflow.inputs.iter().enumerate() {
        let field = format!("{prefix}.inputs[{index}]");
        let name = input.name.trim();
        if name.is_empty() {
            issues.push(issue(format!("{field}.name"), "input name is required"));
        } else if !seen.insert(name.to_string()) {
            issues.push(issue(
                format!("{field}.name"),
                format!("duplicate input name '{name}'"),
            ));
        }

        let mut choices = BTreeSet::new();
        for (choice_index, choice) in input.choices.iter().enumerate() {
            if choice.trim().is_empty() {
                issues.push(issue(
                    format!("{field}.choices[{choice_index}]"),
                    "choice must be non-empty",
                ));
            } else if !choices.insert(choice.clone()) {
                issues.push(issue(
                    format!("{field}.choices[{choice_index}]"),
                    format!("duplicate choice '{choice}'"),
                ));
            }
        }

        if let Some(default) = &input.default {
            if default.trim().is_empty() {
                issues.push(issue(
                    format!("{field}.default"),
                    "default must be non-empty",
                ));
            } else if !input.choices.is_empty()
                && !input.choices.iter().any(|choice| choice == default)
            {
                issues.push(issue(
                    format!("{field}.default"),
                    format!("default '{default}' is not one of the declared choices"),
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_step(
    workflow: &Workflow,
    step: &Step,
    step_index: usize,
    prefix: &str,
    environments: &[Environment],
    agent_names: &BTreeSet<String>,
    partial_names: &BTreeSet<String>,
    partial_texts: &BTreeMap<String, String>,
    seen_steps: &mut BTreeSet<String>,
    issues: &mut Vec<WorkflowIssue>,
) {
    validate_step_id(step, prefix, seen_steps, issues);
    validate_environment(step, prefix, environments, issues);
    validate_agent(step, prefix, agent_names, issues);
    validate_prompt(
        step,
        workflow,
        step_index,
        prefix,
        partial_names,
        partial_texts,
        issues,
    );
    validate_reads(workflow, step, step_index, prefix, issues);
    validate_condition(workflow, step, step_index, prefix, issues);
    validate_output_schema(step, prefix, issues);
    validate_deadlines(step, prefix, issues);

    if let Err(error) = OutputLedger::new(&step.outputs) {
        issues.push(issue(format!("{prefix}.outputs"), error.to_string()));
    }
}

fn validate_step_id(
    step: &Step,
    prefix: &str,
    seen_steps: &mut BTreeSet<String>,
    issues: &mut Vec<WorkflowIssue>,
) {
    let id = step.id.trim();
    if id.is_empty() {
        issues.push(issue(format!("{prefix}.id"), "step id is required"));
        return;
    }
    if id.chars().count() > MAX_STEP_ID_CHARS {
        issues.push(issue(
            format!("{prefix}.id"),
            format!("step id must be at most {MAX_STEP_ID_CHARS} characters"),
        ));
    }
    let valid = id
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric())
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if !valid {
        issues.push(issue(
            format!("{prefix}.id"),
            "step id must start alphanumeric and contain only [A-Za-z0-9_-]",
        ));
    }
    if !seen_steps.insert(id.to_string()) {
        issues.push(issue(
            format!("{prefix}.id"),
            format!("duplicate step id '{id}'"),
        ));
    }
}

fn validate_environment(
    step: &Step,
    prefix: &str,
    environments: &[Environment],
    issues: &mut Vec<WorkflowIssue>,
) {
    let Some(selector) = &step.environment else {
        issues.push(issue(
            format!("{prefix}.environment"),
            "a step must select one complete environment",
        ));
        return;
    };
    for environment_issue in environment_svc::selector_issues(selector, environments) {
        issues.push(issue(
            format!("{prefix}.{}", environment_issue.field),
            environment_issue.message,
        ));
    }
}

fn validate_agent(
    step: &Step,
    prefix: &str,
    agent_names: &BTreeSet<String>,
    issues: &mut Vec<WorkflowIssue>,
) {
    let Some(agent) = &step.agent else {
        issues.push(issue(
            format!("{prefix}.agent"),
            "a step must name an agent or declare one complete inline configuration",
        ));
        return;
    };

    match agent {
        AgentRef::Named(name) => {
            let name = name.trim();
            if name.is_empty() {
                issues.push(issue(format!("{prefix}.agent"), "agent name is required"));
            } else if !agent_names.contains(name) {
                issues.push(issue(
                    format!("{prefix}.agent"),
                    format!("unknown named agent '{name}'"),
                ));
            }
        }
        AgentRef::Inline(inline) => {
            for (field, value) in [
                ("provider", inline.provider.as_str()),
                ("model", inline.model.as_str()),
                ("execution_mode", inline.execution_mode.as_str()),
            ] {
                if value.trim().is_empty() {
                    issues.push(issue(
                        format!("{prefix}.agent.{field}"),
                        format!("{field} is required for an inline agent"),
                    ));
                } else if value.contains("${{") {
                    issues.push(issue(
                        format!("{prefix}.agent.{field}"),
                        "an inline agent must be static; expressions are not allowed",
                    ));
                }
            }
            if inline
                .thinking_option
                .as_deref()
                .is_some_and(|option| option.trim().is_empty())
            {
                issues.push(issue(
                    format!("{prefix}.agent.thinking_option"),
                    "thinking option must be non-empty",
                ));
            }
        }
        AgentRef::Dynamic(_) => {
            issues.push(issue(
                format!("{prefix}.agent"),
                "a dynamic inline agent object is not allowed; name an agent or declare one complete static inline configuration",
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_prompt(
    step: &Step,
    workflow: &Workflow,
    step_index: usize,
    prefix: &str,
    partial_names: &BTreeSet<String>,
    partial_texts: &BTreeMap<String, String>,
    issues: &mut Vec<WorkflowIssue>,
) {
    for (part_index, part) in step.prompt.parts().iter().enumerate() {
        if let PromptPart::Partial(partial) = part {
            let name = partial.partial.trim();
            if name.is_empty() {
                issues.push(issue(
                    format!("{prefix}.prompt[{part_index}].partial"),
                    "partial name is required",
                ));
            } else if !partial_names.contains(name) {
                issues.push(issue(
                    format!("{prefix}.prompt[{part_index}].partial"),
                    format!("unknown partial '{name}'"),
                ));
            } else if let Some(text) = partial_texts.get(name) {
                for referenced in step_references(text) {
                    check_earlier_step(
                        workflow,
                        step_index,
                        referenced,
                        format!("{prefix}.prompt[{part_index}].partial"),
                        "step output shown in a partial",
                        issues,
                    );
                }
            }
        }
    }

    for referenced in step.prompt.referenced_steps() {
        check_earlier_step(
            workflow,
            step_index,
            referenced,
            format!("{prefix}.prompt"),
            "step output shown in a prompt",
            issues,
        );
    }
}

fn validate_reads(
    workflow: &Workflow,
    step: &Step,
    step_index: usize,
    prefix: &str,
    issues: &mut Vec<WorkflowIssue>,
) {
    for (index, read) in step.reads.iter().enumerate() {
        let field = format!("{prefix}.reads[{index}]");
        if read.trim().is_empty() {
            issues.push(issue(field, "a read must name an earlier step"));
            continue;
        }
        check_earlier_step(
            workflow,
            step_index,
            read.trim(),
            field,
            "step read",
            issues,
        );
    }
}

fn validate_condition(
    workflow: &Workflow,
    step: &Step,
    step_index: usize,
    prefix: &str,
    issues: &mut Vec<WorkflowIssue>,
) {
    let Some(condition) = &step.condition else {
        return;
    };
    let field = format!("{prefix}.if");

    for (index, referenced) in condition.referenced_steps().into_iter().enumerate() {
        check_earlier_step(
            workflow,
            step_index,
            referenced,
            format!("{field}.steps[{index}]"),
            "condition step output",
            issues,
        );
    }

    for (index, referenced) in condition.referenced_inputs().into_iter().enumerate() {
        if workflow.input(referenced).is_none() {
            issues.push(issue(
                format!("{field}.inputs[{index}]"),
                format!("unknown input '{referenced}'"),
            ));
        }
    }

    for predicate in condition.predicates() {
        if predicate.op.needs_right() && predicate.right.is_none() {
            issues.push(issue(
                field.clone(),
                format!(
                    "operator '{}' requires a right operand",
                    op_name(predicate.op)
                ),
            ));
            continue;
        }
        if !predicate.op.needs_right() && predicate.right.is_some() {
            issues.push(issue(
                field.clone(),
                format!(
                    "operator '{}' takes no right operand",
                    op_name(predicate.op)
                ),
            ));
        }

        for operand in [Some(&predicate.left), predicate.right.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Operand::Step(reference) = operand
                && let Some(property) = &reference.property
            {
                validate_output_property(
                    workflow,
                    step_index,
                    &reference.step,
                    property,
                    &field,
                    issues,
                );
            }
        }

        if let Some((input_name, reference)) = predicate.finite_choice_bound()
            && let Some(input) = workflow.input(input_name)
            && !input.choices.is_empty()
            && let Some(property) = &reference.property
            && let Some(before) = earlier_step_index(workflow, step_index, &reference.step)
            && let Some(schema) = workflow.steps[before].output.as_ref()
            && let Some(finite) = schema.finite_values(property)
        {
            for choice in &input.choices {
                if !finite.contains(choice) {
                    issues.push(issue(
                        format!("{field}.choices"),
                        format!(
                            "input '{input_name}' choice '{choice}' is not allowed by step '{}' finite output '{property}'",
                            reference.step
                        ),
                    ));
                }
            }
        }
    }
}

fn validate_output_property(
    workflow: &Workflow,
    step_index: usize,
    step_id: &str,
    property: &str,
    field: &str,
    issues: &mut Vec<WorkflowIssue>,
) {
    let Some(before) = earlier_step_index(workflow, step_index, step_id) else {
        return;
    };
    match workflow.steps[before].output.as_ref() {
        None => issues.push(issue(
            field.to_string(),
            format!("step '{step_id}' declares no output schema property '{property}'"),
        )),
        Some(schema) if !schema.has_property(property) => issues.push(issue(
            field.to_string(),
            format!("step '{step_id}' has no output property '{property}'"),
        )),
        Some(_) => {}
    }
}

fn validate_output_schema(step: &Step, prefix: &str, issues: &mut Vec<WorkflowIssue>) {
    let Some(schema) = &step.output else {
        return;
    };
    let field = format!("{prefix}.output");
    for (name, property) in &schema.properties {
        if name.trim().is_empty() {
            issues.push(issue(
                format!("{field}.properties"),
                "property name is required",
            ));
        }
        for (index, value) in property.enumerated.iter().enumerate() {
            if value.trim().is_empty() {
                issues.push(issue(
                    format!("{field}.properties.{name}.enum[{index}]"),
                    "enumerated value must be non-empty",
                ));
            }
        }
        if let Some(constant) = &property.constant {
            if constant.trim().is_empty() {
                issues.push(issue(
                    format!("{field}.properties.{name}.const"),
                    "constant value must be non-empty",
                ));
            } else if !property.enumerated.is_empty()
                && !property.enumerated.iter().any(|value| value == constant)
            {
                issues.push(issue(
                    format!("{field}.properties.{name}.const"),
                    format!("constant '{constant}' is not one of the enumerated values"),
                ));
            }
        }
    }
}

fn validate_deadlines(step: &Step, prefix: &str, issues: &mut Vec<WorkflowIssue>) {
    if let Some(runtime) = step.max_runtime_seconds
        && (runtime == 0 || runtime > MAX_STEP_TIMEOUT_SECONDS)
    {
        issues.push(issue(
            format!("{prefix}.maxRuntime"),
            format!(
                "step maximum runtime must be 1-{MAX_STEP_TIMEOUT_SECONDS} seconds, got {runtime}"
            ),
        ));
    }
    if let Some(idle) = step.idle_timeout_seconds
        && (idle == 0 || idle > MAX_STEP_TIMEOUT_SECONDS)
    {
        issues.push(issue(
            format!("{prefix}.idleTimeout"),
            format!("step idle timeout must be 1-{MAX_STEP_TIMEOUT_SECONDS} seconds, got {idle}"),
        ));
    }
}

fn check_earlier_step(
    workflow: &Workflow,
    step_index: usize,
    referenced: &str,
    field: String,
    purpose: &str,
    issues: &mut Vec<WorkflowIssue>,
) {
    if earlier_step_index(workflow, step_index, referenced).is_some() {
        return;
    }
    if workflow.step(referenced).is_some() {
        issues.push(issue(
            field,
            format!(
                "{purpose} '{referenced}' is not an earlier step; steps may read only earlier steps"
            ),
        ));
    } else {
        issues.push(issue(
            field,
            format!("{purpose} names unknown step '{referenced}'"),
        ));
    }
}

fn earlier_step_index(workflow: &Workflow, before: usize, id: &str) -> Option<usize> {
    workflow.steps[..before]
        .iter()
        .position(|step| step.id == id)
}

fn op_name(op: ComparisonOp) -> &'static str {
    match op {
        ComparisonOp::Eq => "eq",
        ComparisonOp::Ne => "ne",
        ComparisonOp::In => "in",
        ComparisonOp::NotIn => "not_in",
        ComparisonOp::Contains => "contains",
        ComparisonOp::StartsWith => "starts_with",
        ComparisonOp::EndsWith => "ends_with",
        ComparisonOp::Exists => "exists",
        ComparisonOp::NotExists => "not_exists",
    }
}

/// A later choice may be bounded by a finite output schema.
///
/// Returns the finite values the later choice is limited to, or `None` when the
/// referenced output property is absent or unbounded. This is the predicate
/// behind validation's subset check and is exposed for dispatch (task 1.4.6).
pub fn finite_choice_bounds(
    workflow: &Workflow,
    step_index: usize,
    input_name: &str,
) -> Option<BTreeSet<String>> {
    let step = workflow.steps.get(step_index)?;
    let condition = step.condition.as_ref()?;
    let mut bounds: Option<BTreeSet<String>> = None;
    for predicate in condition.predicates() {
        let Some((input, reference)) = predicate.finite_choice_bound() else {
            continue;
        };
        if input != input_name {
            continue;
        }
        let Some(property) = reference.property.as_deref() else {
            continue;
        };
        let Some(earlier) = earlier_step_index(workflow, step_index, &reference.step) else {
            continue;
        };
        let Some(values) = workflow.steps[earlier]
            .output
            .as_ref()
            .and_then(|schema| schema.finite_values(property))
        else {
            continue;
        };
        bounds = Some(match bounds {
            None => values,
            Some(existing) => existing.intersection(&values).cloned().collect(),
        });
    }
    bounds
}

/// A reusable check for a prompt/content expression that names a step.
pub fn prompt_references_later_step(
    workflow: &Workflow,
    step_index: usize,
    step: &Step,
) -> Vec<WorkflowIssue> {
    let mut issues = Vec::new();
    let prefix = format!(
        "workflows.{}.steps[{step_index}].prompt",
        workflow.name.trim()
    );
    for referenced in step.prompt.referenced_steps() {
        check_earlier_step(
            workflow,
            step_index,
            referenced,
            prefix.clone(),
            "step output shown in a prompt",
            &mut issues,
        );
    }
    issues
}

/// Parse a workflow document from JSON, naming the failing field on error.
pub fn parse_workflow(value: &serde_json::Value) -> Result<Workflow, serde_json::Error> {
    serde_json::from_value(value.clone())
}

/// Check that an output schema property is finite (has `enum` or `const`).
pub fn is_finite(schema: &OutputSchema, property: &str) -> bool {
    schema.finite_values(property).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::environment::{Environment, Worktree, WorktreeMode};
    use crate::entities::workflow::{Condition, InlineAgent, WorkflowInput};
    use crate::services::outputs::OutputDeclaration;
    use serde_json::json;

    fn environment(name: &str) -> Environment {
        Environment {
            name: name.into(),
            host: "studio-mac".into(),
            directory: "/Users/you/code/repo".into(),
            worktree: None,
        }
    }

    fn environments() -> Vec<Environment> {
        vec![environment("default"), environment("review")]
    }

    fn agents() -> BTreeSet<String> {
        ["coder".to_string()].into_iter().collect()
    }

    fn partials() -> BTreeSet<String> {
        ["system".to_string()].into_iter().collect()
    }

    fn base_workflow() -> Workflow {
        serde_json::from_value(json!({
            "name": "triage",
            "trigger": "issue-comment",
            "maxRuntime": 900,
            "inputs": [{ "name": "route", "choices": ["bug", "docs"] }],
            "steps": [
                {
                    "id": "classify",
                    "environment": "default",
                    "agent": "coder",
                    "prompt": { "parts": [{ "text": "Classify." }, { "partial": "system" }] },
                    "output": {
                        "properties": {
                            "route": { "enum": ["bug", "docs"] },
                            "kind": { "const": "request" }
                        }
                    },
                    "outputs": [{ "capability": "slack.reply", "maximum": 1 }],
                    "maxRuntime": 300,
                    "idleTimeout": 60
                },
                {
                    "id": "fix",
                    "environment": "default",
                    "agent": {
                        "provider": "anthropic",
                        "model": "claude",
                        "execution_mode": "default"
                    },
                    "prompt": [{ "text": "Fix it." }],
                    "if": {
                        "left": { "input": "route" },
                        "op": "in",
                        "right": { "step": "classify", "property": "route" }
                    },
                    "reads": ["classify"]
                }
            ]
        }))
        .expect("workflow")
    }

    fn check(workflow: &Workflow) -> Vec<WorkflowIssue> {
        workflow_issues(
            std::slice::from_ref(workflow),
            &environments(),
            &agents(),
            &partials(),
        )
    }

    #[test]
    fn a_complete_two_step_workflow_resolves_cleanly() {
        let workflow = base_workflow();
        assert_eq!(check(&workflow), Vec::new());
    }

    #[test]
    fn a_step_reading_a_later_step_is_rejected() {
        let mut workflow = base_workflow();
        workflow.steps[0].reads.push("fix".into());
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "workflows.triage.steps[0].reads[0]"
                    && issue.message.contains("not an earlier step")),
            "got {issues:?}"
        );
    }

    #[test]
    fn a_condition_reading_a_later_step_is_rejected() {
        let mut workflow = base_workflow();
        workflow.steps[0].condition = Some(
            serde_json::from_value(json!({
                "left": { "step": "fix", "property": "kind" },
                "op": "exists"
            }))
            .expect("condition"),
        );
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.field.contains("steps[0].if")),
            "got {issues:?}"
        );
    }

    #[test]
    fn a_prompt_reading_a_later_step_is_rejected() {
        let mut workflow = base_workflow();
        workflow.steps[0].prompt =
            crate::entities::workflow::Prompt::Text("Use ${{ steps.fix.output }}".into());
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.field.ends_with("steps[0].prompt")),
            "got {issues:?}"
        );
    }

    #[test]
    fn a_partial_reading_a_later_step_is_rejected() {
        let mut workflow = base_workflow();
        workflow.steps[0].prompt = crate::entities::workflow::Prompt::Parts(vec![
            crate::entities::workflow::PromptPart::Partial(
                crate::entities::workflow::PartialPart {
                    partial: "system".into(),
                },
            ),
        ]);
        let texts = BTreeMap::from([(
            "system".to_string(),
            "Use ${{ steps.fix.output }}".to_string(),
        )]);
        let issues = workflow_issues_with_partial_texts(
            std::slice::from_ref(&workflow),
            &environments(),
            &agents(),
            &partials(),
            &texts,
        );
        assert!(
            issues.iter().any(|issue| {
                issue.field == "workflows.triage.steps[0].prompt[0].partial"
                    && issue.message.contains("not an earlier step")
            }),
            "got {issues:?}"
        );
    }

    #[test]
    fn a_step_reading_itself_is_rejected() {
        let mut workflow = base_workflow();
        workflow.steps[1].reads.push("fix".into());
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("not an earlier step")),
            "got {issues:?}"
        );
    }

    #[test]
    fn step_ids_are_unique_and_well_formed() {
        let mut workflow = base_workflow();
        workflow.steps[1].id = "classify".into();
        workflow.steps.push(workflow.steps[0].clone());
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("duplicate step id 'classify'")),
            "got {issues:?}"
        );

        workflow.steps[2].id = "has space".into();
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("step id must start alphanumeric")),
            "got {issues:?}"
        );
    }

    #[test]
    fn a_dynamic_inline_agent_is_rejected_while_named_and_static_are_accepted() {
        let workflow = base_workflow();
        assert!(check(&workflow).is_empty());

        let mut workflow = base_workflow();
        workflow.steps[1].agent = Some(AgentRef::Dynamic(
            [("input".to_string(), json!("agent_choice"))]
                .into_iter()
                .collect(),
        ));
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "workflows.triage.steps[1].agent"
                    && issue.message.contains("dynamic inline agent")),
            "got {issues:?}"
        );

        let mut workflow = base_workflow();
        workflow.steps[1].agent = Some(AgentRef::Inline(InlineAgent {
            provider: "openai".into(),
            model: "gpt".into(),
            execution_mode: "default".into(),
            thinking_option: None,
            options: None,
        }));
        assert!(check(&workflow).is_empty());
    }

    #[test]
    fn an_incomplete_or_expressive_inline_agent_is_named() {
        let mut workflow = base_workflow();
        workflow.steps[1].agent = Some(AgentRef::Inline(InlineAgent {
            provider: "${{ inputs.provider }}".into(),
            model: String::new(),
            execution_mode: "default".into(),
            thinking_option: None,
            options: None,
        }));
        let issues = check(&workflow);
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"workflows.triage.steps[1].agent.provider"));
        assert!(fields.contains(&"workflows.triage.steps[1].agent.model"));
    }

    #[test]
    fn unknown_named_agents_environments_and_partials_are_named() {
        let mut workflow = base_workflow();
        workflow.steps[0].agent = Some(AgentRef::Named("ghost".into()));
        workflow.steps[0].environment = Some(
            crate::entities::environment::EnvironmentSelector::Named("nowhere".into()),
        );
        workflow.steps[0].prompt = crate::entities::workflow::Prompt::Parts(vec![
            crate::entities::workflow::PromptPart::Partial(
                crate::entities::workflow::PartialPart {
                    partial: "missing".into(),
                },
            ),
        ]);
        let issues = check(&workflow);
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"workflows.triage.steps[0].agent"));
        assert!(fields.contains(&"workflows.triage.steps[0].environment"));
        assert!(fields.contains(&"workflows.triage.steps[0].prompt[0].partial"));
    }

    #[test]
    fn a_finite_output_schema_bounds_a_later_choice() {
        let mut workflow = base_workflow();
        workflow.inputs[0].choices = vec!["bug".into(), "docs".into()];
        assert_eq!(
            finite_choice_bounds(&workflow, 1, "route"),
            Some(
                ["bug".to_string(), "docs".to_string()]
                    .into_iter()
                    .collect()
            )
        );
        assert!(check(&workflow).is_empty());

        // A choice outside the finite output is rejected, naming the choice.
        workflow.inputs[0].choices = vec!["bug".into(), "chore".into()];
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.field.contains("steps[1].if.choices")
                    && issue.message.contains("'chore'")),
            "got {issues:?}"
        );
    }

    #[test]
    fn an_unbounded_output_does_not_bound_a_later_choice() {
        let mut workflow = base_workflow();
        workflow.steps[0].output = Some(
            serde_json::from_value(json!({
                "properties": { "route": { "type": "string" } }
            }))
            .expect("schema"),
        );
        workflow.inputs[0].choices = vec!["bug".into(), "chore".into()];
        assert_eq!(finite_choice_bounds(&workflow, 1, "route"), None);
        assert!(
            check(&workflow).is_empty(),
            "an unbounded output must not constrain the choice"
        );
    }

    #[test]
    fn conditions_name_unknown_inputs_and_output_properties() {
        let mut workflow = base_workflow();
        workflow.steps[1].condition = Some(
            serde_json::from_value(json!({
                "left": { "input": "missing" },
                "op": "in",
                "right": { "step": "classify", "property": "missing" }
            }))
            .expect("condition"),
        );
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("unknown input 'missing'")),
            "got {issues:?}"
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("has no output property 'missing'")),
            "got {issues:?}"
        );
    }

    #[test]
    fn comparison_operators_require_the_right_operand_shape() {
        let mut workflow = base_workflow();
        workflow.steps[1].condition = Some(
            serde_json::from_value(json!({
                "left": { "input": "route" },
                "op": "eq"
            }))
            .expect("condition"),
        );
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("requires a right operand")),
            "got {issues:?}"
        );

        workflow.steps[1].condition = Some(
            serde_json::from_value(json!({
                "left": { "input": "route" },
                "op": "exists",
                "right": { "value": "x" }
            }))
            .expect("condition"),
        );
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("takes no right operand")),
            "got {issues:?}"
        );
    }

    #[test]
    fn every_condition_shape_is_walked() {
        let condition: Condition = serde_json::from_value(json!({
            "all": [
                { "left": { "input": "route" }, "op": "eq", "right": { "value": "bug" } },
                { "any": [
                    { "left": { "step": "classify", "property": "kind" }, "op": "exists" }
                ]},
                { "not": { "left": { "input": "route" }, "op": "eq", "right": { "value": "docs" } } }
            ]
        }))
        .expect("condition");
        assert_eq!(condition.predicates().len(), 3);
        assert_eq!(
            condition.referenced_steps(),
            ["classify"].into_iter().collect()
        );
    }

    #[test]
    fn output_capabilities_are_checked_against_the_registry() {
        let mut workflow = base_workflow();
        workflow.steps[0].outputs = vec![OutputDeclaration::new("slack.reply")];
        assert!(check(&workflow).is_empty());

        workflow.steps[0].outputs = vec![OutputDeclaration::new("unknown.reply")];
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.field == "workflows.triage.steps[0].outputs"),
            "got {issues:?}"
        );
    }

    #[test]
    fn output_schema_constants_and_enums_are_checked() {
        let mut workflow = base_workflow();
        workflow.steps[0].output = Some(
            serde_json::from_value(json!({
                "properties": {
                    "route": { "enum": ["bug"], "const": "docs" },
                    "blank": { "const": " " }
                }
            }))
            .expect("schema"),
        );
        let issues = check(&workflow);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("not one of the enumerated values")),
            "got {issues:?}"
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("constant value must be non-empty")),
            "got {issues:?}"
        );
    }

    #[test]
    fn deadlines_must_be_positive_and_bounded() {
        let mut workflow = base_workflow();
        workflow.max_runtime_seconds = Some(0);
        workflow.steps[0].max_runtime_seconds = Some(0);
        workflow.steps[0].idle_timeout_seconds = Some(0);
        let issues = check(&workflow);
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"workflows.triage.maxRuntime"));
        assert!(fields.contains(&"workflows.triage.steps[0].maxRuntime"));
        assert!(fields.contains(&"workflows.triage.steps[0].idleTimeout"));
    }

    #[test]
    fn a_workflow_needs_a_name_a_trigger_and_at_least_one_step() {
        let mut workflow = base_workflow();
        workflow.name = String::new();
        workflow.trigger = String::new();
        workflow.steps.clear();
        let issues = workflow_issues(&[workflow], &environments(), &agents(), &partials());
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"workflows[0].name"));
        assert!(fields.contains(&"workflows[0].trigger"));
        assert!(fields.contains(&"workflows[0].steps"));
    }

    #[test]
    fn duplicate_workflow_names_are_refused() {
        let workflow = base_workflow();
        let issues = workflow_issues(
            &[workflow.clone(), workflow],
            &environments(),
            &agents(),
            &partials(),
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("duplicate workflow name")),
            "got {issues:?}"
        );
    }

    #[test]
    fn input_choices_defaults_and_duplicates_are_named() {
        let mut workflow = base_workflow();
        workflow.inputs = vec![
            WorkflowInput {
                name: "route".into(),
                required: true,
                choices: vec!["bug".into(), "bug".into(), " ".into()],
                default: Some("docs".into()),
            },
            WorkflowInput {
                name: "route".into(),
                required: false,
                choices: Vec::new(),
                default: None,
            },
        ];
        let issues = check(&workflow);
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"workflows.triage.inputs[0].choices[1]"));
        assert!(fields.contains(&"workflows.triage.inputs[0].choices[2]"));
        assert!(fields.contains(&"workflows.triage.inputs[0].default"));
        assert!(fields.contains(&"workflows.triage.inputs[1].name"));
    }

    #[test]
    fn finite_choice_bounds_intersect_multiple_predicates() {
        let mut workflow = base_workflow();
        workflow.steps[0].output = Some(
            serde_json::from_value(json!({
                "properties": {
                    "route": { "enum": ["bug", "docs", "chore"] },
                    "kind": { "const": "request" }
                }
            }))
            .expect("schema"),
        );
        workflow.steps[1].condition = Some(
            serde_json::from_value(json!({
                "all": [
                    { "left": { "input": "route" }, "op": "in",
                      "right": { "step": "classify", "property": "route" } },
                    { "left": { "input": "route" }, "op": "in",
                      "right": { "step": "classify", "property": "kind" } }
                ]
            }))
            .expect("condition"),
        );
        // route ∩ {"request"} is empty, so the later choice is fully bounded.
        assert_eq!(
            finite_choice_bounds(&workflow, 1, "route"),
            Some(BTreeSet::new())
        );
    }

    #[test]
    fn prompt_reference_helper_reports_later_steps() {
        let mut workflow = base_workflow();
        workflow.steps[0].prompt =
            crate::entities::workflow::Prompt::Text("Use ${{ steps.fix.output }}".into());
        let later = prompt_references_later_step(&workflow, 0, &workflow.steps[0]);
        assert_eq!(later.len(), 1);
        assert!(later[0].message.contains("not an earlier step"));
    }

    #[test]
    fn output_schema_finiteness_is_exposed() {
        let schema: OutputSchema = serde_json::from_value(json!({
            "properties": { "route": { "enum": ["bug"] }, "note": { "type": "string" } }
        }))
        .expect("schema");
        assert!(is_finite(&schema, "route"));
        assert!(!is_finite(&schema, "note"));
    }

    #[test]
    fn parse_workflow_reports_malformed_documents() {
        assert!(parse_workflow(&json!({ "name": "x" })).is_ok());
        assert!(parse_workflow(&json!({ "notAWorkflow": true })).is_err());
    }

    #[test]
    fn a_worktree_is_never_part_of_a_step() {
        let value = json!({
            "name": "triage",
            "trigger": "issue",
            "steps": [{
                "id": "one",
                "environment": "default",
                "agent": "coder",
                "worktree": { "mode": WorktreeMode::BranchOff }
            }]
        });
        assert!(parse_workflow(&value).is_err());
        let _ = Worktree {
            mode: WorktreeMode::BranchOff,
            new_branch: None,
            base: None,
            branch: None,
        };
    }
}
