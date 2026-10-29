//! The client's session loop: list, start, follow, steer, queue and abort.
//!
//! A session runs on this machine. [`SessionController`] is the whole state
//! machine around it: it asks the local host agent (through [`HostSessions`])
//! what is running, starts one in a chosen project directory, keeps a live
//! timeline, delivers follow-ups, steers a busy agent, queues work behind it
//! and aborts.
//!
//! # The timeline is four kinds, never a colour
//!
//! [`timeline`] maps every host event to [`EntryKind::User`],
//! [`EntryKind::Agent`], [`EntryKind::Tool`] or [`EntryKind::Status`]. Each kind
//! carries its own word (`You`, `Agent`, `Tool`, `Session`), so a row can be
//! told apart with the colour stripped out, and a `message` with no author is
//! read as the session's rather than the person's.
//!
//! # Steering is a different action, not a second prompt
//!
//! [`SessionController::input_mode`] answers [`InputMode::Steer`] for a running
//! session and [`InputMode::Prompt`] only for an idle one, and
//! [`available_actions`] offers `Steer this session` while busy and never
//! `Send follow-up`. The host is asked for the mode that was chosen, and a
//! receipt that comes back with a different mode is refused rather than shown.
//!
//! # Interrupted is never complete
//!
//! [`SessionStatus::is_complete`] is true only for `succeeded`. An abort
//! answers with an interruption, and [`SessionStatus::after_abort`] turns a
//! host's unexpected `succeeded` into `interrupted`: a session a person stopped
//! is labelled interrupted, never drawn as a finished one.
//!
//! # The pool only hears the lifecycle
//!
//! A session started from the pool-connected client opts into the relay and
//! publishes `started`, `status` and `stopped` through [`PoolRelay`]. Output,
//! messages and tool results stay on this machine unless content publication is
//! explicitly enabled with [`SessionController::set_publish_content`]. A pool
//! that is away is a notice, never a failed start: the local session comes
//! first.
//!
//! The relay only accepts a session the pool already knows as a host execution,
//! so a standalone local session may be refused until the pool grows a
//! session-create route (see [`pool`]). That refusal is a notice; the local
//! session keeps running.

pub mod pool;
pub mod protocol;
pub mod timeline;

use crate::host::client::PoolError;
use crate::sessions::timeline::{Origin, Timeline};

pub use crate::sessions::pool::{HttpPoolRelay, PoolRelay, is_lifecycle};
pub use crate::sessions::protocol::{
    EventPage, HostEvent, HostSessions, HttpHostSessions, InputMode, InputReceipt, MAX_EVENT_PAGE,
    MAX_INPUT_CHARS, SessionError, SessionStatus, SessionSummary, StartSession, normalize_host_url,
};
pub use crate::sessions::timeline::{EntryKind, TimelineEntry};

/// How many events one timeline read asks for.
pub const EVENT_PAGE: u32 = 100;
/// How many backfill pages an open may walk before it gives up and stays live.
pub const MAX_BACKFILL_PAGES: usize = 50;

/// An action the session surface offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionAction {
    /// Send a follow-up to an idle session.
    SendFollowUp,
    /// Redirect a running session.
    Steer,
    /// Hold an input until the running turn ends.
    Queue,
    /// Stop the session.
    Abort,
}

impl SessionAction {
    /// The button's words.
    pub fn label(self) -> &'static str {
        match self {
            Self::SendFollowUp => InputMode::Prompt.label(),
            Self::Steer => InputMode::Steer.label(),
            Self::Queue => InputMode::Queue.label(),
            Self::Abort => "Abort session",
        }
    }

    /// What the action does, in one sentence.
    pub fn hint(self) -> &'static str {
        match self {
            Self::SendFollowUp => InputMode::Prompt.hint(),
            Self::Steer => InputMode::Steer.hint(),
            Self::Queue => InputMode::Queue.hint(),
            Self::Abort => "Stops the session; it is then labelled interrupted, not complete.",
        }
    }

    /// The input mode this action delivers, when it delivers one.
    pub fn input_mode(self) -> Option<InputMode> {
        match self {
            Self::SendFollowUp => Some(InputMode::Prompt),
            Self::Steer => Some(InputMode::Steer),
            Self::Queue => Some(InputMode::Queue),
            Self::Abort => None,
        }
    }
}

/// The actions a session in `status` offers.
///
/// A busy session is steered or queued; it is never offered a second prompt.
pub fn available_actions(status: Option<&SessionStatus>) -> Vec<SessionAction> {
    match status {
        Some(status) if status.is_busy() => vec![
            SessionAction::Steer,
            SessionAction::Queue,
            SessionAction::Abort,
        ],
        Some(SessionStatus::Queued) => vec![SessionAction::Queue, SessionAction::Abort],
        Some(status) if status.is_terminal() => Vec::new(),
        Some(_) => vec![SessionAction::SendFollowUp, SessionAction::Abort],
        None => Vec::new(),
    }
}

/// One session and its timeline, ready to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDetail {
    /// The session as the host last described it.
    pub summary: SessionSummary,
    /// Its timeline.
    pub timeline: Timeline,
}

/// Everything a surface needs to draw the session pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    /// Every session on this machine, newest first.
    pub sessions: Vec<SessionSummary>,
    /// The open session, when one is open.
    pub selected: Option<SessionDetail>,
    /// The last problem worth telling the person, if any.
    pub notice: Option<String>,
    /// How the composer would deliver the next input.
    pub input_mode: InputMode,
    /// The actions the open session offers.
    pub actions: Vec<SessionAction>,
}

impl Default for SessionSnapshot {
    fn default() -> Self {
        Self {
            sessions: Vec::new(),
            selected: None,
            notice: None,
            input_mode: InputMode::Prompt,
            actions: Vec::new(),
        }
    }
}

/// The open session: its summary and its timeline.
#[derive(Debug, Clone)]
struct Selected {
    summary: SessionSummary,
    timeline: Timeline,
}

/// The client's session loop.
pub struct SessionController<H: HostSessions, P: PoolRelay> {
    host: H,
    pool: Option<P>,
    sessions: Vec<SessionSummary>,
    selected: Option<Selected>,
    notice: Option<String>,
    publish_content: bool,
    announced: std::collections::BTreeSet<String>,
}

impl<H: HostSessions, P: PoolRelay> std::fmt::Debug for SessionController<H, P> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionController")
            .field("sessions", &self.sessions.len())
            .field(
                "selected",
                &self.selected.as_ref().map(|selected| &selected.summary.id),
            )
            .field("notice", &self.notice)
            .field("publish_content", &self.publish_content)
            .finish_non_exhaustive()
    }
}

impl<H: HostSessions, P: PoolRelay> SessionController<H, P> {
    /// Build the loop over a host agent and, when signed in, a pool relay.
    pub fn new(host: H, pool: Option<P>) -> Self {
        Self {
            host,
            pool,
            sessions: Vec::new(),
            selected: None,
            notice: None,
            publish_content: false,
            announced: std::collections::BTreeSet::new(),
        }
    }

    /// Every session on this machine, as last read.
    pub fn sessions(&self) -> &[SessionSummary] {
        &self.sessions
    }

    /// The open session's summary.
    pub fn selected_summary(&self) -> Option<&SessionSummary> {
        self.selected.as_ref().map(|selected| &selected.summary)
    }

    /// The open session's timeline.
    pub fn timeline(&self) -> Option<&Timeline> {
        self.selected.as_ref().map(|selected| &selected.timeline)
    }

    /// The last problem worth telling the person.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// How the composer would deliver the next input.
    pub fn input_mode(&self) -> InputMode {
        match self.selected.as_ref() {
            Some(selected) => InputMode::for_status(&selected.summary.status),
            None => InputMode::Prompt,
        }
    }

    /// The actions the open session offers.
    pub fn actions(&self) -> Vec<SessionAction> {
        available_actions(
            self.selected
                .as_ref()
                .map(|selected| &selected.summary.status),
        )
    }

    /// Whether content, not only lifecycle, is published to the pool.
    pub fn publishes_content(&self) -> bool {
        self.publish_content
    }

    /// Turn content publication on or off.
    ///
    /// Off by default: a session's output, messages and tool results stay on
    /// this machine unless the person asks for them to be shared.
    pub fn set_publish_content(&mut self, enabled: bool) {
        self.publish_content = enabled;
    }

    /// The state a surface draws.
    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            sessions: self.sessions.clone(),
            selected: self.selected.as_ref().map(|selected| SessionDetail {
                summary: selected.summary.clone(),
                timeline: selected.timeline.clone(),
            }),
            notice: self.notice.clone(),
            input_mode: self.input_mode(),
            actions: self.actions(),
        }
    }

    /// Re-read every session from the host.
    pub fn refresh(&mut self) -> Result<Vec<SessionSummary>, SessionError> {
        match self.host.list() {
            Ok(sessions) => {
                self.sessions = sessions;
                self.sync_selected();
                self.notice = None;
                Ok(self.sessions.clone())
            }
            Err(error) => {
                self.record(&error);
                Err(error)
            }
        }
    }

    /// Start a session in `request.project` and open it.
    ///
    /// The session is announced to the pool as `started`; a pool that refuses
    /// or is away leaves a notice and does not undo the local start.
    pub fn start(&mut self, request: StartSession) -> Result<SessionSummary, SessionError> {
        request.validate()?;
        let summary = match self.host.start(&request) {
            Ok(summary) => summary,
            Err(error) => {
                self.record(&error);
                return Err(error);
            }
        };
        self.sessions.retain(|session| session.id != summary.id);
        self.sessions.insert(0, summary.clone());
        let mut timeline = Timeline::default();
        // A session this client just started has no history to catch up on.
        timeline.mark_live();
        self.selected = Some(Selected {
            summary: summary.clone(),
            timeline,
        });
        self.announce_start(&summary);
        Ok(summary)
    }

    /// Open an already known session and backfill its timeline.
    pub fn open(&mut self, id: &str) -> Result<(), SessionError> {
        let Some(summary) = self
            .sessions
            .iter()
            .find(|session| session.id == id)
            .cloned()
        else {
            return Err(SessionError::UnknownSession { id: id.to_owned() });
        };
        match self.backfill(&summary) {
            Ok(selected) => {
                self.selected = Some(selected);
                self.notice = None;
                Ok(())
            }
            Err(error) => {
                self.record(&error);
                Err(error)
            }
        }
    }

    /// Read the timeline forward from its cursor.
    ///
    /// Returns how many rows were added. A newly started or already open
    /// session reads live; a backfill that has not caught up reads as backfill.
    pub fn poll(&mut self) -> Result<usize, SessionError> {
        let Some((id, cursor)) = self
            .selected
            .as_ref()
            .map(|selected| (selected.summary.id.clone(), selected.timeline.cursor()))
        else {
            return Ok(0);
        };
        let page = match self.host.events(&id, cursor, EVENT_PAGE) {
            Ok(page) => page,
            Err(error) => {
                self.record(&error);
                return Err(error);
            }
        };
        let inserted = {
            let Some(selected) = self.selected.as_mut() else {
                return Ok(0);
            };
            let origin = if selected.timeline.is_live() {
                Origin::Live
            } else {
                Origin::Backfill
            };
            let inserted = selected.timeline.merge_page(&page.events, origin);
            if let Some(status) = selected.timeline.status().cloned() {
                selected.summary.status = status;
            }
            if page.events.is_empty() {
                selected.timeline.mark_live();
            }
            inserted
        };
        self.publish_events(&page.events);
        Ok(inserted)
    }

    /// Deliver the next input in the mode the session calls for.
    ///
    /// A busy session is steered; an idle one takes a follow-up.
    pub fn submit(&mut self, text: &str) -> Result<InputReceipt, SessionError> {
        let mode = self.input_mode();
        self.send(text, mode)
    }

    /// Steer the open session explicitly.
    pub fn steer(&mut self, text: &str) -> Result<InputReceipt, SessionError> {
        self.send(text, InputMode::Steer)
    }

    /// Queue an input behind the open session's current turn.
    pub fn queue(&mut self, text: &str) -> Result<InputReceipt, SessionError> {
        self.send(text, InputMode::Queue)
    }

    /// Abort the open session.
    ///
    /// The returned summary is always interrupted when it would otherwise look
    /// complete: a session a person stopped is never shown as finished.
    pub fn abort(&mut self) -> Result<SessionSummary, SessionError> {
        let Some(id) = self
            .selected
            .as_ref()
            .map(|selected| selected.summary.id.clone())
        else {
            return Err(SessionError::NoSessionSelected);
        };
        let summary = match self.host.abort(&id) {
            Ok(summary) => summary,
            Err(error) => {
                self.record(&error);
                return Err(error);
            }
        };
        let summary = SessionSummary {
            status: summary.status.after_abort(),
            ..summary
        };
        if let Some(selected) = self.selected.as_mut() {
            selected.summary = summary.clone();
        }
        if let Some(known) = self.sessions.iter_mut().find(|session| session.id == id) {
            *known = summary.clone();
        }
        self.announce_stop(&summary);
        Ok(summary)
    }

    /// Ask the host for the next page and catch up.
    fn backfill(&mut self, summary: &SessionSummary) -> Result<Selected, SessionError> {
        let mut selected = Selected {
            summary: summary.clone(),
            timeline: Timeline::default(),
        };
        for _ in 0..MAX_BACKFILL_PAGES {
            let before = selected.timeline.cursor();
            let page = self.host.events(&selected.summary.id, before, EVENT_PAGE)?;
            if page.events.is_empty() {
                break;
            }
            selected.timeline.merge_page(&page.events, Origin::Backfill);
            if selected.timeline.cursor() == before {
                // Nothing advanced: the host repeated a page. Stop rather than
                // walk it forever.
                break;
            }
            if page.next.is_none() {
                break;
            }
        }
        selected.timeline.mark_live();
        if let Some(status) = selected.timeline.status().cloned() {
            selected.summary.status = status;
        }
        Ok(selected)
    }

    /// Deliver one input in an explicit mode and merge what the host records.
    fn send(&mut self, text: &str, mode: InputMode) -> Result<InputReceipt, SessionError> {
        if text.trim().is_empty() {
            return Err(SessionError::InvalidRequest {
                detail: "an input needs some text".to_owned(),
            });
        }
        let Some(id) = self
            .selected
            .as_ref()
            .map(|selected| selected.summary.id.clone())
        else {
            return Err(SessionError::NoSessionSelected);
        };
        let receipt = match self.host.input(&id, text, mode) {
            Ok(receipt) => receipt,
            Err(error) => {
                self.record(&error);
                return Err(error);
            }
        };
        if receipt.mode != mode {
            let error = SessionError::Protocol {
                detail: format!(
                    "the host delivered a {} input as {}",
                    mode.as_str(),
                    receipt.mode.as_str()
                ),
            };
            self.record(&error);
            return Err(error);
        }
        if let Some(selected) = self.selected.as_mut() {
            selected.timeline.merge_event(&receipt.event, Origin::Live);
            match mode {
                InputMode::Queue => {
                    selected.summary.queued = selected.summary.queued.saturating_add(1);
                }
                InputMode::Steer => {
                    selected.summary.status = SessionStatus::Running;
                }
                InputMode::Prompt => {}
            }
        }
        self.publish_events(std::slice::from_ref(&receipt.event));
        Ok(receipt)
    }

    /// Announce a new session's existence to the pool.
    fn announce_start(&mut self, summary: &SessionSummary) {
        let Some(pool) = self.pool.as_mut() else {
            return;
        };
        let event = HostEvent {
            sequence: summary.last_sequence,
            kind: "started".to_owned(),
            status: Some(summary.status.as_str().to_owned()),
            ..HostEvent::default()
        };
        let published = match pool.opt_in(&summary.id) {
            Ok(()) => pool.publish(&summary.id, &event),
            Err(error) => Err(error),
        };
        match published {
            Ok(()) => {
                self.announced.insert(summary.id.clone());
            }
            Err(error) => {
                self.notice = Some(format!("the pool did not receive this session: {error}"));
            }
        }
    }

    /// Announce that the session stopped.
    fn announce_stop(&mut self, summary: &SessionSummary) {
        if !self.announced.contains(&summary.id) {
            return;
        }
        let Some(pool) = self.pool.as_mut() else {
            return;
        };
        let event = HostEvent {
            sequence: summary.last_sequence,
            kind: "stopped".to_owned(),
            status: Some(summary.status.as_str().to_owned()),
            ..HostEvent::default()
        };
        if let Err(error) = pool.publish(&summary.id, &event) {
            self.notice = Some(format!("the pool did not receive this session: {error}"));
        }
    }

    /// Publish the events a page carried, filtered by the publication policy.
    fn publish_events(&mut self, events: &[HostEvent]) {
        let Some(id) = self
            .selected
            .as_ref()
            .map(|selected| selected.summary.id.clone())
        else {
            return;
        };
        if !self.announced.contains(&id) {
            return;
        }
        let publish_content = self.publish_content;
        let Some(pool) = self.pool.as_mut() else {
            return;
        };
        let mut failure: Option<PoolError> = None;
        for event in events {
            if !is_lifecycle(event) && !publish_content {
                continue;
            }
            if let Err(error) = pool.publish(&id, event) {
                failure = Some(error);
                break;
            }
        }
        if let Some(error) = failure {
            self.notice = Some(format!(
                "the pool did not receive this session's events: {error}"
            ));
        }
    }

    /// Keep the open session's summary in step with a fresh list.
    fn sync_selected(&mut self) {
        let Some(selected) = self.selected.as_mut() else {
            return;
        };
        if let Some(found) = self
            .sessions
            .iter()
            .find(|session| session.id == selected.summary.id)
        {
            selected.summary = found.clone();
        }
    }

    fn record(&mut self, error: &SessionError) {
        self.notice = Some(error.to_string());
    }

    /// The host, for the tests' call log.
    #[cfg(test)]
    pub(crate) fn host(&self) -> &H {
        &self.host
    }

    /// The relay, for the tests' call log.
    #[cfg(test)]
    pub(crate) fn pool(&self) -> Option<&P> {
        self.pool.as_ref()
    }
}

/// Everything the tests need from a host agent, without a socket.
///
/// Each method pops one scripted answer and records the call, so an unexpected
/// request is a clear failure rather than a silent success.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct ScriptedHost {
    pub(crate) calls: Vec<String>,
    pub(crate) listed: std::collections::VecDeque<Result<Vec<SessionSummary>, SessionError>>,
    pub(crate) started: std::collections::VecDeque<Result<SessionSummary, SessionError>>,
    pub(crate) pages: std::collections::VecDeque<Result<EventPage, SessionError>>,
    pub(crate) receipts: std::collections::VecDeque<Result<InputReceipt, SessionError>>,
    pub(crate) aborted: std::collections::VecDeque<Result<SessionSummary, SessionError>>,
}

#[cfg(test)]
impl ScriptedHost {
    fn pop<T>(
        queue: &mut std::collections::VecDeque<Result<T, SessionError>>,
        what: &str,
    ) -> Result<T, SessionError> {
        queue.pop_front().unwrap_or_else(|| {
            Err(SessionError::Protocol {
                detail: format!("unscripted {what} call"),
            })
        })
    }
}

#[cfg(test)]
impl HostSessions for ScriptedHost {
    fn list(&mut self) -> Result<Vec<SessionSummary>, SessionError> {
        self.calls.push("list".to_owned());
        Self::pop(&mut self.listed, "list")
    }

    fn start(&mut self, request: &StartSession) -> Result<SessionSummary, SessionError> {
        self.calls
            .push(format!("start:{}:{}", request.project, request.prompt));
        Self::pop(&mut self.started, "start")
    }

    fn events(&mut self, session: &str, after: u64, limit: u32) -> Result<EventPage, SessionError> {
        self.calls.push(format!("events:{session}:{after}:{limit}"));
        Self::pop(&mut self.pages, "events")
    }

    fn input(
        &mut self,
        session: &str,
        text: &str,
        mode: InputMode,
    ) -> Result<InputReceipt, SessionError> {
        self.calls
            .push(format!("input:{session}:{}:{text}", mode.as_str()));
        Self::pop(&mut self.receipts, "input")
    }

    fn abort(&mut self, session: &str) -> Result<SessionSummary, SessionError> {
        self.calls.push(format!("abort:{session}"));
        Self::pop(&mut self.aborted, "abort")
    }
}

/// A relay that records what it was asked to publish.
#[cfg(test)]
#[derive(Default)]
pub(crate) struct ScriptedRelay {
    pub(crate) calls: Vec<String>,
    pub(crate) opt_ins: std::collections::VecDeque<Result<(), PoolError>>,
    pub(crate) published: std::collections::VecDeque<Result<(), PoolError>>,
}

#[cfg(test)]
impl PoolRelay for ScriptedRelay {
    fn opt_in(&mut self, session: &str) -> Result<(), PoolError> {
        self.calls.push(format!("opt_in:{session}"));
        self.opt_ins.pop_front().unwrap_or_else(|| {
            Err(PoolError::Protocol {
                detail: "unscripted opt_in call".to_owned(),
            })
        })
    }

    fn publish(&mut self, session: &str, event: &HostEvent) -> Result<(), PoolError> {
        self.calls.push(format!("publish:{session}:{}", event.kind));
        self.published.pop_front().unwrap_or_else(|| {
            Err(PoolError::Protocol {
                detail: "unscripted publish call".to_owned(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(status: SessionStatus) -> SessionSummary {
        SessionSummary {
            id: "ses_1".to_owned(),
            project: "/Users/operator/work/tidepool".to_owned(),
            title: "Add the sessions pane".to_owned(),
            status,
            queued: 0,
            last_sequence: 7,
            started_at: "2026-10-29T09:00:00Z".to_owned(),
        }
    }

    fn message(sequence: u64, author: &str, text: &str) -> HostEvent {
        HostEvent {
            sequence,
            kind: "message".to_owned(),
            author: Some(author.to_owned()),
            text: Some(text.to_owned()),
            ..HostEvent::default()
        }
    }

    fn output(sequence: u64, text: &str) -> HostEvent {
        HostEvent {
            sequence,
            kind: "output".to_owned(),
            text: Some(text.to_owned()),
            ..HostEvent::default()
        }
    }

    fn receipt(mode: InputMode, sequence: u64, author: &str, text: &str) -> InputReceipt {
        InputReceipt {
            mode,
            event: message(sequence, author, text),
        }
    }

    fn page(events: Vec<HostEvent>) -> EventPage {
        EventPage {
            next: events.iter().map(|event| event.sequence).max(),
            events,
        }
    }

    fn started_controller() -> SessionController<ScriptedHost, ScriptedRelay> {
        SessionController::new(ScriptedHost::default(), None)
    }

    fn start(
        controller: &mut SessionController<ScriptedHost, ScriptedRelay>,
        status: SessionStatus,
    ) {
        controller.host.started.push_back(Ok(summary(status)));
        controller
            .start(StartSession::new(
                "/Users/operator/work/tidepool",
                "Add the sessions pane",
            ))
            .expect("start");
    }

    #[test]
    fn starting_a_session_selects_it_and_announces_its_lifecycle() {
        let mut host = ScriptedHost::default();
        host.started.push_back(Ok(summary(SessionStatus::Running)));
        let mut relay = ScriptedRelay::default();
        relay.opt_ins.push_back(Ok(()));
        relay.published.push_back(Ok(()));
        let mut controller = SessionController::new(host, Some(relay));

        let started = controller
            .start(StartSession::new(
                "/Users/operator/work/tidepool",
                "Add the sessions pane",
            ))
            .expect("start");

        assert_eq!(started.id, "ses_1");
        assert_eq!(controller.sessions().len(), 1);
        assert_eq!(
            controller
                .selected_summary()
                .map(|session| session.id.as_str()),
            Some("ses_1")
        );
        assert!(controller.notice().is_none());
        let relay = controller.pool().expect("relay");
        assert_eq!(relay.calls, vec!["opt_in:ses_1", "publish:ses_1:started"]);
        assert_eq!(
            controller.host().calls,
            vec!["start:/Users/operator/work/tidepool:Add the sessions pane"]
        );
    }

    #[test]
    fn an_unreachable_pool_does_not_stop_a_local_start() {
        let mut host = ScriptedHost::default();
        host.started.push_back(Ok(summary(SessionStatus::Running)));
        let mut relay = ScriptedRelay::default();
        relay.opt_ins.push_back(Err(PoolError::Unreachable {
            detail: "connection refused".to_owned(),
        }));
        let mut controller = SessionController::new(host, Some(relay));

        let started = controller
            .start(StartSession::new("/Users/operator/work/tidepool", "Add it"))
            .expect("the local session still starts");
        assert_eq!(started.id, "ses_1");
        assert!(
            controller
                .notice()
                .is_some_and(|notice| notice.contains("pool"))
        );
        // Nothing was published, because the opt-in failed first.
        assert_eq!(
            controller.pool().expect("relay").calls,
            vec!["opt_in:ses_1"]
        );
    }

    #[test]
    fn a_busy_session_is_offered_steering_not_a_second_prompt() {
        let mut controller = started_controller();
        start(&mut controller, SessionStatus::Running);

        assert_eq!(controller.input_mode(), InputMode::Steer);
        let actions = controller.actions();
        assert!(actions.contains(&SessionAction::Steer));
        assert!(actions.contains(&SessionAction::Queue));
        assert!(
            !actions.contains(&SessionAction::SendFollowUp),
            "a busy session must not offer a second prompt"
        );
        assert_eq!(SessionAction::Steer.label(), "Steer this session");

        controller.host.receipts.push_back(Ok(receipt(
            InputMode::Steer,
            8,
            "user",
            "tighten the wording",
        )));
        let receipt = controller.submit("tighten the wording").expect("steer");
        assert_eq!(receipt.mode, InputMode::Steer);
        assert_eq!(
            controller.host().calls.last().map(String::as_str),
            Some("input:ses_1:steer:tighten the wording")
        );
        let timeline = controller.timeline().expect("timeline");
        assert_eq!(
            timeline.entries().last().expect("row").kind,
            EntryKind::User
        );
    }

    #[test]
    fn an_idle_session_takes_a_follow_up() {
        let mut controller = started_controller();
        start(&mut controller, SessionStatus::Waiting);

        assert_eq!(controller.input_mode(), InputMode::Prompt);
        let actions = controller.actions();
        assert!(actions.contains(&SessionAction::SendFollowUp));
        assert!(!actions.contains(&SessionAction::Steer));

        controller.host.receipts.push_back(Ok(receipt(
            InputMode::Prompt,
            8,
            "user",
            "and add a test",
        )));
        let receipt = controller.submit("and add a test").expect("prompt");
        assert_eq!(receipt.mode, InputMode::Prompt);
        assert_eq!(
            controller.host().calls.last().map(String::as_str),
            Some("input:ses_1:prompt:and add a test")
        );
    }

    #[test]
    fn queueing_while_busy_is_a_queue_and_not_a_prompt() {
        let mut controller = started_controller();
        start(&mut controller, SessionStatus::Running);
        controller.host.receipts.push_back(Ok(receipt(
            InputMode::Queue,
            8,
            "user",
            "and add a test",
        )));

        let receipt = controller.queue("and add a test").expect("queue");
        assert_eq!(receipt.mode, InputMode::Queue);
        assert_eq!(
            controller.host().calls.last().map(String::as_str),
            Some("input:ses_1:queue:and add a test")
        );
        assert_eq!(controller.selected_summary().expect("selected").queued, 1);
        assert_eq!(controller.input_mode(), InputMode::Steer);
    }

    #[test]
    fn a_host_that_delivers_the_wrong_mode_is_not_trusted() {
        let mut controller = started_controller();
        start(&mut controller, SessionStatus::Running);
        controller.host.receipts.push_back(Ok(receipt(
            InputMode::Prompt,
            8,
            "user",
            "tighten the wording",
        )));

        let error = controller
            .submit("tighten the wording")
            .expect_err("a prompt is not a steer");
        assert!(matches!(error, SessionError::Protocol { .. }));
        assert!(controller.notice().is_some());
        assert!(controller.timeline().expect("timeline").is_empty());
    }

    #[test]
    fn abort_labels_the_session_interrupted_and_never_complete() {
        let mut controller = started_controller();
        start(&mut controller, SessionStatus::Running);
        controller
            .host
            .aborted
            .push_back(Ok(summary(SessionStatus::Interrupted)));

        let aborted = controller.abort().expect("abort");
        assert_eq!(aborted.status, SessionStatus::Interrupted);
        assert_eq!(aborted.status.label(), "interrupted");
        assert!(aborted.is_terminal());
        assert!(!aborted.is_complete());
        let snapshot = controller.snapshot();
        let selected = snapshot.selected.expect("selected");
        assert_eq!(selected.summary.status, SessionStatus::Interrupted);
        assert!(!selected.summary.status_note().contains("complete"));
    }

    #[test]
    fn an_abort_is_not_believed_when_the_host_says_succeeded() {
        let mut controller = started_controller();
        start(&mut controller, SessionStatus::Running);
        controller
            .host
            .aborted
            .push_back(Ok(summary(SessionStatus::Succeeded)));

        let aborted = controller.abort().expect("abort");
        assert_eq!(aborted.status, SessionStatus::Interrupted);
        assert!(!aborted.is_complete());
    }

    #[test]
    fn polling_appends_only_events_past_the_cursor() {
        let mut controller = started_controller();
        start(&mut controller, SessionStatus::Running);
        let first = vec![
            message(8, "session", "I added the pane."),
            message(9, "user", "and a test"),
        ];
        controller.host.pages.push_back(Ok(page(first.clone())));
        controller.host.pages.push_back(Ok(page(first)));
        controller
            .host
            .pages
            .push_back(Ok(page(vec![message(10, "session", "done")])));

        assert_eq!(controller.poll().expect("first poll"), 2);
        assert_eq!(controller.poll().expect("repeat poll"), 0);
        assert_eq!(controller.poll().expect("new poll"), 1);
        let timeline = controller.timeline().expect("timeline");
        assert_eq!(timeline.len(), 3);
        assert_eq!(timeline.live_from(), Some(8));
        let sequences: Vec<u64> = timeline.entries().iter().map(|row| row.sequence).collect();
        assert_eq!(sequences, vec![8, 9, 10]);
        assert_eq!(timeline.entries()[0].kind, EntryKind::Agent);
    }

    #[test]
    fn opening_a_session_backfills_and_then_follows_live() {
        let mut controller = started_controller();
        controller
            .host
            .listed
            .push_back(Ok(vec![summary(SessionStatus::Running)]));
        controller.refresh().expect("list");
        controller.host.pages.push_back(Ok(EventPage {
            events: vec![
                message(1, "user", "Add the sessions pane"),
                output(2, "I started on the pane."),
            ],
            next: Some(2),
        }));
        controller.host.pages.push_back(Ok(EventPage {
            events: Vec::new(),
            next: None,
        }));
        controller.open("ses_1").expect("open");

        let timeline = controller.timeline().expect("timeline");
        assert!(timeline.is_live());
        assert_eq!(timeline.len(), 2);
        assert!(
            timeline
                .entries()
                .iter()
                .all(|row| row.origin == Origin::Backfill)
        );

        controller
            .host
            .pages
            .push_back(Ok(page(vec![message(3, "session", "The pane is in.")])));
        assert_eq!(controller.poll().expect("live poll"), 1);
        let timeline = controller.timeline().expect("timeline");
        assert_eq!(timeline.live_from(), Some(3));
        assert_eq!(timeline.entries()[2].origin, Origin::Live);
    }

    #[test]
    fn content_is_published_only_when_the_person_opted_in() {
        let mut host = ScriptedHost::default();
        host.started.push_back(Ok(summary(SessionStatus::Running)));
        let mut relay = ScriptedRelay::default();
        relay.opt_ins.push_back(Ok(()));
        relay.published.push_back(Ok(())); // started
        let mut controller = SessionController::new(host, Some(relay));
        start(&mut controller, SessionStatus::Running);

        controller
            .host
            .pages
            .push_back(Ok(page(vec![output(8, "I added the pane.")])));
        controller.poll().expect("poll");
        assert_eq!(
            controller.pool().expect("relay").calls,
            vec!["opt_in:ses_1", "publish:ses_1:started"],
            "an output must not leave the machine by default"
        );

        controller.set_publish_content(true);
        assert!(controller.publishes_content());
        controller
            .pool
            .as_mut()
            .expect("relay")
            .published
            .push_back(Ok(()));
        controller
            .host
            .pages
            .push_back(Ok(page(vec![output(9, "The pane is in.")])));
        controller.poll().expect("poll");
        assert_eq!(
            controller
                .pool()
                .expect("relay")
                .calls
                .last()
                .map(String::as_str),
            Some("publish:ses_1:output")
        );
    }

    #[test]
    fn reopening_an_announced_session_keeps_publishing_its_lifecycle() {
        let mut host = ScriptedHost::default();
        host.started.push_back(Ok(summary(SessionStatus::Running)));
        let mut relay = ScriptedRelay::default();
        relay.opt_ins.push_back(Ok(()));
        relay.published.push_back(Ok(())); // started
        relay.published.push_back(Ok(())); // status after reopening
        let mut controller = SessionController::new(host, Some(relay));
        start(&mut controller, SessionStatus::Running);

        controller.host.pages.push_back(Ok(EventPage {
            events: Vec::new(),
            next: None,
        }));
        controller.open("ses_1").expect("reopen");
        let stopped = HostEvent {
            sequence: 8,
            kind: "status".to_owned(),
            status: Some("interrupted".to_owned()),
            ..HostEvent::default()
        };
        controller.host.pages.push_back(Ok(page(vec![stopped])));
        controller.poll().expect("poll");

        assert_eq!(
            controller
                .pool()
                .expect("relay")
                .calls
                .last()
                .map(String::as_str),
            Some("publish:ses_1:status")
        );
    }

    #[test]
    fn an_empty_input_is_refused_before_the_host() {
        let mut controller = started_controller();
        start(&mut controller, SessionStatus::Running);
        let calls_before = controller.host().calls.len();

        assert!(matches!(
            controller.submit("   "),
            Err(SessionError::InvalidRequest { .. })
        ));
        assert_eq!(controller.host().calls.len(), calls_before);
    }

    #[test]
    fn a_refresh_keeps_the_open_session_in_step() {
        let mut controller = started_controller();
        start(&mut controller, SessionStatus::Running);
        controller
            .host
            .listed
            .push_back(Ok(vec![summary(SessionStatus::Interrupted)]));

        controller.refresh().expect("list");
        let selected = controller.selected_summary().expect("selected");
        assert_eq!(selected.status, SessionStatus::Interrupted);
        assert!(!selected.status.is_complete());
    }

    #[test]
    fn a_session_that_was_stopped_somewhere_else_is_interrupted() {
        let mut controller = started_controller();
        start(&mut controller, SessionStatus::Running);
        let stopped = HostEvent {
            sequence: 8,
            kind: "stopped".to_owned(),
            status: Some("stopped".to_owned()),
            ..HostEvent::default()
        };
        controller.host.pages.push_back(Ok(page(vec![stopped])));
        controller.poll().expect("poll");
        let selected = controller.selected_summary().expect("selected");
        assert_eq!(selected.status, SessionStatus::Interrupted);
        assert!(!selected.status.is_complete());
        let row = controller.timeline().expect("timeline").entries().last();
        assert_eq!(row.expect("row").kind, EntryKind::Status);
        assert!(row.expect("row").text.contains("interrupted"));
    }

    #[test]
    fn the_actions_follow_the_session_state() {
        assert_eq!(
            available_actions(Some(&SessionStatus::Running)),
            vec![
                SessionAction::Steer,
                SessionAction::Queue,
                SessionAction::Abort
            ]
        );
        assert_eq!(
            available_actions(Some(&SessionStatus::Queued)),
            vec![SessionAction::Queue, SessionAction::Abort]
        );
        assert_eq!(
            available_actions(Some(&SessionStatus::Waiting)),
            vec![SessionAction::SendFollowUp, SessionAction::Abort]
        );
        assert!(available_actions(Some(&SessionStatus::Interrupted)).is_empty());
        assert!(available_actions(None).is_empty());
    }

    #[test]
    fn the_composer_never_offers_a_prompt_to_a_busy_session() {
        for status in [SessionStatus::Running, SessionStatus::Queued] {
            assert_ne!(
                InputMode::for_status(&status),
                InputMode::Prompt,
                "{status} must not be offered as a prompt"
            );
        }
    }
}
