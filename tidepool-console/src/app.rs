//! Console state: the tabs, the actions, key handling and the one connection
//! probe the console makes.
//!
//! Everything an operator can do is an [`Action`]. The keyboard and the command
//! palette both dispatch the same values, and [`Action::ALL`] is the palette's
//! only source, so an action that is not in the palette cannot be bound to a
//! key without a test noticing.
//!
//! The network is touched at most once at startup, and only when the credential
//! store names a pool and a token. A pool that does not answer becomes
//! [`Connection::Unreachable`] plus a single [`App::notice`]; the console keeps
//! taking keys, so an operator who started it on a disconnected laptop can still
//! read the tabs and quit cleanly.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tidepool_cli::credentials::CredentialFile;

/// Everything that can go wrong before or during a console run.
#[derive(Debug, thiserror::Error)]
pub enum ConsoleError {
    /// The credential store could not be read.
    #[error("credential store: {0}")]
    Store(#[from] tidepool_cli::CliError),
    /// The SDK could not build or perform a request.
    #[error("pool: {0}")]
    Pool(#[from] tidepool_client::Error),
    /// The terminal could not be prepared, read or restored.
    #[error("terminal: {0}")]
    Terminal(#[from] std::io::Error),
}

/// The result alias used across the console.
pub type Result<T> = std::result::Result<T, ConsoleError>;

/// The four tabs of the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// Hosts and their presence.
    Hosts,
    /// Triggers and the runs they produced.
    Runs,
    /// Chatrooms and their relay.
    Rooms,
    /// The pool, the member and the connection.
    Settings,
}

impl Tab {
    /// Every tab, in title-bar order.
    pub const ALL: [Tab; 4] = [Tab::Hosts, Tab::Runs, Tab::Rooms, Tab::Settings];

    /// The tab's title in the title bar.
    pub fn title(self) -> &'static str {
        match self {
            Tab::Hosts => "Hosts",
            Tab::Runs => "Runs",
            Tab::Rooms => "Rooms",
            Tab::Settings => "Settings",
        }
    }

    /// Position in [`Tab::ALL`], used by the title bar.
    pub fn index(self) -> usize {
        match self {
            Tab::Hosts => 0,
            Tab::Runs => 1,
            Tab::Rooms => 2,
            Tab::Settings => 3,
        }
    }

    /// The tab after this one, wrapping.
    pub fn next(self) -> Self {
        Tab::ALL[(self.index() + 1) % Tab::ALL.len()]
    }

    /// The tab before this one, wrapping.
    pub fn previous(self) -> Self {
        Tab::ALL[(self.index() + Tab::ALL.len() - 1) % Tab::ALL.len()]
    }
}

/// Something an operator can ask the console to do.
///
/// Both the keyboard and the command palette produce these; neither has a
/// private path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Switch to one tab.
    ShowTab(Tab),
    /// Move one tab to the right, wrapping.
    NextTab,
    /// Move one tab to the left, wrapping.
    PreviousTab,
    /// Ask the pool again whether it answers.
    Reconnect,
    /// Leave the console.
    Quit,
}

impl Action {
    /// Every action, in the order the palette lists them.
    pub const ALL: [Action; 8] = [
        Action::ShowTab(Tab::Hosts),
        Action::ShowTab(Tab::Runs),
        Action::ShowTab(Tab::Rooms),
        Action::ShowTab(Tab::Settings),
        Action::NextTab,
        Action::PreviousTab,
        Action::Reconnect,
        Action::Quit,
    ];

    /// The words the palette shows for this action.
    pub fn label(self) -> &'static str {
        match self {
            Action::ShowTab(Tab::Hosts) => "Go to Hosts",
            Action::ShowTab(Tab::Runs) => "Go to Runs",
            Action::ShowTab(Tab::Rooms) => "Go to Rooms",
            Action::ShowTab(Tab::Settings) => "Go to Settings",
            Action::NextTab => "Next tab",
            Action::PreviousTab => "Previous tab",
            Action::Reconnect => "Reconnect to the pool",
            Action::Quit => "Quit the console",
        }
    }

    /// Whether the label contains `query`, case-insensitively.
    pub fn matches(self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty() || self.label().to_lowercase().contains(&query)
    }
}

/// How the console currently stands with its pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    /// The credential store names no pool.
    NoPool,
    /// A pool is configured but the store carries no token.
    SignedOut,
    /// The startup or reconnect probe is in flight.
    Connecting,
    /// The pool answered.
    Online,
    /// The pool is configured but did not answer.
    Unreachable {
        /// What the SDK said, without any credential in it.
        reason: String,
    },
}

impl Connection {
    /// The word the status line always carries.
    pub fn label(&self) -> &'static str {
        match self {
            Connection::NoPool => "NO POOL",
            Connection::SignedOut => "SIGNED OUT",
            Connection::Connecting => "CONNECTING",
            Connection::Online => "ONLINE",
            Connection::Unreachable { .. } => "UNREACHABLE",
        }
    }

    /// Why the pool is unreachable, when it is.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Connection::Unreachable { reason } => Some(reason),
            _ => None,
        }
    }
}

/// Who the pool says the token belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// The signed-in member, normally the email the pool reported.
    pub member: String,
}

/// What the console needs from a pool.
///
/// The trait is deliberately small: the scaffold asks who it is talking to and
/// nothing else. Later screens add reads here, which keeps the scripted client
/// in the tests the same shape as the SDK-backed one.
pub trait PoolClient {
    /// Ask the pool who the token belongs to.
    fn identity(&mut self) -> Result<Identity>;
}

/// The command palette: a query, the matching actions and the highlighted one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Palette {
    query: String,
    selected: usize,
}

impl Palette {
    /// The text typed so far.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The actions matching the query, in [`Action::ALL`] order.
    pub fn entries(&self) -> Vec<Action> {
        Action::ALL
            .iter()
            .copied()
            .filter(|action| action.matches(&self.query))
            .collect()
    }

    /// The highlighted entry, as an index into [`Palette::entries`].
    pub fn selected(&self) -> usize {
        self.selected
    }

    fn push(&mut self, character: char) {
        self.query.push(character);
        self.selected = 0;
    }

    fn backspace(&mut self) {
        self.query.pop();
        self.selected = 0;
    }

    fn move_by(&mut self, delta: isize) {
        let count = self.entries().len();
        if count == 0 {
            self.selected = 0;
            return;
        }
        let count = count as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(count) as usize;
    }

    fn current(&self) -> Option<Action> {
        self.entries().get(self.selected).copied()
    }
}

/// The whole console state.
pub struct App {
    tab: Tab,
    palette: Option<Palette>,
    notice: Option<String>,
    connection: Connection,
    pool: Option<String>,
    member: Option<String>,
    client: Option<Box<dyn PoolClient>>,
    quit: bool,
}

impl App {
    /// Build the console from the CLI's credential file.
    ///
    /// `client` is `Some` only when a pool and a token were both present; with
    /// no pool the client is dropped here, which is the mechanical reason the
    /// startup path cannot make a network call when nothing is configured.
    ///
    /// The returned app has not probed anything yet; call [`App::start`].
    pub fn from_credentials(file: &CredentialFile, client: Option<Box<dyn PoolClient>>) -> Self {
        let pool = file.pool.clone().filter(|pool| !pool.trim().is_empty());
        let member = file.email.clone().filter(|email| !email.trim().is_empty());
        let (connection, notice, client) = match pool.clone() {
            None => (Connection::NoPool, Some(no_pool_notice()), None),
            Some(pool) => match client {
                Some(client) => (Connection::Connecting, None, Some(client)),
                None => (Connection::SignedOut, Some(sign_in_notice(&pool)), None),
            },
        };
        Self {
            tab: Tab::Hosts,
            palette: None,
            notice,
            connection,
            pool,
            member,
            client,
            quit: false,
        }
    }

    /// Perform the one startup probe, when there is a pool to probe.
    ///
    /// With no pool — or a pool and no token — this returns without touching
    /// the network.
    pub fn start(&mut self) {
        if self.connection == Connection::Connecting {
            self.probe();
        }
    }

    /// Record that the pool client could not even be built.
    ///
    /// This is the other unreachable case: a configured pool whose URL the SDK
    /// refused. It is a notice, never a panic.
    pub fn startup_failure(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        let pool = self.pool.clone().unwrap_or_else(|| "the pool".to_owned());
        self.connection = Connection::Unreachable {
            reason: reason.clone(),
        };
        self.notice = Some(unreachable_notice(&pool, &reason));
    }

    /// Ask the pool again, keeping the console usable on failure.
    pub fn reconnect(&mut self) {
        let Some(pool) = self.pool.clone() else {
            self.connection = Connection::NoPool;
            self.notice = Some(no_pool_notice());
            return;
        };
        if self.client.is_none() {
            self.connection = Connection::SignedOut;
            self.notice = Some(sign_in_notice(&pool));
            return;
        }
        self.connection = Connection::Connecting;
        self.notice = None;
        self.probe();
    }

    fn probe(&mut self) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        match client.identity() {
            Ok(identity) => {
                self.member = Some(identity.member);
                self.connection = Connection::Online;
                self.notice = None;
            }
            Err(error) => {
                let reason = error.to_string();
                self.connection = Connection::Unreachable {
                    reason: reason.clone(),
                };
                let pool = self.pool.clone().unwrap_or_else(|| "the pool".to_owned());
                self.notice = Some(unreachable_notice(&pool, &reason));
            }
        }
    }

    /// Run one action.
    pub fn dispatch(&mut self, action: Action) {
        match action {
            Action::ShowTab(tab) => self.tab = tab,
            Action::NextTab => self.tab = self.tab.next(),
            Action::PreviousTab => self.tab = self.tab.previous(),
            Action::Reconnect => self.reconnect(),
            Action::Quit => self.quit = true,
        }
    }

    /// Handle one key press.
    ///
    /// The palette owns printable keys while it is open, so `q` filters rather
    /// than quitting; `Ctrl-C` always quits.
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') => {
                    self.dispatch(Action::Quit);
                    return;
                }
                KeyCode::Char('p') => {
                    self.open_palette();
                    return;
                }
                _ => {}
            }
        }
        if self.palette.is_some() {
            self.on_palette_key(key);
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.dispatch(Action::Quit),
            KeyCode::Char('p') | KeyCode::Char(':') => self.open_palette(),
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => self.dispatch(Action::NextTab),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                self.dispatch(Action::PreviousTab)
            }
            KeyCode::Char('r') => self.dispatch(Action::Reconnect),
            KeyCode::Char(digit @ '1'..='4') => {
                let index = digit as usize - '1' as usize;
                if let Some(tab) = Tab::ALL.get(index) {
                    self.dispatch(Action::ShowTab(*tab));
                }
            }
            _ => {}
        }
    }

    fn on_palette_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Esc {
            self.palette = None;
            return;
        }
        let Some(palette) = self.palette.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Up => palette.move_by(-1),
            KeyCode::Down | KeyCode::Tab => palette.move_by(1),
            KeyCode::Backspace => palette.backspace(),
            KeyCode::Char(character) => palette.push(character),
            KeyCode::Enter => {
                let action = palette.current();
                self.palette = None;
                if let Some(action) = action {
                    self.dispatch(action);
                }
            }
            _ => {}
        }
    }

    /// Open the palette with an empty query.
    pub fn open_palette(&mut self) {
        self.palette = Some(Palette::default());
    }

    /// The tab in front.
    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// The palette, while it is open.
    pub fn palette(&self) -> Option<&Palette> {
        self.palette.as_ref()
    }

    /// The single notice the shell shows, if any.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// How the console stands with its pool.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// The configured pool, when there is one.
    pub fn pool(&self) -> Option<&str> {
        self.pool.as_deref()
    }

    /// The signed-in member, when the store or the pool named one.
    pub fn member(&self) -> Option<&str> {
        self.member.as_deref()
    }

    /// Whether the event loop should stop.
    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// The status line: connection state, pool and member, always all three.
    pub fn status_line(&self) -> String {
        format!(
            "{}  pool {}  member {}",
            self.connection.label(),
            self.pool.as_deref().unwrap_or("no pool"),
            self.member.as_deref().unwrap_or("not signed in")
        )
    }
}

fn no_pool_notice() -> String {
    "No pool configured: run `tidepool login --email <you> --pool <url>`; \
     the console reads the CLI's credential store."
        .to_owned()
}

fn sign_in_notice(pool: &str) -> String {
    format!("Not signed in to {pool}: run `tidepool login` and the console will reuse that store.")
}

fn unreachable_notice(pool: &str, reason: &str) -> String {
    format!(
        "Cannot reach {pool}: {reason}. The console stays usable; choose Reconnect to try again."
    )
}

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use super::{ConsoleError, Identity, PoolClient};

    /// A client that answers from a script and counts its calls.
    pub(crate) struct ScriptedClient {
        results: VecDeque<Result<Identity, ConsoleError>>,
        calls: Arc<Mutex<usize>>,
    }

    /// A transport failure the way the SDK reports one.
    pub(crate) fn transport(reason: &str) -> ConsoleError {
        ConsoleError::Pool(tidepool_client::Error::Transport(Box::new(
            std::io::Error::new(std::io::ErrorKind::ConnectionRefused, reason.to_owned()),
        )))
    }

    impl ScriptedClient {
        pub(crate) fn online(member: &str) -> Self {
            Self::scripted(vec![Ok(Identity {
                member: member.to_owned(),
            })])
        }

        pub(crate) fn unreachable(reason: &str) -> Self {
            Self::scripted(vec![Err(transport(reason))])
        }

        pub(crate) fn scripted(results: Vec<Result<Identity, ConsoleError>>) -> Self {
            Self {
                results: results.into(),
                calls: Arc::new(Mutex::new(0)),
            }
        }

        pub(crate) fn handle(&self) -> Arc<Mutex<usize>> {
            Arc::clone(&self.calls)
        }
    }

    impl PoolClient for ScriptedClient {
        fn identity(&mut self) -> Result<Identity, ConsoleError> {
            if let Ok(mut calls) = self.calls.lock() {
                *calls += 1;
            }
            self.results
                .pop_front()
                .unwrap_or_else(|| Err(transport("no scripted answer")))
        }
    }

    /// How many times a scripted client was asked.
    pub(crate) fn calls(handle: &Arc<Mutex<usize>>) -> usize {
        handle.lock().map(|calls| *calls).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{ScriptedClient, calls};
    use super::*;

    fn file(pool: Option<&str>, email: Option<&str>, token: Option<&str>) -> CredentialFile {
        CredentialFile {
            pool: pool.map(str::to_owned),
            email: email.map(str::to_owned),
            token: token.map(str::to_owned),
            ..CredentialFile::default()
        }
    }

    fn online_app(member: &str) -> App {
        let mut app = App::from_credentials(
            &file(Some("pool.test"), Some(member), Some("token")),
            Some(Box::new(ScriptedClient::online(member))),
        );
        app.start();
        app
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(character: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(character), KeyModifiers::CONTROL)
    }

    #[test]
    fn no_pool_opens_with_a_notice_that_names_what_is_missing() {
        let mut app = App::from_credentials(&file(None, None, None), None);
        app.start();
        assert_eq!(app.connection(), &Connection::NoPool);
        let notice = app.notice().expect("a notice");
        assert!(notice.contains("No pool configured"), "{notice}");
        assert!(notice.contains("tidepool login"), "{notice}");
        assert_eq!(app.pool(), None);
        assert_eq!(app.member(), None);
    }

    #[test]
    fn no_pool_makes_no_network_call_even_with_a_client_in_hand() {
        let client = ScriptedClient::online("ops@x");
        let handle = client.handle();
        let mut app = App::from_credentials(&file(None, None, None), Some(Box::new(client)));
        app.start();
        app.dispatch(Action::Reconnect);
        assert_eq!(calls(&handle), 0);
        assert_eq!(app.connection(), &Connection::NoPool);
    }

    #[test]
    fn a_pool_without_a_token_asks_for_a_sign_in() {
        let mut app = App::from_credentials(&file(Some("pool.test"), None, None), None);
        app.start();
        assert_eq!(app.connection(), &Connection::SignedOut);
        let notice = app.notice().expect("a notice");
        assert!(notice.contains("Not signed in to pool.test"), "{notice}");
        assert!(notice.contains("tidepool login"), "{notice}");
    }

    #[test]
    fn startup_probes_once_and_adopts_the_member() {
        let client = ScriptedClient::online("member@pool.test");
        let handle = client.handle();
        let mut app = App::from_credentials(
            &file(Some("pool.test"), None, Some("token")),
            Some(Box::new(client)),
        );
        app.start();
        assert_eq!(app.connection(), &Connection::Online);
        assert_eq!(app.member(), Some("member@pool.test"));
        assert_eq!(app.notice(), None);
        assert_eq!(calls(&handle), 1);
    }

    #[test]
    fn an_unreachable_pool_leaves_one_notice_and_the_console_usable() {
        let mut app = App::from_credentials(
            &file(Some("pool.test"), Some("ops@x"), Some("token")),
            Some(Box::new(ScriptedClient::unreachable("connection refused"))),
        );
        app.start();
        assert!(
            app.connection()
                .reason()
                .is_some_and(|reason| reason.contains("connection refused"))
        );
        let notice = app.notice().expect("a notice").to_owned();
        assert!(notice.contains("Cannot reach pool.test"), "{notice}");
        assert!(notice.contains("connection refused"), "{notice}");

        // Still usable: navigation, palette and quitting all work.
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.tab(), Tab::Runs);
        app.on_key(ctrl('p'));
        assert!(app.palette().is_some());
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.palette(), None);
        assert_eq!(app.notice(), Some(notice.as_str()));
        app.on_key(key(KeyCode::Char('q')));
        assert!(app.should_quit());
    }

    #[test]
    fn reconnecting_to_an_unreachable_pool_keeps_one_notice() {
        let mut app = App::from_credentials(
            &file(Some("pool.test"), Some("ops@x"), Some("token")),
            Some(Box::new(ScriptedClient::unreachable("connection refused"))),
        );
        app.start();
        app.dispatch(Action::Reconnect);
        assert_eq!(
            app.notice()
                .map(|notice| notice.matches("Cannot reach").count()),
            Some(1)
        );
        assert_eq!(app.connection().label(), "UNREACHABLE");
    }

    #[test]
    fn reconnecting_without_a_pool_says_what_is_missing() {
        let mut app = App::from_credentials(&file(None, None, None), None);
        app.dispatch(Action::Reconnect);
        assert_eq!(app.connection(), &Connection::NoPool);
        assert!(
            app.notice()
                .is_some_and(|notice| notice.contains("No pool configured"))
        );
    }

    #[test]
    fn a_client_that_fails_to_build_is_a_notice_not_a_panic() {
        let mut app =
            App::from_credentials(&file(Some("not a url"), Some("ops@x"), Some("token")), None);
        app.startup_failure("invalid client configuration: not a url");
        assert_eq!(app.connection().label(), "UNREACHABLE");
        assert!(
            app.notice()
                .is_some_and(|notice| notice.contains("Cannot reach not a url"))
        );
        app.on_key(key(KeyCode::Char('2')));
        assert_eq!(app.tab(), Tab::Runs);
    }

    #[test]
    fn tabs_navigate_by_keyboard() {
        let mut app = online_app("ops@x");
        assert_eq!(app.tab(), Tab::Hosts);

        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.tab(), Tab::Runs);
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.tab(), Tab::Rooms);
        app.on_key(key(KeyCode::BackTab));
        assert_eq!(app.tab(), Tab::Runs);
        app.on_key(key(KeyCode::Left));
        assert_eq!(app.tab(), Tab::Hosts);
        app.on_key(key(KeyCode::Right));
        assert_eq!(app.tab(), Tab::Runs);
        app.on_key(key(KeyCode::Char('h')));
        assert_eq!(app.tab(), Tab::Hosts);
        app.on_key(key(KeyCode::Char('l')));
        assert_eq!(app.tab(), Tab::Runs);
        app.on_key(key(KeyCode::Tab));
        app.on_key(key(KeyCode::Tab));
        app.on_key(key(KeyCode::Tab));
        assert_eq!(app.tab(), Tab::Hosts, "Tab wraps");
    }

    #[test]
    fn digits_jump_to_each_tab() {
        let mut app = online_app("ops@x");
        for (digit, tab) in [
            ('1', Tab::Hosts),
            ('2', Tab::Runs),
            ('3', Tab::Rooms),
            ('4', Tab::Settings),
        ] {
            app.on_key(key(KeyCode::Char(digit)));
            assert_eq!(app.tab(), tab);
        }
    }

    #[test]
    fn the_palette_offers_every_action() {
        let mut app = online_app("ops@x");
        app.on_key(ctrl('p'));
        let palette = app.palette().expect("open palette");
        assert_eq!(palette.entries(), Action::ALL.to_vec());
        for action in Action::ALL {
            assert!(palette.entries().contains(&action), "{action:?}");
        }
    }

    #[test]
    fn every_action_runs_from_the_palette() {
        for (index, action) in Action::ALL.iter().enumerate() {
            let client = ScriptedClient::scripted(vec![
                Ok(Identity {
                    member: "ops@x".to_owned(),
                }),
                Ok(Identity {
                    member: "ops@x".to_owned(),
                }),
            ]);
            let handle = client.handle();
            let mut app = App::from_credentials(
                &file(Some("pool.test"), Some("ops@x"), Some("token")),
                Some(Box::new(client)),
            );
            app.start();
            app.on_key(ctrl('p'));
            for _ in 0..index {
                app.on_key(key(KeyCode::Down));
            }
            app.on_key(key(KeyCode::Enter));

            assert_eq!(app.palette(), None, "{action:?} closes the palette");
            match action {
                Action::ShowTab(tab) => assert_eq!(app.tab(), *tab),
                Action::NextTab => assert_eq!(app.tab(), Tab::Runs),
                Action::PreviousTab => assert_eq!(app.tab(), Tab::Settings),
                Action::Reconnect => {
                    assert_eq!(app.connection(), &Connection::Online);
                    assert_eq!(calls(&handle), 2);
                }
                Action::Quit => assert!(app.should_quit()),
            }
        }
    }

    #[test]
    fn the_palette_filters_and_runs_the_selected_action() {
        let mut app = online_app("ops@x");
        app.on_key(ctrl('p'));
        for character in "rooms".chars() {
            app.on_key(key(KeyCode::Char(character)));
        }
        let palette = app.palette().expect("open palette");
        assert_eq!(palette.query(), "rooms");
        assert_eq!(palette.entries(), vec![Action::ShowTab(Tab::Rooms)]);
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.tab(), Tab::Rooms);
        assert_eq!(app.palette(), None);
    }

    #[test]
    fn typing_in_the_palette_does_not_quit_or_navigate() {
        let mut app = online_app("ops@x");
        app.on_key(ctrl('p'));
        app.on_key(key(KeyCode::Char('q')));
        assert!(!app.should_quit());
        assert_eq!(app.tab(), Tab::Hosts);
        assert_eq!(app.palette().map(Palette::query), Some("q"));
        app.on_key(key(KeyCode::Backspace));
        assert_eq!(app.palette().map(Palette::query), Some(""));
    }

    #[test]
    fn escape_closes_the_palette_without_running_anything() {
        let mut app = online_app("ops@x");
        app.on_key(ctrl('p'));
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.palette(), None);
        assert_eq!(app.tab(), Tab::Hosts);
    }

    #[test]
    fn q_and_ctrl_c_quit() {
        let mut app = online_app("ops@x");
        app.on_key(key(KeyCode::Char('q')));
        assert!(app.should_quit());

        let mut app = online_app("ops@x");
        app.on_key(ctrl('c'));
        assert!(app.should_quit());

        let mut app = online_app("ops@x");
        app.on_key(ctrl('p'));
        app.on_key(ctrl('c'));
        assert!(app.should_quit(), "Ctrl-C quits even with the palette open");
    }

    #[test]
    fn the_status_line_carries_pool_member_and_state() {
        let app = online_app("ops@x");
        let line = app.status_line();
        assert!(line.starts_with("ONLINE"), "{line}");
        assert!(line.contains("pool pool.test"), "{line}");
        assert!(line.contains("member ops@x"), "{line}");

        let app = App::from_credentials(&file(None, None, None), None);
        let line = app.status_line();
        assert!(line.starts_with("NO POOL"), "{line}");
        assert!(line.contains("pool no pool"), "{line}");
        assert!(line.contains("member not signed in"), "{line}");
    }
}
