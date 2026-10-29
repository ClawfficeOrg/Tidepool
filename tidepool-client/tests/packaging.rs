//! Packaging smoke test.
//!
//! `scripts/test-packaging.sh` packages a stub executable for all three
//! platforms and checks the shape of each artifact: the macOS bundle and zip,
//! the Windows zip and the Linux tarball with its desktop entry and icons. The
//! test runs the script where the runner allows it and skips on a platform
//! without the POSIX tools it needs.

use std::path::Path;
use std::process::Command;

#[test]
fn every_platform_artifact_has_an_installable_shape() {
    if cfg!(windows) {
        // The script is POSIX shell; Windows packaging is exercised by
        // `scripts/package.sh` under Git Bash or WSL.
        return;
    }

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let script = manifest.join("scripts/test-packaging.sh");
    if !script.is_file() {
        return;
    }

    let output = Command::new("bash")
        .arg(&script)
        .output()
        .expect("run the packaging smoke script");
    assert!(
        output.status.success(),
        "packaging smoke failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("test-packaging: PASS"),
        "the smoke script did not reach its final assertion"
    );
}
