use crate::client::PrefectClient;
use crate::error::{PfpError, Result};
use crate::resolve;

/// Answer a gateway plan record or a flow run paused on an `approved: bool` input. Behind the
/// Prefect gateway an approval is subject to the operator's policy; a denial exits 3.
pub async fn run(client: PrefectClient, flow_run_id: String, approved: bool) -> Result<()> {
    if is_full_uuid(&flow_run_id) {
        if let Some(plan) = client.get_gateway_plan(&flow_run_id).await? {
            return decide_plan(&client, &flow_run_id, &plan, approved).await;
        }
    }
    let resolved_id = resolve::resolve_flow_run(&client, &flow_run_id).await?;
    let result = client.resume_flow_run(&resolved_id, approved).await?;
    let status = result["status"].as_str().unwrap_or("UNKNOWN");
    if status != "ACCEPT" {
        let reason = result["details"]["reason"]
            .as_str()
            .unwrap_or("no reason given");
        return Err(PfpError::Api(format!(
            "Prefect did not resume flow run {resolved_id}: {status}: {reason}"
        )));
    }
    let verb = if approved { "Approved" } else { "Rejected" };
    eprintln!("{verb} flow run: {resolved_id}");
    Ok(())
}

fn is_full_uuid(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    parts.len() == 5
        && parts.iter().zip([8, 4, 4, 4, 12]).all(|(part, length)| {
            part.len() == length && part.chars().all(|c| c.is_ascii_hexdigit())
        })
}

/// A plan record waits on the gateway; approving it makes the gateway start its one apply run.
async fn decide_plan(
    client: &PrefectClient,
    plan_id: &str,
    plan: &serde_json::Value,
    approved: bool,
) -> Result<()> {
    let target = plan["target"].as_str().unwrap_or("?");
    let decided = client.decide_gateway_plan(plan_id, approved).await?;
    if approved {
        eprintln!(
            "Approved {target} plan {plan_id}; the gateway started apply run {}",
            decided["apply_flow_run_id"].as_str().unwrap_or("-")
        );
    } else {
        eprintln!("Rejected {target} plan {plan_id}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    const RUN: &str = "0b6a3b4e-62f2-4cd5-9a5c-2c7c1f0d3e11";

    fn test_client(server: &mockito::Server) -> PrefectClient {
        PrefectClient::new(Config {
            api_url: server.url(),
            auth_header: Some("Basic dGVzdDp0ZXN0".to_string()),
        })
    }

    async fn resume_mock(
        server: &mut mockito::Server,
        approved: bool,
        status: usize,
        body: &str,
    ) -> mockito::Mock {
        server
            .mock("POST", format!("/flow_runs/{RUN}/resume").as_str())
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .match_body(mockito::Matcher::Json(
                serde_json::json!({"run_input": {"approved": approved}}),
            ))
            .with_status(status)
            .with_header("content-type", "application/json")
            .with_body(body)
            .create_async()
            .await
    }

    fn gateway_client(server: &mockito::Server) -> PrefectClient {
        PrefectClient::new(Config {
            api_url: format!("{}/api", server.url()),
            auth_header: Some("Basic dGVzdDp0ZXN0".to_string()),
        })
    }

    #[tokio::test]
    async fn a_gateway_plan_record_is_approved_through_the_gateway() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", format!("/gateway/plans/{RUN}").as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"x","target":"protectli","status":"pending"}"#)
            .create_async()
            .await;
        let decide = server
            .mock("POST", format!("/gateway/plans/{RUN}/approve").as_str())
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"approved","apply_flow_run_id":"run-1"}"#)
            .create_async()
            .await;
        run(gateway_client(&server), RUN.to_string(), true)
            .await
            .unwrap();
        decide.assert_async().await;
    }

    #[tokio::test]
    async fn a_gateway_plan_denial_exits_3() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", format!("/gateway/plans/{RUN}").as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"target":"protectli","status":"pending"}"#)
            .create_async()
            .await;
        server
            .mock("POST", format!("/gateway/plans/{RUN}/approve").as_str())
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"detail":"Denied by gateway policy: no policy permits it","gateway":{"decision":"deny"}}"#,
            )
            .create_async()
            .await;
        let err = run(gateway_client(&server), RUN.to_string(), true)
            .await
            .unwrap_err();
        assert!(matches!(err, PfpError::PolicyDenied(_)), "{err}");
    }

    #[tokio::test]
    async fn without_a_plan_record_the_paused_run_is_resumed() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", format!("/gateway/plans/{RUN}").as_str())
            .with_status(404)
            .create_async()
            .await;
        let resume = server
            .mock("POST", format!("/api/flow_runs/{RUN}/resume").as_str())
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status":"ACCEPT","details":{}}"#)
            .create_async()
            .await;
        run(gateway_client(&server), RUN.to_string(), true)
            .await
            .unwrap();
        resume.assert_async().await;
    }

    #[tokio::test]
    async fn approve_sends_approved_true_and_succeeds_on_accept() {
        let mut server = mockito::Server::new_async().await;
        let mock = resume_mock(
            &mut server,
            true,
            201,
            r#"{"status":"ACCEPT","details":{}}"#,
        )
        .await;
        run(test_client(&server), RUN.to_string(), true)
            .await
            .unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn reject_sends_approved_false() {
        let mut server = mockito::Server::new_async().await;
        let mock = resume_mock(
            &mut server,
            false,
            201,
            r#"{"status":"ACCEPT","details":{}}"#,
        )
        .await;
        run(test_client(&server), RUN.to_string(), false)
            .await
            .unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn a_run_prefect_refuses_to_resume_is_an_error() {
        let mut server = mockito::Server::new_async().await;
        resume_mock(
            &mut server,
            true,
            200,
            r#"{"status":"ABORT","details":{"reason":"Cannot resume a flow run that is not paused."}}"#,
        )
        .await;
        let err = run(test_client(&server), RUN.to_string(), true)
            .await
            .unwrap_err();
        assert_eq!(err.exit_code(), 2);
        assert!(err.to_string().contains("not paused"), "{err}");
    }

    #[tokio::test]
    async fn a_gateway_policy_denial_exits_3_with_the_policy_reason() {
        let mut server = mockito::Server::new_async().await;
        resume_mock(
            &mut server,
            true,
            403,
            r#"{"detail":"Denied by gateway policy: The operator approves Vault changes.",
                "gateway":{"action":"approve","decision":"deny","rule":1}}"#,
        )
        .await;
        let err = run(test_client(&server), RUN.to_string(), true)
            .await
            .unwrap_err();
        assert_eq!(err.exit_code(), 3);
        assert_eq!(
            err.to_string(),
            "Denied by gateway policy: The operator approves Vault changes."
        );
    }

    #[tokio::test]
    async fn a_plain_403_is_not_mistaken_for_a_policy_denial() {
        let mut server = mockito::Server::new_async().await;
        resume_mock(&mut server, true, 403, r#"{"detail":"Forbidden"}"#).await;
        let err = run(test_client(&server), RUN.to_string(), true)
            .await
            .unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }
}
