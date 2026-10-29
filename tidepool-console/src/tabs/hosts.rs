//! The hosts tab: the enrolled-host table, presence, the capability pane and
//! the two operator actions (enrol, grant or revoke execution).
//!
//! The tab owns no network code. [`HostsClient`] is the whole surface it needs,
//! [`HostsState::drive`] performs one pending request and one throttled presence
//! refresh, and the tests script that trait, so sorting, filtering and a host
//! going offline are all exercised without a pool.
//!
//! Presence is read by polling `GET /v1/hosts`, not by a push channel: the
//! contract exposes no host-presence stream, so the tab re-reads on a timer and
//! [`HostsState::drive`] is called from the console's event loop even when no
//! key arrived. The pool also does not report a per-host `last_seen_at`, so the
//! table's "last seen" column is what this console observed: the instant a poll
//! last saw the host `connected`, kept when a later poll reports it offline.
//! That is stated in the pane rather than inferred from a timestamp the pool
//! never sent.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tidepool_client::types::{AgentCapability, HostCapabilityReport, HostView};

use crate::app::Result;

/// How often the tab re-reads the pool's host list.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// The four table columns, in table order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// The mutable friendly slug.
    Slug,
    /// Presence: connected, pending approval, offline or revoked.
    Status,
    /// When this console last saw the host connected.
    LastSeen,
    /// Whether `host.execute` is granted, so the host may run agents.
    Execution,
}

impl Column {
    /// Every column, left to right.
    pub const ALL: [Column; 4] = [
        Column::Slug,
        Column::Status,
        Column::LastSeen,
        Column::Execution,
    ];

    /// The column's heading in the table.
    pub fn title(self) -> &'static str {
        match self {
            Column::Slug => "SLUG",
            Column::Status => "STATUS",
            Column::LastSeen => "LAST SEEN",
            Column::Execution => "EXEC",
        }
    }

    /// The column after this one, wrapping.
    pub fn next(self) -> Self {
        match self {
            Column::Slug => Column::Status,
            Column::Status => Column::LastSeen,
            Column::LastSeen => Column::Execution,
            Column::Execution => Column::Slug,
        }
    }

    /// The lowercase word used in the header line.
    pub fn word(self) -> &'static str {
        match self {
            Column::Slug => "slug",
            Column::Status => "status",
            Column::LastSeen => "last seen",
            Column::Execution => "execution",
        }
    }
}

/// The status filter: `All` or exactly one reported status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    /// Every host the pool lists.
    All,
    /// Hosts holding a live presence connection.
    Connected,
    /// Hosts waiting for an operator's approval.
    PendingApproval,
    /// Hosts with no live connection.
    Offline,
    /// Revoked hosts, when the pool still lists one.
    Revoked,
}

impl StatusFilter {
    /// Every filter, in the order `f` cycles them.
    pub const ALL: [StatusFilter; 5] = [
        StatusFilter::All,
        StatusFilter::Connected,
        StatusFilter::PendingApproval,
        StatusFilter::Offline,
        StatusFilter::Revoked,
    ];

    /// The word the header shows for this filter.
    pub fn title(self) -> &'static str {
        match self {
            StatusFilter::All => "all",
            StatusFilter::Connected => "connected",
            StatusFilter::PendingApproval => "pending approval",
            StatusFilter::Offline => "offline",
            StatusFilter::Revoked => "revoked",
        }
    }

    /// Whether a host with this raw status passes the filter.
    pub fn matches(self, status: &str) -> bool {
        match self {
            StatusFilter::All => true,
            StatusFilter::Connected => status == "connected",
            StatusFilter::PendingApproval => status == "pending_approval",
            StatusFilter::Offline => status == "offline",
            StatusFilter::Revoked => status == "revoked",
        }
    }

    /// The filter after this one, wrapping.
    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|filter| *filter == self)
            .unwrap_or_default();
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

/// One row: the host as the pool last reported it, plus the presence instant
/// this console observed.
#[derive(Debug, Clone)]
pub struct HostRecord {
    host: HostView,
    last_seen: Option<Instant>,
}

impl HostRecord {
    /// The host exactly as the pool reported it.
    pub fn host(&self) -> &HostView {
        &self.host
    }

    /// When this console last saw the host `connected`, if it ever did.
    pub fn last_seen(&self) -> Option<Instant> {
        self.last_seen
    }

    /// The "last seen" cell: relative to `now`, or `never` when the console
    /// has not seen the host connected.
    pub fn last_seen_label(&self, now: Instant) -> String {
        let Some(seen) = self.last_seen() else {
            return "never".to_owned();
        };
        let seconds = now.saturating_duration_since(seen).as_secs();
        match seconds {
            0..=4 => "just now".to_owned(),
            5..=59 => format!("{seconds}s ago"),
            60..=3599 => format!("{}m ago", seconds / 60),
            3600..=86_399 => format!("{}h ago", seconds / 3600),
            _ => format!("{}d ago", seconds / 86_400),
        }
    }
}

/// The last capability read for one host.
#[derive(Debug, Clone, PartialEq)]
pub enum CapabilityState {
    /// The report the host sent, verbatim.
    Reported(HostCapabilityReport),
    /// Why the report could not be read; `c` retries.
    Failed(String),
}

/// A freshly minted enrollment: the token is single use and shown once.
#[derive(Debug, Clone, PartialEq)]
pub struct Enrollment {
    /// The host the token was minted for.
    pub host: HostView,
    /// The raw, single-use enrollment token.
    pub token: String,
    /// When the token stops working, as the pool reported it.
    pub expires_at: String,
}

impl Enrollment {
    /// The command an operator runs on the machine being enrolled.
    ///
    /// It names the reference host agent, which performs the enroll exchange
    /// with the token. `pool` is the console's configured pool; the agent's own
    /// default is used when the console has none.
    pub fn command(&self, pool: Option<&str>) -> String {
        let pool = pool.unwrap_or("http://127.0.0.1:8080");
        format!(
            "HOST_ID={} ENROLLMENT_TOKEN={} POOL_HTTP_URL={} tidepool-host-agent",
            self.host.id, self.token, pool
        )
    }
}

/// What the hosts tab needs from a pool.
///
/// One method per thing the screen does, so the scripted client in the tests is
/// the same shape as the SDK-backed one in `main`.
pub trait HostsClient {
    /// Read every live host in the pool.
    fn list_hosts(&mut self) -> Result<Vec<HostView>>;
    /// Read one host's last reported capabilities.
    fn host_capabilities(&mut self, host_id: &str) -> Result<HostCapabilityReport>;
    /// Register a host and mint its single-use enrollment token.
    fn enroll_host(&mut self, hostname: &str) -> Result<Enrollment>;
    /// Grant `host.execute` so the host may run agents.
    fn grant_execution(&mut self, host_id: &str) -> Result<HostView>;
    /// Revoke `host.execute`; the host becomes connected-only.
    fn revoke_execution(&mut self, host_id: &str) -> Result<HostView>;
}

/// Which field the tab's bottom line is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Filtering,
    Enrolling,
}

/// The action a key asked for; `drive` performs it with the client in hand.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Request {
    Enroll(String),
    Grant(String),
    Revoke(String),
}

/// The whole hosts tab.
#[derive(Debug)]
pub struct HostsState {
    hosts: Vec<HostRecord>,
    capabilities: HashMap<String, CapabilityState>,
    sort: Column,
    descending: bool,
    filter: StatusFilter,
    query: String,
    mode: Mode,
    draft: String,
    selected: usize,
    enrollment: Option<Enrollment>,
    notice: Option<String>,
    last_refresh: Option<Instant>,
    pending: Option<Request>,
}

impl Default for HostsState {
    fn default() -> Self {
        Self {
            hosts: Vec::new(),
            capabilities: HashMap::new(),
            sort: Column::Slug,
            descending: false,
            filter: StatusFilter::All,
            query: String::new(),
            mode: Mode::Normal,
            draft: String::new(),
            selected: 0,
            enrollment: None,
            notice: None,
            last_refresh: None,
            pending: None,
        }
    }
}

impl HostsState {
    /// An empty tab: no hosts, sorted by slug, no filter.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many hosts the pool last reported.
    pub fn len(&self) -> usize {
        self.hosts.len()
    }

    /// Whether the pool has reported no hosts.
    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty()
    }

    /// The column in use for sorting.
    pub fn sort(&self) -> Column {
        self.sort
    }

    /// Whether the sort is descending.
    pub fn descending(&self) -> bool {
        self.descending
    }

    /// The indicator a column heading carries: up, down or nothing.
    pub fn sort_arrow(&self, column: Column) -> &'static str {
        if self.sort != column {
            return "";
        }
        if self.descending { " ▼" } else { " ▲" }
    }

    /// The status filter in use.
    pub fn filter(&self) -> StatusFilter {
        self.filter
    }

    /// The free-text filter, matched case-insensitively against every column.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The index of the highlighted row among the visible rows.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The one-time enrollment offer, while it is on screen.
    pub fn enrollment(&self) -> Option<&Enrollment> {
        self.enrollment.as_ref()
    }

    /// The last action's outcome, when it was not a plain success.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// The prompt and text the bottom line is editing, when it is editing one.
    pub fn prompt(&self) -> Option<(&'static str, &str)> {
        match self.mode {
            Mode::Normal => None,
            Mode::Filtering => Some(("filter", &self.query)),
            Mode::Enrolling => Some(("hostname", &self.draft)),
        }
    }

    /// The last capability read for one host.
    pub fn capabilities(&self, host_id: &str) -> Option<&CapabilityState> {
        self.capabilities.get(host_id)
    }

    /// The visible rows, filtered and sorted, given `now` for the last-seen
    /// labels.
    pub fn visible_rows(&self, now: Instant) -> Vec<&HostRecord> {
        let query = self.query.trim().to_lowercase();
        let mut rows: Vec<&HostRecord> = self
            .hosts
            .iter()
            .filter(|record| self.filter.matches(&record.host.status))
            .filter(|record| query.is_empty() || matches_query(record, &query, now))
            .collect();
        rows.sort_by(|left, right| self.compare(left, right));
        rows
    }

    /// The highlighted row, given `now` for the visible ordering.
    pub fn selected_host(&self, now: Instant) -> Option<&HostRecord> {
        self.visible_rows(now).get(self.selected).copied()
    }

    /// Move the highlight down, wrapping.
    pub fn select_next(&mut self, now: Instant) {
        let count = self.visible_rows(now).len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + 1) % count;
    }

    /// Move the highlight up, wrapping.
    pub fn select_previous(&mut self, now: Instant) {
        let count = self.visible_rows(now).len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected + count - 1) % count;
    }

    /// Handle one key press. Returns whether the tab consumed it.
    ///
    /// While a prompt is open every key belongs to the prompt, so `q` types a
    /// `q` instead of quitting the console.
    pub fn on_key(&mut self, key: KeyEvent) -> bool {
        match self.mode {
            Mode::Filtering => {
                self.edit_query(key);
                return true;
            }
            Mode::Enrolling => {
                self.edit_hostname(key);
                return true;
            }
            Mode::Normal => {}
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return false;
        }
        let now = Instant::now();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous(now);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next(now);
                true
            }
            KeyCode::Char('s') => {
                self.sort = self.sort.next();
                self.selected = 0;
                true
            }
            KeyCode::Char('S') => {
                self.descending = !self.descending;
                self.selected = 0;
                true
            }
            KeyCode::Char('f') => {
                self.filter = self.filter.next();
                self.selected = 0;
                true
            }
            KeyCode::Char('/') => {
                self.mode = Mode::Filtering;
                true
            }
            KeyCode::Char('e') => {
                self.mode = Mode::Enrolling;
                self.draft.clear();
                true
            }
            KeyCode::Char('g') => {
                if let Some(id) = self.selected_id(now) {
                    self.pending = Some(Request::Grant(id));
                }
                true
            }
            KeyCode::Char('x') => {
                if let Some(id) = self.selected_id(now) {
                    self.pending = Some(Request::Revoke(id));
                }
                true
            }
            KeyCode::Char('c') => {
                if let Some(id) = self.selected_id(now) {
                    self.capabilities.remove(&id);
                }
                true
            }
            KeyCode::Esc => {
                if self.enrollment.take().is_none() {
                    self.query.clear();
                }
                self.selected = 0;
                true
            }
            _ => false,
        }
    }

    /// Perform the pending action, then refresh on the timer and read the
    /// selected host's capabilities when they are not cached.
    ///
    /// The console calls this from its event loop, so a host that goes offline
    /// is visible on the next frame without any key press.
    pub fn drive(&mut self, client: &mut dyn HostsClient, now: Instant) {
        if let Some(request) = self.pending.take() {
            self.run(request, client, now);
        }
        let due = self
            .last_refresh
            .is_none_or(|last| now.saturating_duration_since(last) >= REFRESH_INTERVAL);
        if due {
            self.last_refresh = Some(now);
            match client.list_hosts() {
                Ok(hosts) => self.apply_snapshot(hosts, now),
                Err(error) => self.notice = Some(format!("hosts: {error}")),
            }
        }
        self.ensure_capabilities(client, now);
    }

    fn edit_query(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Char(character) => {
                self.query.push(character);
                self.selected = 0;
            }
            _ => {}
        }
    }

    fn edit_hostname(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.draft.clear();
            }
            KeyCode::Backspace => {
                self.draft.pop();
            }
            KeyCode::Char(character) => self.draft.push(character),
            KeyCode::Enter => {
                let hostname = self.draft.trim().to_owned();
                self.mode = Mode::Normal;
                self.draft.clear();
                if !hostname.is_empty() {
                    self.pending = Some(Request::Enroll(hostname));
                }
            }
            _ => {}
        }
    }

    fn selected_id(&self, now: Instant) -> Option<String> {
        self.selected_host(now).map(|record| record.host.id.clone())
    }

    fn run(&mut self, request: Request, client: &mut dyn HostsClient, now: Instant) {
        match request {
            Request::Enroll(hostname) => match client.enroll_host(&hostname) {
                Ok(offer) => {
                    let slug = offer.host.slug.clone();
                    self.upsert(offer.host.clone(), now);
                    self.enrollment = Some(offer);
                    self.notice = Some(format!("Enrolled {slug}; the token is shown once."));
                }
                Err(error) => self.notice = Some(format!("enroll failed: {error}")),
            },
            Request::Grant(host_id) => match client.grant_execution(&host_id) {
                Ok(host) => {
                    let slug = host.slug.clone();
                    self.upsert(host, now);
                    self.capabilities.remove(&host_id);
                    self.notice = Some(format!("Execution granted to {slug}."));
                }
                Err(error) => self.notice = Some(format!("grant failed: {error}")),
            },
            Request::Revoke(host_id) => match client.revoke_execution(&host_id) {
                Ok(host) => {
                    let slug = host.slug.clone();
                    self.upsert(host, now);
                    self.capabilities.remove(&host_id);
                    self.notice = Some(format!("Execution revoked for {slug}; connected-only."));
                }
                Err(error) => self.notice = Some(format!("revoke failed: {error}")),
            },
        }
    }

    fn ensure_capabilities(&mut self, client: &mut dyn HostsClient, now: Instant) {
        let Some(host_id) = self.selected_id(now) else {
            return;
        };
        if self.capabilities.contains_key(&host_id) {
            return;
        }
        let state = match client.host_capabilities(&host_id) {
            Ok(report) => CapabilityState::Reported(report),
            Err(error) => CapabilityState::Failed(error.to_string()),
        };
        self.capabilities.insert(host_id, state);
    }

    fn apply_snapshot(&mut self, hosts: Vec<HostView>, now: Instant) {
        let previous: HashMap<String, Option<Instant>> = self
            .hosts
            .iter()
            .map(|record| (record.host.id.clone(), record.last_seen))
            .collect();
        self.hosts = hosts
            .into_iter()
            .map(|host| {
                let last_seen = if host.status == "connected" {
                    Some(now)
                } else {
                    previous.get(&host.id).copied().flatten()
                };
                HostRecord { host, last_seen }
            })
            .collect();
        self.clamp_selection(now);
    }

    /// Insert or replace one host, keeping a presence instant this console
    /// already observed.
    fn upsert(&mut self, host: HostView, now: Instant) {
        let last_seen = if host.status == "connected" {
            Some(now)
        } else {
            self.hosts
                .iter()
                .find(|record| record.host.id == host.id)
                .and_then(|record| record.last_seen)
        };
        match self
            .hosts
            .iter_mut()
            .find(|record| record.host.id == host.id)
        {
            Some(record) => {
                record.host = host;
                record.last_seen = last_seen;
            }
            None => self.hosts.push(HostRecord { host, last_seen }),
        }
        self.clamp_selection(now);
    }

    fn clamp_selection(&mut self, now: Instant) {
        let count = self.visible_rows(now).len();
        self.selected = if count == 0 {
            0
        } else {
            self.selected.min(count - 1)
        };
    }

    fn compare(&self, left: &HostRecord, right: &HostRecord) -> Ordering {
        let order = match self.sort {
            Column::Slug => left.host.slug.cmp(&right.host.slug),
            Column::Status => status_rank(&left.host.status).cmp(&status_rank(&right.host.status)),
            Column::LastSeen => left.last_seen.cmp(&right.last_seen),
            Column::Execution => left
                .host
                .execution_enabled
                .cmp(&right.host.execution_enabled),
        };
        let order = if self.descending {
            order.reverse()
        } else {
            order
        };
        // A stable tiebreak keeps equal keys from reordering between polls.
        order
            .then_with(|| left.host.slug.cmp(&right.host.slug))
            .then_with(|| left.host.id.cmp(&right.host.id))
    }
}

/// Connected first, then pending, then offline, then revoked, then anything the
/// contract adds later.
fn status_rank(status: &str) -> u8 {
    match status {
        "connected" => 0,
        "pending_approval" => 1,
        "offline" => 2,
        "revoked" => 3,
        _ => 4,
    }
}

fn execution_label(enabled: bool) -> &'static str {
    if enabled { "yes" } else { "no" }
}

/// Whether a row matches `query` (already lowercased) in any of its columns.
fn matches_query(record: &HostRecord, query: &str, now: Instant) -> bool {
    let last_seen = record.last_seen_label(now);
    [
        record.host.slug.as_str(),
        record.host.status.as_str(),
        last_seen.as_str(),
        execution_label(record.host.execution_enabled),
    ]
    .iter()
    .any(|field| field.to_lowercase().contains(query))
}

/// One capability line, exactly as the host reported it.
pub(crate) fn capability_line(capability: &AgentCapability) -> String {
    let mut line = format!(
        "{}/{} · {}",
        capability.provider, capability.model, capability.execution_mode
    );
    if let Some(thinking) = &capability.thinking_option {
        line.push_str(&format!(" · thinking {thinking}"));
    }
    if let Some(options) = &capability.options {
        line.push_str(&format!(" · options {options}"));
    }
    line
}

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::VecDeque;

    use tidepool_client::types::{HostCapabilityReport, HostView};

    use super::{Enrollment, HostsClient};
    use crate::app::Result;
    use crate::app::testing::transport;

    /// A host as the pool would report it.
    pub(crate) fn host(id: &str, slug: &str, status: &str, execution_enabled: bool) -> HostView {
        HostView {
            id: id.to_owned(),
            slug: slug.to_owned(),
            display_name: slug.to_owned(),
            hostname: format!("{slug}.local"),
            status: status.to_owned(),
            execution_enabled,
            revoked_at: None,
            created_at: "2026-10-20T00:00:00Z".to_owned(),
        }
    }

    /// A capability report with one reported entry.
    pub(crate) fn report(host_id: &str) -> HostCapabilityReport {
        HostCapabilityReport {
            host_id: host_id.to_owned(),
            reported_at: Some("2026-10-29T00:00:00Z".to_owned()),
            offered_providers: vec!["anthropic".to_owned()],
            capabilities: vec![tidepool_client::types::AgentCapability {
                provider: "anthropic".to_owned(),
                model: "claude".to_owned(),
                execution_mode: "default".to_owned(),
                thinking_option: None,
                options: None,
            }],
        }
    }

    /// A client that answers from queues and records every call it served.
    #[derive(Debug, Default)]
    pub(crate) struct ScriptedHosts {
        /// Queued host lists, oldest first.
        pub(crate) lists: VecDeque<Result<Vec<HostView>>>,
        /// Queued capability reports, oldest first.
        pub(crate) reports: VecDeque<Result<HostCapabilityReport>>,
        /// Queued enrollment offers, oldest first.
        pub(crate) enrollments: VecDeque<Result<Enrollment>>,
        /// Queued grant answers, oldest first.
        pub(crate) grants: VecDeque<Result<HostView>>,
        /// Queued revoke answers, oldest first.
        pub(crate) revocations: VecDeque<Result<HostView>>,
        /// Every call served, in order.
        pub(crate) calls: Vec<String>,
    }

    impl ScriptedHosts {
        /// An empty script.
        pub(crate) fn new() -> Self {
            Self::default()
        }

        /// Queue one host list.
        pub(crate) fn queue_hosts(&mut self, hosts: Vec<HostView>) {
            self.lists.push_back(Ok(hosts));
        }
    }

    impl HostsClient for ScriptedHosts {
        fn list_hosts(&mut self) -> Result<Vec<HostView>> {
            self.calls.push("list".to_owned());
            self.lists.pop_front().unwrap_or_else(|| Ok(Vec::new()))
        }

        fn host_capabilities(&mut self, host_id: &str) -> Result<HostCapabilityReport> {
            self.calls.push(format!("report:{host_id}"));
            self.reports
                .pop_front()
                .unwrap_or_else(|| Err(transport("no scripted capability report")))
        }

        fn enroll_host(&mut self, hostname: &str) -> Result<Enrollment> {
            self.calls.push(format!("enroll:{hostname}"));
            self.enrollments
                .pop_front()
                .unwrap_or_else(|| Err(transport("no scripted enrollment")))
        }

        fn grant_execution(&mut self, host_id: &str) -> Result<HostView> {
            self.calls.push(format!("grant:{host_id}"));
            self.grants
                .pop_front()
                .unwrap_or_else(|| Err(transport("no scripted grant")))
        }

        fn revoke_execution(&mut self, host_id: &str) -> Result<HostView> {
            self.calls.push(format!("revoke:{host_id}"));
            self.revocations
                .pop_front()
                .unwrap_or_else(|| Err(transport("no scripted revoke")))
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::testing::{ScriptedHosts, host, report};
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn slugs(state: &HostsState, now: Instant) -> Vec<String> {
        state
            .visible_rows(now)
            .iter()
            .map(|record| record.host.slug.clone())
            .collect()
    }

    fn seeded() -> (HostsState, ScriptedHosts, Instant) {
        let now = Instant::now();
        let mut state = HostsState::new();
        let mut client = ScriptedHosts::new();
        client.queue_hosts(vec![
            host("h-1", "alpha", "connected", false),
            host("h-2", "bravo", "offline", true),
            host("h-3", "charlie", "pending_approval", false),
        ]);
        client.reports.push_back(Ok(report("h-1")));
        state.drive(&mut client, now);
        (state, client, now)
    }

    #[test]
    fn the_filter_cycle_matches_the_published_order() {
        for pair in StatusFilter::ALL.windows(2) {
            assert_eq!(pair[0].next(), pair[1]);
        }
        assert_eq!(
            StatusFilter::ALL[StatusFilter::ALL.len() - 1].next(),
            StatusFilter::ALL[0]
        );
        assert_eq!(StatusFilter::ALL.len(), 5);
    }

    #[test]
    fn sorts_by_every_column_in_both_directions() {
        let (mut state, _client, now) = seeded();

        assert_eq!(state.sort(), Column::Slug);
        assert_eq!(slugs(&state, now), ["alpha", "bravo", "charlie"]);

        state.on_key(key(KeyCode::Char('S')));
        assert_eq!(slugs(&state, now), ["charlie", "bravo", "alpha"]);

        state.on_key(key(KeyCode::Char('s')));
        assert_eq!(state.sort(), Column::Status);
        assert!(state.descending(), "the direction stays until toggled");
        state.on_key(key(KeyCode::Char('S')));
        assert_eq!(
            slugs(&state, now),
            ["alpha", "charlie", "bravo"],
            "connected, pending, offline"
        );

        state.on_key(key(KeyCode::Char('s')));
        assert_eq!(state.sort(), Column::LastSeen);
        assert_eq!(
            slugs(&state, now),
            ["bravo", "charlie", "alpha"],
            "never seen first, then the oldest presence"
        );

        state.on_key(key(KeyCode::Char('s')));
        assert_eq!(state.sort(), Column::Execution);
        assert_eq!(
            slugs(&state, now),
            ["alpha", "charlie", "bravo"],
            "no execution first, then yes"
        );

        state.on_key(key(KeyCode::Char('s')));
        assert_eq!(state.sort(), Column::Slug, "the columns wrap");
    }

    #[test]
    fn sorts_by_last_seen_by_observed_presence_order() {
        let base = Instant::now();
        let mut state = HostsState::new();
        let mut client = ScriptedHosts::new();

        client.queue_hosts(vec![
            host("a", "alpha", "connected", false),
            host("b", "bravo", "offline", false),
        ]);
        state.drive(&mut client, base);

        client.queue_hosts(vec![
            host("a", "alpha", "offline", false),
            host("b", "bravo", "connected", false),
        ]);
        state.drive(&mut client, base + REFRESH_INTERVAL);

        client.queue_hosts(vec![
            host("a", "alpha", "offline", false),
            host("b", "bravo", "offline", false),
            host("c", "charlie", "offline", false),
        ]);
        let later = base + REFRESH_INTERVAL * 2;
        state.drive(&mut client, later);

        state.on_key(key(KeyCode::Char('s')));
        state.on_key(key(KeyCode::Char('s')));
        assert_eq!(state.sort(), Column::LastSeen);
        assert!(!state.descending());
        assert_eq!(slugs(&state, later), ["charlie", "alpha", "bravo"]);

        let alpha = state
            .visible_rows(later)
            .into_iter()
            .find(|record| record.host.slug == "alpha")
            .expect("alpha");
        assert_eq!(alpha.last_seen(), Some(base));
        let charlie = state
            .visible_rows(later)
            .into_iter()
            .find(|record| record.host.slug == "charlie")
            .expect("charlie");
        assert_eq!(charlie.last_seen(), None);
        assert_eq!(charlie.last_seen_label(later), "never");
    }

    #[test]
    fn filters_by_status_and_wraps_back_to_all() {
        let (mut state, _client, now) = seeded();
        assert_eq!(state.filter(), StatusFilter::All);

        state.on_key(key(KeyCode::Char('f')));
        assert_eq!(state.filter(), StatusFilter::Connected);
        assert_eq!(slugs(&state, now), ["alpha"]);

        state.on_key(key(KeyCode::Char('f')));
        assert_eq!(slugs(&state, now), ["charlie"]);

        state.on_key(key(KeyCode::Char('f')));
        assert_eq!(slugs(&state, now), ["bravo"]);

        state.on_key(key(KeyCode::Char('f')));
        assert!(slugs(&state, now).is_empty(), "no revoked host listed");

        state.on_key(key(KeyCode::Char('f')));
        assert_eq!(state.filter(), StatusFilter::All);
        assert_eq!(slugs(&state, now).len(), 3);
    }

    #[test]
    fn the_text_filter_matches_every_column() {
        let (mut state, _client, now) = seeded();
        state.on_key(key(KeyCode::Char('/')));

        for character in "bravo".chars() {
            state.on_key(key(KeyCode::Char(character)));
        }
        assert_eq!(slugs(&state, now), ["bravo"], "slug");

        state.on_key(key(KeyCode::Enter));
        state.on_key(key(KeyCode::Char('/')));
        for _ in 0.."bravo".len() {
            state.on_key(key(KeyCode::Backspace));
        }
        for character in "connected".chars() {
            state.on_key(key(KeyCode::Char(character)));
        }
        assert_eq!(slugs(&state, now), ["alpha"], "status");

        state.on_key(key(KeyCode::Enter));
        state.on_key(key(KeyCode::Char('/')));
        for _ in 0.."connected".len() {
            state.on_key(key(KeyCode::Backspace));
        }
        for character in "never".chars() {
            state.on_key(key(KeyCode::Char(character)));
        }
        assert_eq!(slugs(&state, now), ["bravo", "charlie"], "last seen");

        state.on_key(key(KeyCode::Enter));
        state.on_key(key(KeyCode::Char('/')));
        for _ in 0.."never".len() {
            state.on_key(key(KeyCode::Backspace));
        }
        for character in "yes".chars() {
            state.on_key(key(KeyCode::Char(character)));
        }
        assert_eq!(slugs(&state, now), ["bravo"], "execution");
    }

    #[test]
    fn live_status_changes_without_a_key_press() {
        let base = Instant::now();
        let mut state = HostsState::new();
        let mut client = ScriptedHosts::new();
        client.queue_hosts(vec![host("h-1", "alpha", "connected", false)]);
        client.reports.push_back(Ok(report("h-1")));

        state.drive(&mut client, base);
        assert_eq!(state.visible_rows(base)[0].host().status, "connected");
        assert_eq!(
            state.visible_rows(base)[0].last_seen_label(base),
            "just now"
        );

        client.queue_hosts(vec![host("h-1", "alpha", "offline", false)]);
        state.drive(&mut client, base + REFRESH_INTERVAL);

        let row = state.visible_rows(base + REFRESH_INTERVAL)[0].clone();
        assert_eq!(row.host().status, "offline");
        assert_eq!(
            row.last_seen(),
            Some(base),
            "a host that goes offline keeps the instant it was last seen"
        );
        assert_eq!(
            client.calls,
            ["list", "report:h-1", "list"],
            "the refresh happened with no key press, and the report was cached"
        );
    }

    #[test]
    fn enrolling_shows_the_token_and_the_command_to_run() {
        let base = Instant::now();
        let mut state = HostsState::new();
        let mut client = ScriptedHosts::new();
        client.queue_hosts(Vec::new());
        state.drive(&mut client, base);
        assert!(state.is_empty());

        client.enrollments.push_back(Ok(Enrollment {
            host: host("h-9", "ops-01", "pending_approval", false),
            token: "tok_secret_once".to_owned(),
            expires_at: "2026-10-29T12:10:00Z".to_owned(),
        }));

        state.on_key(key(KeyCode::Char('e')));
        for character in "ops-01".chars() {
            state.on_key(key(KeyCode::Char(character)));
        }
        state.on_key(key(KeyCode::Enter));
        state.drive(&mut client, base);

        let offer = state.enrollment().expect("an enrollment offer").clone();
        assert_eq!(offer.token, "tok_secret_once");
        assert_eq!(offer.expires_at, "2026-10-29T12:10:00Z");
        let command = offer.command(Some("https://pool.test"));
        assert!(
            command.contains("ENROLLMENT_TOKEN=tok_secret_once"),
            "{command}"
        );
        assert!(command.contains("HOST_ID=h-9"), "{command}");
        assert!(
            command.contains("POOL_HTTP_URL=https://pool.test"),
            "{command}"
        );
        assert!(command.contains("tidepool-host-agent"), "{command}");
        assert_eq!(state.len(), 1, "the registered host joins the table");
        assert_eq!(state.prompt(), None);
    }

    #[test]
    fn typing_the_hostname_never_reaches_the_global_keys() {
        let mut state = HostsState::new();
        state.on_key(key(KeyCode::Char('e')));
        assert!(state.on_key(key(KeyCode::Char('q'))));
        assert!(state.on_key(key(KeyCode::Char('h'))));
        assert_eq!(state.prompt(), Some(("hostname", "qh")));
        state.on_key(key(KeyCode::Esc));
        assert_eq!(state.prompt(), None);
    }

    #[test]
    fn escape_dismisses_the_offer_before_clearing_the_query() {
        let base = Instant::now();
        let mut state = HostsState::new();
        let mut client = ScriptedHosts::new();
        client.queue_hosts(vec![host("h-1", "alpha", "connected", false)]);
        client.reports.push_back(Ok(report("h-1")));
        state.drive(&mut client, base);
        client.enrollments.push_back(Ok(Enrollment {
            host: host("h-9", "ops-01", "pending_approval", false),
            token: "tok".to_owned(),
            expires_at: "later".to_owned(),
        }));
        state.on_key(key(KeyCode::Char('e')));
        state.on_key(key(KeyCode::Char('o')));
        state.on_key(key(KeyCode::Enter));
        state.drive(&mut client, base);
        assert!(state.enrollment().is_some());

        state.on_key(key(KeyCode::Esc));
        assert!(state.enrollment().is_none());
        assert_eq!(state.len(), 2);
    }

    #[test]
    fn grant_and_revoke_update_the_row_and_drop_the_cached_report() {
        let (mut state, mut client, now) = seeded();
        assert_eq!(state.selected(), 0);
        assert_eq!(
            state
                .selected_host(now)
                .map(|record| record.host().slug.as_str()),
            Some("alpha")
        );

        client
            .grants
            .push_back(Ok(host("h-1", "alpha", "connected", true)));
        state.on_key(key(KeyCode::Char('g')));
        state.drive(&mut client, now);
        assert!(state.visible_rows(now)[0].host().execution_enabled);
        assert!(
            state
                .notice()
                .is_some_and(|notice| notice.contains("Execution granted"))
        );
        assert!(
            matches!(state.capabilities("h-1"), Some(CapabilityState::Failed(_))),
            "the cached report was dropped and re-read"
        );
        assert_eq!(
            client
                .calls
                .iter()
                .filter(|call| call.as_str() == "report:h-1")
                .count(),
            2,
            "one read at first selection, one after the grant"
        );
        assert!(client.calls.contains(&"grant:h-1".to_owned()));

        client
            .revocations
            .push_back(Ok(host("h-1", "alpha", "connected", false)));
        state.on_key(key(KeyCode::Char('x')));
        state.drive(&mut client, now);
        assert!(!state.visible_rows(now)[0].host().execution_enabled);
        assert!(
            state
                .notice()
                .is_some_and(|notice| notice.contains("connected-only"))
        );
    }

    #[test]
    fn a_failed_capability_read_is_retried_with_c() {
        let base = Instant::now();
        let mut state = HostsState::new();
        let mut client = ScriptedHosts::new();
        client.queue_hosts(vec![host("h-1", "alpha", "connected", false)]);
        state.drive(&mut client, base);
        assert!(
            matches!(state.capabilities("h-1"), Some(CapabilityState::Failed(reason)) if reason.contains("no scripted capability report"))
        );

        client.reports.push_back(Ok(report("h-1")));
        state.on_key(key(KeyCode::Char('c')));
        state.drive(&mut client, base);
        assert!(matches!(
            state.capabilities("h-1"),
            Some(CapabilityState::Reported(report)) if report.host_id == "h-1"
        ));
    }

    #[test]
    fn a_failed_refresh_keeps_the_rows_on_screen() {
        let (mut state, mut client, now) = seeded();
        client
            .lists
            .push_back(Err(crate::app::testing::transport("connection refused")));
        state.drive(&mut client, now + REFRESH_INTERVAL);
        assert_eq!(state.len(), 3);
        assert!(
            state
                .notice()
                .is_some_and(|notice| notice.contains("connection refused"))
        );
    }

    #[test]
    fn selection_wraps_and_clamps_after_a_filter_narrows_the_rows() {
        let (mut state, _client, now) = seeded();
        state.select_next(now);
        assert_eq!(state.selected(), 1);
        state.select_next(now);
        assert_eq!(state.selected(), 2);
        state.select_next(now);
        assert_eq!(state.selected(), 0, "selection wraps");

        state.on_key(key(KeyCode::Char('f')));
        assert_eq!(state.selected(), 0);
        state.on_key(key(KeyCode::Char('f')));
        assert_eq!(state.filter(), StatusFilter::PendingApproval);
        assert_eq!(state.visible_rows(now).len(), 1);
        state.on_key(key(KeyCode::Esc));
        assert_eq!(state.selected(), 0);
    }

    #[test]
    fn empty_state_ignores_selection_and_render_inputs() {
        let now = Instant::now();
        let mut state = HostsState::new();
        state.select_next(now);
        state.select_previous(now);
        assert_eq!(state.selected(), 0);
        assert!(state.selected_host(now).is_none());
        assert!(state.is_empty());
        assert_eq!(state.sort_arrow(Column::Slug), " ▲");
        assert_eq!(state.sort_arrow(Column::Status), "");
    }

    #[test]
    fn last_seen_labels_are_relative() {
        let base = Instant::now();
        let mut state = HostsState::new();
        let mut client = ScriptedHosts::new();
        client.queue_hosts(vec![host("h-1", "alpha", "connected", false)]);
        state.drive(&mut client, base);
        let record = state.visible_rows(base)[0].clone();
        assert_eq!(record.last_seen_label(base), "just now");
        assert_eq!(
            record.last_seen_label(base + Duration::from_secs(12)),
            "12s ago"
        );
        assert_eq!(
            record.last_seen_label(base + Duration::from_secs(3 * 60)),
            "3m ago"
        );
        assert_eq!(
            record.last_seen_label(base + Duration::from_secs(2 * 3600)),
            "2h ago"
        );
        assert_eq!(
            record.last_seen_label(base + Duration::from_secs(3 * 86_400)),
            "3d ago"
        );
    }
}
