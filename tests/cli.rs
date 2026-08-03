use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn write_profiles(home: &std::path::Path, contents: &str) {
    let prefect_dir = home.join(".prefect");
    std::fs::create_dir_all(&prefect_dir).unwrap();
    std::fs::write(prefect_dir.join("profiles.toml"), contents).unwrap();
}

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
        .stdout(predicate::str::contains("schedule-resume"))
        .stdout(predicate::str::contains("cancel"));
}

#[test]
fn explicit_server_selects_named_profile_instead_of_environment_url() {
    let mut selected_server = mockito::Server::new();
    let selected_request = selected_server
        .mock("POST", "/deployments/filter")
        .match_header("authorization", "Basic bm9ybWE6c2VjcmV0")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("[]")
        .expect(1)
        .create();
    let home = tempfile::tempdir().unwrap();
    write_profiles(
        home.path(),
        &format!(
            r#"active = "default"

[profiles.default]
PREFECT_API_URL = "http://127.0.0.1:1"
PREFECT_API_AUTH_STRING = "default:secret"

[profiles.norma]
PREFECT_API_URL = "{}"
PREFECT_API_AUTH_STRING = "norma:secret"
"#,
            selected_server.url()
        ),
    );

    cargo_bin_cmd!("pfp")
        .args(["--server", "norma", "ls", "--json"])
        .env("HOME", home.path())
        .env("PREFECT_API_URL", "http://127.0.0.1:1")
        .env("PREFECT_API_AUTH_STRING", "environment:wrong")
        .assert()
        .success();

    selected_request.assert();
}

#[test]
fn active_profile_auth_is_used_when_environment_is_absent() {
    let mut selected_server = mockito::Server::new();
    let selected_request = selected_server
        .mock("POST", "/deployments/filter")
        .match_header("authorization", "Basic cGxlaWFkZXM6c2VjcmV0")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("[]")
        .expect(1)
        .create();
    let home = tempfile::tempdir().unwrap();
    write_profiles(
        home.path(),
        &format!(
            r#"active = "pleiades"

[profiles.pleiades]
PREFECT_API_URL = "{}"
PREFECT_API_AUTH_STRING = "pleiades:secret"
"#,
            selected_server.url()
        ),
    );

    cargo_bin_cmd!("pfp")
        .args(["ls", "--json"])
        .env("HOME", home.path())
        .env_remove("PREFECT_API_URL")
        .env_remove("PREFECT_API_AUTH_STRING")
        .assert()
        .success();

    selected_request.assert();
}

#[test]
fn unknown_explicit_server_is_a_hard_error_without_fallback() {
    let mut fallback_server = mockito::Server::new();
    let fallback_request = fallback_server
        .mock("POST", "/deployments/filter")
        .expect(0)
        .create();
    let home = tempfile::tempdir().unwrap();
    write_profiles(
        home.path(),
        &format!(
            r#"active = "default"

[profiles.default]
PREFECT_API_URL = "{}"
"#,
            fallback_server.url()
        ),
    );

    cargo_bin_cmd!("pfp")
        .args(["--server", "missing", "ls"])
        .env("HOME", home.path())
        .env("PREFECT_API_URL", fallback_server.url())
        .env_remove("PREFECT_API_AUTH_STRING")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("Profile 'missing' not found"));

    fallback_request.assert();
}

#[test]
fn schedule_resume_requires_a_query() {
    cargo_bin_cmd!("pfp")
        .arg("schedule-resume")
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("<QUERY>"));
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

#[test]
fn pool_assert_idle_json_succeeds_with_stable_result() {
    let mut server = mockito::Server::new();
    let pool = server
        .mock("GET", "/work_pools/docker-secure")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"name":"docker-secure","is_paused":true,"status":"PAUSED"}"#)
        .expect(1)
        .create();
    let count = server
        .mock("POST", "/flow_runs/count")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("0")
        .expect(1)
        .create();

    cargo_bin_cmd!("pfp")
        .args(["pool", "assert-idle", "docker-secure", "--json"])
        .env("PREFECT_API_URL", server.url())
        .env_remove("PREFECT_API_AUTH_STRING")
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""pool": "docker-secure""#))
        .stdout(predicate::str::contains(r#""idle": true"#))
        .stdout(predicate::str::contains(r#""nonterminal_run_count": 0"#));

    pool.assert();
    count.assert();
}

#[test]
fn pool_assert_idle_json_fails_with_count() {
    let mut server = mockito::Server::new();
    let pool = server
        .mock("GET", "/work_pools/docker-secure")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"name":"docker-secure","is_paused":true,"status":"PAUSED"}"#)
        .expect(1)
        .create();
    let count = server
        .mock("POST", "/flow_runs/count")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("2")
        .expect(1)
        .create();

    cargo_bin_cmd!("pfp")
        .args(["pool", "assert-idle", "docker-secure", "--json"])
        .env("PREFECT_API_URL", server.url())
        .env_remove("PREFECT_API_AUTH_STRING")
        .assert()
        .failure()
        .code(2)
        .stdout(predicate::str::contains(r#""pool": "docker-secure""#))
        .stdout(predicate::str::contains(r#""idle": false"#))
        .stdout(predicate::str::contains(r#""nonterminal_run_count": 2"#))
        .stderr(predicate::str::contains("is not idle"));

    pool.assert();
    count.assert();
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
