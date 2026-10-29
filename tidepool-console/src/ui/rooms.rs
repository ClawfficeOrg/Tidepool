//! The rooms tab's drawing: the member's rooms, the open transcript, the
//! composer, the presence pane and the relay follower.
//!
//! The renderer takes the tab's state and never changes it; the tests draw into
//! a `TestBackend` and assert the words an operator sees. The relay pane labels
//! every event with its origin and colours the backfilled rows grey and the live
//! rows green, with a divider at the joining point, so the backfilled and live
//! portions are distinct on screen rather than implied by the header.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};
use tidepool_client::types::{MessageView, RoomMemberView, RoomView};

use crate::tabs::rooms::{Origin, RoomsState, author_label};

/// Draw the rooms tab into `area`.
pub fn render(state: &RoomsState, frame: &mut Frame, area: Rect) {
    let footer = footer_lines(state);
    let footer_height = footer.len().max(1) as u16;
    let [header_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(footer_height),
    ])
    .areas(area);

    frame.render_widget(Paragraph::new(header_line(state)), header_area);

    let main_area = if state.relay().is_some() {
        let [main, relay] =
            Layout::vertical([Constraint::Percentage(58), Constraint::Percentage(42)])
                .areas(body_area);
        render_relay(state, frame, relay);
        main
    } else {
        body_area
    };

    if state.open_room_id().is_some() {
        render_room_view(state, frame, main_area);
    } else {
        render_room_list(state, frame, main_area);
    }

    frame.render_widget(
        Paragraph::new(footer).wrap(Wrap { trim: false }),
        footer_area,
    );
}

fn header_line(state: &RoomsState) -> String {
    if let Some(room_id) = state.open_room_id() {
        let name = state
            .rooms()
            .iter()
            .find(|room| room.id == room_id)
            .map(|room| room.name.as_str())
            .unwrap_or(room_id);
        let older = if state.has_older() {
            "older available"
        } else {
            "at the start"
        };
        return format!(
            "room {} ({room_id})  {} messages  {} lines  {older}  presence {}",
            name,
            state.messages().len(),
            state.transcript_lines(),
            state.members().len()
        );
    }
    format!(
        "{} room(s)  {}  member {}",
        state.rooms().len(),
        state
            .selected_room()
            .map(|room| room.name.as_str())
            .unwrap_or("none selected"),
        state.member().unwrap_or("not signed in")
    )
}

fn footer_lines(state: &RoomsState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if state.composer().is_some() {
        lines.push(Line::from(
            "Enter newline · Ctrl-S send · Esc cancel the message",
        ));
    } else if state.follow_prompt().is_some() {
        lines.push(Line::from("type a session id · Enter follow · Esc cancel"));
    } else if state.open_room_id().is_some() {
        lines.push(Line::from(
            "↑↓ scroll (older loads at the top) · PgUp/PgDn page · c compose · \
             f follow relay · x stop follow · Esc close room · r reload",
        ));
    } else {
        lines.push(Line::from(
            "↑↓ select room · Enter open · f follow a relay · x stop follow · \
             r read the rooms again",
        ));
    }
    if state.loading_older() {
        lines.push(Line::from(Span::styled(
            "reading the older page through the cursor…".to_owned(),
            Style::new().fg(Color::Cyan),
        )));
    }
    if let Some(problem) = state.older_problem() {
        lines.push(Line::from(Span::styled(
            format!("older page failed: {problem}"),
            Style::new().fg(Color::Red),
        )));
    }
    if let Some(problem) = state.history_problem() {
        lines.push(Line::from(Span::styled(
            format!("history: {problem}"),
            Style::new().fg(Color::Red),
        )));
    }
    if let Some(problem) = state.members_problem() {
        lines.push(Line::from(Span::styled(
            format!("presence: {problem}"),
            Style::new().fg(Color::Yellow),
        )));
    }
    if let Some(problem) = state.rooms_problem() {
        lines.push(Line::from(Span::styled(
            format!("rooms: {problem}"),
            Style::new().fg(Color::Red),
        )));
    }
    if let Some(follow) = state.relay()
        && let Some(problem) = follow.problem()
    {
        lines.push(Line::from(Span::styled(
            format!("relay: {problem}"),
            Style::new().fg(Color::Yellow),
        )));
    }
    if let Some(notice) = state.notice() {
        lines.push(Line::from(Span::styled(
            notice.to_owned(),
            Style::new().fg(Color::Yellow),
        )));
    }
    lines
}

fn render_room_list(state: &RoomsState, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title(format!(" Rooms ({}) ", state.rooms().len()));
    if state.rooms().is_empty() {
        let text = if state.rooms_problem().is_some() {
            "No rooms loaded yet. Press r to read the pool again."
        } else {
            "No rooms loaded yet. The pool listed none for this member."
        };
        frame.render_widget(
            Paragraph::new(text.to_owned())
                .block(block)
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let items: Vec<ListItem<'static>> = state
        .rooms()
        .iter()
        .map(|room| ListItem::new(room_lines(room)))
        .collect();
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▸ ");
    let mut list_state = ListState::default();
    list_state.select(Some(state.selected()));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn room_lines(room: &RoomView) -> Line<'static> {
    Line::from(vec![
        Span::styled(room.name.clone(), Style::new().add_modifier(Modifier::BOLD)),
        Span::raw(format!("  {}  ", room.visibility)),
        Span::styled(room.id.clone(), Style::new().fg(Color::DarkGray)),
    ])
}

fn render_room_view(state: &RoomsState, frame: &mut Frame, area: Rect) {
    if area.width >= 100 {
        let [transcript, members] =
            Layout::horizontal([Constraint::Percentage(72), Constraint::Percentage(28)])
                .areas(area);
        render_transcript(state, frame, transcript);
        render_members(state, frame, members);
    } else {
        let [transcript, members] =
            Layout::vertical([Constraint::Percentage(70), Constraint::Percentage(30)]).areas(area);
        render_transcript(state, frame, transcript);
        render_members(state, frame, members);
    }
}

fn render_transcript(state: &RoomsState, frame: &mut Frame, area: Rect) {
    let composer_height = state
        .composer()
        .map(|composer| composer.body().split('\n').count().max(1) as u16 + 2)
        .unwrap_or(0)
        .min(area.height.saturating_sub(3));
    let [messages_area, composer_area] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(composer_height)]).areas(area);

    let lines = transcript_lines(state);
    let inner_height = messages_area.height.saturating_sub(2) as usize;
    let max_offset = lines.len().saturating_sub(inner_height);
    let offset = max_offset.saturating_sub(state.scroll());
    let offset = u16::try_from(offset).unwrap_or(u16::MAX);
    let block = Block::bordered().title(format!(
        " {} · {} messages ",
        state.open_room_id().unwrap_or("room"),
        state.messages().len()
    ));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((offset, 0)),
        messages_area,
    );

    if let Some(composer) = state.composer() {
        render_composer(composer.body(), frame, composer_area);
    }
}

fn transcript_lines(state: &RoomsState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if state.messages().is_empty() {
        lines.push(Line::from(
            "No messages in this room yet. Press c to write one.",
        ));
        return lines;
    }
    for message in state.messages() {
        lines.push(message_heading(message));
        if message.body.is_empty() {
            lines.push(Line::from("  "));
        } else {
            for body_line in message.body.lines() {
                lines.push(Line::from(format!("  {body_line}")));
            }
        }
        lines.push(Line::from(""));
    }
    lines
}

fn message_heading(message: &MessageView) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("#{} ", message.sequence),
            Style::new().fg(Color::DarkGray),
        ),
        Span::styled(
            author_label(message),
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(message.created_at.clone(), Style::new().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(message.id.clone(), Style::new().fg(Color::Gray)),
    ])
}

fn render_composer(body: &str, frame: &mut Frame, area: Rect) {
    let lines: Vec<&str> = body.split('\n').collect();
    let mut rendered = Vec::new();
    let last = lines.len().saturating_sub(1);
    for (index, line) in lines.iter().enumerate() {
        let mut text = format!("{} {line}", if index == 0 { "▸" } else { " " });
        if index == last {
            text.push('▏');
        }
        rendered.push(Line::from(text));
    }
    frame.render_widget(
        Paragraph::new(rendered)
            .block(Block::bordered().title(" Compose · Ctrl-S sends "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_members(state: &RoomsState, frame: &mut Frame, area: Rect) {
    let block = Block::bordered().title(format!(" Presence ({}) ", state.members().len()));
    let mut lines = Vec::new();
    if state.members().is_empty() {
        lines.push(Line::from("No members reported yet."));
    }
    for member in state.members() {
        lines.push(member_line(member));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "The pool exposes the room's member list; the console reads it every 3 s.",
        Style::new().fg(Color::DarkGray),
    )));
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

fn member_line(member: &RoomMemberView) -> Line<'static> {
    let (kind, style) = match member.principal.kind.as_str() {
        "session" => ("session", Style::new().fg(Color::Magenta)),
        "user" => ("user", Style::new().fg(Color::Green)),
        other => (other, Style::new().fg(Color::Gray)),
    };
    Line::from(vec![
        Span::styled(member.principal.id.clone(), style),
        Span::raw(format!("  {} · {kind}", member.role)),
        Span::styled(
            format!("  joined {}", member.joined_at),
            Style::new().fg(Color::DarkGray),
        ),
    ])
}

fn render_relay(state: &RoomsState, frame: &mut Frame, area: Rect) {
    let Some(follow) = state.relay() else {
        return;
    };
    let block = Block::bordered().title(format!(
        " Relay {} · {} ",
        follow.session(),
        follow.phase().word()
    ));
    let mut lines = Vec::new();
    match follow.publication() {
        Some(publication) => {
            let state_word = if publication.enabled { "on" } else { "off" };
            lines.push(Line::from(format!(
                "publication {state_word}  started {}  stopped {}",
                publication.started_at.as_deref().unwrap_or("-"),
                publication.stopped_at.as_deref().unwrap_or("-")
            )));
        }
        None => lines.push(Line::from("Reading the relay's publication state…")),
    }
    lines.push(Line::from(format!(
        "backfill {}  live {}  cursor {}",
        follow.backfill_count(),
        follow.live_count(),
        follow
            .cursor()
            .map(|cursor| cursor.to_string())
            .unwrap_or_else(|| "start".to_owned())
    )));
    if follow.rows().is_empty() {
        lines.push(Line::from(
            "No events published yet. Backfill is read through the cursor; live events appear after the joining point.",
        ));
    }
    let live_from = follow.live_from();
    for (index, row) in follow.rows().iter().enumerate() {
        if live_from == Some(index) {
            lines.push(Line::from(Span::styled(
                "──── backfill ends · live from here ────",
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )));
        }
        let (word, style) = (
            row.origin.word(),
            match row.origin {
                Origin::Backfill => Style::new().fg(Color::Gray),
                Origin::Live => Style::new().fg(Color::Green),
            },
        );
        lines.push(Line::from(vec![
            Span::styled(format!(" {word} "), style.add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(
                    "#{} {}  {}  {}",
                    row.event.sequence, row.event.kind, row.event.created_at, row.event.id
                ),
                style,
            ),
        ]));
        let payload = serde_json::to_string(&row.event.payload).unwrap_or_default();
        lines.push(Line::from(Span::styled(
            format!("    {payload}"),
            Style::new().fg(Color::DarkGray),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    use super::*;
    use crate::tabs::rooms::testing::{
        ScriptedRooms, member, message, relay_event, relay_state, room,
    };
    use crate::tabs::rooms::{RELAY_POLL_INTERVAL, RoomsState};
    use std::time::Instant;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn draw(state: &RoomsState, width: u16, height: u16) -> String {
        styled_lines(state, width, height)
            .into_iter()
            .map(|(text, _colours)| text)
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn styled_lines(state: &RoomsState, width: u16, height: u16) -> Vec<(String, Vec<Color>)> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(state, frame, frame.area()))
            .expect("draw a frame");
        let buffer = terminal.backend().buffer();
        let mut lines = Vec::new();
        for y in 0..height {
            let mut text = String::new();
            let mut colours = Vec::new();
            for x in 0..width {
                text.push_str(buffer[(x, y)].symbol());
                colours.push(buffer[(x, y)].fg);
            }
            lines.push((text, colours));
        }
        lines
    }

    fn opened() -> (RoomsState, ScriptedRooms) {
        let base = Instant::now();
        let mut client = ScriptedRooms::new();
        client.queue_rooms(vec![room("room_1", "General"), room("room_2", "Standup")]);
        client.set_history(
            "room_1",
            vec![
                message(1, "srv-1", "alice", "hello there"),
                message(2, "srv-2", "bob", "line one\nline two"),
            ],
        );
        client.queue_members(vec![
            member("alice", "owner", "user"),
            member("swaygent", "member", "session"),
        ]);
        let mut state = RoomsState::new();
        state.set_member(Some("ops@example.com".to_owned()));
        state.drive(&mut client, base);
        state.open_room("room_1");
        state.drive(&mut client, base);
        (state, client)
    }

    #[test]
    fn the_room_list_renders_every_room_and_the_empty_state() {
        let (mut state, _client) = opened();
        state.close_room();
        let screen = draw(&state, 120, 30);
        assert!(screen.contains("Rooms (2)"), "{screen}");
        assert!(screen.contains("General"), "{screen}");
        assert!(screen.contains("Standup"), "{screen}");
        assert!(screen.contains("room_1"), "{screen}");
        assert!(screen.contains("f follow a relay"), "{screen}");

        let empty = RoomsState::new();
        let screen = draw(&empty, 120, 20);
        assert!(screen.contains("No rooms loaded yet."), "{screen}");
    }

    #[test]
    fn the_room_view_renders_messages_with_their_server_ids_and_members() {
        let (state, _client) = opened();
        let screen = draw(&state, 140, 40);
        assert!(screen.contains("room General (room_1)"), "{screen}");
        assert!(screen.contains("hello there"), "{screen}");
        assert!(screen.contains("line one"), "{screen}");
        assert!(screen.contains("line two"), "{screen}");
        assert!(screen.contains("alice"), "{screen}");
        assert!(screen.contains("srv-1"), "{screen}");
        assert!(screen.contains("srv-2"), "{screen}");
        assert!(screen.contains("Presence (2)"), "{screen}");
        assert!(screen.contains("owner"), "{screen}");
        assert!(screen.contains("swaygent"), "{screen}");
        assert!(screen.contains("session"), "{screen}");
    }

    #[test]
    fn the_composer_renders_a_multiline_message_and_says_how_to_send() {
        let (mut state, _client) = opened();
        state.on_key(key(KeyCode::Char('c')));
        for character in "draft one".chars() {
            state.on_key(key(KeyCode::Char(character)));
        }
        state.on_key(key(KeyCode::Enter));
        for character in "draft two".chars() {
            state.on_key(key(KeyCode::Char(character)));
        }
        let screen = draw(&state, 140, 40);
        assert!(screen.contains("Compose"), "{screen}");
        assert!(screen.contains("Ctrl-S sends"), "{screen}");
        assert!(screen.contains("draft one"), "{screen}");
        assert!(screen.contains("draft two"), "{screen}");
        assert!(screen.contains("Enter newline"), "{screen}");
    }

    #[test]
    fn the_relay_backfill_and_live_rows_are_distinct_on_screen() {
        let base = Instant::now();
        let mut client = ScriptedRooms::new();
        for sequence in 1..=3 {
            client.push_relay(
                "sess-1",
                relay_event(sequence, "output", serde_json::json!({"step": sequence})),
            );
        }
        client.set_relay_state(relay_state("sess-1", true));
        let mut state = RoomsState::new();
        state.follow_session("sess-1");
        state.drive(&mut client, base);
        client.push_relay(
            "sess-1",
            relay_event(4, "status", serde_json::json!({"state": "done"})),
        );
        state.drive(&mut client, base + RELAY_POLL_INTERVAL);

        let lines = styled_lines(&state, 140, 44);
        let screen = lines
            .iter()
            .map(|(text, _colours)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(screen.contains("Relay sess-1"), "{screen}");
        assert!(screen.contains("live"), "{screen}");
        assert!(screen.contains("backfill"), "{screen}");
        assert!(
            screen.contains("──── backfill ends · live from here ────"),
            "the joining point is marked:\n{screen}"
        );
        assert!(screen.contains("evt-4"), "live ids are kept:\n{screen}");
        assert!(screen.contains("#4 status"), "{screen}");

        let backfill_row = lines
            .iter()
            .find(|(text, _colours)| text.contains("backfill") && text.contains("#1 output"))
            .map(|(_text, colours)| colours)
            .expect("a backfill row");
        let live_row = lines
            .iter()
            .find(|(text, _colours)| text.contains("live") && text.contains("#4 status"))
            .map(|(_text, colours)| colours)
            .expect("a live row");
        assert!(
            backfill_row.contains(&Color::Gray),
            "the backfilled row is grey: {backfill_row:?}"
        );
        assert!(
            !backfill_row.contains(&Color::Green),
            "the backfilled row is not live-coloured"
        );
        assert!(
            live_row.contains(&Color::Green),
            "the live row is green: {live_row:?}"
        );
        assert!(
            !live_row.contains(&Color::Gray),
            "the live row is not backfill-coloured"
        );
    }

    #[test]
    fn the_follow_prompt_renders_the_session_being_typed() {
        let mut state = RoomsState::new();
        state.on_key(key(KeyCode::Char('f')));
        for character in "sess-42".chars() {
            state.on_key(key(KeyCode::Char(character)));
        }
        let screen = draw(&state, 120, 20);
        assert!(screen.contains("session id"), "{screen}");
        assert!(screen.contains("Enter follow"), "{screen}");
    }

    #[test]
    fn scrolling_up_renders_the_older_messages() {
        let base = Instant::now();
        let mut client = ScriptedRooms::new();
        client.queue_rooms(vec![room("room_1", "General")]);
        client.set_history(
            "room_1",
            (1..=40)
                .map(|sequence| {
                    let body = if sequence == 1 {
                        "the very first line"
                    } else if sequence == 40 {
                        "the very last line"
                    } else {
                        "filler"
                    };
                    message(sequence, &format!("srv-{sequence}"), "alice", body)
                })
                .collect(),
        );
        let mut state = RoomsState::new();
        state.drive(&mut client, base);
        state.open_room("room_1");
        state.drive(&mut client, base);

        let screen = draw(&state, 100, 30);
        assert!(screen.contains("the very last line"), "{screen}");
        assert!(!screen.contains("the very first line"), "{screen}");

        for _ in 0..state.transcript_lines() + 2 {
            state.on_key(key(KeyCode::Up));
        }
        let screen = draw(&state, 100, 30);
        assert!(
            screen.contains("the very first line"),
            "scrolling to the top shows the oldest message:\n{screen}"
        );
    }

    #[test]
    fn the_rooms_tab_draws_at_a_small_and_a_large_size() {
        let (state, _client) = opened();
        for (width, height) in [(40_u16, 12_u16), (140, 40)] {
            let screen = draw(&state, width, height);
            assert!(
                screen.contains("General") || screen.contains("room General"),
                "{width}x{height}:\n{screen}"
            );
        }
    }
}
