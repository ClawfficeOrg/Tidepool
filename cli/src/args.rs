//! The command-line grammar.
//!
//! No argument-parsing crate: the flag table below is the whole grammar, so an
//! unknown flag is an error the CLI owns and every command's flags are visible
//! in one place.
//!
//! Every command accepts `--json` (the SDK type as JSON) and `--pool <url>`
//! (the pool to talk to, overriding the stored one).

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{CliError, Result};

/// The help text `tidepool help` prints.
pub const USAGE: &str = "\
tidepool — a thin client over the Tidepool hub

usage:
  tidepool <group> <verb> [flags]

commands:
  login                              sign in and remember the token
  pool status                        report whether the pool needs setup
  host list                          list the hosts in the pool
  host enroll                        register a host and exchange its token
  host grant                         grant or revoke host.execute
  trigger list                       read a project's active authored revision
  trigger validate                   validate one trigger document
  trigger install                    install or replace one trigger
  run dispatch                       dispatch a manual run
  run watch                          follow a run until it finishes
  room list                          list the rooms this member can see
  room send                          post a message to a room
  relay follow                       follow a session's relayed events

common flags:
  --json                             print the SDK's type as JSON
  --pool <url>                       the pool to talk to
  --help                             this text
";

/// One accepted flag.
#[derive(Debug, Clone, Copy)]
struct FlagSpec {
    name: &'static str,
    takes_value: bool,
}

const FLAGS: &[FlagSpec] = &[
    FlagSpec {
        name: "json",
        takes_value: false,
    },
    FlagSpec {
        name: "pool",
        takes_value: true,
    },
    FlagSpec {
        name: "help",
        takes_value: false,
    },
    FlagSpec {
        name: "email",
        takes_value: true,
    },
    FlagSpec {
        name: "password-stdin",
        takes_value: false,
    },
    FlagSpec {
        name: "hostname",
        takes_value: true,
    },
    FlagSpec {
        name: "display-name",
        takes_value: true,
    },
    FlagSpec {
        name: "host",
        takes_value: true,
    },
    FlagSpec {
        name: "revoke",
        takes_value: false,
    },
    FlagSpec {
        name: "project",
        takes_value: true,
    },
    FlagSpec {
        name: "file",
        takes_value: true,
    },
    FlagSpec {
        name: "trigger",
        takes_value: true,
    },
    FlagSpec {
        name: "actor",
        takes_value: true,
    },
    FlagSpec {
        name: "delivery-key",
        takes_value: true,
    },
    FlagSpec {
        name: "input",
        takes_value: true,
    },
    FlagSpec {
        name: "revision",
        takes_value: true,
    },
    FlagSpec {
        name: "run",
        takes_value: true,
    },
    FlagSpec {
        name: "interval-ms",
        takes_value: true,
    },
    FlagSpec {
        name: "max-polls",
        takes_value: true,
    },
    FlagSpec {
        name: "room",
        takes_value: true,
    },
    FlagSpec {
        name: "body",
        takes_value: true,
    },
    FlagSpec {
        name: "reply-to",
        takes_value: true,
    },
    FlagSpec {
        name: "session",
        takes_value: true,
    },
    FlagSpec {
        name: "after",
        takes_value: true,
    },
    FlagSpec {
        name: "limit",
        takes_value: true,
    },
    FlagSpec {
        name: "max-pages",
        takes_value: true,
    },
];

/// The command a run was asked for. One variant per command the task names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Print the usage text.
    Help,
    /// Sign in and remember the token.
    Login {
        /// The member's email.
        email: Option<String>,
        /// Whether the password arrives on stdin.
        password_stdin: bool,
    },
    /// Report whether the pool still needs first-run setup.
    PoolStatus,
    /// List the hosts in the pool.
    HostList,
    /// Register a host (unless `--host` names one) and exchange its token.
    HostEnroll {
        /// The machine's hostname, used when registering.
        hostname: Option<String>,
        /// A display name for a newly registered host.
        display_name: Option<String>,
        /// An existing host's id, which skips registration.
        host: Option<String>,
    },
    /// Grant `host.execute` to a host, or revoke it with `--revoke`.
    HostGrant {
        /// The host's id.
        host: String,
        /// Revoke instead of grant.
        revoke: bool,
    },
    /// Read a project's active authored revision, which is where the pool keeps
    /// its trigger documents.
    TriggerList {
        /// The project's id.
        project: String,
    },
    /// Validate one trigger document without writing pool state.
    TriggerValidate {
        /// File holding the trigger document as JSON.
        file: String,
    },
    /// Install or replace one trigger.
    TriggerInstall {
        /// The project the trigger belongs to.
        project: String,
        /// File holding the trigger document as JSON.
        file: String,
    },
    /// Dispatch a manual run.
    RunDispatch {
        /// The project's id.
        project: String,
        /// The trigger's name.
        trigger: String,
        /// The actor; defaults to the signed-in member.
        actor: Option<String>,
        /// Idempotency key; generated when absent.
        delivery_key: Option<String>,
        /// Free-form request text.
        input: Option<String>,
        /// The configuration revision to run; the active one when absent.
        revision: Option<String>,
    },
    /// Follow a run until the pool reports a terminal status.
    RunWatch {
        /// The run's id.
        run: String,
        /// Milliseconds between polls.
        interval_ms: u64,
        /// Stop after this many polls even if the run has not finished.
        max_polls: Option<u64>,
    },
    /// List the rooms visible to the caller.
    RoomList,
    /// Post a message to a room.
    RoomSend {
        /// The room's id.
        room: String,
        /// The message body.
        body: String,
        /// The message this one replies to.
        reply_to: Option<String>,
    },
    /// Follow a session's relayed events, page by page.
    RelayFollow {
        /// The session's id.
        session: String,
        /// Resume after this sequence.
        after: Option<u64>,
        /// Page size to ask the pool for.
        limit: Option<u64>,
        /// Stop after this many pages.
        max_pages: Option<usize>,
    },
}

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// What to run.
    pub command: Command,
    /// Whether to print the SDK's type as JSON.
    pub json: bool,
    /// The pool overrides the stored one when set.
    pub pool: Option<String>,
}

const COMMON_FLAGS: [&str; 3] = ["json", "pool", "help"];

/// Parse one command line, without the program name.
pub fn parse<I>(tokens: I) -> Result<Invocation>
where
    I: IntoIterator<Item = String>,
{
    let mut tokens = tokens.into_iter();
    let mut positionals: Vec<String> = Vec::new();
    let mut values: BTreeMap<String, String> = BTreeMap::new();
    let mut switches: BTreeSet<String> = BTreeSet::new();

    while let Some(token) = tokens.next() {
        if let Some(rest) = token.strip_prefix("--") {
            let (name, inline) = match rest.split_once('=') {
                Some((name, value)) => (name, Some(value.to_owned())),
                None => (rest, None),
            };
            let spec = FLAGS.iter().find(|spec| spec.name == name).ok_or_else(|| {
                CliError::Usage(format!("unknown flag `--{name}`; run `tidepool help`"))
            })?;
            if spec.takes_value {
                let value = match inline {
                    Some(value) => value,
                    None => tokens
                        .next()
                        .ok_or_else(|| CliError::Usage(format!("`--{name}` needs a value")))?,
                };
                if values.insert(name.to_owned(), value).is_some() {
                    return Err(CliError::Usage(format!("`--{name}` was given twice")));
                }
            } else {
                if inline.is_some() {
                    return Err(CliError::Usage(format!("`--{name}` takes no value")));
                }
                if !switches.insert(name.to_owned()) {
                    return Err(CliError::Usage(format!("`--{name}` was given twice")));
                }
            }
            continue;
        }
        if positionals.len() >= 2 {
            return Err(CliError::Usage(format!("unexpected argument `{token}`")));
        }
        positionals.push(token);
    }

    // `--help` anywhere wins over the command it was attached to, so a person
    // can ask for help without spelling the command's required flags.
    if switches.contains("help") {
        return Ok(Invocation {
            command: Command::Help,
            json: false,
            pool: values.get("pool").cloned(),
        });
    }

    let mut flags = Flags { values, switches };
    let json = flags.switch("json");
    let pool = flags.value("pool");

    let group = positionals.first().map(String::as_str).unwrap_or("help");
    let verb = positionals.get(1).map(String::as_str);

    let command = match (group, verb) {
        ("help", _) => {
            flags.allow(&[])?;
            Command::Help
        }
        ("login", None) => {
            flags.allow(&["email", "password-stdin"])?;
            Command::Login {
                email: flags.value("email"),
                password_stdin: flags.switch("password-stdin"),
            }
        }
        ("pool", Some("status")) => {
            flags.allow(&[])?;
            Command::PoolStatus
        }
        ("host", Some("list")) => {
            flags.allow(&[])?;
            Command::HostList
        }
        ("host", Some("enroll")) => {
            flags.allow(&["hostname", "display-name", "host"])?;
            Command::HostEnroll {
                hostname: flags.value("hostname"),
                display_name: flags.value("display-name"),
                host: flags.value("host"),
            }
        }
        ("host", Some("grant")) => {
            flags.allow(&["host", "revoke"])?;
            Command::HostGrant {
                host: flags.required("host", "host grant")?,
                revoke: flags.switch("revoke"),
            }
        }
        ("trigger", Some("list")) => {
            flags.allow(&["project"])?;
            Command::TriggerList {
                project: flags.required("project", "trigger list")?,
            }
        }
        ("trigger", Some("validate")) => {
            flags.allow(&["file"])?;
            Command::TriggerValidate {
                file: flags.required("file", "trigger validate")?,
            }
        }
        ("trigger", Some("install")) => {
            flags.allow(&["project", "file"])?;
            Command::TriggerInstall {
                project: flags.required("project", "trigger install")?,
                file: flags.required("file", "trigger install")?,
            }
        }
        ("run", Some("dispatch")) => {
            flags.allow(&[
                "project",
                "trigger",
                "actor",
                "delivery-key",
                "input",
                "revision",
            ])?;
            Command::RunDispatch {
                project: flags.required("project", "run dispatch")?,
                trigger: flags.required("trigger", "run dispatch")?,
                actor: flags.value("actor"),
                delivery_key: flags.value("delivery-key"),
                input: flags.value("input"),
                revision: flags.value("revision"),
            }
        }
        ("run", Some("watch")) => {
            flags.allow(&["run", "interval-ms", "max-polls"])?;
            Command::RunWatch {
                run: flags.required("run", "run watch")?,
                interval_ms: flags.number("interval-ms")?.unwrap_or(500),
                max_polls: flags.number("max-polls")?,
            }
        }
        ("room", Some("list")) => {
            flags.allow(&[])?;
            Command::RoomList
        }
        ("room", Some("send")) => {
            flags.allow(&["room", "body", "reply-to"])?;
            Command::RoomSend {
                room: flags.required("room", "room send")?,
                body: flags.required("body", "room send")?,
                reply_to: flags.value("reply-to"),
            }
        }
        ("relay", Some("follow")) => {
            flags.allow(&["session", "after", "limit", "max-pages"])?;
            Command::RelayFollow {
                session: flags.required("session", "relay follow")?,
                after: flags.number("after")?,
                limit: flags.number("limit")?,
                max_pages: flags.number("max-pages")?,
            }
        }
        (group, verb) => {
            let spelled = match verb {
                Some(verb) => format!("{group} {verb}"),
                None => group.to_owned(),
            };
            return Err(CliError::Usage(format!(
                "unknown command `{spelled}`; run `tidepool help` to see the commands"
            )));
        }
    };

    Ok(Invocation {
        command,
        json,
        pool,
    })
}

struct Flags {
    values: BTreeMap<String, String>,
    switches: BTreeSet<String>,
}

impl Flags {
    fn value(&mut self, name: &str) -> Option<String> {
        self.values.remove(name)
    }

    fn switch(&mut self, name: &str) -> bool {
        self.switches.remove(name)
    }

    fn required(&mut self, name: &str, command: &str) -> Result<String> {
        self.value(name)
            .ok_or_else(|| CliError::Usage(format!("`{command}` needs `--{name}`")))
    }

    fn number<T>(&mut self, name: &str) -> Result<Option<T>>
    where
        T: std::str::FromStr,
    {
        match self.value(name) {
            Some(value) => value.parse::<T>().map(Some).map_err(|_| {
                CliError::Usage(format!("`--{name}` must be a number, not `{value}`"))
            }),
            None => Ok(None),
        }
    }

    /// Reject any flag the command did not claim.
    fn allow(&mut self, allowed: &[&str]) -> Result<()> {
        if let Some(name) = self.values.keys().chain(self.switches.iter()).find(|name| {
            !allowed.contains(&name.as_str()) && !COMMON_FLAGS.contains(&name.as_str())
        }) {
            return Err(CliError::Usage(format!(
                "`--{name}` does not apply to this command"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(words: &[&str]) -> Result<Invocation> {
        parse(words.iter().map(|word| (*word).to_owned()))
    }

    #[test]
    fn every_command_accepts_json() {
        let commands: [&[&str]; 13] = [
            &["login", "--email", "a@b.c", "--password-stdin"],
            &["pool", "status"],
            &["host", "list"],
            &["host", "enroll", "--hostname", "build-box"],
            &["host", "grant", "--host", "host_1"],
            &["trigger", "list", "--project", "proj_1"],
            &["trigger", "validate", "--file", "trigger.json"],
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
            &["room", "send", "--room", "room_1", "--body", "hello"],
            &["relay", "follow", "--session", "session_1"],
        ];
        for words in commands {
            let mut with_json: Vec<&str> = words.to_vec();
            with_json.push("--json");
            let invocation = parse_words(&with_json).unwrap_or_else(|error| {
                panic!("{words:?} must accept --json: {error}");
            });
            assert!(invocation.json, "{words:?} must set json");
        }
    }

    #[test]
    fn a_pool_override_is_global() {
        let invocation =
            parse_words(&["--pool", "https://pool.example.com", "host", "list"]).expect("parse");
        assert_eq!(invocation.pool.as_deref(), Some("https://pool.example.com"));
        assert_eq!(invocation.command, Command::HostList);
    }

    #[test]
    fn values_can_be_inline_or_separate() {
        let inline = parse_words(&["host", "grant", "--host=host_1"]).expect("parse");
        let separate = parse_words(&["host", "grant", "--host", "host_1"]).expect("parse");
        assert_eq!(inline, separate);
        match separate.command {
            Command::HostGrant { host, revoke } => {
                assert_eq!(host, "host_1");
                assert!(!revoke);
            }
            other => panic!("unexpected command {other:?}"),
        }
    }

    #[test]
    fn dispatch_defaults_and_flags() {
        let invocation = parse_words(&[
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
            "--input",
            "fix it",
            "--revision",
            "rev_1",
        ])
        .expect("parse");
        assert_eq!(
            invocation.command,
            Command::RunDispatch {
                project: "proj_1".to_owned(),
                trigger: "on-push".to_owned(),
                actor: Some("ops@example.com".to_owned()),
                delivery_key: Some("key-1".to_owned()),
                input: Some("fix it".to_owned()),
                revision: Some("rev_1".to_owned()),
            }
        );
    }

    #[test]
    fn watch_and_follow_parse_numbers_and_defaults() {
        let watch =
            parse_words(&["run", "watch", "--run", "run_1", "--interval-ms", "0"]).expect("parse");
        assert_eq!(
            watch.command,
            Command::RunWatch {
                run: "run_1".to_owned(),
                interval_ms: 0,
                max_polls: None,
            }
        );

        let follow = parse_words(&[
            "relay",
            "follow",
            "--session",
            "session_1",
            "--after",
            "9",
            "--limit",
            "50",
            "--max-pages",
            "2",
        ])
        .expect("parse");
        assert_eq!(
            follow.command,
            Command::RelayFollow {
                session: "session_1".to_owned(),
                after: Some(9),
                limit: Some(50),
                max_pages: Some(2),
            }
        );
    }

    #[test]
    fn a_missing_required_flag_names_the_command() {
        let error = parse_words(&["room", "send", "--room", "room_1"]).expect_err("body");
        assert!(error.to_string().contains("room send"));
        assert!(error.to_string().contains("--body"));
    }

    #[test]
    fn an_unknown_flag_is_refused() {
        let error = parse_words(&["host", "list", "--bogus"]).expect_err("flag");
        assert!(error.to_string().contains("--bogus"));
    }

    #[test]
    fn a_flag_that_does_not_apply_is_refused() {
        let error =
            parse_words(&["pool", "status", "--email", "ops@example.com"]).expect_err("flag");
        assert!(error.to_string().contains("--email"));
    }

    #[test]
    fn an_unknown_command_is_refused() {
        let error = parse_words(&["host", "frobnicate"]).expect_err("command");
        assert!(error.to_string().contains("host frobnicate"));
    }

    #[test]
    fn a_value_flag_without_a_value_is_refused() {
        let error = parse_words(&["host", "grant", "--host"]).expect_err("value");
        assert!(error.to_string().contains("--host"));
    }

    #[test]
    fn a_switch_cannot_carry_a_value() {
        let error = parse_words(&["host", "list", "--json=yes"]).expect_err("switch");
        assert!(error.to_string().contains("takes no value"));
    }

    #[test]
    fn duplicate_flags_are_refused() {
        let error =
            parse_words(&["host", "grant", "--host", "a", "--host", "b"]).expect_err("duplicate");
        assert!(error.to_string().contains("twice"));
    }

    #[test]
    fn no_arguments_prints_help() {
        assert_eq!(parse_words(&[]).expect("parse").command, Command::Help);
        assert_eq!(
            parse_words(&["--help"]).expect("parse").command,
            Command::Help
        );
        assert_eq!(
            parse_words(&["host", "list", "--help"])
                .expect("parse")
                .command,
            Command::Help
        );
    }
}
