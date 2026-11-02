//! The runs tab: the activity feed, the live run detail, the manual dispatch
//! form and the read-only trigger list.
//!
//! The tab owns no network code. [`RunsClient`] is the whole surface it needs,
//! and [`RunsState::drive`] performs one feed refresh, one live detail poll and
//! the pending dispatch. The tests script that trait, so the filter combination,
//! output following, actor refusal and failure-code wording are all exercised
//! without a pool.
//!
//! # Filters combine
//!
//! Project, provider, trigger and status narrow one union of receipts, runs,
//! steps and executions. Every active filter must match, and the tab also
//! filters the page it receives before rendering it, so a pool that ignored a
//! filter (or a stale page that arrived after the filter changed) can never show
//! another project's runs.
//!
//! # A running execution follows itself
//!
//! A detail that is still pending or running is read again on a timer until it
//! reaches a terminal state, so the console updates without a key press. Once
//! every step and the run itself are terminal, polling stops.
//!
//! # Failure codes are sentences
//!
//! The pool stores a stable code with a step failure
//! (`daemon_not_connected: host is offline`). The screen shows a sentence a
//! person understands and keeps the code beside it for a bug report.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tidepool_client::endpoints::GetV1ActivityQuery;
use tidepool_client::types::{
    ActivityEntry, ActivityPage, ActivityRunDetail, ManualDispatchRequest, ManualDispatchResponse,
    ProjectView, RevisionView, RunStepView, RunView,
};

use crate::app::Result;

/// How often the activity feed is read again.
pub const FEED_INTERVAL: Duration = Duration::from_secs(5);

/// How often a live run detail is read again.
pub const LIVE_INTERVAL: Duration = Duration::from_secs(1);

/// How many trailing output lines one step shows.
pub const TAIL_LINES: usize = 8;

/// Entries asked of one activity page.
pub const ACTIVITY_PAGE_SIZE: u64 = 50;

/// Every status an activity entry can carry.
pub const ACTIVITY_STATUSES: [&str; 12] = [
    "pending",
    "running",
    "succeeded",
    "failed",
    "skipped",
    "interrupted",
    "received",
    "matched",
    "unmatched",
    "rejected",
    "queued",
    "cancelled",
];

/// The providers a receipt or run can name.
pub const ACTIVITY_PROVIDERS: [&str; 3] = ["github", "slack", "discord"];

/// What the runs tab needs from a pool.
pub trait RunsClient {
    /// List the organization's projects, for the project filter.
    fn list_projects(&mut self) -> Result<Vec<ProjectView>>;
    /// Read one page of the combined activity feed.
    fn list_activity(&mut self, query: GetV1ActivityQuery) -> Result<ActivityPage>;
    /// Read one run plus the arrival that started it.
    fn run_detail(&mut self, run: &str) -> Result<ActivityRunDetail>;
    /// Start a workflow run from a manual request.
    fn dispatch(&mut self, request: ManualDispatchRequest) -> Result<ManualDispatchResponse>;
    /// Read the triggers authored in one project's active revision.
    fn list_triggers(&mut self, project: &str) -> Result<Vec<TriggerSummary>>;
}

/// The four combined filters. An empty string means "any".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    project: String,
    provider: String,
    trigger: String,
    status: String,
}

impl Filters {
    /// The selected project id, or an empty string for every project.
    pub fn project(&self) -> &str {
        &self.project
    }

    /// The selected provider, lowercase.
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// The selected status, lowercase.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Whether at least one filter narrows the feed.
    pub fn is_active(&self) -> bool {
        !self.project.is_empty()
            || !self.provider.is_empty()
            || !self.trigger.is_empty()
            || !self.status.is_empty()
    }

    /// Drop every filter.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The query the pool receives: only non-empty filters are sent.
    pub fn query(&self) -> GetV1ActivityQuery {
        GetV1ActivityQuery {
            project: non_empty(&self.project),
            provider: non_empty(&self.provider),
            trigger: non_empty(&self.trigger),
            status: non_empty(&self.status),
            limit: Some(ACTIVITY_PAGE_SIZE),
            ..GetV1ActivityQuery::default()
        }
    }

    /// True when one entry satisfies every active filter.
    pub fn matches(&self, entry: &ActivityEntry) -> bool {
        if !self.project.is_empty() && entry.project.as_deref() != Some(self.project.as_str()) {
            return false;
        }
        if !self.provider.is_empty() && entry.provider.as_deref() != Some(self.provider.as_str()) {
            return false;
        }
        if !self.trigger.is_empty() {
            let id_matches = entry.trigger.as_deref() == Some(self.trigger.as_str());
            let name_matches = entry.trigger_name.as_deref() == Some(self.trigger.as_str());
            if !id_matches && !name_matches {
                return false;
            }
        }
        if !self.status.is_empty() && entry.status != self.status {
            return false;
        }
        true
    }

    /// The filter combination as one readable line.
    pub fn summary(&self) -> String {
        format!(
            "project {}  provider {}  trigger {}  status {}",
            or_all(&self.project, "all"),
            if self.provider.is_empty() {
                "all".to_owned()
            } else {
                provider_word(self.provider()).to_lowercase()
            },
            or_all(&self.trigger, "all"),
            if self.status.is_empty() {
                "any".to_owned()
            } else {
                status_word(self.status())
            }
        )
    }

    /// Move the project filter to the next project, wrapping through "all".
    pub fn cycle_project(&mut self, projects: &[ProjectView]) {
        let mut values = vec![String::new()];
        values.extend(projects.iter().map(|project| project.id.clone()));
        self.project = next_in(&values, &self.project);
    }

    /// Move the provider filter to the next provider.
    pub fn cycle_provider(&mut self) {
        let mut values = vec![String::new()];
        values.extend(ACTIVITY_PROVIDERS.iter().map(|value| (*value).to_owned()));
        self.provider = next_in(&values, &self.provider);
    }

    /// Move the trigger filter to the next authored trigger.
    pub fn cycle_trigger(&mut self, triggers: &[TriggerSummary]) {
        let mut values = vec![String::new()];
        values.extend(triggers.iter().map(|trigger| trigger.name.clone()));
        self.trigger = next_in(&values, &self.trigger);
    }

    /// Move the status filter to the next status.
    pub fn cycle_status(&mut self) {
        let mut values = vec![String::new()];
        values.extend(ACTIVITY_STATUSES.iter().map(|value| (*value).to_owned()));
        self.status = next_in(&values, &self.status);
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn or_all(value: &str, all: &str) -> String {
    if value.is_empty() {
        all.to_owned()
    } else {
        value.to_owned()
    }
}

fn next_in(values: &[String], current: &str) -> String {
    let index = values
        .iter()
        .position(|value| value == current)
        .unwrap_or_default();
    values[(index + 1) % values.len()].clone()
}

/// The provider's own name.
pub fn provider_word(provider: &str) -> String {
    match provider {
        "github" => "GitHub".to_owned(),
        "slack" => "Slack".to_owned(),
        "discord" => "Discord".to_owned(),
        "" => "Unknown provider".to_owned(),
        other => other.to_owned(),
    }
}

/// The words a status is rendered with.
pub fn status_word(status: &str) -> String {
    match status {
        "pending" => "Waiting to start".to_owned(),
        "queued" => "Queued".to_owned(),
        "running" => "Running".to_owned(),
        "succeeded" => "Succeeded".to_owned(),
        "failed" => "Failed".to_owned(),
        "skipped" => "Skipped".to_owned(),
        "interrupted" => "Interrupted".to_owned(),
        "received" => "Received".to_owned(),
        "matched" => "Matched".to_owned(),
        "unmatched" => "Matched nothing".to_owned(),
        "rejected" => "Rejected".to_owned(),
        "cancelled" => "Cancelled".to_owned(),
        "" => "Unknown".to_owned(),
        other => other.to_owned(),
    }
}

/// The label for one record kind.
pub fn kind_word(kind: &str) -> String {
    match kind {
        "receipt" => "Arrival".to_owned(),
        "run" => "Run".to_owned(),
        "step" => "Step".to_owned(),
        "execution" => "Execution".to_owned(),
        "" => "Record".to_owned(),
        other => other.to_owned(),
    }
}

/// A short heading for one activity entry.
pub fn entry_title(entry: &ActivityEntry) -> String {
    match entry.kind.as_str() {
        "receipt" => {
            let event = entry.event_type.as_deref().unwrap_or("delivery").trim();
            let event = if event.is_empty() { "delivery" } else { event };
            format!(
                "{} {event}",
                provider_word(entry.provider.as_deref().unwrap_or(""))
            )
        }
        "run" => match entry.trigger_name.as_deref() {
            Some(name) if !name.trim().is_empty() => format!("Run of '{}'", name.trim()),
            _ => "A run".to_owned(),
        },
        "step" => match entry.step.as_deref() {
            Some(step) if !step.trim().is_empty() => format!("Step '{}'", step.trim()),
            _ => "A step".to_owned(),
        },
        "execution" => "Agent execution".to_owned(),
        other => kind_word(other),
    }
}

/// The run an activity entry can open, or `None` when it names none.
pub fn entry_run_id(entry: &ActivityEntry) -> Option<&str> {
    match entry.run.as_deref() {
        Some(run) if !run.trim().is_empty() => Some(run),
        _ if entry.kind == "run" => Some(entry.id.as_str()),
        _ => None,
    }
}

/// The step's output exactly as the agent wrote it.
pub fn step_output(step: &RunStepView) -> &str {
    step.output.as_deref().unwrap_or("")
}

/// The trailing lines of one step's output, newest last.
pub fn output_tail(output: Option<&str>, max_lines: usize) -> Vec<String> {
    let text = output.unwrap_or("");
    if text.is_empty() || max_lines == 0 {
        return Vec::new();
    }
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..]
        .iter()
        .map(|line| (*line).to_owned())
        .collect()
}

fn live_status(status: &str) -> bool {
    status == "pending" || status == "running"
}

/// True when a run detail still needs following.
///
/// A run that says `succeeded` but still has a running step is live too: the run
/// status is written when the last step finishes, and stopping on the run word
/// alone would miss that step's output.
pub fn run_is_live(run: &RunView) -> bool {
    live_status(&run.status) || run.steps.iter().any(|step| live_status(&step.status))
}

/// One named failure: the sentence a person reads and the code they can quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureExplanation {
    /// The stable pool code, when one was recorded.
    pub code: Option<String>,
    /// A sentence that says what happened, in a person's words.
    pub sentence: String,
    /// The pool's own remaining detail, kept underneath.
    pub detail: Option<String>,
}

/// The code-to-sentence table, mirroring the pool's named failures.
pub const FAILURE_SENTENCES: [(&str, &str); 6] = [
    (
        "daemon_not_connected",
        "The host went offline before it could run this step. Start the host, then dispatch the run again.",
    ),
    (
        "host_not_found",
        "The host this step names is no longer enrolled with the pool, so the step could not start.",
    ),
    (
        "execution_failed",
        "The agent on the host reported that this step failed.",
    ),
    (
        "interrupted",
        "This step was interrupted before it finished, so its work is incomplete.",
    ),
    (
        "invalid_output",
        "The step finished, but its output was not the JSON the workflow declares for it.",
    ),
    (
        "step_deadline",
        "The step ran past its deadline and was stopped.",
    ),
];

/// The sentence for a named failure code, when the table has one.
pub fn failure_sentence(code: &str) -> Option<&'static str> {
    FAILURE_SENTENCES
        .iter()
        .find(|(candidate, _)| *candidate == code)
        .map(|(_, sentence)| *sentence)
}

fn valid_code(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_lowercase()
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Split a stored `code: detail` error into its leading code and the rest.
pub fn split_failure_code(error_text: &str) -> (Option<String>, String) {
    let text = error_text.trim();
    if let Some((prefix, rest)) = text.split_once(':')
        && valid_code(prefix.trim())
    {
        return (Some(prefix.trim().to_owned()), rest.trim().to_owned());
    }
    (None, text.to_owned())
}

/// Explain one stored step failure.
///
/// The error text wins because it is the pool's record of the failure itself;
/// `reason` is used when the error has no leading code. An unknown code is
/// never hidden: its text stays on screen and the code stays beside it.
pub fn failure_explanation(
    error_text: Option<&str>,
    reason: Option<&str>,
) -> Option<FailureExplanation> {
    let raw_text = error_text.unwrap_or("").trim();
    let trimmed_reason = reason.unwrap_or("").trim();
    let (error_code, error_detail) = split_failure_code(raw_text);

    if let Some(code) = error_code {
        if let Some(sentence) = failure_sentence(&code) {
            return Some(FailureExplanation {
                code: Some(code),
                sentence: sentence.to_owned(),
                detail: non_empty(&error_detail),
            });
        }
        let sentence = if error_detail.is_empty() {
            format!("The step failed with the pool's '{code}' code.")
        } else {
            format!("The step failed with the pool's '{code}' code: {error_detail}")
        };
        return Some(FailureExplanation {
            code: Some(code),
            sentence,
            detail: None,
        });
    }

    if let Some(sentence) = failure_sentence(trimmed_reason) {
        return Some(FailureExplanation {
            code: Some(trimmed_reason.to_owned()),
            sentence: sentence.to_owned(),
            detail: if raw_text.is_empty() || raw_text == trimmed_reason {
                None
            } else {
                Some(raw_text.to_owned())
            },
        });
    }

    if let Some(sentence) = failure_sentence(raw_text) {
        return Some(FailureExplanation {
            code: Some(raw_text.to_owned()),
            sentence: sentence.to_owned(),
            detail: None,
        });
    }

    if !raw_text.is_empty() {
        return Some(FailureExplanation {
            code: None,
            sentence: format!("The step failed: {raw_text}"),
            detail: None,
        });
    }
    if !trimmed_reason.is_empty() {
        return Some(FailureExplanation {
            code: Some(trimmed_reason.to_owned()),
            sentence: format!("The step stopped with the pool's '{trimmed_reason}' code."),
            detail: None,
        });
    }
    None
}

/// The first failure a person should read for a run, if any.
pub fn run_failure_explanation(run: &RunView) -> Option<FailureExplanation> {
    for step in &run.steps {
        if step.status != "failed" && step.status != "interrupted" {
            continue;
        }
        if let Some(failure) = failure_explanation(step.error.as_deref(), step.reason.as_deref()) {
            return Some(failure);
        }
    }
    match run.status.as_str() {
        "failed" => Some(FailureExplanation {
            code: None,
            sentence: "This run failed, but the pool recorded no step failure detail.".to_owned(),
            detail: None,
        }),
        "interrupted" => Some(FailureExplanation {
            code: None,
            sentence: "This run was interrupted before it finished.".to_owned(),
            detail: None,
        }),
        _ => None,
    }
}

/// One trigger as the read-only list shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TriggerSummary {
    /// Trigger name, unique per project.
    pub name: String,
    /// Event provider (`github`, `slack`, `discord`).
    pub provider: String,
    /// Provider event type.
    pub event: String,
    /// Connection slug the event must arrive through.
    pub connection: String,
    /// Resource kind.
    pub resource_kind: String,
    /// Provider-side resource id.
    pub resource_id: String,
    /// The allowlist of users who may start the run.
    pub from_users: Vec<String>,
    /// Optional channel guard.
    pub channel: Option<String>,
    /// Optional label guard.
    pub label: Option<String>,
    /// Optional substring guard.
    pub contains: Option<String>,
    /// Optional pattern guard.
    pub pattern: Option<String>,
    /// Whether the revision holding this trigger is active and valid.
    pub active: bool,
    /// Why the trigger document could not be read, when it could not.
    pub problem: Option<String>,
}

impl TriggerSummary {
    /// The event as one line, e.g. `GitHub push`.
    pub fn event_line(&self) -> String {
        format!("{} {}", provider_word(&self.provider), self.event)
    }

    /// The filters as one line, in the order the document declares them.
    pub fn filters_line(&self) -> String {
        let mut parts = vec![
            format!("connection {}", or_dash(&self.connection)),
            format!(
                "{} {}",
                or_dash(&self.resource_kind),
                or_dash(&self.resource_id)
            ),
        ];
        if self.from_users.is_empty() {
            parts.push("from nobody".to_owned());
        } else {
            parts.push(format!("from {}", self.from_users.join(", ")));
        }
        for (label, value) in [
            ("channel", self.channel.as_deref()),
            ("label", self.label.as_deref()),
            ("contains", self.contains.as_deref()),
            ("pattern", self.pattern.as_deref()),
        ] {
            if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
                parts.push(format!("{label} {value}"));
            }
        }
        parts.join(" · ")
    }

    /// Whether the trigger's allowlist permits `actor` (case-insensitive).
    pub fn allows(&self, actor: &str) -> bool {
        let actor = actor.trim();
        !actor.is_empty()
            && self
                .from_users
                .iter()
                .any(|allowed| allowed.trim().eq_ignore_ascii_case(actor))
    }
}

fn or_dash(value: &str) -> &str {
    if value.trim().is_empty() { "-" } else { value }
}

#[derive(Debug, Default)]
struct ParsedTrigger {
    name: String,
    provider: String,
    event: String,
    connection: String,
    resource_kind: String,
    resource_id: String,
    from_users: Vec<String>,
    channel: Option<String>,
    label: Option<String>,
    contains: Option<String>,
    pattern: Option<String>,
}

/// Read the triggers authored in one project's active configuration revision.
///
/// The pool exposes no trigger-list route, so the console reads the same
/// authored files the CLI reads: every `triggers/` file, YAML or JSON. A file
/// that does not parse is still listed, with the parse problem beside it.
pub fn triggers_from_revision(revision: &RevisionView) -> Vec<TriggerSummary> {
    let active = revision.active && revision.status == "valid";
    revision
        .files
        .iter()
        .filter(|file| file.path.starts_with("triggers/"))
        .map(|file| {
            let fallback = file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&file.path)
                .split('.')
                .next()
                .unwrap_or(&file.path)
                .to_owned();
            match parse_trigger(&file.content) {
                Ok(mut parsed) => {
                    if parsed.name.trim().is_empty() {
                        parsed.name = fallback;
                    }
                    summary_from(parsed, active, None)
                }
                Err(problem) => TriggerSummary {
                    name: fallback,
                    active: false,
                    problem: Some(problem),
                    ..TriggerSummary::default()
                },
            }
        })
        .collect()
}

fn summary_from(parsed: ParsedTrigger, active: bool, problem: Option<String>) -> TriggerSummary {
    TriggerSummary {
        name: parsed.name,
        provider: parsed.provider,
        event: parsed.event,
        connection: parsed.connection,
        resource_kind: parsed.resource_kind,
        resource_id: parsed.resource_id,
        from_users: parsed.from_users,
        channel: parsed.channel,
        label: parsed.label,
        contains: parsed.contains,
        pattern: parsed.pattern,
        active,
        problem,
    }
}

/// Parse one authored trigger file as JSON when it starts with `{`, as the
/// block YAML the editor writes otherwise.
fn parse_trigger(content: &str) -> std::result::Result<ParsedTrigger, String> {
    if content.trim_start().starts_with('{') {
        return parse_trigger_json(content);
    }
    parse_trigger_yaml(content)
}

fn parse_trigger_json(content: &str) -> std::result::Result<ParsedTrigger, String> {
    let value: serde_json::Value =
        serde_json::from_str(content).map_err(|error| format!("not JSON: {error}"))?;
    let mut parsed = ParsedTrigger {
        name: json_string(&value, "name").unwrap_or_default(),
        ..ParsedTrigger::default()
    };
    if let Some(event) = value.get("event") {
        parsed.provider = json_string(event, "provider").unwrap_or_default();
        parsed.event = json_string(event, "type").unwrap_or_default();
    }
    if let Some(filters) = value.get("filters") {
        parsed.connection = json_string(filters, "connection").unwrap_or_default();
        parsed.channel = json_string(filters, "channel");
        parsed.label = json_string(filters, "label");
        parsed.contains = json_string(filters, "contains");
        parsed.pattern = json_string(filters, "pattern");
        if let Some(resource) = filters.get("resource") {
            parsed.resource_kind = json_string(resource, "kind").unwrap_or_default();
            parsed.resource_id = json_string(resource, "id").unwrap_or_default();
        }
        if let Some(users) = filters.get("from_users").and_then(|value| value.as_array()) {
            parsed.from_users = users
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect();
        }
    }
    Ok(parsed)
}

fn json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum YamlSection {
    Root,
    Event,
    Filters,
    Resource,
    FromUsers,
    Run,
    Agent,
}

fn parse_trigger_yaml(content: &str) -> std::result::Result<ParsedTrigger, String> {
    let mut parsed = ParsedTrigger::default();
    let mut section = YamlSection::Root;
    for raw in content.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        if section == YamlSection::FromUsers && !trimmed.starts_with("- ") {
            section = YamlSection::Filters;
        }
        if section == YamlSection::Resource && indent <= 2 {
            section = YamlSection::Filters;
        }
        if section == YamlSection::Agent && indent <= 2 {
            section = YamlSection::Run;
        }
        if let Some(item) = trimmed.strip_prefix("- ") {
            if section == YamlSection::FromUsers
                && let Some(user) = yaml_scalar(item)
            {
                parsed.from_users.push(user);
            }
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if indent == 0 {
            section = match key {
                "event" => YamlSection::Event,
                "filters" => YamlSection::Filters,
                "run" => YamlSection::Run,
                _ => YamlSection::Root,
            };
            if key == "name" {
                parsed.name = yaml_scalar(value).unwrap_or_default();
            }
            continue;
        }
        match section {
            YamlSection::Event => match key {
                "provider" => parsed.provider = yaml_scalar(value).unwrap_or_default(),
                "type" => parsed.event = yaml_scalar(value).unwrap_or_default(),
                _ => {}
            },
            YamlSection::Filters => match key {
                "connection" => parsed.connection = yaml_scalar(value).unwrap_or_default(),
                "resource" => section = YamlSection::Resource,
                "from_users" => {
                    parsed.from_users = yaml_list(value);
                    if value.is_empty() {
                        section = YamlSection::FromUsers;
                    }
                }
                "channel" => parsed.channel = yaml_scalar(value),
                "label" => parsed.label = yaml_scalar(value),
                "contains" => parsed.contains = yaml_scalar(value),
                "pattern" => parsed.pattern = yaml_scalar(value),
                _ => {}
            },
            YamlSection::Resource => match key {
                "kind" => parsed.resource_kind = yaml_scalar(value).unwrap_or_default(),
                "id" => parsed.resource_id = yaml_scalar(value).unwrap_or_default(),
                _ => {}
            },
            YamlSection::Run => {
                if key == "agent" {
                    section = YamlSection::Agent;
                }
            }
            YamlSection::FromUsers | YamlSection::Agent | YamlSection::Root => {}
        }
    }
    Ok(parsed)
}

fn yaml_list(value: &str) -> Vec<String> {
    let value = value.trim();
    if !value.starts_with('[') || !value.ends_with(']') {
        return Vec::new();
    }
    value[1..value.len() - 1]
        .split(',')
        .filter_map(|item| yaml_scalar(item.trim()))
        .collect()
}

fn yaml_scalar(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value == "null" || value == "~" {
        return None;
    }
    if value.starts_with('"') && value.ends_with('"') {
        return serde_json::from_str::<String>(value).ok();
    }
    if value.starts_with('\'') && value.ends_with('\'') {
        return Some(value[1..value.len() - 1].to_owned());
    }
    Some(value.to_owned())
}

/// The fields of one manual dispatch form, in tab order.
pub const DISPATCH_FIELDS: [&str; 6] = [
    "project",
    "trigger",
    "actor",
    "delivery key",
    "input",
    "revision",
];

/// One manual dispatch, being edited.
#[derive(Debug, Clone)]
pub struct DispatchForm {
    project: String,
    trigger: String,
    actor: String,
    delivery_key: String,
    input: String,
    revision: String,
    focus: usize,
    issues: Vec<String>,
}

impl DispatchForm {
    /// A form prefilled with the project, trigger, actor and delivery key.
    pub fn new(
        project: impl Into<String>,
        trigger: impl Into<String>,
        actor: impl Into<String>,
        delivery_key: impl Into<String>,
    ) -> Self {
        Self {
            project: project.into(),
            trigger: trigger.into(),
            actor: actor.into(),
            delivery_key: delivery_key.into(),
            input: String::new(),
            revision: String::new(),
            focus: 0,
            issues: Vec::new(),
        }
    }

    /// The field labels, in tab order.
    pub fn labels() -> &'static [&'static str; 6] {
        &DISPATCH_FIELDS
    }

    /// The six field values, in tab order.
    pub fn values(&self) -> [&str; 6] {
        [
            &self.project,
            &self.trigger,
            &self.actor,
            &self.delivery_key,
            &self.input,
            &self.revision,
        ]
    }

    /// The field the keyboard is editing.
    pub fn focus(&self) -> usize {
        self.focus
    }

    /// The label of the field the keyboard is editing.
    pub fn focus_label(&self) -> &'static str {
        DISPATCH_FIELDS[self.focus]
    }

    /// The issues that stopped the last submit, if any.
    pub fn issues(&self) -> &[String] {
        &self.issues
    }

    /// The request this form describes.
    pub fn request(&self) -> ManualDispatchRequest {
        ManualDispatchRequest {
            actor: self.actor.trim().to_owned(),
            delivery_key: self.delivery_key.trim().to_owned(),
            input: non_empty(&self.input),
            project: self.project.trim().to_owned(),
            revision: non_empty(&self.revision),
            trigger: self.trigger.trim().to_owned(),
        }
    }

    /// Collect the form's issues against the project's installed triggers.
    ///
    /// An actor the trigger's `from_users` allowlist does not name is refused
    /// here, before a request leaves the console.
    pub fn validate(&mut self, triggers: &[TriggerSummary]) -> bool {
        let mut issues = Vec::new();
        if self.project.trim().is_empty() {
            issues.push("Choose the project the run belongs to.".to_owned());
        }
        if self.trigger.trim().is_empty() {
            issues.push("Name the installed trigger to run.".to_owned());
        }
        if self.actor.trim().is_empty() {
            issues.push("Name the actor the trigger must allow.".to_owned());
        }
        if self.delivery_key.trim().is_empty() {
            issues.push(
                "A delivery key is required, so a repeated press cannot start a second run."
                    .to_owned(),
            );
        }
        if !self.trigger.trim().is_empty() {
            match triggers
                .iter()
                .find(|trigger| trigger.name == self.trigger.trim())
            {
                None if !triggers.is_empty() => issues.push(format!(
                    "Trigger '{}' is not installed in this project.",
                    self.trigger.trim()
                )),
                Some(trigger) if !trigger.active => issues.push(format!(
                    "Trigger '{}' is not active, so it cannot run.",
                    trigger.name
                )),
                Some(trigger) if !self.actor.trim().is_empty() && !trigger.allows(&self.actor) => {
                    issues.push(format!(
                        "Actor '{}' is not allowed by trigger '{}'.",
                        self.actor.trim(),
                        trigger.name
                    ));
                }
                _ => {}
            }
        }
        self.issues = issues;
        self.issues.is_empty()
    }

    fn edit(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % DISPATCH_FIELDS.len(),
            KeyCode::BackTab | KeyCode::Up => {
                self.focus = (self.focus + DISPATCH_FIELDS.len() - 1) % DISPATCH_FIELDS.len();
            }
            KeyCode::Backspace => {
                self.field_mut().pop();
            }
            KeyCode::Char(character) => self.field_mut().push(character),
            _ => {}
        }
    }

    fn field_mut(&mut self) -> &mut String {
        match self.focus {
            0 => &mut self.project,
            1 => &mut self.trigger,
            2 => &mut self.actor,
            3 => &mut self.delivery_key,
            4 => &mut self.input,
            _ => &mut self.revision,
        }
    }
}

/// A client-supplied delivery key that is stable for one form.
pub fn manual_delivery_key(actor: &str, now_ms: u128, sequence: u64) -> String {
    let who: String = actor
        .trim()
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect();
    let who = who.trim_matches('-');
    let who = if who.is_empty() { "operator" } else { who };
    format!("console-{who}-{now_ms}-{sequence}")
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default()
}

/// One open run detail.
#[derive(Debug, Clone)]
struct DetailState {
    run_id: String,
    detail: Option<ActivityRunDetail>,
    following: bool,
    last_poll: Option<Instant>,
    problem: Option<String>,
}

/// The whole runs tab.
#[derive(Debug, Default)]
pub struct RunsState {
    projects: Vec<ProjectView>,
    entries: Vec<ActivityEntry>,
    triggers: Vec<TriggerSummary>,
    filters: Filters,
    selected: usize,
    detail: Option<DetailState>,
    form: Option<DispatchForm>,
    notice: Option<String>,
    last_feed: Option<Instant>,
    projects_read: bool,
    triggers_project: Option<String>,
    pending: Option<ManualDispatchRequest>,
    sequence: u64,
    member: Option<String>,
}

impl RunsState {
    /// An empty tab: no feed, no filters, no detail.
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember the signed-in member, used as the dispatch actor's default.
    pub fn set_member(&mut self, member: Option<String>) {
        self.member = member;
    }

    /// Point the project filter at one project without touching the clock.
    ///
    /// The rendering tests use this to show the trigger pane without waiting
    /// for a feed interval.
    #[cfg(test)]
    pub(crate) fn set_project_filter_for_test(&mut self, project: &str) {
        self.filters.project = project.to_owned();
        self.triggers_project = None;
    }

    /// The projects the pool last reported.
    pub fn projects(&self) -> &[ProjectView] {
        &self.projects
    }

    /// The activity entries the pool last reported.
    pub fn entries(&self) -> &[ActivityEntry] {
        &self.entries
    }

    /// The visible entries: every active filter must match.
    pub fn visible_entries(&self) -> Vec<&ActivityEntry> {
        self.entries
            .iter()
            .filter(|entry| self.filters.matches(entry))
            .collect()
    }

    /// The filter combination in use.
    pub fn filters(&self) -> &Filters {
        &self.filters
    }

    /// The index of the highlighted feed row.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The highlighted feed row.
    pub fn selected_entry(&self) -> Option<&ActivityEntry> {
        self.visible_entries().get(self.selected).copied()
    }

    /// The triggers authored in the selected project's active revision.
    pub fn triggers(&self) -> &[TriggerSummary] {
        &self.triggers
    }

    /// The open run detail, once it has loaded.
    pub fn detail(&self) -> Option<&ActivityRunDetail> {
        self.detail
            .as_ref()
            .and_then(|detail| detail.detail.as_ref())
    }

    /// The id of the open run.
    pub fn selected_run_id(&self) -> Option<&str> {
        self.detail.as_ref().map(|detail| detail.run_id.as_str())
    }

    /// Whether the open run is still being followed.
    pub fn following(&self) -> bool {
        self.detail.as_ref().is_some_and(|detail| detail.following)
    }

    /// Why the open run's last read failed, when it did.
    pub fn detail_problem(&self) -> Option<&str> {
        self.detail
            .as_ref()
            .and_then(|detail| detail.problem.as_deref())
    }

    /// The open dispatch form, while it is on screen.
    pub fn form(&self) -> Option<&DispatchForm> {
        self.form.as_ref()
    }

    /// The last action's outcome, when it was not a plain success.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Move the highlight down, wrapping.
    pub fn select_next(&mut self) {
        let count = self.visible_entries().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + 1) % count;
    }

    /// Move the highlight up, wrapping.
    pub fn select_previous(&mut self) {
        let count = self.visible_entries().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + count - 1) % count;
    }

    /// Handle one key press. Returns whether the tab consumed it.
    ///
    /// While the dispatch form is open every key belongs to it, so `q` types a
    /// `q` instead of quitting the console.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        if self.form.is_some() {
            self.on_form_key(key);
            return true;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return false;
        }
        if self.detail.is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.detail = None;
                    return true;
                }
                KeyCode::Char('r') => {
                    if let Some(detail) = self.detail.as_mut() {
                        detail.last_poll = None;
                        detail.following = true;
                    }
                    return true;
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                true
            }
            KeyCode::Enter | KeyCode::Char('o') => {
                if let Some(run_id) = self.selected_entry().and_then(entry_run_id) {
                    self.open_run(run_id.to_owned());
                }
                true
            }
            KeyCode::Char('d') => {
                self.open_form();
                true
            }
            KeyCode::Char('p') => {
                let projects = self.projects.clone();
                self.filters.cycle_project(&projects);
                self.after_filter_change();
                true
            }
            KeyCode::Char('v') => {
                self.filters.cycle_provider();
                self.after_filter_change();
                true
            }
            KeyCode::Char('t') => {
                let triggers = self.triggers.clone();
                self.filters.cycle_trigger(&triggers);
                self.after_filter_change();
                true
            }
            KeyCode::Char('f') => {
                self.filters.cycle_status();
                self.after_filter_change();
                true
            }
            KeyCode::Char('x') => {
                self.filters.clear();
                self.after_filter_change();
                true
            }
            KeyCode::Char('r') => {
                self.last_feed = None;
                self.projects_read = false;
                self.triggers_project = None;
                true
            }
            KeyCode::Esc => {
                if self.filters.is_active() {
                    self.filters.clear();
                    self.after_filter_change();
                }
                true
            }
            _ => false,
        }
    }

    fn on_form_key(&mut self, key: KeyEvent) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.form = None,
            KeyCode::Enter => {
                let triggers = self.triggers.clone();
                if form.validate(&triggers) {
                    self.pending = Some(form.request());
                }
            }
            KeyCode::Tab
            | KeyCode::BackTab
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Backspace
            | KeyCode::Char(_) => form.edit(key),
            _ => {}
        }
    }

    fn after_filter_change(&mut self) {
        self.selected = 0;
        self.last_feed = None;
        self.triggers.clear();
        self.triggers_project = None;
    }

    fn open_run(&mut self, run_id: String) {
        self.detail = Some(DetailState {
            run_id,
            detail: None,
            following: true,
            last_poll: None,
            problem: None,
        });
    }

    fn open_form(&mut self) {
        let project = if self.filters.project().is_empty() {
            self.projects
                .first()
                .map(|project| project.id.clone())
                .unwrap_or_default()
        } else {
            self.filters.project().to_owned()
        };
        let trigger = self
            .triggers
            .iter()
            .find(|trigger| trigger.active)
            .or_else(|| self.triggers.first())
            .map(|trigger| trigger.name.clone())
            .unwrap_or_default();
        let actor = self.member.clone().unwrap_or_default();
        let key = manual_delivery_key(&actor, now_millis(), self.sequence);
        self.sequence = self.sequence.saturating_add(1);
        self.form = Some(DispatchForm::new(project, trigger, actor, key));
    }

    /// Perform the pending dispatch, follow the open run and refresh on timers.
    ///
    /// The console calls this from its event loop, so a running step's output
    /// arrives without a key press and the feed refreshes on its own.
    pub fn drive(&mut self, client: &mut dyn RunsClient, now: Instant) {
        if let Some(request) = self.pending.take() {
            match client.dispatch(request) {
                Ok(response) => {
                    let run = response.run;
                    let run_id = run.id.clone();
                    self.notice = Some(if response.deduplicated {
                        format!("This request had already run, so the pool returned run {run_id}.")
                    } else {
                        format!("The pool started run {run_id}.")
                    });
                    self.form = None;
                    self.detail = Some(DetailState {
                        run_id,
                        detail: Some(ActivityRunDetail {
                            arrival: Default::default(),
                            run,
                        }),
                        following: true,
                        last_poll: Some(now),
                        problem: None,
                    });
                    self.last_feed = None;
                }
                Err(error) => {
                    self.notice = Some(format!("dispatch failed: {error}"));
                }
            }
        }
        self.poll_detail(client, now);
        self.refresh_feed(client, now);
        self.ensure_triggers(client);
    }

    fn poll_detail(&mut self, client: &mut dyn RunsClient, now: Instant) {
        let Some(detail) = self.detail.as_mut() else {
            return;
        };
        let due = detail.detail.is_none()
            || (detail.following
                && detail
                    .last_poll
                    .is_none_or(|last| now.saturating_duration_since(last) >= LIVE_INTERVAL));
        if !due {
            return;
        }
        detail.last_poll = Some(now);
        match client.run_detail(&detail.run_id) {
            Ok(answer) => {
                detail.following = run_is_live(&answer.run);
                detail.detail = Some(answer);
                detail.problem = None;
            }
            Err(error) => {
                detail.problem = Some(error.to_string());
                detail.following = false;
            }
        }
    }

    fn refresh_feed(&mut self, client: &mut dyn RunsClient, now: Instant) {
        if !self.projects_read {
            match client.list_projects() {
                Ok(projects) => {
                    self.projects = projects;
                    self.projects_read = true;
                }
                Err(error) => self.notice = Some(format!("projects: {error}")),
            }
        }
        let due = self
            .last_feed
            .is_none_or(|last| now.saturating_duration_since(last) >= FEED_INTERVAL);
        if !due {
            return;
        }
        self.last_feed = Some(now);
        match client.list_activity(self.filters.query()) {
            Ok(page) => {
                self.entries = page
                    .items
                    .into_iter()
                    .filter(|entry| self.filters.matches(entry))
                    .collect();
                self.clamp_selection();
            }
            Err(error) => self.notice = Some(format!("activity: {error}")),
        }
    }

    fn ensure_triggers(&mut self, client: &mut dyn RunsClient) {
        let project = self.filters.project().to_owned();
        if project.is_empty() {
            self.triggers.clear();
            self.triggers_project = None;
            return;
        }
        if self.triggers_project.as_deref() == Some(project.as_str()) {
            return;
        }
        self.triggers_project = Some(project.clone());
        match client.list_triggers(&project) {
            Ok(triggers) => self.triggers = triggers,
            Err(error) => {
                self.triggers = Vec::new();
                self.notice = Some(format!("triggers: {error}"));
            }
        }
    }

    fn clamp_selection(&mut self) {
        let count = self.visible_entries().len();
        self.selected = if count == 0 {
            0
        } else {
            self.selected.min(count - 1)
        };
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::VecDeque;

    use tidepool_client::endpoints::GetV1ActivityQuery;
    use tidepool_client::types::{
        ActivityEntry, ActivityPage, ActivityRunDetail, ManualDispatchRequest,
        ManualDispatchResponse, ProjectView, RevisionFileView, RevisionView, RunStepView, RunView,
    };

    use super::{RunsClient, TriggerSummary};
    use crate::app::Result;
    use crate::app::testing::transport;

    /// One activity entry, with only the fields the feed reads set.
    pub(crate) fn entry(
        id: &str,
        kind: &str,
        status: &str,
        project: Option<&str>,
        provider: Option<&str>,
        trigger: Option<&str>,
    ) -> ActivityEntry {
        ActivityEntry {
            id: id.to_owned(),
            kind: kind.to_owned(),
            status: status.to_owned(),
            project: project.map(str::to_owned),
            provider: provider.map(str::to_owned),
            trigger: trigger.map(str::to_owned),
            trigger_name: trigger.map(str::to_owned),
            run: (kind == "run").then(|| id.to_owned()),
            ..ActivityEntry::default()
        }
    }

    /// One step with an id, status and output.
    pub(crate) fn step(id: &str, status: &str, output: Option<&str>) -> RunStepView {
        RunStepView {
            id: id.to_owned(),
            step: id.to_owned(),
            status: status.to_owned(),
            output: output.map(str::to_owned),
            position: 1,
            ..RunStepView::default()
        }
    }

    /// One run with its steps.
    pub(crate) fn run(id: &str, status: &str, steps: Vec<RunStepView>) -> RunView {
        RunView {
            id: id.to_owned(),
            status: status.to_owned(),
            steps,
            project: "proj_1".to_owned(),
            trigger: "on-push".to_owned(),
            trigger_name: "on-push".to_owned(),
            delivery_key: "key-1".to_owned(),
            origin: "manual".to_owned(),
            actor: Some("alice".to_owned()),
            request_text: "deploy".to_owned(),
            created_at: "2026-10-29T00:00:00Z".to_owned(),
            ..RunView::default()
        }
    }

    /// One run detail with an empty arrival record.
    pub(crate) fn detail(run: RunView) -> ActivityRunDetail {
        ActivityRunDetail {
            arrival: Default::default(),
            run,
        }
    }

    /// One project.
    pub(crate) fn project(id: &str, name: &str) -> ProjectView {
        ProjectView {
            id: id.to_owned(),
            name: name.to_owned(),
            slug: name.to_lowercase(),
            created_at: "2026-10-29T00:00:00Z".to_owned(),
            updated_at: "2026-10-29T00:00:00Z".to_owned(),
        }
    }

    /// One active revision holding the authored files.
    pub(crate) fn revision(files: Vec<(&str, &str)>) -> RevisionView {
        RevisionView {
            id: "rev_1".to_owned(),
            version: 1,
            active: true,
            status: "valid".to_owned(),
            errors: Vec::new(),
            created_at: "2026-10-29T00:00:00Z".to_owned(),
            files: files
                .into_iter()
                .map(|(path, content)| RevisionFileView {
                    path: path.to_owned(),
                    content: content.to_owned(),
                })
                .collect(),
        }
    }

    /// A client that answers from queues and records every call it served.
    #[derive(Debug, Default)]
    pub(crate) struct ScriptedRuns {
        /// Queued project lists.
        pub(crate) projects: VecDeque<Result<Vec<ProjectView>>>,
        /// Queued activity pages.
        pub(crate) pages: VecDeque<Result<ActivityPage>>,
        /// Queued run details.
        pub(crate) details: VecDeque<Result<ActivityRunDetail>>,
        /// Queued dispatch answers.
        pub(crate) dispatches: VecDeque<Result<ManualDispatchResponse>>,
        /// Queued trigger lists.
        pub(crate) trigger_lists: VecDeque<Result<Vec<TriggerSummary>>>,
        /// Every call served, in order.
        pub(crate) calls: Vec<String>,
    }

    impl ScriptedRuns {
        /// An empty script.
        pub(crate) fn new() -> Self {
            Self::default()
        }

        /// Queue one page.
        pub(crate) fn queue_page(&mut self, items: Vec<ActivityEntry>) {
            self.pages.push_back(Ok(ActivityPage {
                items,
                has_more: false,
                next_cursor: None,
            }));
        }
    }

    impl RunsClient for ScriptedRuns {
        fn list_projects(&mut self) -> Result<Vec<ProjectView>> {
            self.calls.push("projects".to_owned());
            self.projects.pop_front().unwrap_or_else(|| Ok(Vec::new()))
        }

        fn list_activity(&mut self, query: GetV1ActivityQuery) -> Result<ActivityPage> {
            self.calls.push(format!(
                "activity:{}:{}:{}:{}",
                query.project.unwrap_or_default(),
                query.provider.unwrap_or_default(),
                query.trigger.unwrap_or_default(),
                query.status.unwrap_or_default()
            ));
            self.pages
                .pop_front()
                .unwrap_or_else(|| Ok(ActivityPage::default()))
        }

        fn run_detail(&mut self, run: &str) -> Result<ActivityRunDetail> {
            self.calls.push(format!("run:{run}"));
            self.details
                .pop_front()
                .unwrap_or_else(|| Err(transport("no scripted run detail")))
        }

        fn dispatch(&mut self, request: ManualDispatchRequest) -> Result<ManualDispatchResponse> {
            self.calls.push(format!(
                "dispatch:{}:{}:{}:{}",
                request.project, request.trigger, request.actor, request.delivery_key
            ));
            self.dispatches
                .pop_front()
                .unwrap_or_else(|| Err(transport("no scripted dispatch")))
        }

        fn list_triggers(&mut self, project: &str) -> Result<Vec<TriggerSummary>> {
            self.calls.push(format!("triggers:{project}"));
            self.trigger_lists
                .pop_front()
                .unwrap_or_else(|| Ok(Vec::new()))
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use tidepool_client::types::{ManualDispatchResponse, RunStepView};

    use super::testing::{ScriptedRuns, detail, entry, project, revision, run, step};
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(state: &mut RunsState, text: &str) {
        for character in text.chars() {
            state.on_key(key(KeyCode::Char(character)));
        }
    }

    fn trigger(name: &str, active: bool, from_users: &[&str]) -> TriggerSummary {
        TriggerSummary {
            name: name.to_owned(),
            provider: "github".to_owned(),
            event: "push".to_owned(),
            connection: "conn_1".to_owned(),
            resource_kind: "repository".to_owned(),
            resource_id: "org/repo".to_owned(),
            from_users: from_users.iter().map(|user| (*user).to_owned()).collect(),
            active,
            ..TriggerSummary::default()
        }
    }

    #[test]
    fn filters_combine_and_only_send_what_is_set() {
        let mut filters = Filters::default();
        assert!(!filters.is_active());
        assert_eq!(filters.query().project, None);
        assert_eq!(filters.query().limit, Some(ACTIVITY_PAGE_SIZE));

        filters.cycle_project(&[project("proj_1", "one"), project("proj_2", "two")]);
        filters.cycle_provider();
        filters.cycle_status();
        assert_eq!(filters.project(), "proj_1");
        assert_eq!(filters.provider(), "github");
        assert_eq!(filters.status(), "pending");

        let query = filters.query();
        assert_eq!(query.project.as_deref(), Some("proj_1"));
        assert_eq!(query.provider.as_deref(), Some("github"));
        assert_eq!(query.status.as_deref(), Some("pending"));
        assert_eq!(query.trigger, None);
        assert_eq!(query.kind, None);

        let matching = entry(
            "run_1",
            "run",
            "pending",
            Some("proj_1"),
            Some("github"),
            None,
        );
        let other_project = entry(
            "run_2",
            "run",
            "pending",
            Some("proj_2"),
            Some("github"),
            None,
        );
        let other_provider = entry(
            "run_3",
            "run",
            "pending",
            Some("proj_1"),
            Some("slack"),
            None,
        );
        let other_status = entry(
            "run_4",
            "run",
            "running",
            Some("proj_1"),
            Some("github"),
            None,
        );
        assert!(filters.matches(&matching));
        assert!(!filters.matches(&other_project));
        assert!(!filters.matches(&other_provider));
        assert!(!filters.matches(&other_status));
    }

    #[test]
    fn a_missing_field_never_satisfies_an_active_filter() {
        let mut filters = Filters::default();
        filters.cycle_project(&[project("proj_1", "one")]);
        let unrelated = entry("run_1", "run", "pending", None, None, None);
        assert!(!filters.matches(&unrelated));
    }

    #[test]
    fn the_page_is_filtered_again_so_a_loose_pool_cannot_leak_rows() {
        let base = Instant::now();
        let mut state = RunsState::new();
        state.filters = Filters {
            project: "proj_1".to_owned(),
            provider: "github".to_owned(),
            trigger: "on-push".to_owned(),
            status: "succeeded".to_owned(),
        };
        let mut client = ScriptedRuns::new();
        client
            .projects
            .push_back(Ok(vec![project("proj_1", "one")]));
        client.queue_page(vec![
            entry(
                "run_1",
                "run",
                "succeeded",
                Some("proj_1"),
                Some("github"),
                Some("on-push"),
            ),
            entry(
                "run_2",
                "run",
                "succeeded",
                Some("proj_2"),
                Some("github"),
                Some("on-push"),
            ),
            entry(
                "run_3",
                "run",
                "failed",
                Some("proj_1"),
                Some("github"),
                Some("on-push"),
            ),
            entry(
                "run_4",
                "run",
                "succeeded",
                Some("proj_1"),
                Some("slack"),
                Some("on-push"),
            ),
        ]);
        state.drive(&mut client, base);

        let ids: Vec<&str> = state
            .visible_entries()
            .iter()
            .map(|entry| entry.id.as_str())
            .collect();
        assert_eq!(ids, ["run_1"]);
        assert_eq!(
            client.calls,
            [
                "projects",
                "activity:proj_1:github:on-push:succeeded",
                "triggers:proj_1"
            ]
        );
    }

    #[test]
    fn the_feed_refreshes_from_the_clock_without_a_key_press() {
        let base = Instant::now();
        let mut state = RunsState::new();
        let mut client = ScriptedRuns::new();
        client
            .projects
            .push_back(Ok(vec![project("proj_1", "one")]));
        client.queue_page(vec![entry(
            "run_1",
            "run",
            "running",
            Some("proj_1"),
            Some("github"),
            None,
        )]);
        client.queue_page(vec![entry(
            "run_1",
            "run",
            "succeeded",
            Some("proj_1"),
            Some("github"),
            None,
        )]);
        state.drive(&mut client, base);
        assert_eq!(state.visible_entries()[0].status, "running");

        state.drive(&mut client, base + FEED_INTERVAL);
        assert_eq!(state.visible_entries()[0].status, "succeeded");
        assert_eq!(
            client
                .calls
                .iter()
                .filter(|call| call.starts_with("activity:"))
                .count(),
            2
        );
    }

    #[test]
    fn a_live_detail_follows_output_and_stops_when_it_ends() {
        let base = Instant::now();
        let mut state = RunsState::new();
        let mut client = ScriptedRuns::new();
        client
            .projects
            .push_back(Ok(vec![project("proj_1", "one")]));
        client.queue_page(vec![entry(
            "run_1",
            "run",
            "running",
            Some("proj_1"),
            Some("github"),
            None,
        )]);
        client.details.push_back(Ok(detail(run(
            "run_1",
            "running",
            vec![step("build", "running", Some("line one\nline two"))],
        ))));
        state.drive(&mut client, base);

        state.on_key(key(KeyCode::Enter));
        state.drive(&mut client, base);
        assert!(state.following());
        let visible = output_tail(
            state.detail().expect("detail").run.steps[0]
                .output
                .as_deref(),
            TAIL_LINES,
        );
        assert_eq!(visible, ["line one", "line two"]);

        client.details.push_back(Ok(detail(run(
            "run_1",
            "succeeded",
            vec![step(
                "build",
                "succeeded",
                Some("line one\nline two\nline three"),
            )],
        ))));
        state.drive(&mut client, base + LIVE_INTERVAL);
        assert!(!state.following(), "the terminal detail stopped following");
        let visible = output_tail(
            state.detail().expect("detail").run.steps[0]
                .output
                .as_deref(),
            TAIL_LINES,
        );
        assert_eq!(visible, ["line one", "line two", "line three"]);

        let calls = client.calls.len();
        state.drive(&mut client, base + LIVE_INTERVAL * 4);
        assert_eq!(
            client.calls.len(),
            calls,
            "a finished run is not read again"
        );
    }

    #[test]
    fn a_run_with_a_live_step_is_still_followed() {
        let still_running = run(
            "run_1",
            "succeeded",
            vec![step("build", "running", Some("still writing"))],
        );
        assert!(run_is_live(&still_running));
        let finished = run(
            "run_1",
            "succeeded",
            vec![step("build", "succeeded", Some("done"))],
        );
        assert!(!run_is_live(&finished));
    }

    #[test]
    fn output_tail_keeps_the_last_lines_only() {
        let output = (1..=20)
            .map(|number| format!("line {number}"))
            .collect::<Vec<_>>()
            .join("\n");
        let tail = output_tail(Some(&output), 3);
        assert_eq!(tail, ["line 18", "line 19", "line 20"]);
        assert!(output_tail(None, 3).is_empty());
        assert!(output_tail(Some(""), 3).is_empty());
    }

    #[test]
    fn dispatch_collects_its_inputs_and_refuses_a_disallowed_actor() {
        let base = Instant::now();
        let mut state = RunsState::new();
        state.set_member(Some("alice".to_owned()));
        state.filters = Filters {
            project: "proj_1".to_owned(),
            ..Filters::default()
        };
        let mut client = ScriptedRuns::new();
        client
            .trigger_lists
            .push_back(Ok(vec![trigger("on-push", true, &["bob"])]));
        client.dispatches.push_back(Ok(ManualDispatchResponse {
            deduplicated: false,
            run: run(
                "run_9",
                "running",
                vec![step("build", "running", Some("started"))],
            ),
        }));
        state.drive(&mut client, base);
        assert_eq!(state.triggers().len(), 1);

        state.on_key(key(KeyCode::Char('d')));
        let form = state.form().expect("a dispatch form").clone();
        assert_eq!(form.values()[0], "proj_1");
        assert_eq!(form.values()[1], "on-push");
        assert_eq!(form.values()[2], "alice");
        assert!(!form.values()[3].is_empty());

        // The actor is the member the trigger does not allow: Enter refuses.
        state.on_key(key(KeyCode::Enter));
        let form = state.form().expect("the form stays open");
        assert!(
            form.issues()
                .iter()
                .any(|issue| issue.contains("not allowed")),
            "{:?}",
            form.issues()
        );
        assert!(
            !client
                .calls
                .iter()
                .any(|call| call.starts_with("dispatch:"))
        );

        // Replace the actor with an allowed one, then add an input string.
        state.on_key(key(KeyCode::Tab));
        state.on_key(key(KeyCode::Tab));
        for _ in 0.."alice".len() {
            state.on_key(key(KeyCode::Backspace));
        }
        type_text(&mut state, "bob");
        state.on_key(key(KeyCode::Tab));
        state.on_key(key(KeyCode::Tab));
        type_text(&mut state, "route=bug deploy it");
        state.on_key(key(KeyCode::Enter));
        state.drive(&mut client, base);
        assert!(state.form().is_none(), "a valid form submitted");

        let sent = client
            .calls
            .iter()
            .find(|call| call.starts_with("dispatch:"))
            .expect("a dispatch call");
        assert!(sent.contains("proj_1:on-push:bob:"), "{sent}");
        assert!(
            state
                .notice()
                .is_some_and(|notice| notice.contains("run_9")),
            "{:?}",
            state.notice()
        );
        assert_eq!(state.selected_run_id(), Some("run_9"));
    }

    #[test]
    fn dispatch_refuses_an_inactive_trigger_and_a_missing_delivery_key() {
        let mut form = DispatchForm::new("proj_1", "on-push", "alice", "key-1");
        assert!(!form.validate(&[trigger("on-push", false, &["alice"])]));
        assert!(
            form.issues()
                .iter()
                .any(|issue| issue.contains("not active"))
        );

        let mut form = DispatchForm::new("proj_1", "on-push", "alice", "");
        assert!(!form.validate(&[trigger("on-push", true, &["alice"])]));
        assert!(
            form.issues()
                .iter()
                .any(|issue| issue.contains("delivery key"))
        );
    }

    #[test]
    fn a_delivery_key_is_stable_for_one_form_and_different_for_the_next() {
        let first = manual_delivery_key("Alice Smith", 1_700_000_000_000, 1);
        let again = manual_delivery_key("Alice Smith", 1_700_000_000_000, 1);
        let next = manual_delivery_key("Alice Smith", 1_700_000_000_001, 2);
        assert_eq!(first, again);
        assert_ne!(first, next);
        assert!(first.starts_with("console-alice-smith-"), "{first}");
    }

    #[test]
    fn dispatch_defaults_the_actor_to_the_signed_in_member() {
        let mut state = RunsState::new();
        state.set_member(Some("ops@example.com".to_owned()));
        state.projects = vec![project("proj_1", "one")];
        state.triggers = vec![trigger("on-push", true, &["ops@example.com"])];
        state.on_key(key(KeyCode::Char('d')));
        let mut form = state.form().expect("form").clone();
        assert_eq!(form.values()[2], "ops@example.com");
        assert!(form.validate(&state.triggers));
    }

    #[test]
    fn a_known_failure_code_renders_as_words_with_the_code() {
        let failure = failure_explanation(
            Some("daemon_not_connected: host 'swaygent' is offline"),
            None,
        )
        .expect("a failure");
        assert_eq!(failure.code.as_deref(), Some("daemon_not_connected"));
        assert!(
            failure.sentence.contains("host went offline"),
            "{failure:?}"
        );
        assert_eq!(
            failure.detail.as_deref(),
            Some("host 'swaygent' is offline")
        );
    }

    #[test]
    fn an_unknown_code_keeps_the_pool_text_and_the_code() {
        let failure =
            failure_explanation(Some("odd_code: the pool said so"), None).expect("a failure");
        assert_eq!(failure.code.as_deref(), Some("odd_code"));
        assert!(failure.sentence.contains("odd_code"), "{failure:?}");
        assert!(failure.sentence.contains("the pool said so"), "{failure:?}");

        let bare = failure_explanation(None, Some("interrupted")).expect("a failure");
        assert_eq!(bare.code.as_deref(), Some("interrupted"));
        assert!(bare.sentence.contains("interrupted"), "{bare:?}");
    }

    #[test]
    fn a_run_failure_prefers_the_failed_step() {
        let mut failed_step: RunStepView = step("deploy", "failed", None);
        failed_step.error = Some("execution_failed: the agent exited 1".to_owned());
        let run = run(
            "run_1",
            "failed",
            vec![step("build", "succeeded", None), failed_step],
        );
        let failure = run_failure_explanation(&run).expect("a failure");
        assert_eq!(failure.code.as_deref(), Some("execution_failed"));
        assert!(
            failure.sentence.contains("agent on the host"),
            "{failure:?}"
        );
    }

    #[test]
    fn triggers_are_read_from_the_active_revision_yaml_and_json() {
        let yaml = "\
name: on-push
event:
  provider: github
  type: push
filters:
  connection: conn_1
  resource:
    kind: repository
    id: org/repo
  from_users:
    - alice
    - \"bob smith\"
  channel: null
  label: triage
  contains: null
  pattern: ^main$
run:
  host: studio
  agent:
    provider: anthropic
    model: claude
    execution_mode: default
    thinking_option: null
";
        let json = r##"{
            "name": "on-comment",
            "event": {"provider": "slack", "type": "message"},
            "filters": {
                "connection": "conn_2",
                "resource": {"kind": "channel", "id": "C1"},
                "from_users": ["carol"],
                "channel": "#ops"
            },
            "run": {"host": "studio", "agent": {"provider": "x", "model": "y", "execution_mode": "z"}}
        }"##;
        let triggers = triggers_from_revision(&revision(vec![
            ("triggers/on-push.yaml", yaml),
            ("triggers/on-comment.json", json),
            ("workflows/build.yaml", "steps: []"),
        ]));
        assert_eq!(triggers.len(), 2);

        let push = &triggers[0];
        assert_eq!(push.name, "on-push");
        assert_eq!(push.event_line(), "GitHub push");
        assert_eq!(push.resource_id, "org/repo");
        assert_eq!(push.from_users, ["alice", "bob smith"]);
        assert_eq!(push.label.as_deref(), Some("triage"));
        assert!(push.active);
        assert!(push.allows("ALICE"));
        assert!(!push.allows("mallory"));

        let comment = &triggers[1];
        assert_eq!(comment.name, "on-comment");
        assert_eq!(comment.event_line(), "Slack message");
        assert_eq!(comment.channel.as_deref(), Some("#ops"));
        assert!(comment.active);
    }

    #[test]
    fn an_inactive_revision_marks_its_triggers_inactive() {
        let mut revision = revision(vec![("triggers/on-push.yaml", "name: on-push\n")]);
        revision.active = false;
        let triggers = triggers_from_revision(&revision);
        assert_eq!(triggers.len(), 1);
        assert!(!triggers[0].active);
    }

    #[test]
    fn an_unreadable_trigger_file_is_still_listed() {
        let triggers = triggers_from_revision(&revision(vec![(
            "triggers/broken.json",
            "{ this is not json",
        )]));
        assert_eq!(triggers.len(), 1);
        assert_eq!(triggers[0].name, "broken");
        assert!(!triggers[0].active);
        assert!(triggers[0].problem.is_some());
    }

    #[test]
    fn the_filter_cycles_wrap_through_all() {
        let mut filters = Filters::default();
        let projects = [project("proj_1", "one")];
        filters.cycle_project(&projects);
        assert_eq!(filters.project(), "proj_1");
        filters.cycle_project(&projects);
        assert_eq!(filters.project(), "");

        filters.cycle_provider();
        assert_eq!(filters.provider(), "github");
        for _ in 0..ACTIVITY_PROVIDERS.len() {
            filters.cycle_provider();
        }
        assert_eq!(filters.provider(), "");

        filters.cycle_status();
        assert_eq!(filters.status(), "pending");
        filters.clear();
        assert!(!filters.is_active());
    }
}
