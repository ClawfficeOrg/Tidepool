//! Guards the cross-SDK conformance suite (`sdk/conformance`).
//!
//! The suite itself runs in the `conformance` CI job against a real pool. This
//! test is the local half of the gate: it proves the harness loaded, the
//! scenario list parses, and the verdict logic still fails a binding that
//! diverges. It needs no database, no Redis and no network.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Repository root, from this test crate's manifest directory.
fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// True when `program` is a working Python 3.
///
/// On Windows `python3` resolves to the Microsoft Store stub, which exists on
/// `PATH` but exits with an error instead of running anything. Probing the
/// version is the only reliable way to tell that apart from a real interpreter.
fn is_python_3(program: &str) -> bool {
    match Command::new(program).arg("--version").output() {
        Ok(output) if output.status.success() => {
            // Python 3.4+ writes the version to stdout; older builds and some
            // wrappers use stderr, so accept either.
            let banner = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            banner.trim_start().starts_with("Python 3")
        }
        _ => false,
    }
}

/// Resolve a usable Python 3 interpreter.
///
/// `PYTHON` wins when set, which is how CI pins the interpreter. Otherwise the
/// usual names are probed in turn, then `py -3`, so the gate runs on Linux,
/// macOS and Windows alike.
fn python() -> String {
    if let Ok(configured) = std::env::var("PYTHON") {
        return configured;
    }
    for candidate in ["python3", "python"] {
        if is_python_3(candidate) {
            return candidate.to_string();
        }
    }
    if is_python_3("py") {
        return "py".to_string();
    }
    panic!("no Python 3 interpreter found; set PYTHON to one to run the conformance suite");
}

/// Run one Python script in the repository and return its output.
fn run_python(script: &str, arguments: &[&str]) -> Output {
    let python = python();
    Command::new(&python)
        .arg(script)
        .args(arguments)
        .current_dir(repository())
        .output()
        .unwrap_or_else(|error| {
            panic!("cannot run {python} {script}: {error}; Python 3 is required for the conformance suite")
        })
}

/// Render stdout and stderr for a failure message.
fn describe(output: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn the_harness_selftest_passes() {
    let output = run_python("sdk/conformance/selftest.py", &[]);
    assert!(
        output.status.success(),
        "conformance selftest failed:\n{}",
        describe(&output)
    );
}

#[test]
fn every_shipped_scenario_loads_and_names_all_three_bindings() {
    let output = run_python(
        "sdk/conformance/run_conformance.py",
        &[
            "--list",
            "--vars",
            "sdk/conformance/fixtures/variables.json",
        ],
    );
    assert!(
        output.status.success(),
        "the scenario list does not load:\n{}",
        describe(&output)
    );
    let listing = String::from_utf8_lossy(&output.stdout);
    for binding in ["python (Python)", "typescript (TypeScript)", "rust (Rust)"] {
        assert!(
            listing.contains(binding),
            "the manifest no longer lists {binding}:\n{listing}"
        );
    }
    let scenarios = listing
        .lines()
        .skip_while(|line| *line != "scenarios:")
        .skip(1)
        .filter(|line| line.trim_start().starts_with(|c: char| c.is_alphanumeric()))
        .count();
    assert!(
        scenarios >= 8,
        "expected at least 8 scenarios, listed {scenarios}:\n{listing}"
    );
}

#[test]
fn the_suite_refuses_to_run_without_a_pool() {
    let output = run_python("sdk/conformance/run_conformance.py", &[]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "running without --base-url must be a suite error:\n{}",
        describe(&output)
    );
}
