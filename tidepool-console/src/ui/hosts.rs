//! The hosts tab's drawing: the table, the detail pane, the enrol prompt and
//! the one-time enrollment offer.
//!
//! The renderer takes the tab's state and never changes it; the tests draw into
//! a `TestBackend` and assert the words an operator sees, including that the
//! capability pane repeats the host's report instead of guessing one.

use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table, TableState, Wrap};

use crate::tabs::hosts::{CapabilityState, Column, HostsState, capability_line};

/// Draw the hosts tab into `area`.
pub fn render(state: &HostsState, pool: Option<&str>, frame: &mut Frame, area: Rect) {
    let now = Instant::now();
    let footer = footer_lines(state, pool);
    let footer_height = footer.len() as u16;
    let [header_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(footer_height),
    ])
    .areas(area);

    frame.render_widget(Paragraph::new(header_line(state)), header_area);

    if body_area.width >= 70 {
        let [table_area, detail_area] =
            Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
                .areas(body_area);
        render_table(state, now, frame, table_area);
        render_detail(state, now, frame, detail_area);
    } else {
        let [table_area, detail_area] =
            Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)])
                .areas(body_area);
        render_table(state, now, frame, table_area);
        render_detail(state, now, frame, detail_area);
    }

    frame.render_widget(
        Paragraph::new(footer).wrap(Wrap { trim: false }),
        footer_area,
    );
}

fn header_line(state: &HostsState) -> String {
    let direction = if state.descending() {
        "descending"
    } else {
        "ascending"
    };
    let query = if state.query().trim().is_empty() {
        "-"
    } else {
        state.query()
    };
    format!(
        "sort {} {direction}  filter {}  query {query}  {} host(s)",
        state.sort().word(),
        state.filter().title(),
        state.len()
    )
}

fn footer_lines(state: &HostsState, pool: Option<&str>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(offer) = state.enrollment() {
        lines.push(Line::from(vec![
            Span::styled(
                "ENROLMENT TOKEN",
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(" (single use) expires {}", offer.expires_at)),
        ]));
        lines.push(Line::from(format!("  {}", offer.token)));
        lines.push(Line::from("Run on the host:"));
        lines.push(Line::from(format!("  {}", offer.command(pool))));
    }
    match state.prompt() {
        Some((label, draft)) => lines.push(Line::from(vec![
            Span::styled(label.to_owned(), Style::new().fg(Color::Cyan)),
            Span::raw(": "),
            Span::styled(
                draft.to_owned(),
                Style::new().add_modifier(Modifier::UNDERLINED),
            ),
        ])),
        None => lines.push(Line::from(
            "↑↓ select · s sort · S direction · f filter · / search · e enrol · \
             g grant · x revoke · c report",
        )),
    }
    if let Some(notice) = state.notice() {
        lines.push(Line::from(Span::styled(
            notice.to_owned(),
            Style::new().fg(Color::Yellow),
        )));
    }
    lines
}

fn render_table(state: &HostsState, now: Instant, frame: &mut Frame, area: Rect) {
    let rows = state.visible_rows(now);
    let block = Block::bordered().title(format!(" Hosts ({}/{}) ", rows.len(), state.len()));

    if rows.is_empty() {
        let text = if state.is_empty() {
            "No hosts enrolled yet. Press e to enrol one."
        } else {
            "No host matches this filter."
        };
        frame.render_widget(
            Paragraph::new(text.to_owned())
                .block(block)
                .wrap(Wrap { trim: true }),
            area,
        );
        return;
    }

    let header = Row::new(
        Column::ALL
            .iter()
            .map(|column| Cell::from(format!("{}{}", column.title(), state.sort_arrow(*column))))
            .collect::<Vec<_>>(),
    )
    .style(Style::new().add_modifier(Modifier::BOLD));

    let body: Vec<Row<'static>> = rows
        .iter()
        .map(|record| {
            let host = record.host();
            Row::new(vec![
                Cell::from(host.slug.clone()),
                Cell::from(Span::styled(
                    host.status.clone(),
                    status_style(&host.status),
                )),
                Cell::from(record.last_seen_label(now)),
                Cell::from(if host.execution_enabled { "yes" } else { "no" }),
            ])
        })
        .collect();

    let widths = [
        Constraint::Percentage(32),
        Constraint::Percentage(30),
        Constraint::Percentage(24),
        Constraint::Percentage(14),
    ];
    let table = Table::new(body, widths)
        .header(header)
        .block(block)
        .column_spacing(1)
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("▸ ");

    let mut table_state = TableState::default();
    table_state.select(Some(state.selected()));
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn render_detail(state: &HostsState, now: Instant, frame: &mut Frame, area: Rect) {
    let title = match state.selected_host(now) {
        Some(record) => format!(" Host {} ", record.host().slug),
        None => " Host ".to_owned(),
    };
    let block = Block::bordered().title(title);
    frame.render_widget(
        Paragraph::new(detail_lines(state, now))
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn detail_lines(state: &HostsState, now: Instant) -> Vec<Line<'static>> {
    let Some(record) = state.selected_host(now) else {
        return vec![
            Line::from("Select a host with ↑/↓."),
            Line::from("Its reported capabilities appear here."),
        ];
    };
    let host = record.host();
    let mut lines = vec![
        Line::from(format!("id: {}", host.id)),
        Line::from(format!("hostname: {}", host.hostname)),
        Line::from(format!("display name: {}", host.display_name)),
        Line::from(format!("status: {}", host.status)),
        Line::from(format!(
            "execution: {}",
            if host.execution_enabled {
                "granted (may run agents)"
            } else {
                "not granted (connected only)"
            }
        )),
        Line::from(format!("registered: {}", host.created_at)),
        Line::from(format!("last seen: {}", record.last_seen_label(now))),
        Line::from(""),
        Line::from(Span::styled(
            "capabilities (as reported)",
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )),
    ];
    match state.capabilities(&host.id) {
        None => lines.push(Line::from("reading the report…")),
        Some(CapabilityState::Failed(reason)) => {
            lines.push(Line::from(format!("report failed: {reason}")));
            lines.push(Line::from("press c to retry"));
        }
        Some(CapabilityState::Reported(report)) => {
            lines.push(Line::from(match &report.reported_at {
                Some(reported_at) => format!("reported at: {reported_at}"),
                None => "reported at: not recorded".to_owned(),
            }));
            lines.push(Line::from(if report.offered_providers.is_empty() {
                "offered providers: none reported".to_owned()
            } else {
                format!("offered providers: {}", report.offered_providers.join(", "))
            }));
            if report.capabilities.is_empty() {
                lines.push(Line::from("no capabilities reported"));
            }
            for capability in &report.capabilities {
                lines.push(Line::from(format!("• {}", capability_line(capability))));
            }
        }
    }
    lines
}

fn status_style(status: &str) -> Style {
    match status {
        "connected" => Style::new().fg(Color::Green),
        "pending_approval" => Style::new().fg(Color::Yellow),
        "offline" => Style::new().fg(Color::Gray),
        "revoked" => Style::new().fg(Color::Red),
        _ => Style::new(),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use tidepool_client::types::HostCapabilityReport;

    use super::*;
    use crate::tabs::hosts::testing::{ScriptedHosts, host, report};
    use crate::tabs::hosts::{Enrollment, REFRESH_INTERVAL};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn draw(state: &HostsState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(state, Some("https://pool.test"), frame, frame.area()))
            .expect("draw a frame");
        let buffer = terminal.backend().buffer();
        let mut screen = String::new();
        for y in 0..height {
            for x in 0..width {
                screen.push_str(buffer[(x, y)].symbol());
            }
            screen.push('\n');
        }
        screen
    }

    fn populated() -> (HostsState, ScriptedHosts) {
        let now = Instant::now();
        let mut state = HostsState::new();
        let mut client = ScriptedHosts::new();
        client.queue_hosts(vec![
            host("h-1", "alpha", "connected", false),
            host("h-2", "bravo", "offline", true),
        ]);
        client.reports.push_back(Ok(report("h-1")));
        state.drive(&mut client, now);
        (state, client)
    }

    #[test]
    fn the_table_renders_every_column_and_the_selected_host() {
        let (state, _client) = populated();
        let screen = draw(&state, 120, 30);

        for heading in ["SLUG", "STATUS", "LAST SEEN", "EXEC"] {
            assert!(screen.contains(heading), "{heading} missing:\n{screen}");
        }
        assert!(screen.contains("alpha"), "{screen}");
        assert!(screen.contains("bravo"), "{screen}");
        assert!(screen.contains("connected"), "{screen}");
        assert!(screen.contains("offline"), "{screen}");
        assert!(screen.contains("yes"), "the execution column:\n{screen}");
        assert!(screen.contains("no"), "the execution column:\n{screen}");
        assert!(screen.contains("last seen: just now"), "{screen}");
        assert!(
            screen.contains("sort slug ascending"),
            "the header names the order:\n{screen}"
        );
    }

    #[test]
    fn the_detail_pane_shows_the_capabilities_as_reported() {
        let (state, _client) = populated();
        let screen = draw(&state, 140, 40);
        assert!(screen.contains("capabilities (as reported)"), "{screen}");
        assert!(
            screen.contains("reported at: 2026-10-29T00:00:00Z"),
            "{screen}"
        );
        assert!(screen.contains("offered providers: anthropic"), "{screen}");
        assert!(
            screen.contains("anthropic/claude · default"),
            "the report entry verbatim:\n{screen}"
        );
        assert!(screen.contains("not granted (connected only)"), "{screen}");
    }

    #[test]
    fn an_empty_report_invents_nothing() {
        let now = Instant::now();
        let mut state = HostsState::new();
        let mut client = ScriptedHosts::new();
        client.queue_hosts(vec![host("h-1", "alpha", "connected", true)]);
        client.reports.push_back(Ok(HostCapabilityReport {
            host_id: "h-1".to_owned(),
            reported_at: None,
            offered_providers: Vec::new(),
            capabilities: Vec::new(),
        }));
        state.drive(&mut client, now);

        let screen = draw(&state, 140, 40);
        assert!(screen.contains("reported at: not recorded"), "{screen}");
        assert!(
            screen.contains("offered providers: none reported"),
            "{screen}"
        );
        assert!(screen.contains("no capabilities reported"), "{screen}");
        assert!(
            !screen.contains("default"),
            "no mode was inferred:\n{screen}"
        );
    }

    #[test]
    fn an_empty_table_says_how_to_enrol() {
        let state = HostsState::new();
        let screen = draw(&state, 120, 20);
        assert!(screen.contains("No hosts enrolled yet."), "{screen}");
        assert!(screen.contains("Press e to enrol one."), "{screen}");
        assert!(screen.contains("Select a host"), "{screen}");
    }

    #[test]
    fn the_enrollment_offer_shows_the_token_and_the_command() {
        let now = Instant::now();
        let mut state = HostsState::new();
        let mut client = ScriptedHosts::new();
        client.queue_hosts(Vec::new());
        state.drive(&mut client, now);
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
        state.drive(&mut client, now);

        let screen = draw(&state, 140, 40);
        assert!(screen.contains("ENROLMENT TOKEN"), "{screen}");
        assert!(screen.contains("tok_secret_once"), "{screen}");
        assert!(screen.contains("2026-10-29T12:10:00Z"), "{screen}");
        assert!(screen.contains("HOST_ID=h-9"), "{screen}");
        assert!(
            screen.contains("ENROLLMENT_TOKEN=tok_secret_once"),
            "{screen}"
        );
        assert!(screen.contains("tidepool-host-agent"), "{screen}");
    }

    #[test]
    fn a_host_going_offline_is_visible_without_an_input() {
        let now = Instant::now();
        let mut state = HostsState::new();
        let mut client = ScriptedHosts::new();
        client.queue_hosts(vec![host("h-1", "alpha", "connected", false)]);
        state.drive(&mut client, now);
        let before = draw(&state, 120, 30);
        assert!(before.contains("status: connected"), "{before}");

        // No key event: the console's tick drives the refresh.
        client.queue_hosts(vec![host("h-1", "alpha", "offline", false)]);
        state.drive(&mut client, now + REFRESH_INTERVAL);
        let after = draw(&state, 120, 30);
        assert!(after.contains("status: offline"), "{after}");
        assert!(!after.contains("status: connected"), "{after}");
    }

    #[test]
    fn the_sorted_column_and_direction_are_visible_in_the_heading() {
        let (mut state, _client) = populated();
        state.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('s'),
            crossterm::event::KeyModifiers::NONE,
        ));
        state.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('S'),
            crossterm::event::KeyModifiers::NONE,
        ));
        let screen = draw(&state, 120, 30);
        assert!(screen.contains("STATUS ▼"), "{screen}");
        assert_eq!(
            state.visible_rows(Instant::now())[0].host().slug,
            "bravo",
            "descending status puts offline before connected"
        );
    }

    #[test]
    fn the_filtered_state_and_the_prompt_are_visible() {
        let (mut state, _client) = populated();
        state.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('f'),
            crossterm::event::KeyModifiers::NONE,
        ));
        let screen = draw(&state, 120, 30);
        assert!(screen.contains("filter connected"), "{screen}");
        assert!(!screen.contains("bravo"), "bravo is offline:\n{screen}");

        state.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('/'),
            crossterm::event::KeyModifiers::NONE,
        ));
        state.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('a'),
            crossterm::event::KeyModifiers::NONE,
        ));
        let screen = draw(&state, 120, 30);
        assert!(screen.contains("filter: a"), "{screen}");
    }

    #[test]
    fn the_tab_draws_at_a_small_and_a_large_size() {
        let (state, _client) = populated();
        for (width, height) in [(40_u16, 10_u16), (140, 40)] {
            let screen = draw(&state, width, height);
            assert!(
                screen.contains("Hosts"),
                "block title missing at {width}x{height}:\n{screen}"
            );
        }
    }
}
