//! The `tidepool` command line: a thin client over the Rust SDK.
//!
//! Every command in [`args`] maps to one call into `tidepool-client` and prints
//! the type the SDK returned; `--json` prints that type as JSON and the default
//! prints a short table. The CLI holds no validation, scheduling, paging or
//! deduplication rules of its own: those live in the pool and in the SDK.
//!
//! Credentials live in the store [`credentials`] owns, outside any repository,
//! and are never printed in full (see [`credentials::redact`]).
//!
//! # Layout
//!
//! - [`args`] — the command-line grammar.
//! - [`commands`] — one function per command.
//! - [`credentials`] — the store, its path rules and redaction.
//! - [`output`] — the `--json` and text printers.
//!
//! # Example
//!
//! ```no_run
//! use std::io::Cursor;
//!
//! let mut input = Cursor::new(Vec::new());
//! let mut output = Vec::new();
//! let runtime = tidepool_cli::Runtime {
//!     stdin: &mut input,
//!     out: &mut output,
//! };
//! tidepool_cli::run(
//!     vec!["host".to_owned(), "list".to_owned(), "--json".to_owned()],
//!     runtime,
//! )
//! .expect("host list");
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// `CliError` carries the SDK's error verbatim, which carries the whole problem
// document (status, code, detail, request id) because that is the diagnostic a
// person needs. Boxing it would only move the allocation, as in the SDK.
#![allow(clippy::result_large_err)]

pub mod args;
pub mod commands;
pub mod credentials;
pub mod error;
pub mod output;

use std::io::{Read, Write};
use std::path::PathBuf;

pub use args::{Command, Invocation};
pub use error::{CliError, Result};

/// The two streams one run reads from and writes to.
pub struct Runtime<'a> {
    /// Standard input; `login` reads the password from it.
    pub stdin: &'a mut dyn Read,
    /// Standard output; every command writes its result here.
    pub out: &'a mut dyn Write,
}

/// Run one command line against the default credential store.
///
/// The store path follows [`credentials::default_path`]: `TIDEPOOL_HOME`, then
/// `XDG_CONFIG_HOME`, then `$HOME/.config`, so nothing is written to a
/// repository.
pub fn run(tokens: Vec<String>, runtime: Runtime<'_>) -> Result<()> {
    let path = credentials::default_path(|name| std::env::var(name).ok())?;
    run_with_path(tokens, path, runtime)
}

/// Run one command line against an explicit store path.
///
/// This is the entry point tests and other tools use: the path is required
/// rather than read from the environment, and it is still refused if it sits
/// inside a repository when a command writes.
pub fn run_with_path(tokens: Vec<String>, store_path: PathBuf, runtime: Runtime<'_>) -> Result<()> {
    let invocation = args::parse(tokens)?;
    let mut store = credentials::CredentialStore::load(store_path)?;
    commands::execute(&invocation, &mut store, runtime.stdin, runtime.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_needs_no_pool_and_no_credentials() {
        let mut input = std::io::Cursor::new(Vec::new());
        let mut output = Vec::new();
        let store = std::env::temp_dir().join("tidepool-cli-help-store.json");
        run_with_path(
            vec!["help".to_owned()],
            store,
            Runtime {
                stdin: &mut input,
                out: &mut output,
            },
        )
        .expect("help");
        let text = String::from_utf8(output).expect("utf8");
        assert!(text.contains("host enroll"));
        assert!(text.contains("relay follow"));
    }

    #[test]
    fn a_command_without_a_pool_says_what_is_missing() {
        let mut input = std::io::Cursor::new(Vec::new());
        let mut output = Vec::new();
        let store = std::env::temp_dir().join("tidepool-cli-no-pool-store.json");
        let error = run_with_path(
            vec!["host".to_owned(), "list".to_owned()],
            store,
            Runtime {
                stdin: &mut input,
                out: &mut output,
            },
        )
        .expect_err("no pool");
        assert!(error.to_string().contains("--pool"));
    }
}
