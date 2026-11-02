//! The session pane: one session's timeline, its status and its controls.
//!
//! The view is deliberately dumb. [`crate::sessions::SessionController`] owns
//! the host and the pool; this module takes a
//! [`SessionSnapshot`] and turns it into rows and controls. That keeps the
//! rendering testable without a socket and keeps a click from reaching the
//! network from inside a `Render` implementation.
//!
//! # The four kinds are words, not colours
//!
//! [`TimelineRow::for_entry`] carries the entry's own label — `You`, `Agent`,
//! `Tool` or `Session` — and the row draws that label before the text. With
//! the palette removed, or for a reader who cannot see the difference between
//! two hues, the kind is still on screen.
//!
//! # Interrupted is on the header
//!
//! [`status_line`] is the session's title and its status sentence. An
//! interrupted session says `interrupted — stopped before it could finish`
//! there and in its lifecycle rows; it is never given the wording of a
//! finished one.

use gpui::{Context, FocusHandle, Focusable, Render, SharedString, Window, div, prelude::*, px};

use gpui_kit::component::ActiveTheme;
use gpui_kit::component::button::{Button, ButtonVariants};

use crate::sessions::{SessionAction, SessionSnapshot, TimelineEntry};

/// One timeline row, with the label that names its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineRow {
    /// The host's sequence, kept so a surface can key rows without a colour.
    pub sequence: u64,
    /// The kind's word: `You`, `Agent`, `Tool` or `Session`.
    pub label: &'static str,
    /// The words the row shows.
    pub text: String,
}

impl TimelineRow {
    /// Build the row for one timeline entry.
    pub fn for_entry(entry: &TimelineEntry) -> Self {
        Self {
            sequence: entry.sequence,
            label: entry.kind.label(),
            text: entry.text.clone(),
        }
    }

    /// The whole row as one line of text.
    pub fn line(&self) -> String {
        format!("{}: {}", self.label, self.text)
    }
}

/// The header line for a snapshot: the session's title and its state.
///
/// `None` when no session is open.
pub fn status_line(snapshot: &SessionSnapshot) -> Option<String> {
    let selected = snapshot.selected.as_ref()?;
    let note = selected.summary.status_note();
    let title = selected.summary.title.trim();
    Some(if title.is_empty() {
        note.to_owned()
    } else {
        format!("{title} — {note}")
    })
}

/// The session pane.
pub struct SessionView {
    snapshot: SessionSnapshot,
    focus_handle: FocusHandle,
    pending: Option<SessionAction>,
}

impl SessionView {
    /// Build the pane over the controller's last snapshot.
    pub fn new(snapshot: SessionSnapshot, cx: &mut Context<Self>) -> Self {
        Self {
            snapshot,
            focus_handle: cx.focus_handle(),
            pending: None,
        }
    }

    /// The state this pane is drawing.
    pub fn snapshot(&self) -> &SessionSnapshot {
        &self.snapshot
    }

    /// Replace the state, for the next frame.
    pub fn set_snapshot(&mut self, snapshot: SessionSnapshot) {
        self.snapshot = snapshot;
    }

    /// The timeline rows, in sequence order.
    pub fn rows(&self) -> Vec<TimelineRow> {
        self.snapshot
            .selected
            .as_ref()
            .map(|selected| {
                selected
                    .timeline
                    .entries()
                    .iter()
                    .map(TimelineRow::for_entry)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The header line.
    pub fn status_line(&self) -> Option<String> {
        status_line(&self.snapshot)
    }

    /// The actions the open session offers.
    pub fn actions(&self) -> &[SessionAction] {
        &self.snapshot.actions
    }

    /// Take the action a click asked for, so the owner can drive the
    /// controller outside the render pass.
    pub fn take_action(&mut self) -> Option<SessionAction> {
        self.pending.take()
    }

    fn session_list(&self, muted: gpui::Hsla) -> impl IntoElement {
        let mut list = div().flex().flex_col().gap_1();
        for session in &self.snapshot.sessions {
            let title = if session.title.trim().is_empty() {
                session.project.clone()
            } else {
                session.title.clone()
            };
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .gap_2()
                    .text_sm()
                    .child(div().child(title))
                    .child(
                        div()
                            .text_color(muted)
                            .child(session.status.label().to_owned()),
                    ),
            );
        }
        list
    }

    fn timeline(&self, muted: gpui::Hsla) -> impl IntoElement {
        let mut timeline = div().flex().flex_col().gap_2();
        for row in self.rows() {
            timeline = timeline.child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        div()
                            .w(px(56.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(row.label.to_owned()),
                    )
                    .child(div().text_sm().child(row.text)),
            );
        }
        timeline
    }
}

impl Focusable for SessionView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SessionView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let foreground = cx.theme().foreground;

        let status = self
            .status_line()
            .map(SharedString::from)
            .unwrap_or_else(|| SharedString::from("No session open"));
        let hint = SharedString::from(self.snapshot.input_mode.hint());
        let mode = SharedString::from(self.snapshot.input_mode.label());
        let notice = self.snapshot.notice.clone();

        let mut column = div()
            .flex()
            .flex_col()
            .size_full()
            .p_4()
            .gap_3()
            .text_color(foreground)
            .child(
                div()
                    .text_lg()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(status),
            )
            .child(div().h(px(1.0)).bg(border))
            .child(self.session_list(muted))
            .child(div().h(px(1.0)).bg(border))
            .child(self.timeline(muted))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().text_sm().child(mode))
                    .child(div().text_xs().text_color(muted).child(hint)),
            );

        if let Some(notice) = notice {
            column = column.child(div().text_sm().text_color(muted).child(notice));
        }

        let mut controls = div().flex().flex_row().gap_2();
        for action in self.snapshot.actions.clone() {
            controls = controls.child(
                Button::new(action.label())
                    .secondary()
                    .label(action.label())
                    .on_click(cx.listener(move |view, _, _, cx| {
                        view.pending = Some(action);
                        cx.notify();
                    })),
            );
        }
        column.child(controls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::protocol::HostEvent;
    use crate::sessions::timeline::{Origin, Timeline};
    use crate::sessions::{EntryKind, SessionDetail, SessionStatus, SessionSummary};

    fn event(sequence: u64, kind: &str, author: Option<&str>, text: &str) -> HostEvent {
        HostEvent {
            sequence,
            kind: kind.to_owned(),
            author: author.map(str::to_owned),
            text: Some(text.to_owned()),
            ..HostEvent::default()
        }
    }

    fn snapshot(status: SessionStatus, events: Vec<HostEvent>) -> SessionSnapshot {
        let mut timeline = Timeline::default();
        timeline.mark_live();
        for event in &events {
            timeline.merge_event(event, Origin::Live);
        }
        SessionSnapshot {
            sessions: vec![SessionSummary {
                id: "ses_1".to_owned(),
                title: "Add the sessions pane".to_owned(),
                status: status.clone(),
                ..SessionSummary::default()
            }],
            selected: Some(SessionDetail {
                summary: SessionSummary {
                    id: "ses_1".to_owned(),
                    title: "Add the sessions pane".to_owned(),
                    status,
                    ..SessionSummary::default()
                },
                timeline,
            }),
            notice: None,
            input_mode: crate::sessions::InputMode::Prompt,
            actions: Vec::new(),
        }
    }

    #[test]
    fn the_four_timeline_kinds_render_distinct_words() {
        let entries = [
            TimelineEntry {
                sequence: 1,
                kind: EntryKind::User,
                text: "Add the sessions pane".to_owned(),
                origin: Origin::Live,
            },
            TimelineEntry {
                sequence: 2,
                kind: EntryKind::Agent,
                text: "I added the pane.".to_owned(),
                origin: Origin::Live,
            },
            TimelineEntry {
                sequence: 3,
                kind: EntryKind::Tool,
                text: "shell — cargo test".to_owned(),
                origin: Origin::Live,
            },
            TimelineEntry {
                sequence: 4,
                kind: EntryKind::Status,
                text: "Session started".to_owned(),
                origin: Origin::Live,
            },
        ];
        let rows: Vec<TimelineRow> = entries.iter().map(TimelineRow::for_entry).collect();
        assert_eq!(
            rows.iter().map(|row| row.label).collect::<Vec<_>>(),
            vec!["You", "Agent", "Tool", "Session"]
        );
        for row in &rows {
            assert!(row.line().starts_with(row.label));
            assert!(row.line().len() > row.label.len());
        }
        // The label is the distinction, so it must be unique per kind.
        let labels: std::collections::BTreeSet<&str> = rows.iter().map(|row| row.label).collect();
        assert_eq!(labels.len(), 4);
    }

    #[test]
    fn a_timeline_row_keeps_the_host_sequence() {
        let entry = TimelineEntry {
            sequence: 42,
            kind: EntryKind::Agent,
            text: "done".to_owned(),
            origin: Origin::Live,
        };
        let row = TimelineRow::for_entry(&entry);
        assert_eq!(row.sequence, 42);
        assert_eq!(row.line(), "Agent: done");
    }

    #[test]
    fn an_interrupted_session_is_labelled_interrupted_on_the_header() {
        let snapshot = snapshot(SessionStatus::Interrupted, Vec::new());
        let line = status_line(&snapshot).expect("header");
        assert!(line.contains("interrupted"));
        assert!(line.contains("Add the sessions pane"));
        assert!(!line.contains("complete"));
        assert!(!line.contains("done"));
    }

    #[test]
    fn a_succeeded_session_says_succeeded() {
        let snapshot = snapshot(SessionStatus::Succeeded, Vec::new());
        let line = status_line(&snapshot).expect("header");
        assert!(line.contains("succeeded"));
        assert!(!line.contains("interrupted"));
    }

    #[test]
    fn no_session_has_no_header() {
        assert_eq!(status_line(&SessionSnapshot::default()), None);
    }

    #[gpui::test]
    fn a_snapshot_becomes_one_row_per_entry_kind(cx: &mut gpui::TestAppContext) {
        let snapshot = snapshot(
            SessionStatus::Interrupted,
            vec![
                event(1, "message", Some("user"), "Add the sessions pane"),
                event(2, "output", Some("session"), "I added the pane."),
                event(3, "tool", None, "cargo test"),
                event(4, "stopped", None, "interrupted"),
            ],
        );
        let entity = cx.update(|cx| cx.new(|cx| SessionView::new(snapshot, cx)));
        entity.read_with(cx, |view, _| {
            let rows = view.rows();
            assert_eq!(rows.len(), 4);
            assert_eq!(
                rows.iter().map(|row| row.label).collect::<Vec<_>>(),
                vec!["You", "Agent", "Tool", "Session"]
            );
            assert!(
                view.status_line()
                    .is_some_and(|line| line.contains("interrupted"))
            );
        });
    }
}
