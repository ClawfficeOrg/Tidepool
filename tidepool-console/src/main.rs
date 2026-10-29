//! `tidepool-console`, the operator's terminal.
//!
//! A ratatui shell over the Tidepool SDK. It reads the credential store the
//! `tidepool` CLI writes, so one `tidepool login` serves both, and it makes no
//! network call at startup unless that store names a pool with a token. A pool
//! that does not answer becomes a notice on the status line, never a panic.
//!
//! The binary is only the composition root: the terminal lifecycle lives here,
//! state and key handling in [`app`], drawing in [`ui`]. The real pool client is
//! a thin wrapper over `tidepool-client`'s blocking facade; the tests script the
//! same [`app::PoolClient`] trait instead of listening on a socket.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// `ConsoleError` carries the SDK's and the CLI's errors verbatim, so it carries
// a whole problem document (status, code, detail, request id) when a pool
// answers with one; boxing would only move the allocation, as in the SDK.
#![allow(clippy::result_large_err)]

mod app;
mod tabs;
mod ui;

use std::io::Stdout;
use std::process::ExitCode;
use std::time::Duration;

use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tidepool_client::blocking::Client as SdkClient;
use tidepool_client::endpoints::{
    DeleteV1HostsIdExecution, GetV1Hosts, GetV1HostsIdCapabilities, GetV1PoolMe, PostV1Hosts,
    PostV1HostsEnrollmentTokens, PostV1HostsIdExecution,
};
use tidepool_client::types::{
    CreateHostRequest, HostCapabilityReport, HostView, JsonValue, MintEnrollmentTokenRequest,
};

use crate::tabs::hosts::Enrollment;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("tidepool-console: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> app::Result<()> {
    let path = tidepool_cli::credentials::default_path(|name| std::env::var(name).ok())?;
    let store = tidepool_cli::credentials::CredentialStore::load(path)?;
    let file = store.file().clone();

    let (client, startup_error): (Option<Box<dyn app::PoolClient>>, Option<String>) =
        match (file.pool.as_deref(), file.token.as_deref()) {
            (Some(pool), Some(token)) => match SdkPoolClient::new(pool, token) {
                Ok(client) => (Some(Box::new(client)), None),
                Err(error) => (None, Some(error.to_string())),
            },
            _ => (None, None),
        };

    let mut console = app::App::from_credentials(&file, client);
    match startup_error {
        Some(reason) => console.startup_failure(reason),
        None => console.start(),
    }
    run_console(&mut console)
}

/// The pool client backed by the SDK's blocking facade.
struct SdkPoolClient {
    client: SdkClient,
}

impl SdkPoolClient {
    fn new(pool: &str, token: &str) -> app::Result<Self> {
        let client = SdkClient::from_base_url(pool.to_owned())?;
        Ok(Self {
            client: client.with_token(token.to_owned()),
        })
    }
}

impl app::PoolClient for SdkPoolClient {
    fn identity(&mut self) -> app::Result<app::Identity> {
        let identity = self.client.call(&GetV1PoolMe::new())?;
        let member = if identity.user.email.trim().is_empty() {
            identity.user.display_name
        } else {
            identity.user.email
        };
        Ok(app::Identity { member })
    }
}

impl tabs::hosts::HostsClient for SdkPoolClient {
    fn list_hosts(&mut self) -> app::Result<Vec<HostView>> {
        Ok(self.client.call(&GetV1Hosts::new())?.hosts)
    }

    fn host_capabilities(&mut self, host_id: &str) -> app::Result<HostCapabilityReport> {
        Ok(self.client.call(&GetV1HostsIdCapabilities::new(host_id))?)
    }

    fn enroll_host(&mut self, hostname: &str) -> app::Result<Enrollment> {
        let host = self.client.call(&PostV1Hosts::new(CreateHostRequest {
            hostname: hostname.to_owned(),
            display_name: None,
        }))?;
        let minted = self.client.call(&PostV1HostsEnrollmentTokens::new(
            MintEnrollmentTokenRequest {
                host_id: host.id.clone(),
            },
        ))?;
        Ok(Enrollment {
            host,
            token: minted.token,
            expires_at: minted.enrollment_token.expires_at,
        })
    }

    fn grant_execution(&mut self, host_id: &str) -> app::Result<HostView> {
        Ok(self.client.call(&PostV1HostsIdExecution::new(
            host_id,
            JsonValue::Object(Default::default()),
        ))?)
    }

    fn revoke_execution(&mut self, host_id: &str) -> app::Result<HostView> {
        Ok(self.client.call(&DeleteV1HostsIdExecution::new(host_id))?)
    }
}

/// A terminal held in raw mode on the alternate screen, restored on drop.
struct Tui {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Tui {
    fn start() -> app::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = terminal::disable_raw_mode();
            return Err(error.into());
        }
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = match Terminal::new(backend) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = terminal::disable_raw_mode();
                return Err(error.into());
            }
        };
        terminal.clear()?;
        Ok(Self { terminal })
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen, Show);
        let _ = self.terminal.show_cursor();
    }
}

/// Draw and dispatch until the operator quits.
///
/// The poll timeout keeps the loop responsive to a resize or a later live feed
/// without spinning; the console redraws on every tick, which also means the
/// connection state cannot go stale on screen.
fn run_console(console: &mut app::App) -> app::Result<()> {
    let mut tui = Tui::start()?;
    while !console.should_quit() {
        tui.terminal.draw(|frame| ui::render(console, frame))?;
        console.tick();
        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            console.on_key(key);
        }
    }
    Ok(())
}
