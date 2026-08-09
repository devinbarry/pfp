use crate::client::PrefectClient;
use crate::error::{PfpError, Result};
use crate::output;
use crate::resolve;

/// Print an intentionally allowlisted flow-run summary.
///
/// This command must never fall back to serializing the raw Prefect response:
/// flow-run parameters, labels, job variables, context, and state data can all
/// contain credentials.
pub async fn run(client: PrefectClient, flow_run_id: String, json: bool) -> Result<()> {
    if !resolve::is_full_uuid(&flow_run_id) {
        return Err(PfpError::Validation(
            "diagnose requires a full flow run UUID".to_string(),
        ));
    }

    let diagnostic = client.get_flow_run_diagnostic(&flow_run_id).await?;
    if json {
        output::print_json(&diagnostic);
    } else {
        output::print_flow_run_diagnostic(&diagnostic);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_client(server: &mockito::Server) -> PrefectClient {
        PrefectClient::new(Config {
            api_url: server.url(),
            auth_header: Some("Basic dGVzdDp0ZXN0".to_string()),
        })
    }

    #[tokio::test]
    async fn fetches_exact_flow_run_by_full_uuid() {
        let mut server = mockito::Server::new_async().await;
        let flow_run_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        let request = server
            .mock("GET", format!("/flow_runs/{flow_run_id}").as_str())
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{"id":"{flow_run_id}","name":"canary","state_type":"RUNNING","state_name":"Running","work_pool_name":"docker-secure","parameters":{{"token":"SENTINEL_SECRET"}},"job_variables":{{"token":"SENTINEL_SECRET"}}}}"#
            ))
            .expect(1)
            .create_async()
            .await;

        run(test_client(&server), flow_run_id.to_string(), true)
            .await
            .unwrap();

        request.assert_async().await;
    }

    #[tokio::test]
    async fn rejects_prefix_without_network_lookup() {
        let server = mockito::Server::new_async().await;

        let error = run(test_client(&server), "aaaaaaaa".to_string(), true)
            .await
            .unwrap_err();

        assert_eq!(error.to_string(), "diagnose requires a full flow run UUID");
    }
}
