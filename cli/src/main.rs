//! The `tidepool` binary.
//!
//! Parsing, dispatch and printing all live in [`tidepool_cli`]; this file binds
//! the process's streams and turns an error into a non-zero exit code.

use std::io::Write as _;
use std::process::ExitCode;

fn main() -> ExitCode {
    let tokens: Vec<String> = std::env::args().skip(1).collect();
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();

    let result = tidepool_cli::run(
        tokens,
        tidepool_cli::Runtime {
            stdin: &mut stdin,
            out: &mut stdout,
        },
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // The error's Display never carries a credential.
            let _ = writeln!(stderr, "tidepool: {error}");
            ExitCode::FAILURE
        }
    }
}
