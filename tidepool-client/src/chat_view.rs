//! The chat pane: a room's transcript, authors, presence and catch-up state.
//!
//! The view is deliberately dumb. [`crate::chat::ChatController`] owns the
//! transport and the merge rules; this module takes a [`ChatSnapshot`] and
//! turns it into rows. That keeps author rendering testable without a window.
//!
//! # The author is a word and a shape, not a tint
//!
//! [`ChatRow::for_message`] carries the author's word (`Person`,
//! `Agent session` or `Unknown author`), its glyph and its tag. A session's
//! row reads differently with the palette removed, so a colour-blind reader or
//! a monochrome screenshot still tells the person from the agent.
//!
//! # The banner says catch-up until it is confirmed
//!
//! [`ChatView::banner`] is the controller's [`Currentness`] sentence. A
//! reconnect says `Reconnecting` with the cached history behind it, and the
//! room is never drawn as current before a read or `chat.current` says so.

use gpui::{Context, FocusHandle, Focusable, Render, SharedString, Window, div, prelude::*, px};

use gpui_kit::component::ActiveTheme;

use crate::chat::{AuthorKind, ChatMessage, ChatSnapshot, Currentness};

/// One row of a room's transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRow {
    /// The pool's sequence, so rows can be keyed without a colour.
    pub sequence: u64,
    /// The pool's immutable message id.
    pub id: String,
    /// The author's word: `Person`, `Agent session` or `Unknown author`.
    pub author_label: &'static str,
    /// The author's shape: `●`, `◆` or `?`.
    pub author_glyph: &'static str,
    /// The author's lowercase tag.
    pub author_tag: &'static str,
    /// The author's id, when the author was not deleted.
    pub author_id: Option<String>,
    /// The message body.
    pub body: String,
    /// The message this replies to, when there is one.
    pub reply_to: Option<String>,
}

impl ChatRow {
    /// Build the row for one message.
    pub fn for_message(message: &ChatMessage) -> Self {
        let author = message.author_descriptor();
        Self {
            sequence: message.sequence,
            id: message.id.clone(),
            author_label: author.label(),
            author_glyph: author.glyph(),
            author_tag: author.tag(),
            author_id: author.id,
            body: message.body.clone(),
            reply_to: message.reply_to.clone(),
        }
    }

    /// Whether this row was written by an agent session.
    pub fn is_session(&self) -> bool {
        self.author_tag == AuthorKind::Session.tag()
    }

    /// The whole row as one line of text.
    ///
    /// The glyph and the word both come before the body, so stripping colour
    /// changes nothing a reader needs.
    pub fn line(&self) -> String {
        let who = match self.author_id.as_deref() {
            Some(id) if !id.is_empty() => {
                format!("{} {} {}", self.author_glyph, self.author_label, id)
            }
            _ => format!("{} {}", self.author_glyph, self.author_label),
        };
        match self.reply_to.as_deref() {
            Some(reply_to) => format!("{who} (reply to {reply_to}): {}", self.body),
            None => format!("{who}: {}", self.body),
        }
    }
}

/// The banner the room draws above its transcript.
pub fn banner(currentness: Currentness) -> &'static str {
    currentness.banner()
}

/// The presence line: members, sessions and watchers.
///
/// Presence is not delivery, so this line never claims a message was lost when
/// the watcher list shrinks.
pub fn presence_line(snapshot: &ChatSnapshot) -> String {
    let sessions = snapshot
        .members
        .iter()
        .filter(|member| member.is_session())
        .count();
    let members = snapshot.members.len();
    let watching = snapshot.watchers.len();
    let mut line = format!(
        "{members} member{}, {sessions} agent session{}",
        if members == 1 { "" } else { "s" },
        if sessions == 1 { "" } else { "s" }
    );
    if watching > 0 {
        line.push_str(&format!(" \u{00b7} {watching} watching now"));
    }
    line
}

/// The sentence above the composer.
pub fn composer_hint(snapshot: &ChatSnapshot) -> &'static str {
    if snapshot.session_members.is_empty() {
        "Message the room. No session is a member here yet."
    } else {
        "Message the room; a session in it answers as an agent session, not as a person."
    }
}

/// The chat pane.
pub struct ChatView {
    snapshot: ChatSnapshot,
    focus_handle: FocusHandle,
}

impl ChatView {
    /// Build the pane over the controller's last snapshot.
    pub fn new(snapshot: ChatSnapshot, cx: &mut Context<Self>) -> Self {
        Self {
            snapshot,
            focus_handle: cx.focus_handle(),
        }
    }

    /// The state this pane is drawing.
    pub fn snapshot(&self) -> &ChatSnapshot {
        &self.snapshot
    }

    /// Replace the state for the next frame.
    pub fn set_snapshot(&mut self, snapshot: ChatSnapshot) {
        self.snapshot = snapshot;
    }

    /// The transcript rows, in sequence order.
    pub fn rows(&self) -> Vec<ChatRow> {
        self.snapshot
            .messages
            .iter()
            .map(ChatRow::for_message)
            .collect()
    }

    /// The catch-up or current banner, when a room is open.
    pub fn banner(&self) -> Option<&'static str> {
        self.snapshot
            .selected_room
            .as_ref()
            .map(|_| banner(self.snapshot.currentness))
    }

    /// The presence line.
    pub fn presence(&self) -> String {
        presence_line(&self.snapshot)
    }

    /// The composer's hint.
    pub fn composer_hint(&self) -> &'static str {
        composer_hint(&self.snapshot)
    }

    fn rooms(&self) -> Vec<SharedString> {
        self.snapshot
            .rooms
            .iter()
            .map(|room| SharedString::from(room.name.clone()))
            .collect()
    }
}

impl Focusable for ChatView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ChatView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let foreground = cx.theme().foreground;

        let mut column = div()
            .flex()
            .flex_col()
            .size_full()
            .p_4()
            .gap_2()
            .text_color(foreground);

        for room in self.rooms() {
            column = column.child(div().text_sm().child(room));
        }

        if let Some(banner) = self.banner() {
            column = column.child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(banner)),
            );
        }

        column = column.child(div().text_xs().text_color(muted).child(self.presence()));
        column = column.child(div().h(px(1.0)).bg(border));

        for row in self.rows() {
            column = column.child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(
                        div()
                            .w(px(150.0))
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(muted)
                            .child(SharedString::from(format!(
                                "{} {}",
                                row.author_glyph, row.author_label
                            ))),
                    )
                    .child(div().text_sm().child(row.body)),
            );
        }

        column = column.child(
            div()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(self.composer_hint())),
        );

        if let Some(notice) = self.snapshot.notice.clone() {
            column = column.child(div().text_xs().text_color(muted).child(notice));
        }

        column
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{ChatAuthor, ChatMember, ChatRoom};

    fn message(id: &str, kind: &str, author: &str, body: &str) -> ChatMessage {
        ChatMessage {
            id: id.to_owned(),
            author: Some(ChatAuthor {
                kind: kind.to_owned(),
                id: author.to_owned(),
            }),
            body: body.to_owned(),
            sequence: 1,
            ..ChatMessage::default()
        }
    }

    fn snapshot(currentness: Currentness, messages: Vec<ChatMessage>) -> ChatSnapshot {
        ChatSnapshot {
            rooms: vec![ChatRoom {
                id: "room-1".to_owned(),
                name: "agents".to_owned(),
                ..ChatRoom::default()
            }],
            selected_room: Some(ChatRoom {
                id: "room-1".to_owned(),
                name: "agents".to_owned(),
                ..ChatRoom::default()
            }),
            messages,
            members: vec![
                ChatMember {
                    principal: ChatAuthor {
                        kind: "user".to_owned(),
                        id: "user-1".to_owned(),
                    },
                    ..ChatMember::default()
                },
                ChatMember {
                    principal: ChatAuthor {
                        kind: "session".to_owned(),
                        id: "sess-1".to_owned(),
                    },
                    ..ChatMember::default()
                },
            ],
            session_members: vec!["sess-1".to_owned()],
            currentness,
            can_post: true,
            ..ChatSnapshot::default()
        }
    }

    #[test]
    fn a_person_and_a_session_render_with_different_words_and_glyphs() {
        let person = ChatRow::for_message(&message("m-1", "user", "user-1", "hello"));
        let session = ChatRow::for_message(&message("m-2", "session", "sess-1", "on it"));

        assert_eq!(person.author_label, "Person");
        assert_eq!(session.author_label, "Agent session");
        assert_ne!(person.author_glyph, session.author_glyph);
        assert_ne!(person.author_tag, session.author_tag);
        assert!(!person.is_session());
        assert!(session.is_session());
        assert!(person.line().contains("Person user-1"));
        assert!(session.line().contains("Agent session sess-1"));
    }

    #[test]
    fn a_deleted_author_is_unknown_not_a_person() {
        let row = ChatRow::for_message(&ChatMessage {
            id: "m-1".to_owned(),
            author: None,
            body: "old".to_owned(),
            sequence: 1,
            ..ChatMessage::default()
        });
        assert_eq!(row.author_label, "Unknown author");
        assert_eq!(row.author_tag, "unknown");
        assert!(!row.is_session());
    }

    #[test]
    fn a_reconnecting_room_is_labelled_catch_up() {
        let view_banner = banner(Currentness::Reconnecting);
        assert!(view_banner.contains("Reconnecting"));
        assert!(!view_banner.contains("Current"));

        let snapshot = snapshot(Currentness::Reconnecting, Vec::new());
        assert!(!snapshot.currentness.is_current());
        assert!(banner(Currentness::CatchingUp).contains("Catching up"));
        assert!(banner(Currentness::Current).contains("Current"));
    }

    #[test]
    fn the_presence_line_counts_sessions_separately() {
        let line = presence_line(&snapshot(Currentness::Current, Vec::new()));
        assert!(line.contains("2 members"));
        assert!(line.contains("1 agent session"));
        assert!(!line.contains("2 agent"));
    }

    #[test]
    fn the_composer_names_the_session_authorship_rule() {
        let snapshot = snapshot(Currentness::Current, Vec::new());
        assert!(composer_hint(&snapshot).contains("agent session"));
        assert!(composer_hint(&snapshot).contains("not as a person"));
    }

    #[gpui::test]
    fn a_snapshot_becomes_one_row_per_message(cx: &mut gpui::TestAppContext) {
        let snapshot = snapshot(
            Currentness::Current,
            vec![
                message("m-1", "user", "user-1", "Can you take it?"),
                message("m-2", "session", "sess-1", "I will take it"),
            ],
        );
        let entity = cx.update(|cx| cx.new(|cx| ChatView::new(snapshot, cx)));
        entity.read_with(cx, |view, _| {
            let rows = view.rows();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].author_label, "Person");
            assert_eq!(rows[1].author_label, "Agent session");
            assert!(view.banner().is_some_and(|line| line.contains("Current")));
        });
    }
}
