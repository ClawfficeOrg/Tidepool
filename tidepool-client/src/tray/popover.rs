//! The tray popover's content: the current host and the sessions in flight.
//!
//! This is a model, not a view. The popover window is one small surface, but
//! what it says is decided here so it can be tested without a window and so the
//! same lines can feed a tooltip or a future native menu:
//!
//! - the host this client enrolled, with whether its pool answered, or the
//!   words for "nothing enrolled yet";
//! - one row per session that has not stopped ([`SessionStatus::is_terminal`]),
//!   because a finished session is history and the popover is about what is
//!   happening now;
//! - the last notice worth showing, when there is one.
//!
//! Rows keep the host's order, so a session does not jump around the popover as
//! its status changes.

use crate::sessions::{SessionStatus, SessionSummary};
use crate::tray::icon::PoolState;

/// The machine this client enrolled, as the popover names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSummary {
    /// The host's public id.
    pub id: String,
    /// The name the pool shows, falling back to the id when it has none.
    pub name: String,
    /// Whether the pool answered the last read.
    pub pool: PoolState,
}

impl HostSummary {
    /// Build a host row.
    pub fn new(id: impl Into<String>, name: impl Into<String>, pool: PoolState) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            pool,
        }
    }

    /// The name to draw: the display name, else the id, else a placeholder.
    pub fn display_name(&self) -> &str {
        if !self.name.trim().is_empty() {
            &self.name
        } else if !self.id.trim().is_empty() {
            &self.id
        } else {
            "this machine"
        }
    }

    /// The host's popover line.
    pub fn line(&self) -> String {
        format!(
            "Host {} \u{2014} {}",
            self.display_name(),
            self.pool.label()
        )
    }
}

/// One session the popover lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// The session's public id.
    pub id: String,
    /// The first line of its prompt, or the id when it has no title.
    pub title: String,
    /// The project directory it runs in.
    pub project: String,
    /// Its lifecycle.
    pub status: SessionStatus,
    /// How many inputs are waiting behind the current turn.
    pub queued: u32,
}

impl SessionRow {
    /// Build a row from a host session.
    pub fn from_summary(summary: &SessionSummary) -> Self {
        Self {
            id: summary.id.clone(),
            title: if summary.title.trim().is_empty() {
                summary.id.clone()
            } else {
                summary.title.clone()
            },
            project: summary.project.clone(),
            status: summary.status.clone(),
            queued: summary.queued,
        }
    }

    /// The row's words: what it is doing, and how much is queued behind it.
    pub fn line(&self) -> String {
        let mut line = format!("{} \u{2014} {}", self.title, self.status.label());
        if self.queued > 0 {
            line.push_str(&format!(", {} queued", self.queued));
        }
        line
    }
}

/// What the popover draws.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrayPopover {
    host: Option<HostSummary>,
    sessions: Vec<SessionRow>,
    notice: Option<String>,
}

impl TrayPopover {
    /// Build the popover from the current host and the host's session list.
    ///
    /// Only sessions that have not stopped are kept; a succeeded, failed or
    /// interrupted session is history, not something the popover is for.
    pub fn new(host: Option<HostSummary>, sessions: &[SessionSummary]) -> Self {
        let mut popover = Self {
            host,
            sessions: Vec::new(),
            notice: None,
        };
        popover.set_sessions(sessions);
        popover
    }

    /// The current host, when one is enrolled.
    pub fn host(&self) -> Option<&HostSummary> {
        self.host.as_ref()
    }

    /// Replace the current host.
    pub fn set_host(&mut self, host: Option<HostSummary>) {
        self.host = host;
    }

    /// The sessions the popover lists.
    pub fn sessions(&self) -> &[SessionRow] {
        &self.sessions
    }

    /// Replace the session rows, dropping sessions that have stopped.
    pub fn set_sessions(&mut self, sessions: &[SessionSummary]) {
        self.sessions = sessions
            .iter()
            .filter(|summary| !summary.status.is_terminal())
            .map(SessionRow::from_summary)
            .collect();
    }

    /// The last problem worth telling the person.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Replace the notice.
    pub fn set_notice(&mut self, notice: Option<String>) {
        self.notice = notice.filter(|notice| !notice.trim().is_empty());
    }

    /// Whether there is nothing at all to draw.
    pub fn is_empty(&self) -> bool {
        self.host.is_none() && self.sessions.is_empty() && self.notice.is_none()
    }

    /// The host line, or the words for a client that has not enrolled.
    pub fn host_line(&self) -> String {
        match self.host.as_ref() {
            Some(host) => host.line(),
            None => "No host enrolled".to_owned(),
        }
    }

    /// One line per listed session, in the host's order.
    pub fn session_lines(&self) -> Vec<String> {
        self.sessions.iter().map(SessionRow::line).collect()
    }

    /// The popover's lines, top to bottom: host, sessions, notice.
    ///
    /// With no session running the middle says so instead of leaving a gap, so
    /// "nothing is running" is a fact the popover states rather than a silence.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![self.host_line()];
        let sessions = self.session_lines();
        if sessions.is_empty() {
            lines.push("No sessions running".to_owned());
        } else {
            lines.extend(sessions);
        }
        if let Some(notice) = self.notice.as_ref() {
            lines.push(notice.clone());
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: &str, title: &str, status: SessionStatus) -> SessionSummary {
        SessionSummary {
            id: id.to_owned(),
            project: "/Users/operator/work/tidepool".to_owned(),
            title: title.to_owned(),
            status,
            queued: 0,
            last_sequence: 4,
            started_at: "2026-10-29T09:00:00Z".to_owned(),
        }
    }

    fn host() -> HostSummary {
        HostSummary::new("host_1", "studio-mac", PoolState::Reachable)
    }

    #[test]
    fn the_popover_names_the_current_host_and_whether_its_pool_answers() {
        let popover = TrayPopover::new(Some(host()), &[]);
        assert_eq!(
            popover.host_line(),
            "Host studio-mac \u{2014} pool reachable"
        );

        let unreachable = HostSummary::new("host_1", "studio-mac", PoolState::Unreachable);
        let popover = TrayPopover::new(Some(unreachable), &[]);
        assert_eq!(
            popover.host_line(),
            "Host studio-mac \u{2014} the pool cannot be reached"
        );
    }

    #[test]
    fn a_client_without_a_host_says_so() {
        let popover = TrayPopover::new(None, &[]);
        assert_eq!(popover.host_line(), "No host enrolled");
        assert!(popover.is_empty(), "nothing enrolled and nothing running");
    }

    #[test]
    fn the_popover_lists_only_sessions_that_have_not_stopped() {
        let sessions = vec![
            summary("ses_1", "Add the tray menu", SessionStatus::Running),
            summary("ses_2", "Fix the flicker", SessionStatus::Succeeded),
            summary("ses_3", "Waiting for a review", SessionStatus::Waiting),
            summary("ses_4", "A crashed one", SessionStatus::Failed),
            summary("ses_5", "An aborted one", SessionStatus::Interrupted),
            summary("ses_6", "Queued work", SessionStatus::Queued),
        ];
        let popover = TrayPopover::new(Some(host()), &sessions);

        let ids: Vec<&str> = popover
            .sessions()
            .iter()
            .map(|row| row.id.as_str())
            .collect();
        assert_eq!(ids, vec!["ses_1", "ses_3", "ses_6"], "order and liveness");
        assert_eq!(
            popover.session_lines(),
            vec![
                "Add the tray menu \u{2014} running",
                "Waiting for a review \u{2014} waiting",
                "Queued work \u{2014} queued",
            ]
        );
    }

    #[test]
    fn a_session_with_no_title_is_named_by_its_id() {
        let sessions = vec![summary("ses_1", "   ", SessionStatus::Running)];
        let popover = TrayPopover::new(None, &sessions);
        assert_eq!(popover.session_lines(), vec!["ses_1 \u{2014} running"]);
    }

    #[test]
    fn queued_inputs_are_counted_on_the_row() {
        let mut queued = summary("ses_1", "Add the tray menu", SessionStatus::Running);
        queued.queued = 2;
        let popover = TrayPopover::new(None, &[queued]);
        assert_eq!(
            popover.session_lines(),
            vec!["Add the tray menu \u{2014} running, 2 queued"]
        );
    }

    #[test]
    fn the_lines_are_host_then_sessions_then_notice() {
        let mut popover = TrayPopover::new(
            Some(host()),
            &[summary(
                "ses_1",
                "Add the tray menu",
                SessionStatus::Running,
            )],
        );
        popover.set_notice(Some("the pool did not receive this session".to_owned()));

        assert_eq!(
            popover.lines(),
            vec![
                "Host studio-mac \u{2014} pool reachable",
                "Add the tray menu \u{2014} running",
                "the pool did not receive this session",
            ]
        );
        assert!(!popover.is_empty());
    }

    #[test]
    fn an_empty_session_list_is_stated_not_left_blank() {
        let popover = TrayPopover::new(Some(host()), &[]);
        assert_eq!(
            popover.lines(),
            vec![
                "Host studio-mac \u{2014} pool reachable",
                "No sessions running"
            ]
        );
    }

    #[test]
    fn a_blank_notice_is_not_shown() {
        let mut popover = TrayPopover::new(Some(host()), &[]);
        popover.set_notice(Some("  \n".to_owned()));
        assert!(popover.notice().is_none());
        assert_eq!(popover.lines().len(), 2);
    }

    #[test]
    fn replacing_the_sessions_applies_the_liveness_filter_again() {
        let mut popover = TrayPopover::new(Some(host()), &[]);
        popover.set_sessions(&[
            summary("ses_1", "One", SessionStatus::Running),
            summary("ses_2", "Two", SessionStatus::Succeeded),
        ]);
        assert_eq!(popover.sessions().len(), 1);
        assert_eq!(popover.sessions()[0].id, "ses_1");

        popover.set_sessions(&[]);
        assert!(popover.sessions().is_empty());
        assert!(!popover.is_empty(), "the host is still enrolled");
    }
}
