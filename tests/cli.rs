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
        .stdout(predicate::str::contains("inspect"))
        .stdout(predicate::str::contains("logs"))
        .stdout(predicate::str::contains("pause"))
        .stdout(predicate::str::contains("resume"))
        .stdout(predicate::str::contains("cancel"));
}

/// Exact inspection deliberately rejects prefixes so concurrent run volume
/// cannot change which run is inspected.
#[test]
fn inspect_requires_full_uuid() {
    cargo_bin_cmd!("pfp")
        .args(["inspect", "e130c152", "--json"])
        .env("PREFECT_API_URL", "http://127.0.0.1:1")
        .env_remove("PREFECT_API_AUTH_STRING")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains(
            "inspect requires a full flow run UUID",
        ));
}

/// Malformed JSON piped via `--params-file -` is rejected with exit code 2,
/// before any config or network work. Guards that the stdin branch is wired
/// and read, and that the payload error surfaces ahead of config loading.
#[test]
fn run_params_file_stdin_malformed_json_rejected() {
    cargo_bin_cmd!("pfp")
        .args(["run", "some-deploy", "--params-file", "-"])
        .write_stdin("not json")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("Invalid JSON in params payload"));
}

/// --tail is accepted as an alias for --follow on `pfp logs`, since "tail"
/// is the more familiar term for this behavior (tail -f).
#[test]
fn logs_tail_is_alias_for_follow() {
    cargo_bin_cmd!("pfp")
        .args(["logs", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--tail"));
}

/// --tail is parsed as a real flag (not just documented): it reaches
/// resolve-flow-run logic rather than erroring as an unrecognized argument.
#[test]
fn logs_tail_flag_is_accepted_by_parser() {
    cargo_bin_cmd!("pfp")
        .args(["logs", "some-run", "--tail"])
        .env("PREFECT_API_URL", "http://127.0.0.1:1")
        .env_remove("PREFECT_API_AUTH_STRING")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument").not());
}

/// Valid JSON piped via `--params-file -` is read exactly once: it parses
/// successfully and execution proceeds past parsing to the (unreachable) API,
/// rather than failing with an empty-stdin JSON/EOF error. Regression guard
/// for the stdin double-read bug. PREFECT_API_URL points at a closed port so
/// the run fails fast on connection refused instead of hitting a real server.
#[test]
fn run_params_file_stdin_valid_json_read_once() {
    cargo_bin_cmd!("pfp")
        .args(["run", "some-deploy", "--params-file", "-"])
        .env("PREFECT_API_URL", "http://127.0.0.1:1")
        .env_remove("PREFECT_API_AUTH_STRING")
        .write_stdin(r#"{"config": {"action": "plan"}}"#)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid JSON").not())
        .stderr(predicate::str::contains("EOF").not());
}
