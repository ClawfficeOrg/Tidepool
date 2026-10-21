//! `Workflow` entity — the ordered steps a trigger starts (task 1.4.4).
//!
//! A workflow is one trigger name and an ordered list of [`Step`]s. Steps run
//! in authored order. Each step names:
//!
//! - a stable `id` that is unique inside the workflow;
//! - an [`EnvironmentSelector`], resolving to one complete environment (task
//!   1.4.5);
//! - an [`AgentRef`], either a named agent or one complete static inline
//!   configuration. A dynamic inline object is deliberately not a complete
//!   configuration, so it is represented by [`AgentRef::Dynamic`] and refused
//!   during validation;
//! - a [`Prompt`] built from ordered text and named partial includes;
//! - an optional [`Condition`] over declared inputs and *earlier* step output;
//! - an optional finite [`OutputSchema`], whose `enum` and `const` properties
//!   may bound a later choice;
//! - the output capabilities it may emit;
//! - its own maximum runtime and idle timeout.
//!
//! Deadlines compose: the workflow's maximum runtime is the outer bound, and
//! the time remaining caps both a step's maximum runtime and its idle timeout
//! ([`Workflow::effective_deadlines`]).
//!
//! This module is pure: parsing, schema inspection and deadline arithmetic.
//! Reference resolution and named issues live in
//! [`crate::services::workflows`]; the bundle caps live in
//! [`crate::services::configuration`].

use crate::entities::environment::EnvironmentSelector;
use crate::services::outputs::OutputDeclaration;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Runtime used when a workflow declares no maximum.
pub const DEFAULT_WORKFLOW_RUNTIME_SECONDS: u64 = 3_600;
/// Largest workflow runtime accepted.
pub const MAX_WORKFLOW_RUNTIME_SECONDS: u64 = 7 * 24 * 60 * 60;
/// Largest step runtime or idle timeout accepted.
pub const MAX_STEP_TIMEOUT_SECONDS: u64 = 7 * 24 * 60 * 60;
/// Maximum number of characters in a step id.
pub const MAX_STEP_ID_CHARS: usize = 128;

/// One workflow: one trigger and the ordered steps it starts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    /// Workflow name, unique per active configuration revision.
    #[serde(default)]
    pub name: String,
    /// Name of the one trigger that starts this workflow.
    #[serde(default)]
    pub trigger: String,
    /// Maximum runtime for the whole run, in seconds.
    #[serde(
        default,
        rename = "maxRuntime",
        alias = "max_runtime",
        alias = "max_runtime_seconds"
    )]
    pub max_runtime_seconds: Option<u64>,
    /// Inputs a run may supply; conditions read them by name.
    #[serde(default)]
    pub inputs: Vec<WorkflowInput>,
    /// Steps in authored order. Steps run in file order.
    #[serde(default)]
    pub steps: Vec<Step>,
}

/// One declared workflow input.
///
/// `choices` makes the input finite. A finite choice may be bounded by an
/// earlier step's finite output schema, and an unbounded output schema does not
/// bound it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowInput {
    /// Input name conditions refer to.
    #[serde(default)]
    pub name: String,
    /// Whether a run must supply the input.
    #[serde(default)]
    pub required: bool,
    /// Allowed values; empty means any value.
    #[serde(default)]
    pub choices: Vec<String>,
    /// Value used when the run omits the input.
    #[serde(default)]
    pub default: Option<String>,
}

/// One ordered workflow step.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    /// Stable step id, unique inside the workflow.
    #[serde(default)]
    pub id: String,
    /// The complete environment this step runs in.
    #[serde(default)]
    pub environment: Option<EnvironmentSelector>,
    /// The agent that runs the step.
    #[serde(default)]
    pub agent: Option<AgentRef>,
    /// Ordered prompt text and partial includes.
    #[serde(default)]
    pub prompt: Prompt,
    /// Optional condition over inputs and earlier step output.
    #[serde(default, rename = "if", alias = "when")]
    pub condition: Option<Condition>,
    /// Optional finite output schema.
    #[serde(default, rename = "output", alias = "outputSchema")]
    pub output: Option<OutputSchema>,
    /// Capabilities this step may emit.
    #[serde(default)]
    pub outputs: Vec<OutputDeclaration>,
    /// This step's maximum runtime, in seconds.
    #[serde(
        default,
        rename = "maxRuntime",
        alias = "max_runtime",
        alias = "max_runtime_seconds"
    )]
    pub max_runtime_seconds: Option<u64>,
    /// This step's idle timeout, in seconds.
    #[serde(
        default,
        rename = "idleTimeout",
        alias = "idle_timeout",
        alias = "idle_timeout_seconds"
    )]
    pub idle_timeout_seconds: Option<u64>,
    /// Earlier steps this step reads directly.
    #[serde(default)]
    pub reads: Vec<String>,
}

/// How a step selects its agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AgentRef {
    /// A named agent declared by the configuration revision.
    Named(String),
    /// One complete static inline agent configuration.
    Inline(InlineAgent),
    /// Any other object. Kept as a parsed shape so validation can name the
    /// field instead of rejecting the whole bundle as malformed JSON.
    Dynamic(BTreeMap<String, Value>),
}

/// A complete static inline agent configuration.
///
/// `deny_unknown_fields` is what makes it complete: a selector such as
/// `{ "input": "agent" }` cannot ride along on an inline object, and is parsed
/// as [`AgentRef::Dynamic`] instead.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InlineAgent {
    /// Provider id.
    #[serde(default)]
    pub provider: String,
    /// Model id.
    #[serde(default)]
    pub model: String,
    /// Execution mode.
    #[serde(default, alias = "executionMode")]
    pub execution_mode: String,
    /// Thinking option, when the mode offers one.
    #[serde(default, alias = "thinkingOption")]
    pub thinking_option: Option<String>,
    /// Provider-native options, passed through untouched.
    #[serde(default)]
    pub options: Option<Value>,
}

impl AgentRef {
    /// The named agent, when this is a name reference.
    pub fn named(&self) -> Option<&str> {
        match self {
            AgentRef::Named(name) => Some(name),
            AgentRef::Inline(_) | AgentRef::Dynamic(_) => None,
        }
    }

    /// The complete inline agent, when this is one.
    pub fn inline(&self) -> Option<&InlineAgent> {
        match self {
            AgentRef::Inline(agent) => Some(agent),
            AgentRef::Named(_) | AgentRef::Dynamic(_) => None,
        }
    }

    /// True when this is the rejected dynamic inline shape.
    pub fn is_dynamic(&self) -> bool {
        matches!(self, AgentRef::Dynamic(_))
    }
}

/// A prompt: either one literal text or an ordered list of parts.
///
/// The parts list is never reordered or merged, and a partial include is a
/// named reference to a bundle file, resolved at activation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Prompt {
    /// One literal prompt.
    Text(String),
    /// Ordered text and partial parts.
    Parts(Vec<PromptPart>),
    /// A document wrapper around ordered parts.
    Document(PromptDocument),
}

impl Default for Prompt {
    fn default() -> Self {
        Prompt::Text(String::new())
    }
}

/// The `{ "parts": [...] }` spelling of an ordered prompt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptDocument {
    /// Ordered text and partial parts.
    #[serde(default)]
    pub parts: Vec<PromptPart>,
}

/// One ordered prompt part: literal text or a named partial include.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PromptPart {
    /// Literal text.
    Text(TextPart),
    /// Named partial include.
    Partial(PartialPart),
}

/// A literal prompt part.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextPart {
    /// Literal text.
    #[serde(default)]
    pub text: String,
}

/// A partial include.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialPart {
    /// Name of a partial declared by the configuration revision.
    #[serde(default)]
    pub partial: String,
}

impl Prompt {
    /// The ordered parts, or an empty slice for a plain text prompt.
    pub fn parts(&self) -> &[PromptPart] {
        match self {
            Prompt::Text(_) => &[],
            Prompt::Parts(parts) => parts,
            Prompt::Document(document) => &document.parts,
        }
    }

    /// The literal text, when this is a plain text prompt.
    pub fn text(&self) -> Option<&str> {
        match self {
            Prompt::Text(text) => Some(text),
            Prompt::Parts(_) | Prompt::Document(_) => None,
        }
    }

    /// Every step id referenced as `${{ steps.<id>... }}` in prompt text.
    pub fn referenced_steps(&self) -> BTreeSet<&str> {
        let mut steps = BTreeSet::new();
        if let Some(text) = self.text() {
            steps.extend(step_references(text));
        }
        for part in self.parts() {
            if let PromptPart::Text(text) = part {
                steps.extend(step_references(&text.text));
            }
        }
        steps
    }
}

/// Find `${{ steps.<id>. ... }}` references in one text.
pub fn step_references(text: &str) -> BTreeSet<&str> {
    let mut steps = BTreeSet::new();
    let mut rest = text;
    while let Some(start) = rest.find("${{") {
        let after_open = &rest[start + 3..];
        let Some(close) = after_open.find("}}") else {
            break;
        };
        let expression = after_open[..close].trim();
        if let Some(step) = expression.strip_prefix("steps.") {
            let end = step
                .char_indices()
                .find(|(_, character)| {
                    !(character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
                })
                .map_or(step.len(), |(index, _)| index);
            let id = step[..end].trim_end_matches('.');
            if !id.is_empty() {
                steps.insert(id);
            }
        }
        rest = &after_open[close + 2..];
    }
    steps
}

/// An optional condition over run inputs and earlier step output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Condition {
    /// Every nested condition must pass.
    All(AllCondition),
    /// At least one nested condition must pass.
    Any(AnyCondition),
    /// The nested condition must fail.
    Not(NotCondition),
    /// One comparison.
    Predicate(Predicate),
}

/// `{ "all": [...] }`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllCondition {
    /// Nested conditions.
    #[serde(default)]
    pub all: Vec<Condition>,
}

/// `{ "any": [...] }`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnyCondition {
    /// Nested conditions.
    #[serde(default)]
    pub any: Vec<Condition>,
}

/// `{ "not": { ... } }`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotCondition {
    /// Nested condition.
    #[serde(default)]
    pub not: Option<Box<Condition>>,
}

/// One comparison over two operands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Predicate {
    /// Left operand.
    pub left: Operand,
    /// Comparison operator.
    pub op: ComparisonOp,
    /// Right operand; omitted for `exists` and `not_exists`.
    #[serde(default)]
    pub right: Option<Operand>,
}

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Left value is one of the right's values.
    In,
    /// Left value is not one of the right's values.
    NotIn,
    /// String contains.
    Contains,
    /// String starts with.
    StartsWith,
    /// String ends with.
    EndsWith,
    /// Value is present.
    Exists,
    /// Value is absent.
    NotExists,
}

impl ComparisonOp {
    /// Whether the operator compares against a right operand.
    pub fn needs_right(self) -> bool {
        !matches!(self, ComparisonOp::Exists | ComparisonOp::NotExists)
    }

    /// Whether the operator expresses membership, which a finite output can
    /// bound.
    pub fn is_membership(self) -> bool {
        matches!(
            self,
            ComparisonOp::In | ComparisonOp::Eq | ComparisonOp::NotIn | ComparisonOp::Ne
        )
    }
}

/// One side of a comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Operand {
    /// A declared run input.
    Input(InputOperand),
    /// Output of an earlier step.
    Step(StepOutputOperand),
    /// A literal value.
    Literal(LiteralOperand),
}

/// `{ "input": "name" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputOperand {
    /// Input name.
    pub input: String,
}

/// `{ "step": "classify", "property": "route" }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepOutputOperand {
    /// Earlier step id.
    pub step: String,
    /// Property within that step's output schema.
    #[serde(default, alias = "path", alias = "output")]
    pub property: Option<String>,
}

/// `{ "value": ... }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiteralOperand {
    /// Literal JSON value.
    pub value: Value,
}

impl Condition {
    /// Every predicate in evaluation order.
    pub fn predicates(&self) -> Vec<&Predicate> {
        let mut predicates = Vec::new();
        self.collect_predicates(&mut predicates);
        predicates
    }

    fn collect_predicates<'a>(&'a self, predicates: &mut Vec<&'a Predicate>) {
        match self {
            Condition::All(all) => {
                for condition in &all.all {
                    condition.collect_predicates(predicates);
                }
            }
            Condition::Any(any) => {
                for condition in &any.any {
                    condition.collect_predicates(predicates);
                }
            }
            Condition::Not(not) => {
                if let Some(condition) = &not.not {
                    condition.collect_predicates(predicates);
                }
            }
            Condition::Predicate(predicate) => predicates.push(predicate),
        }
    }

    /// Every step id the condition reads.
    pub fn referenced_steps(&self) -> BTreeSet<&str> {
        let mut steps = BTreeSet::new();
        for predicate in self.predicates() {
            for operand in [Some(&predicate.left), predicate.right.as_ref()]
                .into_iter()
                .flatten()
            {
                if let Operand::Step(step) = operand {
                    steps.insert(step.step.as_str());
                }
            }
        }
        steps
    }

    /// Every input name the condition reads.
    pub fn referenced_inputs(&self) -> BTreeSet<&str> {
        let mut inputs = BTreeSet::new();
        for predicate in self.predicates() {
            for operand in [Some(&predicate.left), predicate.right.as_ref()]
                .into_iter()
                .flatten()
            {
                if let Operand::Input(input) = operand {
                    inputs.insert(input.input.as_str());
                }
            }
        }
        inputs
    }
}

impl Predicate {
    /// When this predicate bounds a declared input by an earlier step's finite
    /// output property, return `(input, step property)`.
    pub fn finite_choice_bound(&self) -> Option<(&str, &StepOutputOperand)> {
        if !matches!(self.op, ComparisonOp::In | ComparisonOp::Eq) {
            return None;
        }
        let Operand::Input(input) = &self.left else {
            return None;
        };
        let Some(Operand::Step(step)) = self.right.as_ref() else {
            return None;
        };
        Some((input.input.as_str(), step))
    }
}

/// An optional output schema.
///
/// A property is finite when it declares `const` or a non-empty `enum`; only a
/// finite property may bound a later choice. A schema with neither leaves the
/// later choice unconstrained.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSchema {
    /// Named properties.
    #[serde(default)]
    pub properties: BTreeMap<String, SchemaProperty>,
}

/// One schema property.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaProperty {
    /// JSON type name, for the operator surfaces.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// Enumerated values; empty is unbounded.
    #[serde(default, rename = "enum")]
    pub enumerated: Vec<String>,
    /// One constant value.
    #[serde(default, rename = "const")]
    pub constant: Option<String>,
}

impl SchemaProperty {
    /// The finite values this property allows, or `None` when unbounded.
    pub fn finite_values(&self) -> Option<BTreeSet<String>> {
        if let Some(constant) = &self.constant {
            return Some([constant.clone()].into_iter().collect());
        }
        if self.enumerated.is_empty() {
            None
        } else {
            Some(self.enumerated.iter().cloned().collect())
        }
    }
}

impl OutputSchema {
    /// The finite values of one property, or `None` when the property is
    /// absent or unbounded.
    pub fn finite_values(&self, property: &str) -> Option<BTreeSet<String>> {
        self.properties.get(property)?.finite_values()
    }

    /// True when the schema declares the property.
    pub fn has_property(&self, property: &str) -> bool {
        self.properties.contains_key(property)
    }
}

impl Workflow {
    /// The declared input with that name.
    pub fn input(&self, name: &str) -> Option<&WorkflowInput> {
        self.inputs.iter().find(|input| input.name == name)
    }

    /// The step with that id.
    pub fn step(&self, id: &str) -> Option<&Step> {
        self.steps.iter().find(|step| step.id == id)
    }

    /// The index of the step with that id.
    pub fn step_index(&self, id: &str) -> Option<usize> {
        self.steps.iter().position(|step| step.id == id)
    }

    /// The workflow's maximum runtime, applying the documented default.
    pub fn runtime_seconds(&self) -> u64 {
        self.max_runtime_seconds
            .unwrap_or(DEFAULT_WORKFLOW_RUNTIME_SECONDS)
    }

    /// The finite values a step's output property allows, if finite.
    pub fn output_property_values(
        &self,
        step_id: &str,
        property: &str,
    ) -> Option<BTreeSet<String>> {
        self.step(step_id)?.output.as_ref()?.finite_values(property)
    }

    /// Effective deadlines for one step after `elapsed_seconds`.
    ///
    /// The workflow's remaining time caps both the step's maximum runtime and
    /// its idle timeout. The idle timeout additionally never exceeds the
    /// step's maximum runtime, because idling past the step deadline cannot
    /// extend the step.
    pub fn effective_deadlines(&self, step: &Step, elapsed_seconds: u64) -> EffectiveDeadlines {
        let remaining = self.runtime_seconds().saturating_sub(elapsed_seconds);
        let max_runtime_seconds = step
            .max_runtime_seconds
            .map_or(remaining, |declared| declared.min(remaining));
        let idle_timeout_seconds = step
            .idle_timeout_seconds
            .map(|declared| declared.min(remaining).min(max_runtime_seconds));
        EffectiveDeadlines {
            max_runtime_seconds,
            idle_timeout_seconds,
        }
    }
}

/// Deadlines actually applied to one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveDeadlines {
    /// Maximum runtime for the step, capped by the workflow's remaining time.
    pub max_runtime_seconds: u64,
    /// Idle timeout, capped by the workflow's remaining time and the step's
    /// maximum runtime.
    pub idle_timeout_seconds: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::environment::{FiniteEnvironmentInput, WorktreeMode};
    use crate::services::outputs::OutputDeclaration;
    use serde_json::json;

    fn workflow_json() -> Value {
        json!({
            "name": "triage",
            "trigger": "issue-comment",
            "maxRuntime": 900,
            "inputs": [
                { "name": "route", "choices": ["bug", "docs"] }
            ],
            "steps": [
                {
                    "id": "classify",
                    "environment": "default",
                    "agent": "coder",
                    "prompt": {
                        "parts": [
                            { "text": "Classify the request." },
                            { "partial": "system" }
                        ]
                    },
                    "output": {
                        "properties": {
                            "route": { "enum": ["bug", "docs"] },
                            "kind": { "const": "request" }
                        }
                    },
                    "outputs": [ { "capability": "slack.reply", "maximum": 1 } ],
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
                    "prompt": [
                        { "text": "Use " },
                        { "partial": "system" }
                    ],
                    "if": {
                        "left": { "input": "route" },
                        "op": "in",
                        "right": { "step": "classify", "property": "route" }
                    },
                    "reads": ["classify"]
                }
            ]
        })
    }

    fn workflow() -> Workflow {
        serde_json::from_value(workflow_json()).expect("workflow parses")
    }

    #[test]
    fn a_complete_workflow_parses() {
        let workflow = workflow();
        assert_eq!(workflow.name, "triage");
        assert_eq!(workflow.trigger, "issue-comment");
        assert_eq!(workflow.runtime_seconds(), 900);
        assert_eq!(workflow.steps.len(), 2);
        assert_eq!(workflow.steps[0].id, "classify");
        assert_eq!(
            workflow.steps[0].agent.as_ref().and_then(AgentRef::named),
            Some("coder")
        );
        assert!(matches!(
            &workflow.steps[1].agent,
            Some(AgentRef::Inline(_))
        ));
        assert_eq!(workflow.steps[0].prompt.parts().len(), 2);
        assert_eq!(workflow.steps[1].prompt.parts().len(), 2);
        assert_eq!(
            workflow.steps[0].outputs,
            vec![OutputDeclaration {
                capability: "slack.reply".into(),
                maximum: Some(1),
                required: false,
            }]
        );
    }

    #[test]
    fn prompt_parts_keep_their_order() {
        let prompt = Prompt::Parts(vec![
            PromptPart::Text(TextPart {
                text: "first".into(),
            }),
            PromptPart::Partial(PartialPart {
                partial: "system".into(),
            }),
            PromptPart::Text(TextPart {
                text: "last".into(),
            }),
        ]);
        let kinds: Vec<&str> = prompt
            .parts()
            .iter()
            .map(|part| match part {
                PromptPart::Text(_) => "text",
                PromptPart::Partial(_) => "partial",
            })
            .collect();
        assert_eq!(kinds, vec!["text", "partial", "text"]);
    }

    #[test]
    fn a_dynamic_inline_agent_object_is_kept_for_validation() {
        let mut json = workflow_json();
        json["steps"][1]["agent"] = json!({ "input": "agent_choice" });
        let workflow: Workflow = serde_json::from_value(json).expect("parses for validation");
        let agent = workflow.steps[1].agent.as_ref().expect("agent");
        assert!(agent.is_dynamic(), "dynamic inline object is not static");
        assert!(agent.inline().is_none());
    }

    #[test]
    fn a_static_inline_agent_object_is_complete() {
        let mut json = workflow_json();
        json["steps"][1]["agent"] = json!({
            "provider": "openai",
            "model": "gpt",
            "execution_mode": "plan"
        });
        let workflow: Workflow = serde_json::from_value(json).expect("parses");
        let agent = workflow.steps[1].agent.as_ref().expect("agent");
        let inline = agent.inline().expect("static inline");
        assert_eq!(inline.provider, "openai");
        assert_eq!(inline.model, "gpt");
        assert_eq!(inline.execution_mode, "plan");
    }

    #[test]
    fn finite_schema_properties_report_their_values() {
        let workflow = workflow();
        let schema = workflow.steps[0].output.as_ref().expect("schema");
        assert_eq!(
            schema.finite_values("route"),
            Some(
                ["bug".to_string(), "docs".to_string()]
                    .into_iter()
                    .collect()
            )
        );
        assert_eq!(
            workflow.output_property_values("classify", "kind"),
            Some(["request".to_string()].into_iter().collect())
        );
        assert_eq!(schema.finite_values("absent"), None);
    }

    #[test]
    fn an_unbounded_schema_property_is_not_finite() {
        let schema: OutputSchema = serde_json::from_value(json!({
            "properties": { "route": { "type": "string" } }
        }))
        .expect("schema");
        assert_eq!(schema.finite_values("route"), None);
    }

    #[test]
    fn deadline_arithmetic_caps_both_step_values_by_remaining_time() {
        let workflow = workflow();
        let step = &workflow.steps[0];
        assert_eq!(
            workflow.effective_deadlines(step, 0),
            EffectiveDeadlines {
                max_runtime_seconds: 300,
                idle_timeout_seconds: Some(60),
            }
        );
        // Only 9 minutes of the 15-minute workflow remain: both are capped.
        assert_eq!(
            workflow.effective_deadlines(step, 800),
            EffectiveDeadlines {
                max_runtime_seconds: 100,
                idle_timeout_seconds: Some(60),
            }
        );
        // Only 30 seconds remain: the idle timeout is capped too.
        assert_eq!(
            workflow.effective_deadlines(step, 870),
            EffectiveDeadlines {
                max_runtime_seconds: 30,
                idle_timeout_seconds: Some(30),
            }
        );
    }

    #[test]
    fn an_undeclared_step_runtime_uses_the_remaining_workflow_time() {
        let workflow = workflow();
        let step = &workflow.steps[1];
        assert_eq!(
            workflow.effective_deadlines(step, 0).max_runtime_seconds,
            900
        );
        assert_eq!(
            workflow.effective_deadlines(step, 400).max_runtime_seconds,
            500
        );
    }

    #[test]
    fn conditions_expose_their_references() {
        let workflow = workflow();
        let condition = workflow.steps[1].condition.as_ref().expect("condition");
        assert_eq!(
            condition.referenced_steps(),
            ["classify"].into_iter().collect()
        );
        assert_eq!(
            condition.referenced_inputs(),
            ["route"].into_iter().collect()
        );
        let predicate = condition.predicates()[0];
        let (input, step) = predicate.finite_choice_bound().expect("bound");
        assert_eq!(input, "route");
        assert_eq!(step.step, "classify");
        assert_eq!(step.property.as_deref(), Some("route"));
    }

    #[test]
    fn prompt_text_step_references_are_found() {
        let prompt =
            Prompt::Text("Read ${{ steps.classify.output }} then ${{ range(1, 3) }}.".into());
        assert_eq!(
            prompt.referenced_steps(),
            ["classify"].into_iter().collect()
        );
        assert!(step_references("no expressions").is_empty());
    }

    #[test]
    fn environment_selector_supports_a_finite_input() {
        let selector = EnvironmentSelector::FiniteInput(FiniteEnvironmentInput {
            input: "env".into(),
            choices: vec!["default".into(), "review".into()],
        });
        assert!(matches!(selector, EnvironmentSelector::FiniteInput(_)));
    }

    #[test]
    fn worktree_mode_stays_out_of_a_workflow_step() {
        // A workflow names an environment; it never carries worktree details.
        let mut json = workflow_json();
        json["steps"][0]["worktree"] = json!({ "mode": WorktreeMode::BranchOff });
        assert!(
            serde_json::from_value::<Workflow>(json).is_err(),
            "unknown step keys are refused"
        );
    }
}
