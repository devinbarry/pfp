use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

/// Verify that running pfp with no arguments shows help/usage info.
#[test]
fn no_args_shows_usage() {
    cargo_bin_cmd!("pfp")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

/// Verify that --version prints the version.
#[test]
fn version_flag() {
    cargo_bin_cmd!("pfp")
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

/// Verify that --help lists all subcommands.
#[test]
fn help_lists_subcommands() {
    cargo_bin_cmd!("pfp")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("ls"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("runs"))
        .stdout(predicate::str::contains("logs"))
        .stdout(predicate::str::contains("pause"))
        .stdout(predicate::str::contains("resume"))
        .stdout(predicate::str::contains("cancel"));
}
