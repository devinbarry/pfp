use crate::config::Config;
use crate::error::{PfpError, Result};
use reqwest::Client;
use serde::de::DeserializeOwned;

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
        let response = self
            .client
            .get(&url)
            .header("Authorization", &self.config.auth_header)
            .send()
            .await?;

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
        let response = self
            .client
            .post(&url)
            .header("Authorization", &self.config.auth_header)
            .json(body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(PfpError::Api(format!("{}: {}", status, body)));
        }

        Ok(response.json().await?)
    }

    #[allow(dead_code)]
    pub async fn patch_no_content(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<()> {
        let url = format!("{}{}", self.config.api_url, path);
        let response = self
            .client
            .patch(&url)
            .header("Authorization", &self.config.auth_header)
            .json(body)
            .send()
            .await?;

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
        self.post("/deployments/filter", &body).await
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub async fn get_flow_run(&self, flow_run_id: &str) -> Result<serde_json::Value> {
        self.get(&format!("/flow_runs/{}", flow_run_id)).await
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub async fn get_flow_run_logs(
        &self,
        flow_run_id: &str,
    ) -> Result<Vec<serde_json::Value>> {
        let body = serde_json::json!({
            "logs": {
                "flow_run_id": {
                    "any_": [flow_run_id]
                }
            },
            "sort": "TIMESTAMP_ASC",
            "limit": 500
        });
        self.post("/logs/filter", &body).await
    }

    #[allow(dead_code)]
    pub async fn set_deployment_paused(
        &self,
        deployment_id: &str,
        paused: bool,
    ) -> Result<()> {
        let body = serde_json::json!({ "paused": paused });
        self.patch_no_content(
            &format!("/deployments/{}", deployment_id),
            &body,
        )
        .await
    }

    #[allow(dead_code)]
    pub async fn cancel_flow_run(&self, flow_run_id: &str) -> Result<serde_json::Value> {
        let body = serde_json::json!({
            "state": {
                "type": "CANCELLED",
                "message": "Cancelled via pfp CLI"
            },
            "force": true
        });
        self.post(
            &format!("/flow_runs/{}/set_state", flow_run_id),
            &body,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client(server: &mockito::Server) -> PrefectClient {
        let config = Config {
            api_url: server.url(),
            auth_header: "Basic dGVzdDp0ZXN0".to_string(),
        };
        PrefectClient::new(config)
    }

    #[tokio::test]
    async fn list_deployments_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/deployments/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"name":"test-deploy","flow_name":"test_flow"}]"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.list_deployments().await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["name"], "test-deploy");
        mock.assert_async().await;
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
