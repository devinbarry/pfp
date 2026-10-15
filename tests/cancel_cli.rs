use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::json;

const RUN: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

#[test]
fn cancel_requests_orchestrated_cancellation_without_claiming_worker_shutdown() {
    let mut server = mockito::Server::new();
    let home = tempfile::tempdir().unwrap();
    let request = server
        .mock("POST", format!("/api/flow_runs/{RUN}/set_state").as_str())
        .match_body(mockito::Matcher::Json(json!({
            "state": {"type": "CANCELLING", "message": "Cancellation requested via pfp CLI"},
            "force": false
        })))
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"status":"ACCEPT","state":{"type":"CANCELLING"},"details":{}}"#)
        .create();
    cargo_bin_cmd!("pfp")
        .args(["cancel", RUN])
        .env("HOME", home.path())
        .env("PREFECT_API_URL", format!("{}/api/", server.url()))
        .env_remove("PREFECT_API_AUTH_STRING")
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Cancellation requested for flow run:",
        ))
        .stderr(predicate::str::contains("Cancelled flow run").not());
    request.assert();
}

#[test]
fn cancel_does_not_report_success_when_orchestration_refuses_or_delays() {
    for status in ["ABORT", "REJECT", "WAIT", "UNKNOWN"] {
        let mut server = mockito::Server::new();
        let home = tempfile::tempdir().unwrap();
        let request = server
            .mock("POST", format!("/api/flow_runs/{RUN}/set_state").as_str())
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(json!({"status": status, "details": {"reason": "not accepted"}}).to_string())
            .create();
        cargo_bin_cmd!("pfp")
            .args(["cancel", RUN])
            .env("HOME", home.path())
            .env("PREFECT_API_URL", format!("{}/api", server.url()))
            .env_remove("PREFECT_API_AUTH_STRING")
            .assert()
            .code(2)
            .stderr(predicate::str::contains(status))
            .stderr(predicate::str::contains("not accepted"))
            .stderr(predicate::str::contains("Cancellation requested for flow run:").not());
        request.assert();
    }
}

#[test]
fn cancel_preserves_gateway_policy_denial_exit_code() {
    let mut server = mockito::Server::new();
    let home = tempfile::tempdir().unwrap();
    let request = server
        .mock("POST", format!("/api/flow_runs/{RUN}/set_state").as_str())
        .with_status(403)
        .with_header("content-type", "application/json")
        .with_body(
            r#"{"detail":"Denied by gateway policy: operator only","gateway":{"decision":"deny"}}"#,
        )
        .create();
    cargo_bin_cmd!("pfp")
        .args(["cancel", RUN])
        .env("HOME", home.path())
        .env("PREFECT_API_URL", format!("{}/api", server.url()))
        .env_remove("PREFECT_API_AUTH_STRING")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("operator only"));
    request.assert();
}
