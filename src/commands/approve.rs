use crate::client::PrefectClient;
use crate::error::{PfpError, Result};
use crate::resolve;

/// Answer a flow run paused on an `approved: bool` input. Behind the Prefect
/// gateway an approval is subject to the operator's policy; a denial exits 3.
pub async fn run(client: PrefectClient, flow_run_id: String, approved: bool) -> Result<()> {
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
