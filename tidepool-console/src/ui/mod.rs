//! What the console draws: the title bar, one tab's body, the single notice,
//! the status line and the command palette.
//!
//! The renderer takes `&App` and never changes it, so the tests can draw any
//! state into a `TestBackend` and assert the words an operator would see. The
//! status line is drawn last and unconditionally, which is why the connection
//! state cannot go missing from a frame.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Tabs, Wrap};

use crate::app::{App, Connection, Tab};

pub mod hosts;
pub mod runs;

/// Draw one whole frame.
pub fn render(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let notice_height = u16::from(app.notice().is_some());
    let [tabs_area, body_area, notice_area, status_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(notice_height),
        Constraint::Length(1),
    ])
    .areas(area);

    render_tabs(app, frame, tabs_area);
    render_body(app, frame, body_area);
    if let Some(notice) = app.notice() {
        render_notice(notice, frame, notice_area);
    }
    render_status(app, frame, status_area);
    if let Some(palette) = app.palette() {
        render_palette(palette, frame, area);
    }
}

fn render_tabs(app: &App, frame: &mut Frame, area: Rect) {
    let titles = Tab::ALL.iter().map(|tab| Line::from(tab.title()));
    let tabs = Tabs::new(titles)
        .select(app.tab().index())
        .divider(" | ")
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD));
    frame.render_widget(tabs, area);
}

fn render_body(app: &App, frame: &mut Frame, area: Rect) {
    if app.tab() == Tab::Hosts {
        hosts::render(app.hosts(), app.pool(), frame, area);
        return;
    }
    if app.tab() == Tab::Runs {
        runs::render(app.runs(), frame, area);
        return;
    }
    let block = Block::bordered().title(format!(" {} ", app.tab().title()));
    let paragraph = Paragraph::new(body_lines(app))
        .block(block)
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

/// The placeholder body for the tab in front.
///
/// The hosts tab draws itself; the remaining scaffolds say which task brings
/// their screen, and Settings reports the connection in full, including the
/// reason a failed probe gave.
fn body_lines(app: &App) -> Vec<Line<'static>> {
    match app.tab() {
        Tab::Hosts => Vec::new(),
        Tab::Runs => Vec::new(),
        Tab::Rooms => vec![
            Line::from("No rooms loaded yet."),
            Line::from("Chatrooms and the relay arrive with task 2.2.4."),
        ],
        Tab::Settings => {
            let mut lines = vec![
                Line::from(format!("Pool: {}", app.pool().unwrap_or("none configured"))),
                Line::from(format!(
                    "Member: {}",
                    app.member().unwrap_or("not signed in")
                )),
                Line::from(format!("Connection: {}", app.connection().label())),
            ];
            if let Some(reason) = app.connection().reason() {
                lines.push(Line::from(format!("Reason: {reason}")));
            }
            lines.push(Line::from(
                "Credentials: the CLI's store; `tidepool login` writes it.",
            ));
            lines
        }
    }
}

fn render_notice(notice: &str, frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled("notice ", Style::new().fg(Color::Yellow)),
        Span::raw(notice.to_owned()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    let style = match app.connection() {
        Connection::Online => Style::new().fg(Color::Green),
        Connection::Connecting => Style::new().fg(Color::Yellow),
        Connection::Unreachable { .. } => Style::new().fg(Color::Red),
        Connection::NoPool | Connection::SignedOut => Style::new().fg(Color::Gray),
    };
    frame.render_widget(Paragraph::new(app.status_line()).style(style), area);
}

fn render_palette(palette: &crate::app::Palette, frame: &mut Frame, area: Rect) {
    let entries = palette.entries();
    let width = (area.width.saturating_mul(3) / 4).max(24).min(area.width);
    let height = (entries.len() as u16 + 4).min(area.height).max(3);
    let rect = centered(area, width, height);

    frame.render_widget(Clear, rect);
    let mut lines = vec![
        Line::from(vec![
            Span::raw("> "),
            Span::styled(
                palette.query().to_owned(),
                Style::new().add_modifier(Modifier::UNDERLINED),
            ),
        ]),
        Line::from(""),
    ];
    if entries.is_empty() {
        lines.push(Line::from("No matching command."));
    }
    for (index, action) in entries.iter().enumerate() {
        let style = if index == palette.selected() {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        lines.push(Line::from(Span::styled(
            format!(" {} ", action.label()),
            style,
        )));
    }
    let block = Block::bordered()
        .title(" Commands ")
        .border_style(Style::new().fg(Color::Cyan));
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::testing::ScriptedClient;
    use crate::app::{Action, App};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use tidepool_cli::credentials::CredentialFile;

    fn credentials(
        pool: Option<&str>,
        member: Option<&str>,
        token: Option<&str>,
    ) -> CredentialFile {
        CredentialFile {
            pool: pool.map(str::to_owned),
            email: member.map(str::to_owned),
            token: token.map(str::to_owned),
            ..CredentialFile::default()
        }
    }

    fn online_app() -> App {
        let mut app = App::from_credentials(
            &credentials(Some("pool.test"), Some("ops@x"), Some("token")),
            Some(Box::new(ScriptedClient::online("ops@x"))),
        );
        app.start();
        app
    }

    fn draw(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(app, frame))
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

    fn body_marker(tab: Tab) -> &'static str {
        match tab {
            Tab::Hosts => "No hosts enrolled yet.",
            Tab::Runs => "No runs loaded yet.",
            Tab::Rooms => "No rooms loaded yet.",
            Tab::Settings => "Credentials:",
        }
    }

    #[test]
    fn every_tab_renders_at_a_small_and_a_large_size() {
        for tab in Tab::ALL {
            for (width, height) in [(40_u16, 10_u16), (140, 40)] {
                let mut app = online_app();
                app.dispatch(Action::ShowTab(tab));
                let screen = draw(&app, width, height);
                assert!(
                    screen.contains(body_marker(tab)),
                    "{tab:?} body missing at {width}x{height}:\n{screen}"
                );
                for title in Tab::ALL {
                    assert!(
                        screen.contains(title.title()),
                        "{:?} title missing at {width}x{height}:\n{screen}",
                        title
                    );
                }
                assert!(
                    screen.contains("ONLINE"),
                    "connection state missing at {width}x{height}:\n{screen}"
                );
                assert!(
                    screen.contains("pool pool.test"),
                    "pool missing at {width}x{height}:\n{screen}"
                );
                assert!(
                    screen.contains("member ops@x"),
                    "member missing at {width}x{height}:\n{screen}"
                );
            }
        }
    }

    #[test]
    fn the_status_line_shows_every_connection_state() {
        let cases = [
            (
                App::from_credentials(&credentials(None, None, None), None),
                "NO POOL",
            ),
            (
                App::from_credentials(&credentials(Some("pool.test"), None, None), None),
                "SIGNED OUT",
            ),
            (online_app(), "ONLINE"),
            (
                App::from_credentials(
                    &credentials(Some("pool.test"), Some("ops@x"), Some("token")),
                    Some(Box::new(ScriptedClient::unreachable("connection refused"))),
                ),
                "UNREACHABLE",
            ),
        ];
        for (mut app, label) in cases {
            app.start();
            for (width, height) in [(40_u16, 10_u16), (140, 40)] {
                let screen = draw(&app, width, height);
                assert!(
                    screen.contains(label),
                    "{label} missing at {width}x{height}:\n{screen}"
                );
                assert!(
                    screen.contains("member "),
                    "member field missing at {width}x{height}:\n{screen}"
                );
            }
        }
    }

    #[test]
    fn an_unreachable_pool_is_one_notice_on_screen() {
        let mut app = App::from_credentials(
            &credentials(Some("pool.test"), Some("ops@x"), Some("token")),
            Some(Box::new(ScriptedClient::unreachable("connection refused"))),
        );
        app.start();
        let screen = draw(&app, 120, 20);
        assert_eq!(
            screen.matches("Cannot reach pool.test").count(),
            1,
            "{screen}"
        );
        assert!(screen.contains("UNREACHABLE"), "{screen}");
        assert!(screen.contains("No hosts enrolled yet."), "{screen}");
    }

    #[test]
    fn no_pool_says_what_is_missing_on_screen() {
        let app = App::from_credentials(&credentials(None, None, None), None);
        let screen = draw(&app, 120, 20);
        assert!(screen.contains("No pool configured"), "{screen}");
        assert!(screen.contains("tidepool login"), "{screen}");
        assert!(screen.contains("NO POOL"), "{screen}");
    }

    #[test]
    fn the_palette_renders_every_command() {
        let mut app = online_app();
        app.open_palette();
        let screen = draw(&app, 100, 30);
        assert!(screen.contains("Commands"), "{screen}");
        for action in Action::ALL {
            assert!(
                screen.contains(action.label()),
                "{} missing:\n{screen}",
                action.label()
            );
        }
    }

    #[test]
    fn the_palette_renders_a_filtered_query() {
        let mut app = online_app();
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('p'),
            crossterm::event::KeyModifiers::CONTROL,
        ));
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('r'),
            crossterm::event::KeyModifiers::NONE,
        ));
        let screen = draw(&app, 100, 30);
        assert!(screen.contains("> r"), "{screen}");
        assert!(screen.contains("Reconnect to the pool"), "{screen}");
        assert!(!screen.contains("Quit the console"), "{screen}");
    }
}
