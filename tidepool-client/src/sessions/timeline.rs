//! A session's timeline, in the four kinds a reader needs to tell apart.
//!
//! The host writes events; the client keeps them in one sequence-ordered list
//! and maps each to exactly one [`EntryKind`]:
//!
//! | Host event | Entry |
//! |---|---|
//! | `output`, or a `message` authored by the session | [`EntryKind::Agent`] |
//! | `tool` | [`EntryKind::Tool`] |
//! | a `message` authored by the person | [`EntryKind::User`] |
//! | `started`, `status`, `stopped`, or anything unknown | [`EntryKind::Status`] |
//!
//! A `message` with no author is read as the session's, never as the person's:
//! attributing an agent's words to the user is the one mistake a transcript
//! must not make.
//!
//! # Merging is idempotent, and late is better than lost
//!
//! Every entry carries the host's monotonic `sequence`. [`Timeline::merge_page`]
//! ignores a sequence it already holds, so reading the same page twice adds
//! nothing and a poll that overlaps the previous one cannot duplicate a row.
//! An event that arrives below the cursor is still kept: a live read can be
//! reordered, and dropping a row is worse than holding it twice. Entries are
//! held in sequence order even when a page arrives out of order, and the cursor
//! is the highest sequence seen.
//!
//! # Backfill and live are separate
//!
//! Opening a session pages forward from zero until the host reports no more;
//! every row read then is [`Origin::Backfill`]. Once the timeline is live, rows
//! read by later polls are [`Origin::Live`] and [`Timeline::live_from`] names
//! the first of them, so a surface can draw the boundary instead of leaving a
//! reader to guess.

use crate::sessions::protocol::{HostEvent, SessionStatus};

/// The four things a timeline row can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EntryKind {
    /// Something the person asked.
    User,
    /// Something the agent said.
    Agent,
    /// Something a tool did.
    Tool,
    /// The session itself: started, changed state, stopped.
    Status,
}

impl EntryKind {
    /// The word the row is labelled with.
    ///
    /// The label is the distinction: a reader can tell the four kinds apart
    /// with the colour stripped out, because each row says which it is.
    pub fn label(&self) -> &'static str {
        match self {
            Self::User => "You",
            Self::Agent => "Agent",
            Self::Tool => "Tool",
            Self::Status => "Session",
        }
    }

    /// The lowercase marker used in a compact line.
    pub fn marker(&self) -> &'static str {
        match self {
            Self::User => "you",
            Self::Agent => "agent",
            Self::Tool => "tool",
            Self::Status => "session",
        }
    }

    /// The kind a host event becomes.
    pub fn from_event(event: &HostEvent) -> Self {
        match event.kind.as_str() {
            "output" => Self::Agent,
            "tool" => Self::Tool,
            "message" => match event.author.as_deref() {
                Some("user") => Self::User,
                // `session`, `agent`, a missing author and anything new are the
                // session's words, never the person's.
                _ => Self::Agent,
            },
            // `started`, `status`, `stopped` and every kind this build does not
            // know are lifecycle rows.
            _ => Self::Status,
        }
    }
}

/// Where a row came from: the opening backfill, or a later live read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    /// Read while catching up with the session's history.
    #[default]
    Backfill,
    /// Read after the timeline had caught up.
    Live,
}

/// One row of a session's timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    /// The host's monotonic sequence.
    pub sequence: u64,
    /// Which of the four kinds it is.
    pub kind: EntryKind,
    /// The words or the summary shown.
    pub text: String,
    /// Whether it was read during backfill or live.
    pub origin: Origin,
}

impl TimelineEntry {
    /// Turn one host event into a row, or `None` when it carries nothing to
    /// show.
    pub fn from_event(event: &HostEvent, origin: Origin) -> Option<Self> {
        let kind = EntryKind::from_event(event);
        let text = match kind {
            EntryKind::Agent | EntryKind::User => event.text.clone()?,
            EntryKind::Tool => tool_text(event)?,
            EntryKind::Status => status_text(event),
        };
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        Some(Self {
            sequence: event.sequence,
            kind,
            text: text.to_owned(),
            origin,
        })
    }

    /// The compact line a text-only surface draws.
    pub fn line(&self) -> String {
        format!("{}: {}", self.kind.label(), self.text)
    }
}

/// A tool row names the tool and, when the host sent one, its summary.
fn tool_text(event: &HostEvent) -> Option<String> {
    let tool = event.tool.as_deref().unwrap_or("tool");
    match event.summary.as_deref().or(event.text.as_deref()) {
        Some(detail) if !detail.trim().is_empty() => Some(format!("{tool} — {}", detail.trim())),
        _ => Some(tool.to_owned()),
    }
}

/// A lifecycle row says what the session did, in the status's own words.
fn status_text(event: &HostEvent) -> String {
    match event.kind.as_str() {
        "started" => "Session started".to_owned(),
        "status" | "stopped" => event
            .status
            .as_deref()
            .map(SessionStatus::parse)
            .unwrap_or_default()
            .note()
            .to_owned(),
        other => match event.status.as_deref() {
            Some(status) => format!("{other} — {}", SessionStatus::parse(status).note()),
            None => other.to_owned(),
        },
    }
}

/// One session's timeline, held in sequence order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Timeline {
    entries: Vec<TimelineEntry>,
    cursor: u64,
    live: bool,
    live_from: Option<u64>,
    status: Option<SessionStatus>,
}

impl Timeline {
    /// The rows, oldest first.
    pub fn entries(&self) -> &[TimelineEntry] {
        &self.entries
    }

    /// The highest sequence read so far.
    pub fn cursor(&self) -> u64 {
        self.cursor
    }

    /// Whether the opening backfill has finished.
    pub fn is_live(&self) -> bool {
        self.live
    }

    /// The sequence of the first live row, when one has been read.
    pub fn live_from(&self) -> Option<u64> {
        self.live_from
    }

    /// The latest lifecycle the host reported, when it reported one.
    pub fn status(&self) -> Option<&SessionStatus> {
        self.status.as_ref()
    }

    /// How many rows are held.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the timeline is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Drop everything and start again, for a different session.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Mark the backfill finished. Later merges are live.
    pub fn mark_live(&mut self) {
        self.live = true;
    }

    /// Merge one page of events read at `origin`.
    ///
    /// Returns how many rows were added. A sequence already held is ignored;
    /// an event with nothing to show still advances the cursor.
    pub fn merge_page(&mut self, events: &[HostEvent], origin: Origin) -> usize {
        events
            .iter()
            .filter(|event| self.merge_event(event, origin))
            .count()
    }

    /// Merge one event read at `origin`.
    ///
    /// Returns whether it became a row. A sequence that is already held is
    /// ignored; a late event below the cursor is still kept, because a live
    /// read can arrive out of order and losing a row is worse than accepting
    /// one twice.
    pub fn merge_event(&mut self, event: &HostEvent, origin: Origin) -> bool {
        if self
            .entries
            .iter()
            .any(|held| held.sequence == event.sequence)
        {
            return false;
        }
        if event.sequence > self.cursor {
            self.cursor = event.sequence;
        }
        if let Some(status) = event.status_value() {
            self.status = Some(status);
        }
        let Some(entry) = TimelineEntry::from_event(event, origin) else {
            return false;
        };
        let position = self
            .entries
            .binary_search_by_key(&entry.sequence, |held| held.sequence)
            .unwrap_or_else(|position| position);
        if self.live && self.live_from.is_none() {
            self.live_from = Some(entry.sequence);
        }
        self.entries.insert(position, entry);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(sequence: u64, kind: &str) -> HostEvent {
        HostEvent {
            sequence,
            kind: kind.to_owned(),
            ..HostEvent::default()
        }
    }

    fn message(sequence: u64, author: Option<&str>, text: &str) -> HostEvent {
        HostEvent {
            author: author.map(str::to_owned),
            text: Some(text.to_owned()),
            ..event(sequence, "message")
        }
    }

    #[test]
    fn the_four_kinds_are_mapped_from_the_hosts_events() {
        let output = HostEvent {
            text: Some("I added the pane.".to_owned()),
            ..event(1, "output")
        };
        let tool = HostEvent {
            tool: Some("shell".to_owned()),
            summary: Some("cargo test — 55 passed".to_owned()),
            ..event(2, "tool")
        };
        let asked = message(3, Some("user"), "Add the sessions pane");
        let status = HostEvent {
            status: Some("interrupted".to_owned()),
            ..event(4, "status")
        };

        let rows: Vec<TimelineEntry> = [&output, &tool, &asked, &status]
            .into_iter()
            .filter_map(|event| TimelineEntry::from_event(event, Origin::Backfill))
            .collect();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].kind, EntryKind::Agent);
        assert_eq!(rows[1].kind, EntryKind::Tool);
        assert_eq!(rows[2].kind, EntryKind::User);
        assert_eq!(rows[3].kind, EntryKind::Status);

        // The four kinds are distinguishable by their words alone.
        let labels: Vec<&str> = rows.iter().map(|row| row.kind.label()).collect();
        assert_eq!(labels, vec!["Agent", "Tool", "You", "Session"]);
        assert_eq!(
            labels
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            4
        );
        assert!(rows[1].text.starts_with("shell"));
        assert!(rows[3].text.contains("interrupted"));
    }

    #[test]
    fn a_message_without_an_author_is_never_the_persons() {
        let unattributed = message(1, None, "I am the session speaking");
        let session = message(2, Some("session"), "and so am I");
        let unknown = message(3, Some("system"), "still not the person");
        for event in [&unattributed, &session, &unknown] {
            let entry = TimelineEntry::from_event(event, Origin::Backfill).expect("entry");
            assert_eq!(entry.kind, EntryKind::Agent);
        }
        let person = message(4, Some("user"), "the person asked");
        assert_eq!(
            TimelineEntry::from_event(&person, Origin::Backfill)
                .expect("entry")
                .kind,
            EntryKind::User
        );
    }

    #[test]
    fn an_unknown_event_kind_stays_visible_as_a_session_row() {
        let mut unknown = event(1, "checkpoint");
        unknown.status = Some("running".to_owned());
        let entry = TimelineEntry::from_event(&unknown, Origin::Backfill).expect("entry");
        assert_eq!(entry.kind, EntryKind::Status);
        assert!(entry.text.contains("checkpoint"));
    }

    #[test]
    fn merging_the_same_page_twice_adds_nothing() {
        let page = vec![
            message(1, Some("user"), "first"),
            message(2, Some("session"), "second"),
        ];
        let mut timeline = Timeline::default();
        assert_eq!(timeline.merge_page(&page, Origin::Backfill), 2);
        assert_eq!(timeline.merge_page(&page, Origin::Backfill), 0);
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline.cursor(), 2);
    }

    #[test]
    fn rows_are_kept_in_sequence_order_even_from_an_out_of_order_page() {
        let mut timeline = Timeline::default();
        timeline.merge_event(&message(3, Some("user"), "third"), Origin::Backfill);
        timeline.merge_event(&message(1, Some("user"), "first"), Origin::Backfill);
        timeline.merge_event(&message(2, Some("session"), "second"), Origin::Backfill);
        let sequences: Vec<u64> = timeline.entries().iter().map(|row| row.sequence).collect();
        assert_eq!(sequences, vec![1, 2, 3]);
    }

    #[test]
    fn a_late_event_is_kept_and_a_duplicate_is_not() {
        let mut timeline = Timeline::default();
        timeline.merge_event(&message(5, Some("user"), "fifth"), Origin::Backfill);
        assert!(timeline.merge_event(&message(4, Some("user"), "fourth"), Origin::Backfill));
        assert!(!timeline.merge_event(&message(4, Some("user"), "fourth"), Origin::Backfill));
        assert_eq!(timeline.len(), 2);
        let sequences: Vec<u64> = timeline.entries().iter().map(|row| row.sequence).collect();
        assert_eq!(sequences, vec![4, 5]);
        assert_eq!(timeline.cursor(), 5);
    }

    #[test]
    fn the_backfill_and_live_boundary_is_marked() {
        let mut timeline = Timeline::default();
        timeline.merge_event(&message(1, Some("user"), "asked"), Origin::Backfill);
        assert!(!timeline.is_live());
        assert_eq!(timeline.live_from(), None);

        timeline.mark_live();
        timeline.merge_event(&message(2, Some("session"), "answered"), Origin::Live);
        assert!(timeline.is_live());
        assert_eq!(timeline.live_from(), Some(2));
        assert_eq!(timeline.entries()[1].origin, Origin::Live);
    }

    #[test]
    fn a_status_event_moves_the_sessions_lifecycle() {
        let mut timeline = Timeline::default();
        timeline.merge_event(&event(1, "started"), Origin::Backfill);
        assert_eq!(timeline.status(), Some(&SessionStatus::Running));
        let interrupted = HostEvent {
            status: Some("interrupted".to_owned()),
            ..event(2, "stopped")
        };
        timeline.merge_event(&interrupted, Origin::Backfill);
        assert_eq!(timeline.status(), Some(&SessionStatus::Interrupted));
        assert!(!timeline.status().expect("status").is_complete());
    }

    #[test]
    fn an_empty_output_advances_the_cursor_without_a_row() {
        let mut timeline = Timeline::default();
        let empty = HostEvent {
            text: Some("   ".to_owned()),
            ..event(7, "output")
        };
        assert!(!timeline.merge_event(&empty, Origin::Backfill));
        assert_eq!(timeline.cursor(), 7);
        assert!(timeline.is_empty());
    }

    #[test]
    fn a_row_line_names_its_kind() {
        let entry = TimelineEntry {
            sequence: 1,
            kind: EntryKind::Tool,
            text: "shell — ls".to_owned(),
            origin: Origin::Backfill,
        };
        assert_eq!(entry.line(), "Tool: shell — ls");
    }
}
