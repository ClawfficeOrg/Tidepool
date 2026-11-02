//! What the pool has been asked to do, as the menu bar shows it.
//!
//! The controller is deliberately headless: [`RunsController`] owns a
//! [`RunsClient`] and the state a surface draws, and [`crate::runs_view`] turns
//! that state into rows. The tests script the client, so filtering, the live
//! detail, the local-run mark and the failure wording are all exercised without
//! a socket.
//!
//! # Filters combine, and a foreign row is never shown
//!
//! [`ActivityFilters`] narrows one union of receipts, runs, steps and
//! executions. Every active filter must match, and [`RunsController::refresh`]
//! filters the page the pool sent before it stores it, so a pool that ignored a
//! filter (or a page that arrived after the filter changed) cannot show another
//! project's runs. Changing a filter prunes the rows already held for the same
//! reason.
//!
//! # A running execution follows itself
//!
//! [`RunsController::follow`] reads an open run again until the run and every
//! step are terminal, then stops. It is driven by an injected [`Instant`] so a
//! test can prove both the update and the clean stop without sleeping.
//!
//! # A run on this machine says so
//!
//! [`place_of`] compares each step's host with the host this client enrolled
//! ([`RunsController::set_local_host`]). A run whose steps name this machine is
//! [`RunPlace::ThisMachine`]; a run on another host keeps that host's name; a
//! run with no host recorded claims neither.
//!
//! # A failure code is a sentence with the code beside it
//!
//! The pool stores a stable code with a step failure
//! (`daemon_not_connected: host 'studio-mac' is offline`). [`failure_explanation`]
//! turns the code into words a person understands and keeps the code available
//! for a bug report, and [`FailureExplanation::line`] shows both.

pub mod http;

use std::collections::BTreeSet;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use http::HttpRuns;

/// How often the activity feed is read again.
pub const FEED_INTERVAL: Duration = Duration::from_secs(5);
/// How often a live run detail is read again.
pub const LIVE_INTERVAL: Duration = Duration::from_secs(1);
/// How many trailing output lines one step shows.
pub const TAIL_LINES: usize = 8;
/// Entries asked of one activity page.
pub const ACTIVITY_PAGE_SIZE: u32 = 50;

/// Every provider a receipt or run can name.
pub const ACTIVITY_PROVIDERS: [&str; 3] = ["github", "slack", "discord"];

/// Every status the pool's activity filter accepts.
pub const ACTIVITY_STATUSES: [&str; 11] = [
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
];

/// What could not be done with the pool's runs and activity routes.
///
/// No variant carries a credential or a request body.
#[derive(Debug, Error)]
pub enum RunsError {
    /// The pool URL is not an `http`/`https` URL with a host.
    #[error("`{url}` is not a usable pool URL (expected http:// or https:// and a host)")]
    InvalidPoolUrl {
        /// The rejected input.
        url: String,
    },
    /// The pool could not be reached, or the connection failed.
    #[error("the pool could not be reached: {detail}")]
    Unreachable {
        /// The transport's description.
        detail: String,
    },
    /// The pool answered `401`: the session has ended or the credentials are wrong.
    #[error("the pool did not accept these credentials")]
    Unauthorized,
    /// The pool answered `403`: the operator's role may not do this.
    #[error("the pool refused this action: {detail}")]
    Forbidden {
        /// The problem detail, when the pool sent one.
        detail: String,
    },
    /// The pool answered with an error problem document.
    #[error("the pool answered {status} ({code}){detail}")]
    Problem {
        /// HTTP status.
        status: u16,
        /// The pool's problem code, or the status reason when absent.
        code: String,
        /// The problem detail, empty when absent.
        detail: String,
    },
    /// A response was not the documented JSON shape.
    #[error("the pool's answer was not understood: {detail}")]
    Protocol {
        /// What was wrong.
        detail: String,
    },
    /// A request was refused before it left this machine.
    #[error("this request is not valid: {detail}")]
    InvalidRequest {
        /// What is missing or wrong.
        detail: String,
    },
    /// An id is not safe to place in a path.
    #[error("`{id}` is not a usable id")]
    InvalidId {
        /// The rejected input.
        id: String,
    },
}

/// One activity entry, as `GET /v1/activity` reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    /// The record's public id.
    #[serde(default)]
    pub id: String,
    /// `receipt`, `run`, `step` or `execution`.
    #[serde(default)]
    pub kind: String,
    /// The record's status, kind-specific.
    #[serde(default)]
    pub status: String,
    /// The project's public id.
    #[serde(default)]
    pub project: Option<String>,
    /// The event provider.
    #[serde(default)]
    pub provider: Option<String>,
    /// The trigger's public id.
    #[serde(default)]
    pub trigger: Option<String>,
    /// The trigger's authored name.
    #[serde(default)]
    pub trigger_name: Option<String>,
    /// The run this record belongs to.
    #[serde(default)]
    pub run: Option<String>,
    /// The step this record belongs to.
    #[serde(default)]
    pub step: Option<String>,
    /// The host execution this record belongs to.
    #[serde(default)]
    pub execution: Option<String>,
    /// The provider event type.
    #[serde(default)]
    pub event_type: Option<String>,
    /// The person or agent that caused it.
    #[serde(default)]
    pub actor: Option<String>,
    /// Whether the trigger matched.
    #[serde(default)]
    pub matched: Option<bool>,
    /// How many duplicate deliveries were coalesced.
    #[serde(default)]
    pub duplicates: Option<i64>,
    /// A step's output.
    #[serde(default)]
    pub output: Option<String>,
    /// A failure's stored text.
    #[serde(default)]
    pub error: Option<String>,
    /// A stored continuation.
    #[serde(default)]
    pub continuation: Option<String>,
    /// When it happened.
    #[serde(default)]
    pub at: String,
}

/// One cursor-paged activity window.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPage {
    /// The records in this window.
    #[serde(default)]
    pub items: Vec<ActivityEntry>,
    /// The cursor for the next window, when there is one.
    #[serde(default)]
    pub next_cursor: Option<String>,
    /// Whether another window exists.
    #[serde(default)]
    pub has_more: bool,
}

/// One step of a workflow run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunStepView {
    /// The step's public id.
    #[serde(default)]
    pub id: String,
    /// The authored step id.
    #[serde(default)]
    pub step: String,
    /// Its position in the workflow.
    #[serde(default)]
    pub position: i64,
    /// `pending`, `running`, `succeeded`, `failed`, `skipped` or `interrupted`.
    #[serde(default)]
    pub status: String,
    /// The host the step is assigned to, when one was chosen.
    #[serde(default)]
    pub host: Option<String>,
    /// The step's output.
    #[serde(default)]
    pub output: Option<String>,
    /// The failure's stored text, code first.
    #[serde(default)]
    pub error: Option<String>,
    /// A stable failure reason, when one was recorded.
    #[serde(default)]
    pub reason: Option<String>,
    /// When the step started.
    #[serde(default)]
    pub started_at: Option<String>,
    /// When the step finished.
    #[serde(default)]
    pub completed_at: Option<String>,
}

/// One workflow run with its steps.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunView {
    /// The run's public id.
    #[serde(default)]
    pub id: String,
    /// The project's public id.
    #[serde(default)]
    pub project: String,
    /// The trigger's public id.
    #[serde(default)]
    pub trigger: String,
    /// The trigger's authored name.
    #[serde(default)]
    pub trigger_name: String,
    /// The workflow's authored name.
    #[serde(default)]
    pub workflow: String,
    /// `event` or `manual`.
    #[serde(default)]
    pub origin: String,
    /// The person or trigger that caused it.
    #[serde(default)]
    pub actor: Option<String>,
    /// The idempotency key for the arrival.
    #[serde(default)]
    pub delivery_key: String,
    /// `pending`, `running`, `succeeded`, `failed` or `interrupted`.
    #[serde(default)]
    pub status: String,
    /// The request text the agent receives.
    #[serde(default)]
    pub request_text: String,
    /// The run's declared inputs, when the pool returns them.
    #[serde(default)]
    pub inputs: std::collections::BTreeMap<String, String>,
    /// When the run was created.
    #[serde(default)]
    pub created_at: String,
    /// When the run started.
    #[serde(default)]
    pub started_at: Option<String>,
    /// When the run finished.
    #[serde(default)]
    pub completed_at: Option<String>,
    /// The run's steps.
    #[serde(default)]
    pub steps: Vec<RunStepView>,
}

/// How an arrival created, continued or restored an agent.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrivalDetail {
    /// `created`, `continued` or `restored`.
    #[serde(default)]
    pub mode: String,
    /// The provider that ran the agent.
    #[serde(default)]
    pub provider: Option<String>,
    /// The conversation the arrival reused.
    #[serde(default)]
    pub conversation_key: Option<String>,
    /// Whether the arrival steered a conversation.
    #[serde(default)]
    pub steered: bool,
}

/// One run plus the arrival that started it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRunDetail {
    /// The run.
    #[serde(default)]
    pub run: RunView,
    /// The arrival that started it.
    #[serde(default)]
    pub arrival: ArrivalDetail,
}

/// A manual run request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualDispatchRequest {
    /// The project the run belongs to.
    pub project: String,
    /// The installed trigger to run.
    pub trigger: String,
    /// The person asking for the run.
    pub actor: String,
    /// An optional configuration revision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Stable per request; a repeat returns the existing run.
    pub delivery_key: String,
    /// An optional input for the trigger.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
}

impl ManualDispatchRequest {
    /// Validate the fields the pool requires before the request leaves.
    pub fn validate(&self) -> Result<(), RunsError> {
        for (field, value) in [
            ("project", &self.project),
            ("trigger", &self.trigger),
            ("actor", &self.actor),
            ("delivery key", &self.delivery_key),
        ] {
            if value.trim().is_empty() {
                return Err(RunsError::InvalidRequest {
                    detail: format!("a manual dispatch needs a {field}"),
                });
            }
        }
        Ok(())
    }
}

/// The run a manual request created or matched.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualDispatchResponse {
    /// The run.
    #[serde(default)]
    pub run: RunView,
    /// True on an idempotent replay.
    #[serde(default)]
    pub deduplicated: bool,
}

/// The activity query sent to `GET /v1/activity`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityQuery {
    /// Project public id.
    pub project: Option<String>,
    /// Provider kind, lowercase.
    pub provider: Option<String>,
    /// Trigger public id or name.
    pub trigger: Option<String>,
    /// Record status.
    pub status: Option<String>,
    /// Page size, 1..=100.
    pub limit: u32,
}

impl ActivityQuery {
    /// The non-empty filters as query pairs, limit last.
    pub fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        for (name, value) in [
            ("project", &self.project),
            ("provider", &self.provider),
            ("trigger", &self.trigger),
            ("status", &self.status),
        ] {
            if let Some(value) = value.as_deref().filter(|value| !value.trim().is_empty()) {
                pairs.push((name, value.to_owned()));
            }
        }
        if self.limit > 0 {
            pairs.push(("limit", self.limit.to_string()));
        }
        pairs
    }
}

/// What the runs surface needs from a pool.
///
/// One method per route, so a scripted implementation in the tests has the same
/// shape as [`HttpRuns`] and cannot accidentally share logic with it.
pub trait RunsClient {
    /// Read one page of the combined activity feed.
    fn list_activity(&mut self, query: &ActivityQuery) -> Result<ActivityPage, RunsError>;
    /// Read one run plus the arrival that started it.
    fn run_detail(&mut self, run: &str) -> Result<ActivityRunDetail, RunsError>;
    /// Start a workflow run from a manual request.
    fn dispatch(
        &mut self,
        request: &ManualDispatchRequest,
    ) -> Result<ManualDispatchResponse, RunsError>;
}

/// The four combined filters. An empty string means "any".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActivityFilters {
    project: String,
    provider: String,
    trigger: String,
    status: String,
}

impl ActivityFilters {
    /// The selected project id, or an empty string for every project.
    pub fn project(&self) -> &str {
        &self.project
    }

    /// The selected provider, lowercase.
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// The selected trigger id or name.
    pub fn trigger(&self) -> &str {
        &self.trigger
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

    /// Point the project filter at one project. An empty value clears it.
    pub fn set_project(&mut self, project: &str) {
        self.project = project.trim().to_owned();
    }

    /// Point the provider filter at one provider, lowercase. Empty clears it.
    pub fn set_provider(&mut self, provider: &str) {
        self.provider = provider.trim().to_ascii_lowercase();
    }

    /// Point the trigger filter at one trigger id or name. Empty clears it.
    pub fn set_trigger(&mut self, trigger: &str) {
        self.trigger = trigger.trim().to_owned();
    }

    /// Point the status filter at one status, lowercase. Empty clears it.
    pub fn set_status(&mut self, status: &str) {
        self.status = status.trim().to_ascii_lowercase();
    }

    /// The query the pool receives: only non-empty filters are sent.
    pub fn query(&self) -> ActivityQuery {
        ActivityQuery {
            project: non_empty(&self.project),
            provider: non_empty(&self.provider),
            trigger: non_empty(&self.trigger),
            status: non_empty(&self.status),
            limit: ACTIVITY_PAGE_SIZE,
        }
    }

    /// True when one entry satisfies every active filter.
    pub fn matches(&self, entry: &ActivityEntry) -> bool {
        if !self.project.is_empty() && entry.project.as_deref() != Some(self.project.as_str()) {
            return false;
        }
        if !self.provider.is_empty()
            && !entry
                .provider
                .as_deref()
                .is_some_and(|provider| provider.eq_ignore_ascii_case(&self.provider))
        {
            return false;
        }
        if !self.trigger.is_empty() {
            let id_matches = entry.trigger.as_deref() == Some(self.trigger.as_str());
            let name_matches = entry.trigger_name.as_deref() == Some(self.trigger.as_str());
            if !id_matches && !name_matches {
                return false;
            }
        }
        if !self.status.is_empty() && !entry.status.eq_ignore_ascii_case(&self.status) {
            return false;
        }
        true
    }

    /// The filter combination as one readable line.
    pub fn summary(&self) -> String {
        format!(
            "project {} · provider {} · trigger {} · status {}",
            or_all(&self.project, "all"),
            if self.provider.is_empty() {
                "all".to_owned()
            } else {
                provider_word(&self.provider).to_lowercase()
            },
            or_all(&self.trigger, "all"),
            if self.status.is_empty() {
                "any".to_owned()
            } else {
                status_word(&self.status)
            }
        )
    }

    /// Move the project filter to the next project, wrapping through "all".
    pub fn cycle_project(&mut self, projects: &[String]) {
        let mut values = vec![String::new()];
        values.extend(projects.iter().cloned());
        self.project = next_in(&values, &self.project);
    }

    /// Move the provider filter to the next provider.
    pub fn cycle_provider(&mut self) {
        let mut values = vec![String::new()];
        values.extend(ACTIVITY_PROVIDERS.iter().map(|value| (*value).to_owned()));
        self.provider = next_in(&values, &self.provider);
    }

    /// Move the trigger filter to the next trigger, wrapping through "all".
    pub fn cycle_trigger(&mut self, triggers: &[String]) {
        let mut values = vec![String::new()];
        values.extend(triggers.iter().cloned());
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
    match provider.to_ascii_lowercase().as_str() {
        "github" => "GitHub".to_owned(),
        "slack" => "Slack".to_owned(),
        "discord" => "Discord".to_owned(),
        "" => "Unknown provider".to_owned(),
        other => other.to_owned(),
    }
}

/// The words a status is rendered with.
pub fn status_word(status: &str) -> String {
    match status.to_ascii_lowercase().as_str() {
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
        _ => status.to_owned(),
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
    matches!(
        status.to_ascii_lowercase().as_str(),
        "pending" | "running" | "queued"
    )
}

/// True when a run detail still needs following.
///
/// A run that says `succeeded` but still has a running step is live too: the run
/// status is written when the last step finishes, and stopping on the run word
/// alone would miss that step's output.
pub fn run_is_live(run: &RunView) -> bool {
    live_status(&run.status) || run.steps.iter().any(|step| live_status(&step.status))
}

/// Where a run's work happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunPlace {
    /// At least one step names the host this client enrolled.
    ThisMachine,
    /// Every named step belongs to another host; this is the first named one.
    AnotherMachine {
        /// The other host's public id or slug.
        host: String,
    },
    /// The pool recorded no host for any step.
    Unrecorded,
}

impl RunPlace {
    /// The words the place is shown with.
    pub fn label(&self) -> String {
        match self {
            Self::ThisMachine => "This machine".to_owned(),
            Self::AnotherMachine { host } => format!("Another machine ({host})"),
            Self::Unrecorded => "No host recorded".to_owned(),
        }
    }

    /// Whether this run is the machine's own work.
    pub fn is_local(&self) -> bool {
        matches!(self, Self::ThisMachine)
    }
}

/// Work out where a run happens relative to this machine's host.
///
/// A run with any step on this machine is this machine's own work, even when
/// another step names a different host. Without a known local host nothing can
/// be claimed as local.
pub fn place_of(run: &RunView, local_host: Option<&str>) -> RunPlace {
    let mut first_other: Option<String> = None;
    for step in &run.steps {
        let Some(host) = step.host.as_deref().filter(|host| !host.trim().is_empty()) else {
            continue;
        };
        if local_host.is_some_and(|local| local == host) {
            return RunPlace::ThisMachine;
        }
        if first_other.is_none() {
            first_other = Some(host.to_owned());
        }
    }
    match first_other {
        Some(host) => RunPlace::AnotherMachine { host },
        None => RunPlace::Unrecorded,
    }
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

impl FailureExplanation {
    /// The sentence with the code beside it, for a row that keeps both.
    pub fn line(&self) -> String {
        match self.code.as_deref() {
            Some(code) => format!("{} (code {code})", self.sentence),
            None => self.sentence.clone(),
        }
    }
}

/// The code-to-sentence table, mirroring the pool's named failures.
pub const FAILURE_SENTENCES: [(&str, &str); 6] = [
    (
        "daemon_not_connected",
        "The host was offline when this step was due, so the step never ran. Bring the host online, then dispatch the run again.",
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

/// One manual dispatch, as the form holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchForm {
    project: String,
    trigger: String,
    actor: String,
    delivery_key: String,
    input: String,
    revision: String,
}

impl DispatchForm {
    /// A form prefilled with the fields a menu-bar dispatch needs.
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
        }
    }

    /// The project the run belongs to.
    pub fn project(&self) -> &str {
        &self.project
    }

    /// The installed trigger to run.
    pub fn trigger(&self) -> &str {
        &self.trigger
    }

    /// The person asking for the run.
    pub fn actor(&self) -> &str {
        &self.actor
    }

    /// The stable idempotency key.
    pub fn delivery_key(&self) -> &str {
        &self.delivery_key
    }

    /// The optional trigger input.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// The optional configuration revision.
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Set the optional trigger input.
    pub fn set_input(&mut self, input: impl Into<String>) {
        self.input = input.into();
    }

    /// Set the optional configuration revision.
    pub fn set_revision(&mut self, revision: impl Into<String>) {
        self.revision = revision.into();
    }

    /// Set the actor the trigger must allow.
    pub fn set_actor(&mut self, actor: impl Into<String>) {
        self.actor = actor.into();
    }

    /// The request this form describes.
    pub fn request(&self) -> ManualDispatchRequest {
        ManualDispatchRequest {
            project: self.project.trim().to_owned(),
            trigger: self.trigger.trim().to_owned(),
            actor: self.actor.trim().to_owned(),
            revision: non_empty(&self.revision),
            delivery_key: self.delivery_key.trim().to_owned(),
            input: non_empty(&self.input),
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
    format!("client-{who}-{now_ms}-{sequence}")
}

/// The current wall clock in milliseconds, for a delivery key.
pub fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default()
}

/// One open run detail and how it is being followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunDetail {
    /// The run as the pool last described it.
    pub run: RunView,
    /// Whether the run is still being read again.
    pub following: bool,
    /// Why the last read failed, when it did.
    pub problem: Option<String>,
}

/// Everything a surface needs to draw the runs pane.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunsSnapshot {
    /// The filter combination in use.
    pub filters: ActivityFilters,
    /// The visible activity rows.
    pub entries: Vec<ActivityEntry>,
    /// Runs this client has seen running on this machine.
    pub local_runs: BTreeSet<String>,
    /// The host this machine enrolled, when known.
    pub local_host: Option<String>,
    /// The open run, when one is open.
    pub detail: Option<RunDetail>,
    /// The dispatch form, while it is being edited.
    pub dispatch: Option<DispatchForm>,
    /// The last problem worth telling the person.
    pub notice: Option<String>,
}

#[derive(Debug, Clone)]
struct Selected {
    run_id: String,
    run: Option<RunView>,
    following: bool,
    last_poll: Option<Instant>,
    problem: Option<String>,
}

/// The client's runs and activity loop.
pub struct RunsController<C: RunsClient> {
    client: C,
    local_host: Option<String>,
    filters: ActivityFilters,
    entries: Vec<ActivityEntry>,
    local_runs: BTreeSet<String>,
    selected: usize,
    detail: Option<Selected>,
    dispatch: Option<DispatchForm>,
    notice: Option<String>,
    last_feed: Option<Instant>,
}

impl<C: RunsClient> std::fmt::Debug for RunsController<C> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RunsController")
            .field("local_host", &self.local_host)
            .field("filters", &self.filters)
            .field("entries", &self.entries.len())
            .field("selected", &self.selected)
            .field("detail", &self.detail.as_ref().map(|detail| &detail.run_id))
            .field("notice", &self.notice)
            .finish_non_exhaustive()
    }
}

impl<C: RunsClient> RunsController<C> {
    /// Build the loop over a pool client and, when known, this machine's host id.
    pub fn new(client: C, local_host: Option<String>) -> Self {
        Self {
            client,
            local_host,
            filters: ActivityFilters::default(),
            entries: Vec::new(),
            local_runs: BTreeSet::new(),
            selected: 0,
            detail: None,
            dispatch: None,
            notice: None,
            last_feed: None,
        }
    }

    /// Remember which host this machine enrolled, so its runs are marked local.
    pub fn set_local_host(&mut self, local_host: Option<String>) {
        self.local_host = local_host;
    }

    /// The filter combination in use.
    pub fn filters(&self) -> &ActivityFilters {
        &self.filters
    }

    /// Point the project filter at one project and drop rows it hides.
    pub fn set_project_filter(&mut self, project: &str) {
        self.filters.set_project(project);
        self.after_filter_change();
    }

    /// Point the provider filter at one provider and drop rows it hides.
    pub fn set_provider_filter(&mut self, provider: &str) {
        self.filters.set_provider(provider);
        self.after_filter_change();
    }

    /// Point the trigger filter at one trigger and drop rows it hides.
    pub fn set_trigger_filter(&mut self, trigger: &str) {
        self.filters.set_trigger(trigger);
        self.after_filter_change();
    }

    /// Point the status filter at one status and drop rows it hides.
    pub fn set_status_filter(&mut self, status: &str) {
        self.filters.set_status(status);
        self.after_filter_change();
    }

    /// Drop every filter.
    pub fn clear_filters(&mut self) {
        self.filters.clear();
        self.after_filter_change();
    }

    /// The activity rows the pool last reported, already filtered.
    pub fn entries(&self) -> &[ActivityEntry] {
        &self.entries
    }

    /// The visible rows: every active filter must match.
    ///
    /// This is defence in depth. [`Self::refresh`] filters what it stores, and
    /// this filters again so a stale page or a changed filter cannot draw a
    /// foreign project's run even for one frame.
    pub fn visible_entries(&self) -> Vec<&ActivityEntry> {
        self.entries
            .iter()
            .filter(|entry| self.filters.matches(entry))
            .collect()
    }

    /// The project ids seen in the current feed, for a project filter.
    pub fn known_projects(&self) -> Vec<String> {
        let mut projects: Vec<String> = self
            .entries
            .iter()
            .filter_map(|entry| entry.project.clone())
            .collect();
        projects.sort();
        projects.dedup();
        projects
    }

    /// The trigger names seen in the current feed, for a trigger filter.
    pub fn known_triggers(&self) -> Vec<String> {
        let mut triggers: Vec<String> = self
            .entries
            .iter()
            .filter_map(|entry| entry.trigger_name.clone())
            .collect();
        triggers.sort();
        triggers.dedup();
        triggers
    }

    /// The index of the highlighted feed row.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The highlighted feed row.
    pub fn selected_entry(&self) -> Option<&ActivityEntry> {
        self.visible_entries().get(self.selected).copied()
    }

    /// The open run detail, once it has loaded.
    pub fn detail(&self) -> Option<RunDetail> {
        self.detail.as_ref().and_then(|selected| {
            selected.run.as_ref().map(|run| RunDetail {
                run: run.clone(),
                following: selected.following,
                problem: selected.problem.clone(),
            })
        })
    }

    /// The id of the open run.
    pub fn selected_run_id(&self) -> Option<&str> {
        self.detail
            .as_ref()
            .map(|selected| selected.run_id.as_str())
    }

    /// Whether the open run is still being followed.
    pub fn following(&self) -> bool {
        self.detail
            .as_ref()
            .is_some_and(|selected| selected.following)
    }

    /// Why the open run's last read failed, when it did.
    pub fn detail_problem(&self) -> Option<&str> {
        self.detail
            .as_ref()
            .and_then(|selected| selected.problem.as_deref())
    }

    /// Whether a run has been seen running on this machine.
    pub fn is_local_run(&self, run: &str) -> bool {
        self.local_runs.contains(run)
    }

    /// The runs seen running on this machine.
    pub fn local_runs(&self) -> &BTreeSet<String> {
        &self.local_runs
    }

    /// The open dispatch form, while it is on screen.
    pub fn dispatch_form(&self) -> Option<&DispatchForm> {
        self.dispatch.as_ref()
    }

    /// The last action's outcome, when it was not a plain success.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The state a surface draws.
    pub fn snapshot(&self) -> RunsSnapshot {
        RunsSnapshot {
            filters: self.filters.clone(),
            entries: self.visible_entries().into_iter().cloned().collect(),
            local_runs: self.local_runs.clone(),
            local_host: self.local_host.clone(),
            detail: self.detail(),
            dispatch: self.dispatch.clone(),
            notice: self.notice.clone(),
        }
    }

    /// Re-read the activity feed with the active filters.
    ///
    /// Returns how many rows are visible. A page is filtered again on arrival,
    /// so a pool that ignored a filter cannot put another project's run on
    /// screen.
    pub fn refresh(&mut self, now: Instant) -> Result<usize, RunsError> {
        let query = self.filters.query();
        match self.client.list_activity(&query) {
            Ok(page) => {
                self.entries = page
                    .items
                    .into_iter()
                    .filter(|entry| self.filters.matches(entry))
                    .collect();
                self.clamp_selection();
                self.last_feed = Some(now);
                Ok(self.entries.len())
            }
            Err(error) => {
                self.last_feed = Some(now);
                self.record(&error);
                Err(error)
            }
        }
    }

    /// Refresh and follow when their timers are due.
    pub fn tick(&mut self, now: Instant) -> Result<(), RunsError> {
        let feed_due = self
            .last_feed
            .is_none_or(|last| now.saturating_duration_since(last) >= FEED_INTERVAL);
        let mut failure = None;
        if feed_due && let Err(error) = self.refresh(now) {
            failure = Some(error);
        }
        self.follow(now);
        match failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// Read the open run again when it is due, until it reaches a terminal state.
    ///
    /// Returns whether the run is still being followed. Once the run and every
    /// step are terminal this makes no further request, so a finished run stops
    /// cleanly instead of polling forever.
    pub fn follow(&mut self, now: Instant) -> bool {
        let Some(selected) = self.detail.as_ref() else {
            return false;
        };
        if !selected.following {
            return false;
        }
        if selected
            .last_poll
            .is_some_and(|last| now.saturating_duration_since(last) < LIVE_INTERVAL)
        {
            return true;
        }
        let run_id = selected.run_id.clone();
        if let Some(selected) = self.detail.as_mut() {
            selected.last_poll = Some(now);
        }
        match self.client.run_detail(&run_id) {
            Ok(answer) => {
                let run = answer.run;
                let following = run_is_live(&run);
                self.note_local(&run);
                if let Some(selected) = self.detail.as_mut() {
                    selected.run = Some(run);
                    selected.following = following;
                    selected.problem = None;
                }
                following
            }
            Err(error) => {
                self.record(&error);
                let message = error.to_string();
                if let Some(selected) = self.detail.as_mut() {
                    selected.problem = Some(message);
                    selected.following = false;
                }
                false
            }
        }
    }

    /// Open a run and read it on the next [`Self::follow`].
    pub fn open_run(&mut self, run_id: impl Into<String>) {
        self.detail = Some(Selected {
            run_id: run_id.into(),
            run: None,
            following: true,
            last_poll: None,
            problem: None,
        });
    }

    /// Close the open run.
    pub fn close_run(&mut self) {
        self.detail = None;
    }

    /// Begin a manual dispatch, prefilled for the selected project and trigger.
    ///
    /// The delivery key is stable for this form, so pressing dispatch twice
    /// cannot start two runs.
    pub fn begin_dispatch(&mut self, actor: &str, now_ms: u128) {
        let selected_project = self.filters.project().to_owned();
        let project = if selected_project.is_empty() {
            self.known_projects().first().cloned().unwrap_or_default()
        } else {
            selected_project
        };
        let selected_trigger = self.filters.trigger().to_owned();
        let trigger = if selected_trigger.is_empty() {
            self.known_triggers().first().cloned().unwrap_or_default()
        } else {
            selected_trigger
        };
        let key = manual_delivery_key(actor, now_ms, self.entries.len() as u64);
        self.dispatch = Some(DispatchForm::new(project, trigger, actor, key));
    }

    /// Edit the open dispatch form, when one is open.
    pub fn dispatch_form_mut(&mut self) -> Option<&mut DispatchForm> {
        self.dispatch.as_mut()
    }

    /// Close the dispatch form without sending anything.
    pub fn cancel_dispatch(&mut self) {
        self.dispatch = None;
    }

    /// Send the open dispatch form and open the run it returns.
    pub fn submit_dispatch(&mut self, now: Instant) -> Result<RunView, RunsError> {
        let Some(form) = self.dispatch.as_ref() else {
            return Err(RunsError::InvalidRequest {
                detail: "no dispatch form is open".to_owned(),
            });
        };
        let request = form.request();
        if let Err(error) = request.validate() {
            self.record(&error);
            return Err(error);
        }
        match self.client.dispatch(&request) {
            Ok(response) => {
                let run = response.run;
                let following = run_is_live(&run);
                self.dispatch = None;
                self.detail = Some(Selected {
                    run_id: run.id.clone(),
                    run: Some(run.clone()),
                    following,
                    last_poll: Some(now),
                    problem: None,
                });
                self.note_local(&run);
                self.notice = Some(if response.deduplicated {
                    format!(
                        "This request had already run, so the pool returned run {}.",
                        run.id
                    )
                } else {
                    format!("The pool started run {}.", run.id)
                });
                self.last_feed = None;
                Ok(run)
            }
            Err(error) => {
                self.record(&error);
                Err(error)
            }
        }
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

    fn after_filter_change(&mut self) {
        self.selected = 0;
        // A row that no longer matches is dropped now, not when the next page
        // arrives, so another project's run never survives the filter change.
        self.entries.retain(|entry| self.filters.matches(entry));
        self.last_feed = None;
    }

    fn clamp_selection(&mut self) {
        let count = self.visible_entries().len();
        self.selected = if count == 0 {
            0
        } else {
            self.selected.min(count - 1)
        };
    }

    fn note_local(&mut self, run: &RunView) {
        let local = self.local_host.as_deref();
        if place_of(run, local).is_local() {
            self.local_runs.insert(run.id.clone());
        }
    }

    fn record(&mut self, error: &RunsError) {
        self.notice = Some(error.to_string());
    }

    /// The client, for the tests' call log.
    #[cfg(test)]
    pub(crate) fn client(&self) -> &C {
        &self.client
    }
}

/// Everything the tests need from a pool, without a socket.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct ScriptedRuns {
    pub(crate) calls: Vec<String>,
    pub(crate) pages: VecDeque<Result<ActivityPage, RunsError>>,
    pub(crate) details: VecDeque<Result<ActivityRunDetail, RunsError>>,
    pub(crate) dispatched: VecDeque<Result<ManualDispatchResponse, RunsError>>,
}

#[cfg(test)]
impl ScriptedRuns {
    fn pop<T>(queue: &mut VecDeque<Result<T, RunsError>>, what: &str) -> Result<T, RunsError> {
        queue.pop_front().unwrap_or_else(|| {
            Err(RunsError::Protocol {
                detail: format!("unscripted {what} call"),
            })
        })
    }
}

#[cfg(test)]
impl RunsClient for ScriptedRuns {
    fn list_activity(&mut self, query: &ActivityQuery) -> Result<ActivityPage, RunsError> {
        self.calls.push(format!("activity:{}", query.pairs().len()));
        Self::pop(&mut self.pages, "activity")
    }

    fn run_detail(&mut self, run: &str) -> Result<ActivityRunDetail, RunsError> {
        self.calls.push(format!("detail:{run}"));
        Self::pop(&mut self.details, "detail")
    }

    fn dispatch(
        &mut self,
        request: &ManualDispatchRequest,
    ) -> Result<ManualDispatchResponse, RunsError> {
        self.calls.push(format!(
            "dispatch:{}:{}:{}",
            request.project, request.trigger, request.delivery_key
        ));
        Self::pop(&mut self.dispatched, "dispatch")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
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

    fn page(items: Vec<ActivityEntry>) -> ActivityPage {
        ActivityPage {
            items,
            next_cursor: None,
            has_more: false,
        }
    }

    fn step(id: &str, status: &str, host: Option<&str>) -> RunStepView {
        RunStepView {
            id: id.to_owned(),
            step: id.to_owned(),
            position: 1,
            status: status.to_owned(),
            host: host.map(str::to_owned),
            ..RunStepView::default()
        }
    }

    fn run(id: &str, status: &str, steps: Vec<RunStepView>) -> RunView {
        RunView {
            id: id.to_owned(),
            project: "proj_1".to_owned(),
            trigger: "on-push".to_owned(),
            trigger_name: "on-push".to_owned(),
            workflow: "deploy".to_owned(),
            origin: "manual".to_owned(),
            actor: Some("alice".to_owned()),
            delivery_key: "key-1".to_owned(),
            request_text: "deploy".to_owned(),
            created_at: "2026-10-29T00:00:00Z".to_owned(),
            status: status.to_owned(),
            steps,
            ..RunView::default()
        }
    }

    fn detail(run: RunView) -> ActivityRunDetail {
        ActivityRunDetail {
            arrival: ArrivalDetail::default(),
            run,
        }
    }

    fn controller() -> RunsController<ScriptedRuns> {
        RunsController::new(ScriptedRuns::default(), Some("host-local".to_owned()))
    }

    fn scripted_page(controller: &mut RunsController<ScriptedRuns>, items: Vec<ActivityEntry>) {
        controller.client.pages.push_back(Ok(page(items)));
    }

    #[test]
    fn filters_combine_and_a_foreign_project_is_never_visible() {
        let mut controller = controller();
        let foreign_and_local = vec![
            entry(
                "run-1",
                "run",
                "succeeded",
                Some("proj_1"),
                Some("github"),
                Some("on-push"),
            ),
            entry(
                "run-2",
                "run",
                "failed",
                Some("proj_1"),
                Some("slack"),
                Some("on-push"),
            ),
            entry(
                "run-3",
                "run",
                "succeeded",
                Some("proj_2"),
                Some("github"),
                Some("on-push"),
            ),
            entry(
                "run-4",
                "run",
                "succeeded",
                Some("proj_1"),
                Some("github"),
                Some("on-release"),
            ),
        ];
        // The pool ignores the query and returns every project, as a stale or
        // misbehaving page would.
        scripted_page(&mut controller, foreign_and_local);

        controller.set_project_filter("proj_1");
        controller.set_provider_filter("github");
        controller.set_status_filter("succeeded");
        assert_eq!(controller.refresh(Instant::now()).expect("refresh"), 2);
        assert_eq!(
            controller
                .visible_entries()
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["run-1", "run-4"]
        );
        assert!(
            controller
                .entries()
                .iter()
                .all(|entry| entry.project.as_deref() == Some("proj_1")),
            "another project's run must never be held, let alone shown"
        );

        controller.set_trigger_filter("on-push");
        assert_eq!(controller.visible_entries().len(), 1);
        assert_eq!(controller.visible_entries()[0].id, "run-1");
        assert_eq!(
            controller.selected_entry().map(|entry| entry.id.as_str()),
            Some("run-1")
        );
    }

    #[test]
    fn changing_the_project_filter_drops_rows_before_the_next_page() {
        let mut controller = controller();
        scripted_page(
            &mut controller,
            vec![
                entry(
                    "run-1",
                    "run",
                    "succeeded",
                    Some("proj_1"),
                    Some("github"),
                    Some("on-push"),
                ),
                entry(
                    "run-2",
                    "run",
                    "succeeded",
                    Some("proj_2"),
                    Some("github"),
                    Some("on-push"),
                ),
            ],
        );
        controller.refresh(Instant::now()).expect("refresh");
        assert_eq!(controller.visible_entries().len(), 2);

        controller.set_project_filter("proj_2");
        assert_eq!(
            controller
                .visible_entries()
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["run-2"],
            "proj_1's row must disappear with the filter change, not after a poll"
        );

        controller.clear_filters();
        assert!(!controller.filters().is_active());
        // Cleared filters do not resurrect a row that was dropped.
        assert_eq!(controller.visible_entries().len(), 1);
    }

    #[test]
    fn the_query_only_carries_active_filters() {
        let mut filters = ActivityFilters::default();
        assert_eq!(
            filters.query().pairs(),
            vec![("limit", ACTIVITY_PAGE_SIZE.to_string())]
        );

        filters.set_project(" proj_1 ");
        filters.set_provider("GitHub");
        filters.set_trigger("on-push");
        filters.set_status("Failed");
        assert_eq!(filters.project(), "proj_1");
        assert_eq!(filters.provider(), "github");
        assert_eq!(filters.status(), "failed");
        assert_eq!(
            filters.query().pairs(),
            vec![
                ("project", "proj_1".to_owned()),
                ("provider", "github".to_owned()),
                ("trigger", "on-push".to_owned()),
                ("status", "failed".to_owned()),
                ("limit", ACTIVITY_PAGE_SIZE.to_string()),
            ]
        );
    }

    #[test]
    fn filters_cycle_through_every_value_and_back_to_all() {
        let mut filters = ActivityFilters::default();
        let projects = vec!["proj_1".to_owned(), "proj_2".to_owned()];
        filters.cycle_project(&projects);
        assert_eq!(filters.project(), "proj_1");
        filters.cycle_project(&projects);
        assert_eq!(filters.project(), "proj_2");
        filters.cycle_project(&projects);
        assert_eq!(filters.project(), "");

        filters.cycle_provider();
        assert_eq!(filters.provider(), "github");
        filters.cycle_status();
        assert_eq!(filters.status(), "pending");
        filters.cycle_trigger(&["on-push".to_owned()]);
        assert_eq!(filters.trigger(), "on-push");
    }

    #[test]
    fn a_running_execution_updates_live_and_stops_cleanly_when_it_ends() {
        let mut controller = controller();
        controller.client.details.push_back(Ok(detail(run(
            "run-1",
            "running",
            vec![step("build", "running", Some("host-local"))],
        ))));
        controller.client.details.push_back(Ok(detail(run(
            "run-1",
            "succeeded",
            vec![step("build", "succeeded", Some("host-local"))],
        ))));

        controller.open_run("run-1");
        let start = Instant::now();
        assert!(controller.follow(start), "a running run is followed");
        let first = controller.detail().expect("detail");
        assert_eq!(first.run.steps[0].status, "running");
        assert!(first.following);
        assert_eq!(controller.client().calls, vec!["detail:run-1"]);

        // A second read an interval later sees the finished step and stops.
        assert!(!controller.follow(start + Duration::from_secs(2)));
        let second = controller.detail().expect("detail");
        assert_eq!(second.run.steps[0].status, "succeeded");
        assert!(!second.following);

        // And a later tick does not ask the pool again.
        assert!(!controller.follow(start + Duration::from_secs(4)));
        assert_eq!(
            controller.client().calls,
            vec!["detail:run-1", "detail:run-1"],
            "a finished run must not keep polling"
        );
    }

    #[test]
    fn a_read_inside_the_interval_does_not_hit_the_pool() {
        let mut controller = controller();
        controller
            .client
            .details
            .push_back(Ok(detail(run("run-1", "running", Vec::new()))));
        controller.open_run("run-1");
        let start = Instant::now();
        assert!(controller.follow(start));
        assert!(controller.follow(start + Duration::from_millis(200)));
        assert_eq!(controller.client().calls.len(), 1);

        // The run itself still says running, so it stays live with no steps.
        controller
            .client
            .details
            .push_back(Ok(detail(run("run-1", "running", Vec::new()))));
        assert!(controller.follow(start + Duration::from_secs(2)));
        assert_eq!(controller.client().calls.len(), 2, "the next read was due");
    }

    #[test]
    fn a_failed_detail_read_stops_following_with_a_problem() {
        let mut controller = controller();
        controller
            .client
            .details
            .push_back(Err(RunsError::Unreachable {
                detail: "connection refused".to_owned(),
            }));
        controller.open_run("run-1");
        assert!(!controller.follow(Instant::now()));
        assert!(!controller.following());
        assert!(
            controller
                .detail_problem()
                .is_some_and(|problem| problem.contains("reached"))
        );
        assert!(controller.notice().is_some());
    }

    #[test]
    fn a_run_on_this_machine_is_told_apart_from_one_on_another() {
        let local = run(
            "run-local",
            "running",
            vec![step("build", "running", Some("host-local"))],
        );
        assert_eq!(place_of(&local, Some("host-local")), RunPlace::ThisMachine);
        assert!(place_of(&local, Some("host-local")).is_local());

        let remote = run(
            "run-remote",
            "running",
            vec![step("build", "running", Some("host-other"))],
        );
        assert_eq!(
            place_of(&remote, Some("host-local")),
            RunPlace::AnotherMachine {
                host: "host-other".to_owned()
            }
        );
        assert!(!place_of(&remote, Some("host-local")).is_local());
        assert_eq!(
            place_of(&remote, None),
            RunPlace::AnotherMachine {
                host: "host-other".to_owned()
            },
            "without a known local host the run keeps the other host's name"
        );

        let unrecorded = run(
            "run-unknown",
            "running",
            vec![step("build", "running", None)],
        );
        assert_eq!(
            place_of(&unrecorded, Some("host-local")),
            RunPlace::Unrecorded
        );
        assert!(!place_of(&unrecorded, None).is_local());
    }

    #[test]
    fn a_detail_marks_its_run_as_local_work() {
        let mut controller = controller();
        controller.client.details.push_back(Ok(detail(run(
            "run-local",
            "running",
            vec![
                step("prepare", "succeeded", Some("host-other")),
                step("build", "running", Some("host-local")),
            ],
        ))));
        controller.open_run("run-local");
        assert!(controller.follow(Instant::now()));
        assert!(controller.is_local_run("run-local"));
        assert!(controller.local_runs().contains("run-local"));

        let mut controller = self::controller();
        controller.client.details.push_back(Ok(detail(run(
            "run-remote",
            "running",
            vec![step("build", "running", Some("host-other"))],
        ))));
        controller.open_run("run-remote");
        controller.follow(Instant::now());
        assert!(!controller.is_local_run("run-remote"));
    }

    #[test]
    fn a_named_failure_code_renders_as_words_with_the_code_available() {
        let failure = failure_explanation(
            Some("daemon_not_connected: host 'studio-mac' is offline"),
            Some("daemon_not_connected"),
        )
        .expect("a named failure");
        assert_eq!(failure.code.as_deref(), Some("daemon_not_connected"));
        assert!(
            failure.sentence.to_lowercase().contains("offline"),
            "the sentence must say the host was offline: {}",
            failure.sentence
        );
        assert!(
            failure.sentence.contains("host"),
            "the sentence must name the host: {}",
            failure.sentence
        );
        let line = failure.line();
        assert!(line.contains("offline"));
        assert!(
            line.contains("daemon_not_connected"),
            "the code must stay available beside the words: {line}"
        );

        let run = RunView {
            status: "failed".to_owned(),
            steps: vec![RunStepView {
                status: "failed".to_owned(),
                error: Some("daemon_not_connected: host 'studio-mac' is offline".to_owned()),
                reason: Some("daemon_not_connected".to_owned()),
                ..step("build", "failed", Some("host-other"))
            }],
            ..run("run-1", "failed", Vec::new())
        };
        let failure = run_failure_explanation(&run).expect("run failure");
        assert!(failure.line().contains("offline"));
        assert_eq!(failure.code.as_deref(), Some("daemon_not_connected"));
    }

    #[test]
    fn a_plain_text_failure_and_an_unknown_code_are_both_kept() {
        let plain = failure_explanation(Some("the agent said no"), None).expect("failure");
        assert_eq!(plain.code, None);
        assert!(plain.sentence.contains("the agent said no"));

        let unknown = failure_explanation(Some("new_code: something new"), None).expect("failure");
        assert_eq!(unknown.code.as_deref(), Some("new_code"));
        assert!(unknown.sentence.contains("something new"));
        assert!(unknown.line().contains("new_code"));

        let reason_only = failure_explanation(None, Some("step_deadline")).expect("failure");
        assert_eq!(reason_only.code.as_deref(), Some("step_deadline"));
        assert!(reason_only.line().contains("deadline"));

        assert!(failure_explanation(None, None).is_none());
    }

    #[test]
    fn a_manual_dispatch_is_validated_before_it_leaves() {
        let mut controller = controller();
        controller.set_project_filter("proj_1");
        controller.set_trigger_filter("on-push");
        let start = Instant::now();
        controller.begin_dispatch("alice", 1_000);
        let form = controller.dispatch_form_mut().expect("form");
        form.set_input("main");
        form.set_revision("");
        form.set_actor("  ");
        assert!(matches!(
            controller.submit_dispatch(start),
            Err(RunsError::InvalidRequest { .. })
        ));
        assert!(controller.client().calls.is_empty());
        assert!(controller.notice().is_some());

        controller
            .dispatch_form_mut()
            .expect("form")
            .set_actor("alice");
        controller
            .client
            .dispatched
            .push_back(Ok(ManualDispatchResponse {
                run: run(
                    "run-new",
                    "running",
                    vec![step("build", "running", Some("host-local"))],
                ),
                deduplicated: false,
            }));
        let started = controller.submit_dispatch(start).expect("dispatch");
        assert_eq!(started.id, "run-new");
        assert!(controller.dispatch_form().is_none());
        assert_eq!(
            controller.selected_run_id(),
            Some("run-new"),
            "a dispatched run opens its detail"
        );
        assert!(controller.is_local_run("run-new"));
        assert!(
            controller
                .notice()
                .is_some_and(|notice| notice.contains("started run run-new"))
        );
        assert_eq!(controller.client().calls.len(), 1);
    }

    #[test]
    fn a_deduplicated_dispatch_says_the_request_had_already_run() {
        let mut controller = controller();
        controller.set_project_filter("proj_1");
        controller.set_trigger_filter("on-push");
        controller.begin_dispatch("alice", 1_000);
        controller
            .client
            .dispatched
            .push_back(Ok(ManualDispatchResponse {
                run: run("run-existing", "succeeded", Vec::new()),
                deduplicated: true,
            }));
        controller
            .submit_dispatch(Instant::now())
            .expect("dispatch");
        assert!(
            controller
                .notice()
                .is_some_and(|notice| notice.contains("already run"))
        );
        assert!(!controller.following(), "a finished run is not followed");
    }

    #[test]
    fn a_delivery_key_is_stable_for_one_form_and_names_the_actor() {
        let key = manual_delivery_key("Alice Smith", 1_000, 7);
        assert_eq!(key, "client-alice-smith-1000-7");
        assert_eq!(
            key,
            manual_delivery_key(" alice smith ", 1_000, 7),
            "the same form must produce the same key"
        );
        assert_ne!(key, manual_delivery_key("alice smith", 1_000, 8));
        assert!(manual_delivery_key("   ", 1_000, 0).starts_with("client-operator-"));
    }

    #[test]
    fn the_tick_refreshes_on_its_interval_not_on_every_call() {
        let mut controller = controller();
        scripted_page(&mut controller, Vec::new());
        scripted_page(&mut controller, Vec::new());
        let start = Instant::now();
        controller.tick(start).expect("first tick");
        assert_eq!(controller.client().calls, vec!["activity:1"]);

        controller
            .tick(start + Duration::from_secs(1))
            .expect("early tick");
        assert_eq!(
            controller.client().calls,
            vec!["activity:1"],
            "the feed is not read again before its interval"
        );

        controller
            .tick(start + Duration::from_secs(FEED_INTERVAL.as_secs() + 1))
            .expect("due tick");
        assert_eq!(controller.client().calls.len(), 2);
    }

    #[test]
    fn the_selection_clamps_to_the_visible_rows() {
        let mut controller = controller();
        scripted_page(
            &mut controller,
            vec![
                entry(
                    "run-1",
                    "run",
                    "succeeded",
                    Some("proj_1"),
                    Some("github"),
                    Some("on-push"),
                ),
                entry(
                    "run-2",
                    "run",
                    "succeeded",
                    Some("proj_1"),
                    Some("github"),
                    Some("on-push"),
                ),
            ],
        );
        controller.refresh(Instant::now()).expect("refresh");
        controller.select_next();
        assert_eq!(controller.selected(), 1);
        controller.select_next();
        assert_eq!(controller.selected(), 0);
        controller.select_previous();
        assert_eq!(controller.selected(), 1);

        controller.set_status_filter("failed");
        assert_eq!(controller.selected(), 0);
        assert_eq!(controller.visible_entries().len(), 0);
    }

    #[test]
    fn the_failure_table_covers_every_named_code_the_pool_writes() {
        for (code, sentence) in FAILURE_SENTENCES {
            assert_eq!(failure_sentence(code), Some(sentence));
            assert!(!sentence.trim().is_empty());
        }
        assert!(
            FAILURE_SENTENCES
                .iter()
                .any(|(code, _)| *code == "daemon_not_connected")
        );
    }
}
