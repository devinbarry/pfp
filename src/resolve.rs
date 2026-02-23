use crate::client::PrefectClient;
use crate::error::{PfpError, Result};
use crate::models::{Deployment, FlowRun};

/// Resolve a user query to a single deployment via unique substring match.
pub async fn resolve_deployment(client: &PrefectClient, query: &str) -> Result<Deployment> {
    let values = client.list_deployments().await?;
    let deployments: Vec<Deployment> = values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    let matches: Vec<&Deployment> = deployments
        .iter()
        .filter(|d| d.full_name().contains(query))
        .collect();

    match matches.len() {
        0 => Err(PfpError::NoMatch(format!(
            "no deployment matching '{}'",
            query
        ))),
        1 => {
            let idx = deployments
                .iter()
                .position(|d| d.full_name().contains(query))
                .unwrap();
            Ok(deployments.into_iter().nth(idx).unwrap())
        }
        _ => {
            let candidates = matches
                .iter()
                .map(|d| format!("  {}", d.full_name()))
                .collect::<Vec<_>>()
                .join("\n");
            Err(PfpError::AmbiguousMatch {
                query: query.to_string(),
                candidates,
            })
        }
    }
}

/// Check if input looks like a complete UUID (with or without hyphens).
pub fn is_full_uuid(input: &str) -> bool {
    let hex_only = input.replace('-', "");
    hex_only.len() == 32 && hex_only.chars().all(|c| c.is_ascii_hexdigit())
}

/// Resolve a user-provided flow run ID (possibly a short prefix) to a full UUID.
pub async fn resolve_flow_run(client: &PrefectClient, input: &str) -> Result<String> {
    if is_full_uuid(input) {
        return Ok(input.to_string());
    }

    let values = client.filter_flow_runs_global(100).await?;
    let runs: Vec<FlowRun> = values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    let matches: Vec<&FlowRun> = runs
        .iter()
        .filter(|r| r.id.starts_with(input) || r.id.replace('-', "").starts_with(input))
        .collect();

    match matches.len() {
        0 => Err(PfpError::NoMatch(format!(
            "no flow run matching '{}'",
            input
        ))),
        1 => Ok(matches[0].id.clone()),
        _ => {
            let candidates = matches
                .iter()
                .map(|r| format!("  {} ({})", r.short_id(), r.state_name))
                .collect::<Vec<_>>()
                .join("\n");
            Err(PfpError::AmbiguousMatch {
                query: input.to_string(),
                candidates,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::models::Deployment;
    use crate::models::FlowRun;
    use serde_json::json;

    fn sample_deployments() -> Vec<Deployment> {
        vec![
            serde_json::from_value(json!({
                "id": "1", "name": "happy-terraform-prod", "flow_name": "happy_terraform"
            }))
            .unwrap(),
            serde_json::from_value(json!({
                "id": "2", "name": "happy-ansible-prod", "flow_name": "happy_ansible"
            }))
            .unwrap(),
            serde_json::from_value(json!({
                "id": "3", "name": "hello_world-dev", "flow_name": "hello_world"
            }))
            .unwrap(),
        ]
    }

    fn find_match<'a>(deployments: &'a [Deployment], query: &str) -> Vec<&'a Deployment> {
        deployments
            .iter()
            .filter(|d| d.full_name().contains(query))
            .collect()
    }

    #[test]
    fn unique_substring_match() {
        let deps = sample_deployments();
        let matches = find_match(&deps, "happy-t");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            matches[0].full_name(),
            "happy_terraform/happy-terraform-prod"
        );
    }

    #[test]
    fn ambiguous_match() {
        let deps = sample_deployments();
        let matches = find_match(&deps, "happy");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn no_match() {
        let deps = sample_deployments();
        let matches = find_match(&deps, "nonexistent");
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn exact_full_name_match() {
        let deps = sample_deployments();
        let matches = find_match(&deps, "happy_terraform/happy-terraform-prod");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn match_by_deployment_name_only() {
        let deps = sample_deployments();
        let matches = find_match(&deps, "hello_world-dev");
        assert_eq!(matches.len(), 1);
    }

    fn sample_flow_runs() -> Vec<FlowRun> {
        vec![
            serde_json::from_value(json!({
                "id": "171a3f55-e9a5-4100-a2dd-efe5c711f847",
                "name": "cool-run-1",
                "state_type": "COMPLETED",
                "state_name": "Completed"
            }))
            .unwrap(),
            serde_json::from_value(json!({
                "id": "171a3f55-ffff-4200-b3ee-1234567890ab",
                "name": "cool-run-2",
                "state_type": "FAILED",
                "state_name": "Failed"
            }))
            .unwrap(),
            serde_json::from_value(json!({
                "id": "9d9ca60c-abcd-4300-9999-abcdef012345",
                "name": "other-run",
                "state_type": "RUNNING",
                "state_name": "Running"
            }))
            .unwrap(),
        ]
    }

    fn find_flow_run_match<'a>(runs: &'a [FlowRun], prefix: &str) -> Vec<&'a FlowRun> {
        runs.iter()
            .filter(|r| r.id.starts_with(prefix) || r.id.replace('-', "").starts_with(prefix))
            .collect()
    }

    #[test]
    fn flow_run_unique_prefix_match() {
        let runs = sample_flow_runs();
        let matches = find_flow_run_match(&runs, "9d9ca60c");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].id, "9d9ca60c-abcd-4300-9999-abcdef012345");
    }

    #[test]
    fn flow_run_ambiguous_prefix() {
        let runs = sample_flow_runs();
        let matches = find_flow_run_match(&runs, "171a3f55");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn flow_run_no_match() {
        let runs = sample_flow_runs();
        let matches = find_flow_run_match(&runs, "deadbeef");
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn flow_run_full_uuid_matches() {
        let runs = sample_flow_runs();
        let matches = find_flow_run_match(&runs, "9d9ca60c-abcd-4300-9999-abcdef012345");
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn is_full_uuid_with_hyphens() {
        assert!(super::is_full_uuid("171a3f55-e9a5-4100-a2dd-efe5c711f847"));
    }

    #[test]
    fn is_full_uuid_without_hyphens() {
        assert!(super::is_full_uuid("171a3f55e9a54100a2ddefe5c711f847"));
    }

    #[test]
    fn is_full_uuid_short_prefix() {
        assert!(!super::is_full_uuid("171a3f55"));
    }

    #[test]
    fn is_full_uuid_not_hex() {
        assert!(!super::is_full_uuid("not-a-uuid-at-all-zzzz-zzzzzzzzzzzz"));
    }

    // -- Async integration tests (mockito HTTP server) --

    use crate::client::PrefectClient;
    use crate::config::Config;
    use crate::error::PfpError;

    fn test_client(server: &mockito::Server) -> PrefectClient {
        let config = Config {
            api_url: server.url(),
            auth_header: Some("Basic dGVzdDp0ZXN0".to_string()),
        };
        PrefectClient::new(config)
    }

    fn mock_flow_runs_json() -> String {
        serde_json::to_string(&vec![
            json!({
                "id": "171a3f55-e9a5-4100-a2dd-efe5c711f847",
                "name": "cool-run-1",
                "state_type": "COMPLETED",
                "state_name": "Completed"
            }),
            json!({
                "id": "171a3f55-ffff-4200-b3ee-1234567890ab",
                "name": "cool-run-2",
                "state_type": "FAILED",
                "state_name": "Failed"
            }),
            json!({
                "id": "9d9ca60c-abcd-4300-9999-abcdef012345",
                "name": "other-run",
                "state_type": "RUNNING",
                "state_name": "Running"
            }),
        ])
        .unwrap()
    }

    #[tokio::test]
    async fn resolve_full_uuid_skips_api() {
        // No mock server needed — full UUID should return immediately
        let server = mockito::Server::new_async().await;
        let client = test_client(&server);
        // No mocks registered, so any API call would fail
        let result = super::resolve_flow_run(&client, "171a3f55-e9a5-4100-a2dd-efe5c711f847").await;
        assert_eq!(result.unwrap(), "171a3f55-e9a5-4100-a2dd-efe5c711f847");
    }

    #[tokio::test]
    async fn resolve_unique_prefix() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/flow_runs/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_flow_runs_json())
            .create_async()
            .await;

        let client = test_client(&server);
        let result = super::resolve_flow_run(&client, "9d9ca60c").await;

        assert_eq!(result.unwrap(), "9d9ca60c-abcd-4300-9999-abcdef012345");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_ambiguous_prefix() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/flow_runs/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_flow_runs_json())
            .create_async()
            .await;

        let client = test_client(&server);
        let result = super::resolve_flow_run(&client, "171a3f55").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            PfpError::AmbiguousMatch { query, candidates } => {
                assert_eq!(query, "171a3f55");
                assert!(candidates.contains("171a3f55"));
                assert!(candidates.contains("Completed"));
                assert!(candidates.contains("Failed"));
            }
            other => panic!("Expected AmbiguousMatch, got: {:?}", other),
        }
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn resolve_no_match_prefix() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/flow_runs/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_flow_runs_json())
            .create_async()
            .await;

        let client = test_client(&server);
        let result = super::resolve_flow_run(&client, "deadbeef").await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PfpError::NoMatch(msg) if msg.contains("deadbeef")));
        mock.assert_async().await;
    }
}
