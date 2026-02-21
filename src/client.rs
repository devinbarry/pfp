use crate::config::Config;
use crate::error::{PfpError, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;
use std::collections::HashMap;

pub struct PrefectClient {
    client: Client,
    config: Config,
}

impl PrefectClient {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.config.api_url, path);
        let mut req = self.client.get(&url);
        if let Some(auth) = &self.config.auth_header {
            req = req.header("Authorization", auth);
        }
        let response = req.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(PfpError::Api(format!("{}: {}", status, body)));
        }

        Ok(response.json().await?)
    }

    pub async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        let url = format!("{}{}", self.config.api_url, path);
        let mut req = self.client.post(&url);
        if let Some(auth) = &self.config.auth_header {
            req = req.header("Authorization", auth);
        }
        let response = req.json(body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(PfpError::Api(format!("{}: {}", status, body)));
        }

        Ok(response.json().await?)
    }

    pub async fn patch_no_content(&self, path: &str, body: &serde_json::Value) -> Result<()> {
        let url = format!("{}{}", self.config.api_url, path);
        let mut req = self.client.patch(&url);
        if let Some(auth) = &self.config.auth_header {
            req = req.header("Authorization", auth);
        }
        let response = req.json(body).send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(PfpError::Api(format!("{}: {}", status, body)));
        }

        Ok(())
    }

    // -- Prefect API methods --

    pub async fn list_deployments(&self) -> Result<Vec<serde_json::Value>> {
        let body = serde_json::json!({
            "limit": 100,
            "offset": 0
        });
        let mut deployments: Vec<serde_json::Value> =
            self.post("/deployments/filter", &body).await?;

        // Collect unique flow_ids to resolve flow names
        let flow_ids: Vec<String> = deployments
            .iter()
            .filter_map(|d| d["flow_id"].as_str().map(|s| s.to_string()))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        if !flow_ids.is_empty() {
            let flow_names = self.fetch_flow_names(&flow_ids).await?;
            for dep in &mut deployments {
                if let Some(fid) = dep["flow_id"].as_str() {
                    if let Some(name) = flow_names.get(fid) {
                        dep["flow_name"] = serde_json::Value::String(name.clone());
                    }
                }
            }
        }

        Ok(deployments)
    }

    async fn fetch_flow_names(&self, flow_ids: &[String]) -> Result<HashMap<String, String>> {
        let body = serde_json::json!({
            "flows": {
                "id": {
                    "any_": flow_ids
                }
            }
        });
        let flows: Vec<serde_json::Value> = self.post("/flows/filter", &body).await?;
        Ok(flows
            .into_iter()
            .filter_map(|f| {
                let id = f["id"].as_str()?.to_string();
                let name = f["name"].as_str()?.to_string();
                Some((id, name))
            })
            .collect())
    }

    pub async fn create_flow_run(
        &self,
        deployment_id: &str,
        parameters: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let body = serde_json::json!({
            "parameters": parameters
        });
        self.post(
            &format!("/deployments/{}/create_flow_run", deployment_id),
            &body,
        )
        .await
    }

    pub async fn get_flow_run(&self, flow_run_id: &str) -> Result<serde_json::Value> {
        self.get(&format!("/flow_runs/{}", flow_run_id)).await
    }

    pub async fn filter_flow_runs(
        &self,
        deployment_id: &str,
        limit: usize,
    ) -> Result<Vec<serde_json::Value>> {
        let body = serde_json::json!({
            "flow_runs": {
                "deployment_id": {
                    "any_": [deployment_id]
                }
            },
            "sort": "START_TIME_DESC",
            "limit": limit
        });
        self.post("/flow_runs/filter", &body).await
    }

    pub async fn get_flow_run_logs(&self, flow_run_id: &str) -> Result<Vec<serde_json::Value>> {
        let body = serde_json::json!({
            "logs": {
                "flow_run_id": {
                    "any_": [flow_run_id]
                }
            },
            "sort": "TIMESTAMP_ASC",
            "limit": 200
        });
        self.post("/logs/filter", &body).await
    }

    pub async fn set_deployment_paused(&self, deployment_id: &str, paused: bool) -> Result<()> {
        let body = serde_json::json!({ "paused": paused });
        self.patch_no_content(&format!("/deployments/{}", deployment_id), &body)
            .await
    }

    pub async fn cancel_flow_run(&self, flow_run_id: &str) -> Result<serde_json::Value> {
        let body = serde_json::json!({
            "state": {
                "type": "CANCELLED",
                "message": "Cancelled via pfp CLI"
            },
            "force": true
        });
        self.post(&format!("/flow_runs/{}/set_state", flow_run_id), &body)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client(server: &mockito::Server) -> PrefectClient {
        let config = Config {
            api_url: server.url(),
            auth_header: Some("Basic dGVzdDp0ZXN0".to_string()),
        };
        PrefectClient::new(config)
    }

    #[tokio::test]
    async fn list_deployments_success() {
        let mut server = mockito::Server::new_async().await;
        let deploy_mock = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"name":"test-deploy","flow_id":"flow-1"}]"#)
            .create_async()
            .await;
        let flow_mock = server
            .mock("POST", "/flows/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"flow-1","name":"test_flow"}]"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.list_deployments().await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "test-deploy");
        assert_eq!(result[0]["flow_name"], "test_flow");
        deploy_mock.assert_async().await;
        flow_mock.assert_async().await;
    }

    #[tokio::test]
    async fn list_deployments_api_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/deployments/filter")
            .with_status(401)
            .with_body("Unauthorized")
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.list_deployments().await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PfpError::Api(ref msg) if msg.contains("401")));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn get_flow_run_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/flow_runs/abc-123")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"abc-123","state_type":"COMPLETED","state_name":"Completed"}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.get_flow_run("abc-123").await.unwrap();

        assert_eq!(result["state_type"], "COMPLETED");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn create_flow_run_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/deployments/dep-id/create_flow_run")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"run-123","name":"cool-name","state_type":"SCHEDULED"}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let params = serde_json::json!({"config": {"action": "plan"}});
        let result = client.create_flow_run("dep-id", params).await.unwrap();

        assert_eq!(result["id"], "run-123");
        assert_eq!(result["name"], "cool-name");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn patch_no_content_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("PATCH", "/deployments/dep-id")
            .with_status(204)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.set_deployment_paused("dep-id", true).await;

        assert!(result.is_ok());
        mock.assert_async().await;
    }
}
