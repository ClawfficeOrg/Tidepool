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

use std::time::Instant;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tidepool_cli::credentials::CredentialFile;

use crate::auth::{Capability, Role};
use crate::tabs::hosts::{HostsClient, HostsState};
use crate::tabs::rooms::{RoomsClient, RoomsState};
use crate::tabs::runs::{RunsClient, RunsState};
use crate::ui::banner::Theme;

/// Everything that can go wrong before or during a console run.
#[derive(Debug, thiserror::Error)]
pub enum ConsoleError {
    /// The credential store could not be read.
    #[error("credential store: {0}")]
    Store(#[from] tidepool_cli::CliError),
    /// The shared session or the pool-role lookup failed.
    #[error(transparent)]
    Auth(#[from] crate::auth::AuthError),
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
    /// Open the about screen: the banner, the cell sprite and the versions.
    About,
    /// Leave the console.
    Quit,
}

impl Action {
    /// Every action, in the order the palette lists them.
    pub const ALL: [Action; 9] = [
        Action::ShowTab(Tab::Hosts),
        Action::ShowTab(Tab::Runs),
        Action::ShowTab(Tab::Rooms),
        Action::ShowTab(Tab::Settings),
        Action::NextTab,
        Action::PreviousTab,
        Action::Reconnect,
        Action::About,
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
            Action::About => "About Tidepool",
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
    /// The member's role, which gates mutating actions.
    pub role: Role,
}

/// What the console needs from a pool.
///
/// The trait is deliberately small: the scaffold asks who it is talking to, and
/// [`HostsClient`] and [`RunsClient`] add exactly what the hosts and runs tabs
/// read and do. Later screens add reads here, which keeps the scripted client in
/// the tests the same shape as the SDK-backed one.
pub trait PoolClient: HostsClient + RunsClient + RoomsClient {
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
    role: Option<Role>,
    client: Option<Box<dyn PoolClient>>,
    hosts: HostsState,
    runs: RunsState,
    rooms: RoomsState,
    theme: Theme,
    about: bool,
    splash: bool,
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
        let mut runs = RunsState::new();
        runs.set_member(member.clone());
        let mut rooms = RoomsState::new();
        rooms.set_member(member.clone());
        Self {
            tab: Tab::Hosts,
            palette: None,
            notice,
            connection,
            pool,
            member,
            role: None,
            client,
            hosts: HostsState::new(),
            runs,
            rooms,
            theme: Theme::from_environment(),
            about: false,
            splash: false,
            quit: false,
        }
    }

    /// Perform the one startup probe, when there is a pool to probe, then fill
    /// the hosts tab once the pool answers.
    ///
    /// With no pool — or a pool and no token — this returns without touching
    /// the network.
    pub fn start(&mut self) {
        if self.connection == Connection::Connecting {
            self.probe();
        }
        if self.connection == Connection::Online {
            self.pump_hosts(Instant::now());
            self.pump_runs(Instant::now());
            self.pump_rooms(Instant::now());
        }
    }

    /// Record that the pool client could not even be built.
    ///
    /// This is the other unreachable case: a configured pool whose URL the SDK
    /// refused. It is a notice, never a panic.
    pub fn startup_failure(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        let pool = self.pool.clone().unwrap_or_else(|| "the pool".to_owned());
        self.role = None;
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
            self.role = None;
            self.notice = Some(sign_in_notice(&pool));
            return;
        }
        self.connection = Connection::Connecting;
        self.notice = None;
        self.probe();
        if self.connection == Connection::Online {
            self.pump_hosts(Instant::now());
            self.pump_runs(Instant::now());
            self.pump_rooms(Instant::now());
        }
    }

    /// Drive the active screen's timers: no key press is needed for a host that
    /// goes offline to appear offline on the next frame.
    pub fn tick(&mut self) {
        self.tick_at(Instant::now());
    }

    /// [`App::tick`] with the clock supplied, so the tests can advance it.
    pub(crate) fn tick_at(&mut self, now: Instant) {
        if self.connection == Connection::Online {
            self.pump_hosts(now);
            self.pump_runs(now);
            self.pump_rooms(now);
        }
    }

    fn pump_hosts(&mut self, now: Instant) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        self.hosts.drive(&mut **client, now);
    }

    fn pump_runs(&mut self, now: Instant) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        self.runs.drive(&mut **client, now);
    }

    fn pump_rooms(&mut self, now: Instant) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        self.rooms.drive(&mut **client, now);
    }

    fn probe(&mut self) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        match client.identity() {
            Ok(identity) => {
                self.member = Some(identity.member);
                self.role = Some(identity.role);
                self.runs.set_member(self.member.clone());
                self.connection = Connection::Online;
                self.notice = None;
            }
            Err(error) => {
                let reason = error.to_string();
                self.role = None;
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
            Action::About => self.show_about(),
            Action::Quit => self.quit = true,
        }
    }

    /// Handle one key press.
    ///
    /// The palette owns printable keys while it is open, so `q` filters rather
    /// than quitting; `Ctrl-C` always quits. The startup splash is dismissed by
    /// the first key, which then continues to its normal handler, so the first
    /// key is never swallowed.
    pub fn on_key(&mut self, key: KeyEvent) {
        self.splash = false;
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
        if self.about {
            match key.code {
                KeyCode::Esc | KeyCode::Char('a') => self.close_about(),
                KeyCode::Char('q') => self.dispatch(Action::Quit),
                _ => {}
            }
            return;
        }
        if self.palette.is_some() {
            self.on_palette_key(key);
            return;
        }
        if let Some(capability) = self.forbidden_capability(key)
            && let Some(role) = self.role
            && let Some(explanation) = capability.denial(role)
        {
            self.notice = Some(explanation);
            return;
        }
        if self.tab == Tab::Hosts && self.hosts.on_key(key) {
            self.pump_hosts(Instant::now());
            return;
        }
        if self.tab == Tab::Runs && self.runs.on_key(key) {
            self.pump_runs(Instant::now());
            return;
        }
        if self.tab == Tab::Rooms && self.rooms.on_key(key) {
            self.pump_rooms(Instant::now());
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
            KeyCode::Char('a') => self.dispatch(Action::About),
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

    /// The capability a key would use on the tab in front, if it would mutate.
    ///
    /// The table follows the key handling below: an enrol prompt's `Enter`, a
    /// dispatch form's `Enter`, `e`/`g`/`x` on the hosts tab, `d` on the runs
    /// tab, `c` in an open room and `Ctrl-S` in the composer.
    fn capability_for_key(&self, key: KeyEvent) -> Option<Capability> {
        match self.tab {
            Tab::Hosts => {
                if self.hosts.prompt().is_some() {
                    return (key.code == KeyCode::Enter).then_some(Capability::Mutate);
                }
                matches!(key.code, KeyCode::Char('e' | 'g' | 'x')).then_some(Capability::Mutate)
            }
            Tab::Runs => {
                if self.runs.form().is_some() {
                    return (key.code == KeyCode::Enter).then_some(Capability::Mutate);
                }
                (key.code == KeyCode::Char('d')).then_some(Capability::Mutate)
            }
            Tab::Rooms => {
                if self.rooms.composer().is_some() {
                    return (key.code == KeyCode::Char('s')
                        && key.modifiers.contains(KeyModifiers::CONTROL))
                    .then_some(Capability::Mutate);
                }
                (key.code == KeyCode::Char('c')
                    && self.rooms.open_room_id().is_some()
                    && self.rooms.follow_prompt().is_none())
                .then_some(Capability::Mutate)
            }
            Tab::Settings => None,
        }
    }

    /// The capability the current role lacks for this key, if any.
    fn forbidden_capability(&self, key: KeyEvent) -> Option<Capability> {
        let capability = self.capability_for_key(key)?;
        (!self.can(capability)).then_some(capability)
    }

    /// Whether the signed-in member may perform `capability`.
    ///
    /// With no role yet (no pool, a signed-out store or an unanswered probe)
    /// this is `true`: the pool is the authority and will refuse what the
    /// console cannot know about. The key gate and a renderer can both use it,
    /// so a known role hides an affordance and an unknown one defers to the
    /// pool.
    pub fn can(&self, capability: Capability) -> bool {
        self.role.is_none_or(|role| role.allows(capability))
    }

    /// The tab in front.
    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// The palette, while it is open.
    pub fn palette(&self) -> Option<&Palette> {
        self.palette.as_ref()
    }

    /// The Clawffice theme this console draws with.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Replace the theme, so a test can draw a palette or a no-colour terminal.
    #[cfg(test)]
    pub(crate) fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Whether the about screen is in front.
    pub fn about(&self) -> bool {
        self.about
    }

    /// Open the about screen.
    pub fn show_about(&mut self) {
        self.palette = None;
        self.about = true;
    }

    /// Close the about screen.
    pub fn close_about(&mut self) {
        self.about = false;
    }

    /// Show the banner until the first key press.
    pub fn show_startup(&mut self) {
        self.splash = true;
    }

    /// Whether the splash is still in front.
    pub fn splash(&self) -> bool {
        self.splash
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

    /// The hosts tab's state.
    pub fn hosts(&self) -> &HostsState {
        &self.hosts
    }

    /// The runs tab's state.
    pub fn runs(&self) -> &RunsState {
        &self.runs
    }

    /// The rooms tab's state.
    pub fn rooms(&self) -> &RoomsState {
        &self.rooms
    }

    /// Whether the event loop should stop.
    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// The status line: connection state, pool, member and role, always all four.
    pub fn status_line(&self) -> String {
        format!(
            "{}  pool {}  member {}  role {}",
            self.connection.label(),
            self.pool.as_deref().unwrap_or("no pool"),
            self.member.as_deref().unwrap_or("not signed in"),
            self.role.map(Role::as_str).unwrap_or("unknown")
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

    use tidepool_client::endpoints::{
        GetV1ActivityQuery, GetV1ChatroomsRoomMessagesQuery, GetV1SessionsSessionRelayEventsQuery,
    };
    use tidepool_client::types::{
        ActivityPage, ActivityRunDetail, HistoryResponse, HostCapabilityReport, HostView,
        ManualDispatchRequest, ManualDispatchResponse, MessageView, ProjectView, RelayPage,
        RelayState, RoomMemberView, RoomView, SendMessageBody,
    };

    use super::{ConsoleError, Identity, PoolClient, Role};
    use crate::tabs::hosts::{Enrollment, HostsClient};
    use crate::tabs::rooms::RoomsClient;
    use crate::tabs::runs::{RunsClient, TriggerSummary};

    /// A client that answers from a script and counts its calls.
    pub(crate) struct ScriptedClient {
        results: VecDeque<Result<Identity, ConsoleError>>,
        hosts: VecDeque<Result<Vec<HostView>, ConsoleError>>,
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
                role: Role::Member,
            })])
        }

        pub(crate) fn viewer(member: &str) -> Self {
            Self::scripted(vec![Ok(Identity {
                member: member.to_owned(),
                role: Role::Viewer,
            })])
        }

        pub(crate) fn unreachable(reason: &str) -> Self {
            Self::scripted(vec![Err(transport(reason))])
        }

        pub(crate) fn scripted(results: Vec<Result<Identity, ConsoleError>>) -> Self {
            Self {
                results: results.into(),
                hosts: VecDeque::new(),
                calls: Arc::new(Mutex::new(0)),
            }
        }

        /// Queue one host list for the hosts tab's next read.
        pub(crate) fn queue_hosts(&mut self, hosts: Vec<HostView>) {
            self.hosts.push_back(Ok(hosts));
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

    /// The scaffold scripts no capabilities; the hosts tab's own tests script
    /// that surface.
    impl HostsClient for ScriptedClient {
        fn list_hosts(&mut self) -> Result<Vec<HostView>, ConsoleError> {
            self.hosts.pop_front().unwrap_or_else(|| Ok(Vec::new()))
        }

        fn host_capabilities(
            &mut self,
            _host_id: &str,
        ) -> Result<HostCapabilityReport, ConsoleError> {
            Err(transport("no scripted capability report"))
        }

        fn enroll_host(&mut self, _hostname: &str) -> Result<Enrollment, ConsoleError> {
            Err(transport("no scripted enrollment"))
        }

        fn grant_execution(&mut self, _host_id: &str) -> Result<HostView, ConsoleError> {
            Err(transport("no scripted grant"))
        }

        fn revoke_execution(&mut self, _host_id: &str) -> Result<HostView, ConsoleError> {
            Err(transport("no scripted revoke"))
        }
    }

    /// The scaffold scripts no activity; the runs tab's own tests script that
    /// surface.
    impl RunsClient for ScriptedClient {
        fn list_projects(&mut self) -> Result<Vec<ProjectView>, ConsoleError> {
            Ok(Vec::new())
        }

        fn list_activity(
            &mut self,
            _query: GetV1ActivityQuery,
        ) -> Result<ActivityPage, ConsoleError> {
            Ok(ActivityPage::default())
        }

        fn run_detail(&mut self, _run: &str) -> Result<ActivityRunDetail, ConsoleError> {
            Err(transport("no scripted run detail"))
        }

        fn dispatch(
            &mut self,
            _request: ManualDispatchRequest,
        ) -> Result<ManualDispatchResponse, ConsoleError> {
            Err(transport("no scripted dispatch"))
        }

        fn list_triggers(&mut self, _project: &str) -> Result<Vec<TriggerSummary>, ConsoleError> {
            Ok(Vec::new())
        }
    }

    /// The scaffold scripts no rooms; the rooms tab's own tests script that
    /// surface.
    impl RoomsClient for ScriptedClient {
        fn list_rooms(&mut self) -> Result<Vec<RoomView>, ConsoleError> {
            Ok(Vec::new())
        }

        fn room_members(&mut self, _room: &str) -> Result<Vec<RoomMemberView>, ConsoleError> {
            Ok(Vec::new())
        }

        fn room_history(
            &mut self,
            _room: &str,
            _query: GetV1ChatroomsRoomMessagesQuery,
        ) -> Result<HistoryResponse, ConsoleError> {
            Ok(HistoryResponse::default())
        }

        fn send_message(
            &mut self,
            _room: &str,
            _body: SendMessageBody,
        ) -> Result<MessageView, ConsoleError> {
            Err(transport("no scripted send"))
        }

        fn relay_events(
            &mut self,
            _session: &str,
            _query: GetV1SessionsSessionRelayEventsQuery,
        ) -> Result<RelayPage, ConsoleError> {
            Ok(RelayPage::default())
        }

        fn relay_state(&mut self, _session: &str) -> Result<RelayState, ConsoleError> {
            Err(transport("no scripted relay state"))
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
                    role: Role::Member,
                }),
                Ok(Identity {
                    member: "ops@x".to_owned(),
                    role: Role::Member,
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
                Action::About => assert!(app.about()),
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

    #[test]
    fn the_event_loop_refreshes_presence_without_a_key() {
        use crate::tabs::hosts::REFRESH_INTERVAL;
        use crate::tabs::hosts::testing::host;

        let mut client = ScriptedClient::online("ops@x");
        client.queue_hosts(vec![host("h-1", "alpha", "connected", false)]);
        client.queue_hosts(vec![host("h-1", "alpha", "offline", false)]);
        let mut app = App::from_credentials(
            &file(Some("pool.test"), Some("ops@x"), Some("token")),
            Some(Box::new(client)),
        );
        app.start();
        assert_eq!(app.hosts().len(), 1);

        let now = Instant::now();
        app.tick_at(now + REFRESH_INTERVAL);
        let rows = app.hosts().visible_rows(now + REFRESH_INTERVAL);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].host().status,
            "offline",
            "the tick alone moved the host offline"
        );
    }

    #[test]
    fn the_hosts_tab_owns_its_keys() {
        let mut app = online_app("ops@x");
        app.on_key(key(KeyCode::Char('e')));
        assert_eq!(app.hosts().prompt(), Some(("hostname", "")));

        app.on_key(key(KeyCode::Char('q')));
        assert!(!app.should_quit(), "q typed into the hostname prompt");
        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.hosts().prompt(), None);

        app.on_key(key(KeyCode::Char('q')));
        assert!(app.should_quit(), "q quits once the prompt is closed");
    }

    #[test]
    fn the_rooms_tab_owns_its_keys() {
        let mut app = online_app("ops@x");
        app.on_key(key(KeyCode::Char('3')));
        assert_eq!(app.tab(), Tab::Rooms);

        app.on_key(key(KeyCode::Char('f')));
        assert_eq!(app.rooms().follow_prompt(), Some(""));
        app.on_key(key(KeyCode::Char('q')));
        assert!(!app.should_quit(), "q typed into the session prompt");
        assert_eq!(app.rooms().follow_prompt(), Some("q"));

        app.on_key(key(KeyCode::Esc));
        assert_eq!(app.rooms().follow_prompt(), None);
        app.on_key(key(KeyCode::Char('q')));
        assert!(app.should_quit(), "q quits once the prompt is closed");
    }

    #[test]
    fn a_viewer_is_refused_at_every_mutating_key_and_told_why() {
        let client = ScriptedClient::viewer("ops@x");
        let handle = client.handle();
        let mut app = App::from_credentials(
            &file(Some("pool.test"), Some("ops@x"), Some("token")),
            Some(Box::new(client)),
        );
        app.start();
        assert!(app.status_line().contains("role viewer"));
        assert!(!app.can(Capability::Mutate));
        assert!(app.can(Capability::Read));

        // Hosts: enrol, grant and revoke never open or queue anything.
        for character in ['e', 'g', 'x'] {
            app.on_key(key(KeyCode::Char(character)));
            assert_eq!(app.hosts().prompt(), None, "{character} opened a prompt");
            let notice = app.notice().expect("an explanation");
            assert!(notice.contains("`viewer`"), "{notice}");
            assert!(notice.contains("does not attempt"), "{notice}");
        }

        // Runs: the dispatch form never opens.
        app.on_key(key(KeyCode::Char('2')));
        app.on_key(key(KeyCode::Char('d')));
        assert!(app.runs().form().is_none());
        assert!(
            app.notice()
                .is_some_and(|notice| notice.contains("change pool state"))
        );

        // The only call was the startup identity probe; no tab action ran.
        assert_eq!(calls(&handle), 1);
    }

    #[test]
    fn a_viewer_can_still_type_filter_letters_that_are_operation_keys() {
        let client = ScriptedClient::viewer("ops@x");
        let mut app = App::from_credentials(
            &file(Some("pool.test"), Some("ops@x"), Some("token")),
            Some(Box::new(client)),
        );
        app.start();
        app.on_key(key(KeyCode::Char('/')));
        app.on_key(key(KeyCode::Char('e')));
        assert_eq!(app.hosts().prompt(), Some(("filter", "e")));
        assert_eq!(app.hosts().query(), "e");
    }

    #[test]
    fn a_member_may_use_the_mutating_keys() {
        let mut app = online_app("ops@x");
        assert!(app.status_line().contains("role member"));
        assert!(app.can(Capability::Mutate));
        app.on_key(key(KeyCode::Char('e')));
        assert_eq!(app.hosts().prompt(), Some(("hostname", "")));

        app.on_key(key(KeyCode::Esc));
        app.on_key(key(KeyCode::Char('2')));
        app.on_key(key(KeyCode::Char('d')));
        assert!(app.runs().form().is_some());
    }

    #[test]
    fn ticking_without_an_online_pool_is_a_no_op() {
        let mut app = App::from_credentials(&file(None, None, None), None);
        app.tick();
        assert_eq!(app.connection(), &Connection::NoPool);
        assert!(app.hosts().is_empty());
    }
}
