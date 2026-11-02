//! What the console draws: the startup banner, the about screen, the title bar,
//! one tab's body, the single notice, the status line and the command palette.
//!
//! The renderer takes `&App` and never changes it, so the tests can draw any
//! state into a `TestBackend` and assert the words an operator would see. The
//! status line is drawn last and unconditionally, which is why the connection
//! state cannot go missing from a frame.
//!
//! Every colour comes from [`banner`]: the frame installs the app's theme for
//! the duration of one draw, and the components ask for a role rather than a
//! terminal colour. A no-colour terminal therefore drops every style without a
//! second rendering path.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Tabs, Wrap};

use crate::app::{App, Connection, Tab};

pub mod banner;
pub mod hosts;
pub mod rooms;
pub mod runs;

use banner::StyleRole;

/// The smallest terminal that can show the startup splash instead of the tabs.
const SPLASH_MIN_WIDTH: u16 = 24;
/// The smallest height that can show the splash.
const SPLASH_MIN_HEIGHT: u16 = 8;

/// Draw one whole frame.
pub fn render(app: &App, frame: &mut Frame) {
    banner::with_theme(*app.theme(), || render_themed(app, frame));
}

fn render_themed(app: &App, frame: &mut Frame) {
    let area = frame.area();
    frame.render_widget(
        Block::new().style(banner::style(StyleRole::Background)),
        area,
    );

    if app.splash() && area.width >= SPLASH_MIN_WIDTH && area.height >= SPLASH_MIN_HEIGHT {
        banner::render_splash(app.theme(), frame, area);
        return;
    }
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
    if app.about() {
        banner::render_about(app.theme(), app.pool(), app.member(), frame, area);
    }
}

fn render_tabs(app: &App, frame: &mut Frame, area: Rect) {
    let titles = Tab::ALL.iter().map(|tab| Line::from(tab.title()));
    let tabs = Tabs::new(titles)
        .select(app.tab().index())
        .divider(" | ")
        .style(banner::style(StyleRole::Text))
        .highlight_style(
            banner::style(StyleRole::Focus).add_modifier(Modifier::REVERSED | Modifier::BOLD),
        );
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
    if app.tab() == Tab::Rooms {
        rooms::render(app.rooms(), frame, area);
        return;
    }
    let block = Block::bordered()
        .title(format!(" {} ", app.tab().title()))
        .border_style(banner::style(StyleRole::BorderStrong));
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
            lines.push(Line::from(format!(
                "Console: {}  ·  contract: {}  ·  `a` for the about screen.",
                banner::CONSOLE_VERSION,
                banner::CONTRACT_VERSION
            )));
            lines
        }
    }
}

fn render_notice(notice: &str, frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled("notice ", banner::style(StyleRole::Warning)),
        Span::raw(notice.to_owned()),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    let style = match app.connection() {
        Connection::Online => banner::style(StyleRole::Success),
        Connection::Connecting => banner::style(StyleRole::Warning),
        Connection::Unreachable { .. } => banner::style(StyleRole::Danger),
        Connection::NoPool | Connection::SignedOut => banner::style(StyleRole::Muted),
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
            Style::new()
                .bg(banner::colour(StyleRole::Surface))
                .add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new().fg(banner::colour(StyleRole::Text))
        };
        lines.push(Line::from(Span::styled(
            format!(" {} ", action.label()),
            style,
        )));
    }
    let block = Block::bordered()
        .title(" Commands ")
        .border_style(banner::style(StyleRole::Focus));
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(banner::style(StyleRole::Background)),
        rect,
    );
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
    use crate::ui::banner::{PaletteName, Theme};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use tidepool_cli::credentials::CredentialFile;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn plain(buffer: &ratatui::buffer::Buffer) -> bool {
        (0..buffer.area.height).all(|y| {
            (0..buffer.area.width).all(|x| {
                let cell = &buffer[(x, y)];
                cell.fg == ratatui::style::Color::Reset && cell.bg == ratatui::style::Color::Reset
            })
        })
    }

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
        let buffer = draw_buffer(app, width, height);
        let mut screen = String::new();
        for y in 0..height {
            for x in 0..width {
                screen.push_str(buffer[(x, y)].symbol());
            }
            screen.push('\n');
        }
        screen
    }

    fn draw_buffer(app: &App, width: u16, height: u16) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| render(app, frame))
            .expect("draw a frame");
        terminal.backend().buffer().clone()
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

    #[test]
    fn the_startup_banner_is_the_first_frame_and_the_first_key_still_works() {
        for (width, marker) in [(80_u16, "▄▄█▀▀▀▀█▄▄"), (60, "▄▄█▀▀▀▀█▄▄"), (30, "tidepool")]
        {
            let mut app = online_app();
            app.set_theme(Theme::new(PaletteName::Biolume, false));
            app.show_startup();
            let screen = draw(&app, width, 30);
            assert!(screen.contains(marker), "width {width}:\n{screen}");
            assert!(screen.contains("Press any key to begin."), "{screen}");
            assert!(screen.contains(banner::CONSOLE_VERSION), "{screen}");
            assert!(screen.contains(banner::CONTRACT_VERSION), "{screen}");

            // The first key is not swallowed: it dismisses the splash and
            // still selects the Runs tab.
            app.on_key(key(KeyCode::Char('2')));
            assert!(!app.splash());
            assert_eq!(app.tab(), Tab::Runs);
            let screen = draw(&app, width, 30);
            assert!(!screen.contains("Press any key to begin."), "{screen}");
            assert!(screen.contains("ONLINE"), "{screen}");
            assert!(screen.contains("Runs"), "{screen}");
        }
    }

    #[test]
    fn the_about_screen_opens_from_the_keyboard_and_key_palette_entry() {
        let mut app = online_app();
        assert!(!app.about());
        app.on_key(key(KeyCode::Char('a')));
        assert!(app.about());
        assert!(
            Action::ALL.contains(&Action::About),
            "the about screen is a palette command"
        );
        app.on_key(key(KeyCode::Esc));
        assert!(!app.about());
        app.dispatch(Action::About);
        assert!(app.about());
    }

    #[test]
    fn the_about_screen_reports_both_versions_at_three_widths() {
        for (width, height) in [(100_u16, 30_u16), (60, 26), (30, 26)] {
            let mut app = online_app();
            app.set_theme(Theme::new(PaletteName::SeaGlass, false));
            app.show_about();
            let screen = draw(&app, width, height);
            assert!(screen.contains("About Tidepool"), "{screen}");
            assert!(screen.contains(banner::CONSOLE_VERSION), "{screen}");
            assert!(screen.contains(banner::CONTRACT_VERSION), "{screen}");
            assert!(screen.contains("Sea Glass"), "{screen}");
            assert!(screen.contains("no colour"), "{screen}");
        }
    }

    #[test]
    fn a_no_colour_terminal_draws_every_tab_without_style() {
        let mut app = online_app();
        app.set_theme(Theme::new(PaletteName::Biolume, false));
        for tab in Tab::ALL {
            app.dispatch(Action::ShowTab(tab));
            for (width, height) in [(40_u16, 10_u16), (140, 40)] {
                let buffer = draw_buffer(&app, width, height);
                assert!(
                    plain(&buffer),
                    "{tab:?} painted a style at {width}x{height}"
                );
                let screen = draw(&app, width, height);
                assert!(screen.contains(body_marker(tab)), "{screen}");
                assert!(screen.contains("ONLINE"), "{screen}");
            }
        }
    }

    #[test]
    fn a_colour_terminal_takes_its_colours_from_the_theme() {
        let mut app = online_app();
        let theme = Theme::new(PaletteName::DeepTide, true);
        app.set_theme(theme);
        let buffer = draw_buffer(&app, 100, 24);
        assert_eq!(
            buffer[(0, 23)].fg,
            theme.colour_of(StyleRole::Success),
            "the ONLINE status uses the palette's success colour"
        );
        let screen = draw(&app, 100, 24);
        assert!(screen.contains("ONLINE"), "{screen}");
    }
}
