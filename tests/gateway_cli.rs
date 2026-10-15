use assert_cmd::{cargo::cargo_bin_cmd, Command};
use predicates::prelude::*;
use serde_json::json;

const PLAN: &str = "66666666-7777-8888-9999-000000000000";

fn command(server: &mockito::Server, home: &tempfile::TempDir) -> Command {
    let mut cmd = cargo_bin_cmd!("pfp");
    cmd.env("HOME", home.path())
        .env("PREFECT_API_URL", format!("{}/api/", server.url()))
        .env("PREFECT_API_AUTH_STRING", "test:test");
    cmd
}

#[test]
fn plans_outputs_gateway_records_as_json_and_table() {
    let mut server = mockito::Server::new();
    let home = tempfile::tempdir().unwrap();
    let plans = json!([{
        "id": PLAN, "target": "router", "status": "approved", "added": 3, "removed": 1,
        "received_at": "2026-10-14T22:30:00Z", "apply_state": {"name": "Completed"}
    }]);
    let request = server
        .mock("GET", "/gateway/plans")
        .match_header("authorization", "Basic dGVzdDp0ZXN0")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(plans.to_string())
        .expect(2)
        .create();

    let result = command(&server, &home)
        .args(["plans", "--json"])
        .assert()
        .success();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.get_output().stdout).unwrap(),
        plans
    );
    command(&server, &home)
        .arg("plans")
        .assert()
        .success()
        .stdout(predicate::str::contains(PLAN))
        .stdout(predicate::str::contains("+3/-1"))
        .stdout(predicate::str::contains("Completed"));
    request.assert();
}

#[test]
fn policy_outputs_cedar_policy_as_json_and_text() {
    let mut server = mockito::Server::new();
    let home = tempfile::tempdir().unwrap();
    let policy = json!({
        "active": {"digest": "abcdef0123456789", "commit": "1234567890abcdef",
                   "activated_by": "operator", "activated_at": "2026-10-14T22:00:00Z"},
        "policies": [{"id": "agents-read", "effect": "permit", "template": false,
                      "description": "Agents may read Prefect state."}],
        "proposal": null
    });
    let request = server
        .mock("GET", "/gateway/policy")
        .match_header("authorization", "Basic dGVzdDp0ZXN0")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(policy.to_string())
        .expect(2)
        .create();

    let result = command(&server, &home)
        .args(["policy", "--json"])
        .assert()
        .success();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.get_output().stdout).unwrap(),
        policy
    );
    command(&server, &home)
        .arg("policy")
        .assert()
        .success()
        .stdout(predicate::str::contains("Enforcing policy abcdef012345"))
        .stdout(predicate::str::contains("Allowed:"))
        .stdout(predicate::str::contains("Agents may read Prefect state."));
    request.assert();
}

#[test]
fn approve_and_reject_decide_gateway_plans_without_resuming_runs() {
    for (verb, response, message) in [
        (
            "approve",
            r#"{"status":"approved","apply_flow_run_id":"apply-1"}"#,
            "gateway started apply run apply-1",
        ),
        ("reject", r#"{"status":"rejected"}"#, "Rejected router plan"),
    ] {
        let mut server = mockito::Server::new();
        let home = tempfile::tempdir().unwrap();
        let lookup = server
            .mock("GET", format!("/gateway/plans/{PLAN}").as_str())
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"target":"router","status":"pending"}"#)
            .create();
        let decide = server
            .mock("POST", format!("/gateway/plans/{PLAN}/{verb}").as_str())
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(response)
            .create();
        let resume = server
            .mock("POST", format!("/api/flow_runs/{PLAN}/resume").as_str())
            .expect(0)
            .create();
        command(&server, &home)
            .args([verb, PLAN])
            .assert()
            .success()
            .stderr(predicate::str::contains(message));
        lookup.assert();
        decide.assert();
        resume.assert();
    }
}

#[test]
fn gateway_decision_denial_exits_three_without_falling_back() {
    let mut server = mockito::Server::new();
    let home = tempfile::tempdir().unwrap();
    let lookup = server
        .mock("GET", format!("/gateway/plans/{PLAN}").as_str())
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"target":"router","status":"pending"}"#)
        .create();
    let decide = server.mock("POST", format!("/gateway/plans/{PLAN}/approve").as_str())
        .with_status(403).with_header("content-type", "application/json")
        .with_body(r#"{"detail":"Denied by gateway policy: operator approval required","gateway":{"decision":"deny"}}"#).create();
    let resume = server
        .mock("POST", format!("/api/flow_runs/{PLAN}/resume").as_str())
        .expect(0)
        .create();
    command(&server, &home)
        .args(["approve", PLAN])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("operator approval required"));
    lookup.assert();
    decide.assert();
    resume.assert();
}

#[test]
fn gateway_lookup_errors_never_resume_a_flow_run() {
    for (status, body, exit) in [
        (
            403,
            r#"{"detail":"Denied by gateway policy: forbidden","gateway":{"decision":"deny"}}"#,
            3,
        ),
        (500, r#"{"detail":"gateway unavailable"}"#, 2),
    ] {
        let mut server = mockito::Server::new();
        let home = tempfile::tempdir().unwrap();
        let lookup = server
            .mock("GET", format!("/gateway/plans/{PLAN}").as_str())
            .with_status(status)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create();
        let resume = server
            .mock("POST", format!("/api/flow_runs/{PLAN}/resume").as_str())
            .expect(0)
            .create();
        let decide = server
            .mock("POST", format!("/gateway/plans/{PLAN}/approve").as_str())
            .expect(0)
            .create();
        command(&server, &home)
            .args(["approve", PLAN])
            .assert()
            .code(exit);
        lookup.assert();
        resume.assert();
        decide.assert();
    }
}

#[test]
fn missing_gateway_plan_falls_back_to_paused_run_approval_or_rejection() {
    for (verb, approved) in [("approve", true), ("reject", false)] {
        let mut server = mockito::Server::new();
        let home = tempfile::tempdir().unwrap();
        let lookup = server
            .mock("GET", format!("/gateway/plans/{PLAN}").as_str())
            .with_status(404)
            .create();
        let resume = server
            .mock("POST", format!("/api/flow_runs/{PLAN}/resume").as_str())
            .match_body(mockito::Matcher::Json(
                json!({"run_input": {"approved": approved}}),
            ))
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"ACCEPT","details":{}}"#)
            .create();
        command(&server, &home)
            .args([verb, PLAN])
            .assert()
            .success();
        lookup.assert();
        resume.assert();
    }
}
