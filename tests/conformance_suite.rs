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

/// Run one Python script in the repository and return its output.
fn run_python(script: &str, arguments: &[&str]) -> Output {
    let python = std::env::var("PYTHON").unwrap_or_else(|_| "python3".to_string());
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
