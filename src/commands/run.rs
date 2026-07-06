use crate::client::PrefectClient;
use crate::error::{PfpError, Result};
use crate::models::FlowRun;
use crate::output;
use crate::params;
use crate::resolve;
use crate::validate;

/// Load parameters from a file path, or from stdin if `path` is "-".
/// Returns a validated JSON object, or PfpError::Config on any failure.
pub fn load_params_file(path: &str) -> Result<serde_json::Value> {
    let content = if path == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .map_err(|e| PfpError::Config(format!("Failed to read params from stdin: {}", e)))?;
        buf
    } else {
        std::fs::read_to_string(path).map_err(|e| {
            PfpError::Config(format!("Failed to read params file '{}': {}", path, e))
        })?
    };
    params::parse_params(&content).map_err(PfpError::Config)
}

pub async fn run(
    client: PrefectClient,
    query: String,
    watch: bool,
    sets: Vec<String>,
    params_file: Option<String>,
    json: bool,
) -> Result<()> {
    let deployment = resolve::resolve_deployment(&client, &query).await?;
    eprintln!("Resolved: {}", deployment.full_name());

    // Build parameters: payload (--params-file) is the base, --set merges on top.
    let mut overrides = match &params_file {
        Some(path) => load_params_file(path)?,
        None => serde_json::Value::Object(serde_json::Map::new()),
    };
    if !sets.is_empty() {
        let set_overrides = params::build_params(&sets).map_err(PfpError::Config)?;
        overrides = params::merge_params(&overrides, &set_overrides);
    }

    // Validate overrides against deployment's parameter schema
    if let Some(schema) = &deployment.parameter_openapi_schema {
        validate::validate_params(&overrides, schema)?;
    }

    let parameters = params::merge_params(&deployment.parameters, &overrides);

    // Create flow run
    let run_value = client.create_flow_run(&deployment.id, parameters).await?;
    let flow_run: FlowRun =
        serde_json::from_value(run_value.clone()).map_err(|e| PfpError::Api(e.to_string()))?;

    if json && !watch {
        output::print_json(&run_value);
        return Ok(());
    }

    eprintln!(
        "Created flow run '{}' ({})",
        flow_run.name,
        flow_run.short_id()
    );

    if !watch {
        return Ok(());
    }

    // Watch: poll until terminal state
    let mut last_state = String::new();
    loop {
        let current: FlowRun = serde_json::from_value(client.get_flow_run(&flow_run.id).await?)
            .map_err(|e| PfpError::Api(e.to_string()))?;

        if current.state_name != last_state {
            let ts = current
                .start_time
                .as_deref()
                .or(Some(&current.id[..8]))
                .unwrap_or("-");
            if json {
                output::print_json(&serde_json::json!({
                    "state": current.state_name,
                    "state_type": current.state_type,
                    "timestamp": ts
                }));
            } else {
                output::print_watch_state(&current.state_name, ts);
            }
            last_state = current.state_name.clone();
        }

        if current.is_terminal() {
            if current.is_success() {
                return Ok(());
            } else {
                return Err(PfpError::FlowRunFailed(format!(
                    "{} ({})",
                    current.state_name,
                    current.short_id()
                )));
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

#[cfg(test)]
mod tests {
    use crate::client::PrefectClient;
    use crate::config::Config;
    use crate::error::PfpError;
    use serde_json::json;

    fn test_client(server: &mockito::Server) -> PrefectClient {
        let config = Config {
            api_url: server.url(),
            auth_header: Some("Basic dGVzdDp0ZXN0".to_string()),
        };
        PrefectClient::new(config)
    }

    fn mock_deployment_with_schema() -> serde_json::Value {
        json!([{
            "id": "dep-1",
            "name": "test-deploy-prod",
            "flow_id": "flow-1",
            "flow_name": "test_flow",
            "parameters": {"config": {"action": "plan", "dry_run": false}},
            "parameter_openapi_schema": {
                "type": "object",
                "properties": {
                    "config": { "$ref": "#/definitions/FlowConfig" },
                    "environment": { "type": "string" }
                },
                "definitions": {
                    "FlowConfig": {
                        "type": "object",
                        "properties": {
                            "action": { "type": "string" },
                            "dry_run": { "type": "boolean" }
                        }
                    }
                }
            }
        }])
    }

    fn mock_deployment_without_schema() -> serde_json::Value {
        json!([{
            "id": "dep-1",
            "name": "test-deploy-prod",
            "flow_id": "flow-1",
            "flow_name": "test_flow",
            "parameters": {"config": {"action": "plan"}}
        }])
    }

    #[test]
    fn load_params_file_reads_valid_json() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, r#"{{"config": {{"dry_run": true}}}}"#).unwrap();
        let result = super::load_params_file(f.path().to_str().unwrap()).unwrap();
        assert_eq!(result, json!({"config": {"dry_run": true}}));
    }

    #[test]
    fn load_params_file_missing_file_errors() {
        let result = super::load_params_file("/nonexistent/path/payload.json");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::PfpError::Config(_)
        ));
    }

    #[test]
    fn load_params_file_malformed_json_errors() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, "{{not json").unwrap();
        let result = super::load_params_file(f.path().to_str().unwrap());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            crate::error::PfpError::Config(_)
        ));
    }

    #[tokio::test]
    async fn run_valid_params_succeeds() {
        let mut server = mockito::Server::new_async().await;
        let deploy_mock = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_deployment_with_schema().to_string())
            .create_async()
            .await;
        let flow_mock = server
            .mock("POST", "/flows/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"flow-1","name":"test_flow"}]"#)
            .create_async()
            .await;
        let run_mock = server
            .mock("POST", "/deployments/dep-1/create_flow_run")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"run-1","name":"cool-run","state_type":"SCHEDULED","state_name":"Scheduled"}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = super::run(
            client,
            "test-deploy".to_string(),
            false,
            vec![
                "config.action=destroy".to_string(),
                "config.dry_run=true".to_string(),
            ],
            None,
            false,
        )
        .await;

        assert!(result.is_ok());
        deploy_mock.assert_async().await;
        flow_mock.assert_async().await;
        run_mock.assert_async().await;
    }

    #[tokio::test]
    async fn run_invalid_param_rejected_before_api_call() {
        let mut server = mockito::Server::new_async().await;
        let deploy_mock = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_deployment_with_schema().to_string())
            .create_async()
            .await;
        let flow_mock = server
            .mock("POST", "/flows/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"flow-1","name":"test_flow"}]"#)
            .create_async()
            .await;
        // No create_flow_run mock — it should never be called
        let run_mock = server
            .mock("POST", "/deployments/dep-1/create_flow_run")
            .expect(0)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = super::run(
            client,
            "test-deploy".to_string(),
            false,
            vec!["config.dry_urn=true".to_string()],
            None,
            false,
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("dry_urn"),
            "should mention invalid key: {}",
            msg
        );
        assert!(
            msg.contains("dry_run"),
            "should suggest correction: {}",
            msg
        );
        assert!(matches!(err, PfpError::Validation(_)));
        assert_eq!(err.exit_code(), 2);

        deploy_mock.assert_async().await;
        flow_mock.assert_async().await;
        run_mock.assert_async().await; // asserts 0 calls
    }

    #[tokio::test]
    async fn run_no_schema_skips_validation() {
        let mut server = mockito::Server::new_async().await;
        let deploy_mock = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_deployment_without_schema().to_string())
            .create_async()
            .await;
        let flow_mock = server
            .mock("POST", "/flows/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"flow-1","name":"test_flow"}]"#)
            .create_async()
            .await;
        let run_mock = server
            .mock("POST", "/deployments/dep-1/create_flow_run")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"run-1","name":"cool-run","state_type":"SCHEDULED","state_name":"Scheduled"}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        // Pass a bogus param — should NOT be rejected because there's no schema
        let result = super::run(
            client,
            "test-deploy".to_string(),
            false,
            vec!["config.bogus=true".to_string()],
            None,
            false,
        )
        .await;

        assert!(result.is_ok(), "should pass when no schema: {:?}", result);
        deploy_mock.assert_async().await;
        flow_mock.assert_async().await;
        run_mock.assert_async().await;
    }

    #[tokio::test]
    async fn run_empty_sets_with_schema_succeeds() {
        let mut server = mockito::Server::new_async().await;
        let deploy_mock = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_deployment_with_schema().to_string())
            .create_async()
            .await;
        let flow_mock = server
            .mock("POST", "/flows/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"flow-1","name":"test_flow"}]"#)
            .create_async()
            .await;
        let run_mock = server
            .mock("POST", "/deployments/dep-1/create_flow_run")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"run-1","name":"cool-run","state_type":"SCHEDULED","state_name":"Scheduled"}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        // No --set flags — should always succeed
        let result = super::run(
            client,
            "test-deploy".to_string(),
            false,
            vec![],
            None,
            false,
        )
        .await;

        assert!(result.is_ok());
        deploy_mock.assert_async().await;
        flow_mock.assert_async().await;
        run_mock.assert_async().await;
    }

    #[tokio::test]
    async fn run_with_params_file_succeeds() {
        use std::io::Write;
        let mut server = mockito::Server::new_async().await;
        let deploy_mock = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_deployment_with_schema().to_string())
            .create_async()
            .await;
        let flow_mock = server
            .mock("POST", "/flows/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"flow-1","name":"test_flow"}]"#)
            .create_async()
            .await;
        let run_mock = server
            .mock("POST", "/deployments/dep-1/create_flow_run")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"run-1","name":"cool-run","state_type":"SCHEDULED","state_name":"Scheduled"}"#)
            .create_async()
            .await;

        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(
            f,
            r#"{{"environment": "production", "config": {{"action": "destroy"}}}}"#
        )
        .unwrap();

        let client = test_client(&server);
        let result = super::run(
            client,
            "test-deploy".to_string(),
            false,
            vec![],
            Some(f.path().to_str().unwrap().to_string()),
            false,
        )
        .await;

        assert!(result.is_ok(), "{:?}", result);
        deploy_mock.assert_async().await;
        flow_mock.assert_async().await;
        run_mock.assert_async().await;
    }

    #[tokio::test]
    async fn run_params_file_invalid_key_rejected_before_api() {
        let mut server = mockito::Server::new_async().await;
        let deploy_mock = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_deployment_with_schema().to_string())
            .create_async()
            .await;
        let flow_mock = server
            .mock("POST", "/flows/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"flow-1","name":"test_flow"}]"#)
            .create_async()
            .await;
        let run_mock = server
            .mock("POST", "/deployments/dep-1/create_flow_run")
            .expect(0)
            .create_async()
            .await;

        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, r#"{{"config": {{"dry_urn": true}}}}"#).unwrap();

        let client = test_client(&server);
        let result = super::run(
            client,
            "test-deploy".to_string(),
            false,
            vec![],
            Some(f.path().to_str().unwrap().to_string()),
            false,
        )
        .await;

        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("dry_urn"), "should mention typo: {}", msg);
        deploy_mock.assert_async().await;
        flow_mock.assert_async().await;
        run_mock.assert_async().await;
    }

    #[tokio::test]
    async fn run_set_overrides_params_file() {
        // --set config.action=plan must win over the payload's config.action=destroy.
        let mut server = mockito::Server::new_async().await;
        let deploy_mock = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(mock_deployment_with_schema().to_string())
            .create_async()
            .await;
        let flow_mock = server
            .mock("POST", "/flows/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"flow-1","name":"test_flow"}]"#)
            .create_async()
            .await;
        let run_mock = server
            .mock("POST", "/deployments/dep-1/create_flow_run")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"parameters":{"config":{"action":"plan"}}}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"run-1","name":"cool-run","state_type":"SCHEDULED","state_name":"Scheduled"}"#)
            .create_async()
            .await;

        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, r#"{{"config": {{"action": "destroy"}}}}"#).unwrap();

        let client = test_client(&server);
        let result = super::run(
            client,
            "test-deploy".to_string(),
            false,
            vec!["config.action=plan".to_string()],
            Some(f.path().to_str().unwrap().to_string()),
            false,
        )
        .await;

        assert!(result.is_ok(), "{:?}", result);
        deploy_mock.assert_async().await;
        flow_mock.assert_async().await;
        run_mock.assert_async().await;
    }
}
