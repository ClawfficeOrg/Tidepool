//! Declared step output capabilities (tasks 1.3.3 and 1.3.4).
//!
//! A workflow step declares the outputs it may emit. Each declaration names a
//! registered capability, a `maximum` number of emissions, and whether it is
//! `required`:
//!
//! ```text
//! outputs:
//!   - capability: slack.reply
//!     maximum: 2
//!     required: true
//! ```
//!
//! Three rules follow from a declaration, and [`OutputLedger`] is where they
//! are enforced for one step execution:
//!
//! 1. A capability the step did not declare cannot be emitted at all.
//! 2. A capability cannot be emitted more than `maximum` times.
//! 3. A step that declared a `required` capability cannot finish until it has
//!    emitted it at least once — [`OutputLedger::finish`] fails otherwise, so
//!    the run is recorded as failed rather than silently complete.
//!
//! Capabilities are registered centrally in [`CAPABILITIES`], so a provider
//! task adds one entry (`slack.reply`, `discord.reply`) and configuration
//! validation picks it up with no further change. Every provider reply shares
//! the same `maximum`/`required` semantics, enforced by the one ledger: the
//! only provider-specific part is the payload each reply renders.

use crate::entities::connection::ProviderKind;
use crate::services::connections::discord::DiscordReply;
use crate::services::connections::slack::SlackReply;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;

/// The `slack.reply` capability name.
pub const SLACK_REPLY: &str = "slack.reply";

/// The `discord.reply` capability name.
pub const DISCORD_REPLY: &str = "discord.reply";

/// One capability a step may declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputCapabilitySpec {
    /// Capability name as configuration spells it.
    pub name: &'static str,
    /// Provider the capability belongs to.
    pub provider: ProviderKind,
    /// `maximum` used when a declaration omits one.
    pub default_maximum: u32,
    /// Largest `maximum` a declaration may ask for.
    pub hard_maximum: u32,
    /// One-line description for the operator surfaces.
    pub summary: &'static str,
}

/// Every registered output capability.
pub const CAPABILITIES: &[OutputCapabilitySpec] = &[
    OutputCapabilitySpec {
        name: SLACK_REPLY,
        provider: ProviderKind::Slack,
        default_maximum: 1,
        hard_maximum: 20,
        summary: "Post a message back to the channel or thread the event came from",
    },
    OutputCapabilitySpec {
        name: DISCORD_REPLY,
        provider: ProviderKind::Discord,
        default_maximum: 1,
        hard_maximum: 20,
        summary: "Reply in the channel or thread the Discord event came from",
    },
];

/// Look up a capability by name.
pub fn capability(name: &str) -> Option<&'static OutputCapabilitySpec> {
    CAPABILITIES.iter().find(|spec| spec.name == name)
}

/// Domain errors for declared outputs.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum OutputError {
    /// The step declared a capability that is not registered.
    #[error("unknown output capability '{0}'")]
    UnknownCapability(String),
    /// The same capability was declared twice.
    #[error("output capability '{0}' is declared more than once")]
    DuplicateDeclaration(String),
    /// `maximum` is zero or above the capability's hard maximum.
    #[error("output capability '{capability}' allows a maximum of 1-{hard_maximum}, got {maximum}")]
    InvalidMaximum {
        /// Capability name.
        capability: String,
        /// Requested maximum.
        maximum: u32,
        /// Largest allowed maximum.
        hard_maximum: u32,
    },
    /// The step emitted a capability it did not declare.
    #[error("output capability '{0}' was not declared by this step")]
    NotDeclared(String),
    /// The step emitted more than `maximum` outputs.
    #[error("output capability '{capability}' is limited to {maximum} per step")]
    MaximumExceeded {
        /// Capability name.
        capability: String,
        /// Declared maximum.
        maximum: u32,
    },
    /// The step tried to finish without emitting a required output.
    #[error("output capability '{0}' is required and was not emitted")]
    RequiredMissing(String),
}

/// A step's declaration of one output capability, as authored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputDeclaration {
    /// Registered capability name.
    pub capability: String,
    /// How many times the step may emit it; the capability's default when
    /// omitted.
    #[serde(default)]
    pub maximum: Option<u32>,
    /// When true the step cannot finish without emitting it.
    #[serde(default)]
    pub required: bool,
}

impl OutputDeclaration {
    /// Declare a capability with its default maximum, not required.
    pub fn new(capability: impl Into<String>) -> Self {
        Self {
            capability: capability.into(),
            maximum: None,
            required: false,
        }
    }

    /// Set the maximum number of emissions.
    pub fn with_maximum(mut self, maximum: u32) -> Self {
        self.maximum = Some(maximum);
        self
    }

    /// Mark the capability required.
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Resolve against the registry, applying the default maximum and
    /// refusing an unknown capability or an out-of-range maximum.
    pub fn resolve(&self) -> Result<ResolvedOutput, OutputError> {
        let spec = capability(&self.capability)
            .ok_or_else(|| OutputError::UnknownCapability(self.capability.clone()))?;
        let maximum = self.maximum.unwrap_or(spec.default_maximum);
        if maximum == 0 || maximum > spec.hard_maximum {
            return Err(OutputError::InvalidMaximum {
                capability: spec.name.to_string(),
                maximum,
                hard_maximum: spec.hard_maximum,
            });
        }
        Ok(ResolvedOutput {
            spec,
            maximum,
            required: self.required,
        })
    }
}

/// A declaration checked against the registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedOutput {
    /// Registry entry.
    pub spec: &'static OutputCapabilitySpec,
    /// Effective maximum.
    pub maximum: u32,
    /// Whether the step must emit it.
    pub required: bool,
}

/// One output the step emitted.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EmittedOutput {
    /// Capability it was emitted through.
    pub capability: &'static str,
    /// Provider payload, ready for the provider call.
    pub payload: Value,
}

/// What a finished step emitted.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutputSummary {
    /// Outputs in emission order.
    pub outputs: Vec<EmittedOutput>,
}

impl OutputSummary {
    /// Count of outputs emitted through one capability.
    pub fn count(&self, capability: &str) -> usize {
        self.outputs
            .iter()
            .filter(|output| output.capability == capability)
            .count()
    }
}

/// Enforces one step's declared outputs for the life of that step.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputLedger {
    declarations: Vec<ResolvedOutput>,
    counts: BTreeMap<&'static str, u32>,
    emitted: Vec<EmittedOutput>,
}

impl OutputLedger {
    /// Build a ledger from a step's declarations.
    ///
    /// Fails on an unknown capability, a duplicate declaration or a maximum
    /// outside the capability's range, so a bad declaration is refused when
    /// configuration is validated rather than mid-run.
    pub fn new(declarations: &[OutputDeclaration]) -> Result<Self, OutputError> {
        let mut resolved: Vec<ResolvedOutput> = Vec::with_capacity(declarations.len());
        for declaration in declarations {
            let entry = declaration.resolve()?;
            if resolved
                .iter()
                .any(|other| other.spec.name == entry.spec.name)
            {
                return Err(OutputError::DuplicateDeclaration(
                    entry.spec.name.to_string(),
                ));
            }
            resolved.push(entry);
        }
        Ok(Self {
            declarations: resolved,
            counts: BTreeMap::new(),
            emitted: Vec::new(),
        })
    }

    /// The step's resolved declarations.
    pub fn declarations(&self) -> &[ResolvedOutput] {
        &self.declarations
    }

    /// True when the step declared this capability.
    pub fn declares(&self, capability: &str) -> bool {
        self.declarations
            .iter()
            .any(|entry| entry.spec.name == capability)
    }

    /// How many more times a capability may be emitted.
    pub fn remaining(&self, capability: &str) -> u32 {
        self.declarations
            .iter()
            .find(|entry| entry.spec.name == capability)
            .map(|entry| {
                entry
                    .maximum
                    .saturating_sub(self.counts.get(entry.spec.name).copied().unwrap_or(0))
            })
            .unwrap_or(0)
    }

    /// How many times a capability has been emitted.
    pub fn count(&self, capability: &str) -> u32 {
        self.counts.get(capability).copied().unwrap_or(0)
    }

    /// Emit one output through a declared capability.
    pub fn emit(&mut self, capability: &str, payload: Value) -> Result<(), OutputError> {
        let entry = self
            .declarations
            .iter()
            .find(|entry| entry.spec.name == capability)
            .ok_or_else(|| OutputError::NotDeclared(capability.to_string()))?;
        let used = self.counts.entry(entry.spec.name).or_insert(0);
        if *used >= entry.maximum {
            return Err(OutputError::MaximumExceeded {
                capability: entry.spec.name.to_string(),
                maximum: entry.maximum,
            });
        }
        *used += 1;
        self.emitted.push(EmittedOutput {
            capability: entry.spec.name,
            payload,
        });
        Ok(())
    }

    /// Emit a Slack reply through `slack.reply`.
    ///
    /// The reply is already addressed to the event it answers (see
    /// [`SlackReply`]), so the payload is the `chat.postMessage` body.
    pub fn emit_slack_reply(&mut self, reply: &SlackReply) -> Result<(), OutputError> {
        self.emit(SLACK_REPLY, reply.post_message_body())
    }

    /// Emit a Discord reply through `discord.reply`.
    ///
    /// The reply is already addressed to the event it answers (see
    /// [`DiscordReply`]), so the payload is the `POST /channels/{id}/messages`
    /// body; the channel is the thread when the event came from one.
    pub fn emit_discord_reply(&mut self, reply: &DiscordReply) -> Result<(), OutputError> {
        self.emit(DISCORD_REPLY, reply.create_message_body())
    }

    /// Finish the step.
    ///
    /// Fails when a `required` capability was never emitted, which is how a
    /// step that must answer cannot complete silently.
    pub fn finish(self) -> Result<OutputSummary, OutputError> {
        for entry in &self.declarations {
            if entry.required && self.counts.get(entry.spec.name).copied().unwrap_or(0) == 0 {
                return Err(OutputError::RequiredMissing(entry.spec.name.to_string()));
            }
        }
        Ok(OutputSummary {
            outputs: self.emitted,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::connections::discord::{DiscordIngestContext, DiscordMessageEvent};
    use crate::services::connections::slack::{
        SlackIngestContext, SlackMessageEvent, normalize_event,
    };
    use serde_json::json;

    const DISCORD_BOT: &str = "900000000000000001";

    fn discord_message() -> DiscordMessageEvent {
        let context = DiscordIngestContext::new("conn-discord").with_bot_user_id(DISCORD_BOT);
        let event = crate::services::connections::discord::normalize_event(
            &context,
            "MESSAGE_CREATE",
            &json!({
                "id": "300000000000000001",
                "channel_id": "200000000000000001",
                "guild_id": "100000000000000001",
                "author": {"id": "400000000000000001", "bot": false},
                "content": format!("<@{DISCORD_BOT}> status"),
                "mentions": [{"id": DISCORD_BOT}],
            }),
        )
        .expect("normalize");
        event.message().expect("message").clone()
    }

    fn message() -> SlackMessageEvent {
        let context = SlackIngestContext::new("conn-1", "secret")
            .expect("context")
            .with_bot_user_id("U0BOT");
        let event = normalize_event(
            &context,
            "T1",
            &json!({
                "type": "app_mention", "user": "U2", "text": "<@U0BOT> status",
                "channel": "C1", "ts": "1700000000.000100"
            }),
        )
        .expect("normalize");
        event.message().expect("message").clone()
    }

    #[test]
    fn slack_reply_is_registered() {
        let spec = capability(SLACK_REPLY).expect("registered");
        assert_eq!(spec.provider, ProviderKind::Slack);
        assert_eq!(spec.default_maximum, 1);
        assert!(capability("slack.shout").is_none());
    }

    #[test]
    fn declarations_resolve_defaults_and_refuse_bad_input() {
        let resolved = OutputDeclaration::new(SLACK_REPLY)
            .resolve()
            .expect("resolve");
        assert_eq!(resolved.maximum, 1);
        assert!(!resolved.required);

        assert_eq!(
            OutputDeclaration::new("slack.shout").resolve(),
            Err(OutputError::UnknownCapability("slack.shout".into()))
        );
        assert_eq!(
            OutputDeclaration::new(SLACK_REPLY)
                .with_maximum(0)
                .resolve(),
            Err(OutputError::InvalidMaximum {
                capability: SLACK_REPLY.into(),
                maximum: 0,
                hard_maximum: 20,
            })
        );
        assert!(
            OutputDeclaration::new(SLACK_REPLY)
                .with_maximum(21)
                .resolve()
                .is_err()
        );
        assert_eq!(
            OutputLedger::new(&[
                OutputDeclaration::new(SLACK_REPLY),
                OutputDeclaration::new(SLACK_REPLY),
            ]),
            Err(OutputError::DuplicateDeclaration(SLACK_REPLY.into()))
        );
    }

    #[test]
    fn declarations_parse_from_configuration() {
        let declarations: Vec<OutputDeclaration> = serde_json::from_value(json!([
            {"capability": "slack.reply", "maximum": 2, "required": true},
            {"capability": "slack.reply"}
        ]))
        .expect("parse");
        assert_eq!(declarations[0].maximum, Some(2));
        assert!(declarations[0].required);
        assert_eq!(declarations[1].maximum, None);
        assert!(!declarations[1].required);
    }

    #[test]
    fn an_undeclared_capability_cannot_be_emitted() {
        let mut ledger = OutputLedger::new(&[]).expect("ledger");
        let reply = crate::services::connections::slack::SlackReply::to_event(&message(), "hi")
            .expect("reply");
        assert_eq!(
            ledger.emit_slack_reply(&reply),
            Err(OutputError::NotDeclared(SLACK_REPLY.into()))
        );
        assert!(!ledger.declares(SLACK_REPLY));
        assert_eq!(ledger.remaining(SLACK_REPLY), 0);
        assert!(ledger.finish().expect("finish").outputs.is_empty());
    }

    #[test]
    fn the_maximum_is_enforced() {
        let mut ledger = OutputLedger::new(&[OutputDeclaration::new(SLACK_REPLY).with_maximum(2)])
            .expect("ledger");
        let reply = crate::services::connections::slack::SlackReply::to_event(&message(), "one")
            .expect("reply");
        assert_eq!(ledger.remaining(SLACK_REPLY), 2);
        ledger.emit_slack_reply(&reply).expect("first");
        ledger.emit_slack_reply(&reply).expect("second");
        assert_eq!(ledger.remaining(SLACK_REPLY), 0);
        assert_eq!(
            ledger.emit_slack_reply(&reply),
            Err(OutputError::MaximumExceeded {
                capability: SLACK_REPLY.into(),
                maximum: 2
            })
        );
        let summary = ledger.finish().expect("finish");
        assert_eq!(summary.count(SLACK_REPLY), 2);
    }

    #[test]
    fn a_required_reply_must_be_emitted_before_the_step_finishes() {
        let declarations = vec![OutputDeclaration::new(SLACK_REPLY).required()];
        let ledger = OutputLedger::new(&declarations).expect("ledger");
        assert_eq!(
            ledger.finish(),
            Err(OutputError::RequiredMissing(SLACK_REPLY.into()))
        );

        let mut ledger = OutputLedger::new(&declarations).expect("ledger");
        let message = message();
        let reply = crate::services::connections::slack::SlackReply::to_event(&message, "done")
            .expect("reply");
        ledger.emit_slack_reply(&reply).expect("emit");
        let summary = ledger.finish().expect("finish");
        assert_eq!(summary.outputs.len(), 1);
        assert_eq!(summary.outputs[0].capability, SLACK_REPLY);
        assert_eq!(
            summary.outputs[0].payload,
            json!({
                "channel": "C1",
                "thread_ts": "1700000000.000100",
                "text": "done"
            })
        );
    }

    #[test]
    fn discord_reply_is_registered_with_the_same_semantics_as_slack() {
        let discord = capability(DISCORD_REPLY).expect("registered");
        let slack = capability(SLACK_REPLY).expect("registered");
        assert_eq!(discord.provider, ProviderKind::Discord);
        assert_eq!(discord.default_maximum, slack.default_maximum);
        assert_eq!(discord.hard_maximum, slack.hard_maximum);
        assert!(capability("discord.shout").is_none());

        let declarations: Vec<OutputDeclaration> = serde_json::from_value(json!([
            {"capability": "discord.reply", "maximum": 3, "required": true}
        ]))
        .expect("parse");
        let resolved = declarations[0].resolve().expect("resolve");
        assert_eq!(resolved.maximum, 3);
        assert!(resolved.required);
        assert!(
            OutputDeclaration::new(DISCORD_REPLY)
                .with_maximum(discord.hard_maximum + 1)
                .resolve()
                .is_err()
        );
    }

    #[test]
    fn a_required_discord_reply_is_enforced_like_a_slack_one() {
        let declarations = vec![OutputDeclaration::new(DISCORD_REPLY).required()];
        let ledger = OutputLedger::new(&declarations).expect("ledger");
        assert_eq!(
            ledger.finish(),
            Err(OutputError::RequiredMissing(DISCORD_REPLY.into()))
        );

        let message = discord_message();
        let reply = crate::services::connections::discord::DiscordReply::to_event(&message, "done")
            .expect("reply");

        // Undeclared is refused outright.
        let mut empty = OutputLedger::new(&[]).expect("ledger");
        assert_eq!(
            empty.emit_discord_reply(&reply),
            Err(OutputError::NotDeclared(DISCORD_REPLY.into()))
        );

        let mut ledger = OutputLedger::new(&declarations).expect("ledger");
        ledger.emit_discord_reply(&reply).expect("emit");
        // The default maximum is one, so a second reply is refused.
        assert_eq!(
            ledger.emit_discord_reply(&reply),
            Err(OutputError::MaximumExceeded {
                capability: DISCORD_REPLY.into(),
                maximum: 1
            })
        );
        let summary = ledger.finish().expect("finish");
        assert_eq!(summary.count(DISCORD_REPLY), 1);
        assert_eq!(summary.outputs[0].capability, DISCORD_REPLY);
        assert_eq!(
            summary.outputs[0].payload["message_reference"]["message_id"],
            json!("300000000000000001")
        );
    }

    #[test]
    fn the_two_reply_capabilities_are_counted_separately() {
        let mut ledger = OutputLedger::new(&[
            OutputDeclaration::new(SLACK_REPLY),
            OutputDeclaration::new(DISCORD_REPLY),
        ])
        .expect("ledger");
        let slack_message = message();
        let slack_reply =
            crate::services::connections::slack::SlackReply::to_event(&slack_message, "slack")
                .expect("reply");
        let discord_message = discord_message();
        let discord_reply = crate::services::connections::discord::DiscordReply::to_event(
            &discord_message,
            "discord",
        )
        .expect("reply");

        ledger.emit_slack_reply(&slack_reply).expect("slack");
        ledger.emit_discord_reply(&discord_reply).expect("discord");
        assert_eq!(ledger.count(SLACK_REPLY), 1);
        assert_eq!(ledger.count(DISCORD_REPLY), 1);
        assert_eq!(ledger.remaining(SLACK_REPLY), 0);
        let summary = ledger.finish().expect("finish");
        assert_eq!(summary.outputs.len(), 2);
    }

    #[test]
    fn an_optional_declaration_lets_the_step_finish_silently() {
        let ledger = OutputLedger::new(&[OutputDeclaration::new(SLACK_REPLY)]).expect("ledger");
        assert!(ledger.declares(SLACK_REPLY));
        assert_eq!(ledger.count(SLACK_REPLY), 0);
        assert!(ledger.finish().expect("finish").outputs.is_empty());
    }
}
