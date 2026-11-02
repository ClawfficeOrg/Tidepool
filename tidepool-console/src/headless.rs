//! Non-interactive mode: one action, the CLI's own output.
//!
//! `tidepool-console --headless <command>` is the console's second skin, not a
//! second client. It strips the marker, parses the command with
//! [`tidepool_cli::args`], checks the member's role against the pool, and then
//! hands the very same tokens and the very same credential file to
//! [`tidepool_cli::run_with_path`]. The JSON a script reads is therefore the
//! CLI's JSON by construction: there is one printer, one SDK call and one
//! store, and this module adds only the role gate in front.
//!
//! Every command the CLI has is available here. There is no console-only
//! command and no CLI command this mode silently lacks, which is what
//! `tidepool-console/README.md` records and what the parity test asserts.
//!
//! # What the role gate costs
//!
//! A mutating command first asks `GET /v1/pool/me` for the current role. That
//! is one read before the write, never a cached role, so a role changed a
//! moment ago is honoured. A viewer's command stops after that read: no
//! mutating request is sent and the error says which role is missing and who
//! can change it. Read-only commands, `help` and `login` make no extra call.
//!
//! If the role read itself fails, the command does not fall through and try the
//! mutation anyway; the failure is reported. A gate that opens when it cannot
//! read the rule is not a gate.

use std::io::{Read, Write};
use std::path::PathBuf;

use tidepool_cli::args::{self, Command};
use tidepool_cli::credentials;
use tidepool_cli::error::CliError;

use crate::auth::{AuthError, Capability, Role, Session};

/// The marker that selects this mode. Any argument selects it; the marker is
/// the spelled-out form a script uses for clarity.
pub const MARKER: &str = "--headless";

/// Everything headless mode can fail with.
#[derive(Debug, thiserror::Error)]
pub enum HeadlessError {
    /// The command line or the delegated command failed.
    #[error(transparent)]
    Cli(#[from] CliError),
    /// The session or the pool-role lookup failed.
    #[error(transparent)]
    Auth(#[from] AuthError),
    /// The member's role cannot perform the action, so it was not attempted.
    #[error(
        "the `{role}` role cannot {capability}: the pool refuses every mutating \
         request from a viewer, so this action is not attempted. Ask an owner to \
         change your role."
    )]
    Forbidden {
        /// The role the pool reported.
        role: Role,
        /// What the command would have done.
        capability: Capability,
    },
}

/// The result alias for headless mode.
pub type Result<T> = std::result::Result<T, HeadlessError>;

/// Whether this run is non-interactive.
///
/// The console takes no options of its own, so any argument selects the
/// non-interactive mode; no arguments draws the terminal UI.
pub fn requested(args: &[String]) -> bool {
    !args.is_empty()
}

/// Run one command against the CLI's default credential store.
pub fn run(tokens: Vec<String>, stdin: &mut dyn Read, out: &mut dyn Write) -> Result<()> {
    let path = credentials::default_path(|name| std::env::var(name).ok())?;
    run_with_path(tokens, path, stdin, out)
}

/// Run one command against an explicit store path.
///
/// Tests use this entry point; the marker is removed here so the delegated
/// parser sees exactly the CLI grammar.
pub fn run_with_path(
    tokens: Vec<String>,
    store_path: PathBuf,
    stdin: &mut dyn Read,
    out: &mut dyn Write,
) -> Result<()> {
    let tokens: Vec<String> = tokens.into_iter().filter(|token| token != MARKER).collect();
    let invocation = args::parse(tokens.clone())?;

    // The gate reads the role only when the command would mutate, and only
    // when the store already carries a session. A signed-out command is left
    // to the CLI, which has the better "run tidepool login" sentence.
    if capability_for(&invocation.command) == Capability::Mutate {
        let session = Session::load(store_path.clone())?;
        if session.signed_in()
            && let Some(role) = session.role(invocation.pool.as_deref())?
            && !role.allows(Capability::Mutate)
        {
            return Err(HeadlessError::Forbidden {
                role,
                capability: Capability::Mutate,
            });
        }
    }

    tidepool_cli::run_with_path(tokens, store_path, tidepool_cli::Runtime { stdin, out })?;
    Ok(())
}

/// The pool line a command crosses.
///
/// `Mutate` is exactly the set whose handlers call the pool's
/// `require_mutating`; `TriggerValidate` is here because the pool guards it the
/// same way even though it writes no state. Everything else is a read.
fn capability_for(command: &Command) -> Capability {
    match command {
        Command::Help
        | Command::Login { .. }
        | Command::PoolStatus
        | Command::HostList
        | Command::TriggerList { .. }
        | Command::RunWatch { .. }
        | Command::RoomList
        | Command::RelayFollow { .. } => Capability::Read,
        Command::HostEnroll { .. }
        | Command::HostGrant { .. }
        | Command::TriggerValidate { .. }
        | Command::TriggerInstall { .. }
        | Command::RunDispatch { .. }
        | Command::RoomSend { .. } => Capability::Mutate,
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use tidepool_cli::credentials::CredentialStore;

    use super::*;
    use crate::auth::testing::{
        DISPATCHED, HOSTS, LOGIN, POOL_ME_MEMBER, POOL_ME_VIEWER, Reply, SETUP_STATUS,
        ScriptedPool, TempDir,
    };

    /// A store signed in to `pool`, the way either skin leaves one behind.
    fn signed_in(directory: &TempDir, pool: &str) -> PathBuf {
        let path = directory.store();
        let mut store = CredentialStore::load(path.clone()).expect("store");
        store.file_mut().pool = Some(pool.to_owned());
        store.file_mut().email = Some("ops@example.com".to_owned());
        store.file_mut().token = Some("abcd1234-the-whole-operator-token".to_owned());
        store.save().expect("save");
        path
    }

    fn invoke(
        tokens: &[&str],
        store: &Path,
        stdin: &str,
    ) -> std::result::Result<String, HeadlessError> {
        let mut input = std::io::Cursor::new(stdin.as_bytes().to_vec());
        let mut output = Vec::new();
        let result = run_with_path(
            tokens.iter().map(|token| (*token).to_owned()).collect(),
            store.to_path_buf(),
            &mut input,
            &mut output,
        );
        result.map(|()| String::from_utf8(output).expect("utf8"))
    }

    fn invoke_cli(tokens: &[&str], store: &Path, stdin: &str) -> String {
        let mut input = std::io::Cursor::new(stdin.as_bytes().to_vec());
        let mut output = Vec::new();
        tidepool_cli::run_with_path(
            tokens.iter().map(|token| (*token).to_owned()).collect(),
            store.to_path_buf(),
            tidepool_cli::Runtime {
                stdin: &mut input,
                out: &mut output,
            },
        )
        .expect("cli command");
        String::from_utf8(output).expect("utf8")
    }

    #[test]
    fn help_matches_the_cli_byte_for_byte() {
        let directory = TempDir::new("help");
        let store = directory.store();
        let headless = invoke(&[MARKER, "help"], &store, "").expect("headless help");
        let cli = invoke_cli(&["help"], &store, "");
        assert_eq!(headless, cli);
        assert!(headless.contains("host enroll"));
    }

    #[test]
    fn json_for_a_read_matches_the_cli_byte_for_byte() {
        let pool = ScriptedPool::start(vec![Reply::json(200, HOSTS), Reply::json(200, HOSTS)]);
        let directory = TempDir::new("parity-read");
        let store = signed_in(&directory, pool.url());
        let tokens = ["host", "list", "--json", "--pool", pool.url()];

        // The CLI runs first, then the console's headless skin, against the
        // same pool and the same store. The bytes must be identical.
        let cli = invoke_cli(&tokens, &store, "");
        let headless = invoke(&tokens, &store, "").expect("headless host list");
        assert_eq!(headless, cli);

        let requests = pool.join();
        assert_eq!(requests.len(), 2);
        for request in requests {
            assert_eq!(request.method, "GET");
            assert_eq!(request.path, "/v1/hosts");
        }
    }

    #[test]
    fn json_for_a_mutation_matches_the_cli_byte_for_byte() {
        // The console's gate reads the role, the command then dispatches; the
        // CLI dispatches alone. Same action, same JSON.
        let pool = ScriptedPool::start(vec![
            Reply::json(200, POOL_ME_MEMBER),
            Reply::json(201, DISPATCHED),
            Reply::json(201, DISPATCHED),
        ]);
        let directory = TempDir::new("parity-write");
        let store = signed_in(&directory, pool.url());
        let tokens = [
            "run",
            "dispatch",
            "--project",
            "proj_1",
            "--trigger",
            "on-push",
            "--actor",
            "ops@example.com",
            "--delivery-key",
            "key-1",
            "--json",
            "--pool",
            pool.url(),
        ];

        let headless = invoke(&tokens, &store, "").expect("headless dispatch");
        let cli = invoke_cli(&tokens, &store, "");
        assert_eq!(headless, cli);

        let requests = pool.join();
        let paths: Vec<&str> = requests
            .iter()
            .map(|request| request.path.as_str())
            .collect();
        assert_eq!(paths, ["/v1/pool/me", "/v1/runs", "/v1/runs"]);
    }

    #[test]
    fn a_viewer_is_refused_before_the_pool_is_asked_to_mutate() {
        let pool = ScriptedPool::start(vec![Reply::json(200, POOL_ME_VIEWER)]);
        let directory = TempDir::new("viewer");
        let store = signed_in(&directory, pool.url());

        let error = invoke(
            &[
                "run",
                "dispatch",
                "--project",
                "proj_1",
                "--trigger",
                "on-push",
                "--delivery-key",
                "key-1",
                "--json",
                "--pool",
                pool.url(),
            ],
            &store,
            "",
        )
        .expect_err("a viewer cannot dispatch");

        let message = error.to_string();
        assert!(message.contains("`viewer`"), "{message}");
        assert!(message.contains("change pool state"), "{message}");
        assert!(message.contains("Ask an owner"), "{message}");

        // The role read is the only request: the mutation was never attempted.
        let requests = pool.join();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/v1/pool/me");
    }

    #[test]
    fn a_viewer_can_still_read() {
        let pool = ScriptedPool::start(vec![Reply::json(200, HOSTS)]);
        let directory = TempDir::new("viewer-read");
        let store = signed_in(&directory, pool.url());
        let output = invoke(
            &["host", "list", "--json", "--pool", pool.url()],
            &store,
            "",
        )
        .expect("a viewer may read");
        assert!(output.contains("build-box"), "{output}");
        let requests = pool.join();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, "/v1/hosts");
    }

    #[test]
    fn a_headless_sign_in_is_the_store_the_cli_reads() {
        let pool = ScriptedPool::start(vec![
            Reply::json(200, LOGIN),
            Reply::json(200, SETUP_STATUS),
        ]);
        let directory = TempDir::new("console-login");
        let store = directory.store();

        let printed = invoke(
            &[
                MARKER,
                "login",
                "--pool",
                pool.url(),
                "--email",
                "ops@example.com",
                "--password-stdin",
                "--json",
            ],
            &store,
            "hunter2\n",
        )
        .expect("headless login");
        assert!(printed.contains("ops@example.com"), "{printed}");
        assert!(
            !printed.contains("abcd1234-the-whole-operator-token"),
            "the token must stay redacted: {printed}"
        );

        // The CLI's own store read sees the console's sign-in.
        let cli = CredentialStore::load(store.clone()).expect("store");
        assert_eq!(cli.file().pool.as_deref(), Some(pool.url()));
        assert_eq!(cli.file().email.as_deref(), Some("ops@example.com"));
        assert!(cli.file().token.is_some());

        // And a CLI command with no --pool now talks to that pool.
        let status = invoke_cli(&["pool", "status", "--json"], &store, "");
        assert!(status.contains("needs_setup"), "{status}");
        let requests = pool.join();
        let paths: Vec<&str> = requests
            .iter()
            .map(|request| request.path.as_str())
            .collect();
        assert_eq!(paths, ["/v1/pool/login", "/v1/setup/status"]);
    }

    #[test]
    fn a_cli_sign_in_is_visible_to_the_console_session() {
        let pool = ScriptedPool::start(vec![Reply::json(200, LOGIN)]);
        let directory = TempDir::new("cli-login");
        let store = directory.store();
        invoke_cli(
            &[
                "login",
                "--pool",
                pool.url(),
                "--email",
                "ops@example.com",
                "--password-stdin",
            ],
            &store,
            "hunter2\n",
        );

        let session = Session::load(store).expect("session");
        assert!(session.signed_in());
        assert_eq!(session.pool(), Some(pool.url()));

        // The console's own startup path adopts that file.
        let client = crate::app::testing::ScriptedClient::online("ops@example.com");
        let mut app = crate::app::App::from_credentials(session.file(), Some(Box::new(client)));
        app.start();
        assert_eq!(app.connection(), &crate::app::Connection::Online);
        assert_eq!(app.member(), Some("ops@example.com"));
        pool.join();
    }

    #[test]
    fn no_arguments_is_not_headless() {
        assert!(!requested(&[]));
        assert!(requested(&[MARKER.to_owned()]));
        assert!(requested(&["help".to_owned()]));
    }

    #[test]
    fn every_cli_command_has_a_capability() {
        // One minimal command line per command the CLI's usage text names. If
        // the CLI grows a command, this table is what fails first.
        let samples: [&[&str]; 14] = [
            &["help"],
            &["login", "--email", "a@b.c", "--password-stdin"],
            &["pool", "status"],
            &["host", "list"],
            &["host", "enroll", "--hostname", "build-box"],
            &["host", "grant", "--host", "host_1"],
            &["trigger", "list", "--project", "proj_1"],
            &["trigger", "validate", "--file", "t.json"],
            &[
                "trigger",
                "install",
                "--project",
                "proj_1",
                "--file",
                "t.json",
            ],
            &[
                "run",
                "dispatch",
                "--project",
                "proj_1",
                "--trigger",
                "on-push",
            ],
            &["run", "watch", "--run", "run_1"],
            &["room", "list"],
            &["room", "send", "--room", "room_1", "--body", "hi"],
            &["relay", "follow", "--session", "session_1"],
        ];
        for words in samples {
            let invocation = args::parse(words.iter().map(|word| (*word).to_owned()))
                .expect("the sample parses");
            let capability = capability_for(&invocation.command);
            assert!(
                capability == Capability::Read || capability == Capability::Mutate,
                "{words:?}"
            );
        }

        // The CLI's usage text names exactly these commands, in this order.
        let documented = usage_commands(args::USAGE);
        let expected: Vec<String> = samples
            .iter()
            .filter_map(|words| {
                let mut words = words.iter();
                let first = words.next()?;
                if *first == "help" {
                    return None;
                }
                Some(match words.next() {
                    Some(verb) if !verb.starts_with("--") => format!("{first} {verb}"),
                    _ => (*first).to_owned(),
                })
            })
            .collect();
        assert_eq!(documented, expected);
    }

    #[test]
    fn the_readme_documents_every_command_and_only_those() {
        let usage = usage_commands(args::USAGE);
        let readme = include_str!("../README.md");
        let section = readme
            .split("## Commands both skins share")
            .nth(1)
            .and_then(|rest| rest.split("\n## ").next())
            .expect("README has a command section");
        let documented: Vec<String> = section
            .lines()
            .filter_map(|line| {
                let rest = line.trim().strip_prefix("| `")?;
                let (command, _) = rest.split_once('`')?;
                Some(command.to_owned())
            })
            .collect();
        assert_eq!(documented, usage, "the README table and the CLI diverged");
        for command in &usage {
            let console = format!("`tidepool-console --headless {command}");
            assert!(
                section.contains(&console),
                "README lacks the console form of {command}"
            );
        }
    }

    /// The command spellings in the CLI's usage text, in order.
    fn usage_commands(usage: &str) -> Vec<String> {
        usage
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                let (command, _) = trimmed.split_once("  ")?;
                let command = command.trim();
                (!command.is_empty() && !command.starts_with("--")).then(|| command.to_owned())
            })
            .collect()
    }
}
