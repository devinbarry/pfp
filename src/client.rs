use crate::config::Config;
use crate::error::{PfpError, Result};
use crate::models::{DeploymentSchedule, FlowRunDiagnostic, WorkPool, WorkerDiagnostic};
use crate::provenance;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use std::collections::HashMap;

pub struct PrefectClient {
    client: Client,
    config: Config,
    agent_headers: Vec<(&'static str, String)>,
}

impl PrefectClient {
    pub fn new(config: Config) -> Self {
        Self {
            client: Client::new(),
            config,
            agent_headers: provenance::agent_headers(),
        }
    }

    /// Attach the caller's credential and, inside an agent session, its identity.
    fn authorize(&self, mut req: RequestBuilder) -> RequestBuilder {
        if let Some(auth) = &self.config.auth_header {
            req = req.header("Authorization", auth);
        }
        for (name, value) in &self.agent_headers {
            req = req.header(*name, value);
        }
        req
    }

    async fn send(&self, req: RequestBuilder) -> Result<Response> {
        let response = self.authorize(req).send().await?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(error_for(status, &body))
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.config.api_url, path);
        Ok(self.send(self.client.get(&url)).await?.json().await?)
    }

    pub async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        let url = format!("{}{}", self.config.api_url, path);
        Ok(self
            .send(self.client.post(&url).json(body))
            .await?
            .json()
            .await?)
    }

    pub async fn patch_no_content(&self, path: &str, body: &serde_json::Value) -> Result<()> {
        let url = format!("{}{}", self.config.api_url, path);
        self.send(self.client.patch(&url).json(body)).await?;
        Ok(())
    }

    pub async fn delete_no_content(&self, path: &str) -> Result<()> {
        let url = format!("{}{}", self.config.api_url, path);
        self.send(self.client.delete(&url)).await?;
        Ok(())
    }

    /// Read the policy published by the Prefect gateway in front of this server.
    pub async fn get_gateway_policy(&self) -> Result<serde_json::Value> {
        let url = format!("{}/gateway/policy", self.gateway_base()?);
        Ok(self.send(self.client.get(&url)).await?.json().await?)
    }

    /// The gateway answers beside the Prefect API: the API URL without its trailing /api.
    fn gateway_base(&self) -> Result<String> {
        let api_url = self.config.api_url.trim_end_matches('/');
        let base = api_url.strip_suffix("/api").ok_or_else(|| {
            PfpError::Config(format!(
                "cannot locate the gateway: API URL {api_url} does not end in /api"
            ))
        })?;
        Ok(base.to_string())
    }

    /// Plan records waiting on, or decided through, the gateway (newest first).
    pub async fn list_gateway_plans(&self) -> Result<Vec<serde_json::Value>> {
        let url = format!("{}/gateway/plans", self.gateway_base()?);
        Ok(self.send(self.client.get(&url)).await?.json().await?)
    }

    /// One plan record, or None when the gateway holds no plan with this id.
    pub async fn get_gateway_plan(&self, plan_id: &str) -> Result<Option<serde_json::Value>> {
        // Without a gateway beside the API there are no plan records, only paused runs.
        let Ok(base) = self.gateway_base() else {
            return Ok(None);
        };
        let url = format!("{base}/gateway/plans/{plan_id}");
        let response = self.authorize(self.client.get(&url)).send().await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(error_for(status, &body));
        }
        Ok(Some(response.json().await?))
    }

    /// Approve or reject a plan record; the gateway's policy decides whether this agent may.
    pub async fn decide_gateway_plan(
        &self,
        plan_id: &str,
        approved: bool,
    ) -> Result<serde_json::Value> {
        let verb = if approved { "approve" } else { "reject" };
        let url = format!("{}/gateway/plans/{plan_id}/{verb}", self.gateway_base()?);
        Ok(self.send(self.client.post(&url)).await?.json().await?)
    }

    // -- Prefect API methods --

    pub async fn list_deployments(&self) -> Result<Vec<serde_json::Value>> {
        const PAGE_SIZE: usize = 100;
        let mut deployments = Vec::new();

        loop {
            let body = serde_json::json!({
                "limit": PAGE_SIZE,
                "offset": deployments.len()
            });
            let page: Vec<serde_json::Value> = self.post("/deployments/filter", &body).await?;
            let page_len = page.len();
            deployments.extend(page);

            if page_len < PAGE_SIZE {
                break;
            }
        }

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
        additional_tags: Vec<String>,
    ) -> Result<serde_json::Value> {
        let mut tags = vec!["manual".to_string()];
        for tag in additional_tags {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
        let body = serde_json::json!({
            "parameters": parameters,
            "tags": tags
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

    pub async fn get_flow_run_diagnostic(&self, flow_run_id: &str) -> Result<FlowRunDiagnostic> {
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

    pub async fn filter_flow_runs_global(&self, limit: usize) -> Result<Vec<serde_json::Value>> {
        let body = serde_json::json!({
            "sort": "START_TIME_DESC",
            "limit": limit
        });
        self.post("/flow_runs/filter", &body).await
    }

    pub async fn get_flow_run_logs(
        &self,
        flow_run_id: &str,
        limit: usize,
        start_offset: usize,
    ) -> Result<Vec<serde_json::Value>> {
        const PAGE_SIZE: usize = 200;
        let mut all_logs = Vec::new();
        let mut offset: usize = start_offset;

        loop {
            let remaining = limit - all_logs.len();
            let page_limit = remaining.min(PAGE_SIZE);

            let body = serde_json::json!({
                "logs": {
                    "flow_run_id": {
                        "any_": [flow_run_id]
                    }
                },
                "sort": "TIMESTAMP_ASC",
                "limit": page_limit,
                "offset": offset
            });

            let page: Vec<serde_json::Value> = self.post("/logs/filter", &body).await?;
            let page_len = page.len();
            all_logs.extend(page);

            if page_len < page_limit || all_logs.len() >= limit {
                break;
            }

            offset += page_len;
        }

        Ok(all_logs)
    }

    pub async fn set_deployment_paused(&self, deployment_id: &str, paused: bool) -> Result<()> {
        let body = serde_json::json!({ "paused": paused });
        self.patch_no_content(&format!("/deployments/{}", deployment_id), &body)
            .await
    }

    pub async fn delete_deployment(&self, deployment_id: &str) -> Result<()> {
        self.delete_no_content(&format!("/deployments/{}", deployment_id))
            .await
    }

    pub async fn read_deployment_schedules(
        &self,
        deployment_id: &str,
    ) -> Result<Vec<DeploymentSchedule>> {
        self.get(&format!("/deployments/{}/schedules", deployment_id))
            .await
    }

    pub async fn set_deployment_schedule_active(
        &self,
        deployment_id: &str,
        schedule_id: &str,
        active: bool,
    ) -> Result<()> {
        let body = serde_json::json!({ "active": active });
        self.patch_no_content(
            &format!("/deployments/{}/schedules/{}", deployment_id, schedule_id),
            &body,
        )
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

    pub async fn resume_flow_run(
        &self,
        flow_run_id: &str,
        approved: bool,
    ) -> Result<serde_json::Value> {
        let body = serde_json::json!({ "run_input": { "approved": approved } });
        self.post(&format!("/flow_runs/{}/resume", flow_run_id), &body)
            .await
    }

    pub async fn get_work_pool(&self, name: &str) -> Result<WorkPool> {
        let path = work_pool_path(name)?;
        self.get(&path).await
    }

    pub async fn count_nonterminal_flow_runs_for_work_pool(&self, name: &str) -> Result<u64> {
        validate_work_pool_name(name)?;
        let body = serde_json::json!({
            "work_pools": {
                "name": {
                    "any_": [name]
                }
            },
            "flow_runs": {
                "state": {
                    "type": {
                        "not_any_": ["COMPLETED", "FAILED", "CANCELLED", "CRASHED"]
                    }
                }
            }
        });
        self.post("/flow_runs/count", &body).await
    }

    pub async fn list_workers_for_work_pool(&self, name: &str) -> Result<Vec<WorkerDiagnostic>> {
        let path = format!("{}/workers/filter", work_pool_path(name)?);
        let body = serde_json::json!({
            "workers": null,
            "offset": 0,
            "limit": 200
        });
        self.post(&path, &body).await
    }

    pub async fn set_work_pool_paused(&self, name: &str, paused: bool) -> Result<WorkPool> {
        let path = work_pool_path(name)?;
        let body = serde_json::json!({ "is_paused": paused });
        self.patch_no_content(&path, &body).await?;

        let pool = self.get_work_pool(name).await?;
        if pool.is_paused != paused {
            return Err(PfpError::Api(format!(
                "work pool {name:?} did not converge to is_paused={paused}"
            )));
        }
        Ok(pool)
    }
}

/// A gateway policy denial is reported as such; anything else stays an API error.
fn error_for(status: StatusCode, body: &str) -> PfpError {
    if status == StatusCode::FORBIDDEN {
        if let Ok(document) = serde_json::from_str::<serde_json::Value>(body) {
            if document.get("gateway").is_some() {
                let detail = document["detail"]
                    .as_str()
                    .unwrap_or("Denied by gateway policy");
                return PfpError::PolicyDenied(detail.to_string());
            }
        }
    }
    PfpError::Api(format!("{}: {}", status, body))
}

fn work_pool_path(name: &str) -> Result<String> {
    validate_work_pool_name(name)?;

    let mut url = reqwest::Url::parse("http://pfp.invalid/work_pools/")
        .map_err(|error| PfpError::Config(format!("invalid internal work-pool URL: {error}")))?;
    url.path_segments_mut()
        .map_err(|_| {
            PfpError::Config("internal work-pool URL cannot hold path segments".to_string())
        })?
        .pop_if_empty()
        .push(name);
    Ok(url.path().to_string())
}

fn validate_work_pool_name(name: &str) -> Result<()> {
    if name.is_empty() || name.contains('/') {
        return Err(PfpError::Validation(
            "work pool name must not be empty or contain '/'".to_string(),
        ));
    }
    Ok(())
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
    async fn list_deployments_fetches_every_page() {
        let mut server = mockito::Server::new_async().await;
        let first_page: Vec<serde_json::Value> = (0..100)
            .map(|index| {
                serde_json::json!({
                    "name": format!("deployment-{index}"),
                    "flow_id": "flow-1"
                })
            })
            .collect();
        let first_page_mock = server
            .mock("POST", "/deployments/filter")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({"limit": 100, "offset": 0}).to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&first_page).unwrap())
            .expect(1)
            .create_async()
            .await;
        let second_page_mock = server
            .mock("POST", "/deployments/filter")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({"limit": 100, "offset": 100}).to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"name":"deployment-100","flow_id":"flow-1"}]"#)
            .expect(1)
            .create_async()
            .await;
        let flow_mock = server
            .mock("POST", "/flows/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"flow-1","name":"test_flow"}]"#)
            .expect(1)
            .create_async()
            .await;

        let result = test_client(&server).list_deployments().await.unwrap();

        assert_eq!(result.len(), 101);
        assert_eq!(result[100]["name"], "deployment-100");
        assert_eq!(result[100]["flow_name"], "test_flow");
        first_page_mock.assert_async().await;
        second_page_mock.assert_async().await;
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
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"tags":["manual","urgent"]}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"run-123","name":"cool-name","state_type":"SCHEDULED"}"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let params = serde_json::json!({"config": {"action": "plan"}});
        let result = client
            .create_flow_run("dep-id", params, vec!["urgent".to_string()])
            .await
            .unwrap();

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

    #[tokio::test]
    async fn delete_deployment_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("DELETE", "/deployments/dep-id")
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .with_status(204)
            .expect(1)
            .create_async()
            .await;

        test_client(&server)
            .delete_deployment("dep-id")
            .await
            .unwrap();

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn delete_deployment_surfaces_api_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("DELETE", "/deployments/dep-id")
            .with_status(409)
            .with_body("deployment still in use")
            .expect(1)
            .create_async()
            .await;

        let error = test_client(&server)
            .delete_deployment("dep-id")
            .await
            .unwrap_err();

        assert!(
            matches!(error, PfpError::Api(ref message)
                if message.contains("409 Conflict")
                    && message.contains("deployment still in use")),
            "{error}"
        );
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn reads_deployment_schedules() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/deployments/dep-id/schedules")
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"id":"schedule-1","active":false,"slug":"daily"},{"id":"schedule-2","active":true}]"#,
            )
            .expect(1)
            .create_async()
            .await;

        let schedules = test_client(&server)
            .read_deployment_schedules("dep-id")
            .await
            .unwrap();

        assert_eq!(schedules.len(), 2);
        assert!(!schedules[0].active);
        assert_eq!(schedules[0].slug.as_deref(), Some("daily"));
        assert!(schedules[1].active);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn updates_one_deployment_schedule_active_state() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("PATCH", "/deployments/dep-id/schedules/schedule-1")
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({"active": true}).to_string(),
            ))
            .with_status(204)
            .expect(1)
            .create_async()
            .await;

        test_client(&server)
            .set_deployment_schedule_active("dep-id", "schedule-1", true)
            .await
            .unwrap();

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn get_flow_run_logs_single_page() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/logs/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"level":20,"message":"hello","timestamp":"2026-01-01T00:00:00Z"},{"level":20,"message":"world","timestamp":"2026-01-01T00:00:01Z"}]"#)
            .expect(1)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.get_flow_run_logs("run-1", 10_000, 0).await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["message"], "hello");
        assert_eq!(result[1]["message"], "world");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn get_flow_run_logs_multi_page() {
        let mut server = mockito::Server::new_async().await;

        // Build a response with exactly 200 entries (one full page)
        let page1: Vec<serde_json::Value> = (0..200)
            .map(|i| serde_json::json!({"level":20,"message":format!("msg-{}", i),"timestamp":"2026-01-01T00:00:00Z"}))
            .collect();
        let page2 = vec![
            serde_json::json!({"level":20,"message":"msg-200","timestamp":"2026-01-01T00:00:01Z"}),
            serde_json::json!({"level":20,"message":"msg-201","timestamp":"2026-01-01T00:00:02Z"}),
        ];

        let mock1 = server
            .mock("POST", "/logs/filter")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"offset":0}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&page1).unwrap())
            .expect(1)
            .create_async()
            .await;

        let mock2 = server
            .mock("POST", "/logs/filter")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"offset":200}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&page2).unwrap())
            .expect(1)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.get_flow_run_logs("run-1", 10_000, 0).await.unwrap();

        assert_eq!(result.len(), 202);
        assert_eq!(result[0]["message"], "msg-0");
        assert_eq!(result[201]["message"], "msg-201");
        mock1.assert_async().await;
        mock2.assert_async().await;
    }

    #[tokio::test]
    async fn get_flow_run_logs_respects_limit() {
        let mut server = mockito::Server::new_async().await;

        // Return 150 entries — but we set limit to 150, which is less than page size 200
        // so the request should ask for limit=150 and get 150 back, then stop
        let entries: Vec<serde_json::Value> = (0..150)
            .map(|i| serde_json::json!({"level":20,"message":format!("msg-{}", i),"timestamp":"2026-01-01T00:00:00Z"}))
            .collect();

        let mock = server
            .mock("POST", "/logs/filter")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(serde_json::to_string(&entries).unwrap())
            .expect(1)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.get_flow_run_logs("run-1", 150, 0).await.unwrap();

        assert_eq!(result.len(), 150);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn get_flow_run_logs_from_offset() {
        let mut server = mockito::Server::new_async().await;

        let mock = server
            .mock("POST", "/logs/filter")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"offset":5}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"level":20,"message":"new-msg","timestamp":"2026-01-01T00:01:00Z"}]"#)
            .expect(1)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.get_flow_run_logs("run-1", 100, 5).await.unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["message"], "new-msg");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn filter_flow_runs_global_success() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/flow_runs/filter")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"sort":"START_TIME_DESC","limit":50}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"aaa-111","name":"run-1","state_type":"COMPLETED"},{"id":"bbb-222","name":"run-2","state_type":"RUNNING"}]"#)
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.filter_flow_runs_global(50).await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["id"], "aaa-111");
        assert_eq!(result[1]["id"], "bbb-222");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn filter_flow_runs_global_api_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/flow_runs/filter")
            .with_status(500)
            .with_body("Internal Server Error")
            .create_async()
            .await;

        let client = test_client(&server);
        let result = client.filter_flow_runs_global(50).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PfpError::Api(ref msg) if msg.contains("500")));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn gets_exact_work_pool() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/work_pools/docker-secure")
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"name":"docker-secure","type":"docker","is_paused":true,"status":"PAUSED"}"#,
            )
            .expect(1)
            .create_async()
            .await;

        let pool = test_client(&server)
            .get_work_pool("docker-secure")
            .await
            .unwrap();

        assert_eq!(pool.name, "docker-secure");
        assert_eq!(pool.r#type.as_deref(), Some("docker"));
        assert!(pool.is_paused);
        assert_eq!(pool.status.as_deref(), Some("PAUSED"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn counts_every_nonterminal_flow_run_for_exact_work_pool() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/flow_runs/count")
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({
                    "work_pools": {
                        "name": {
                            "any_": ["docker-secure"]
                        }
                    },
                    "flow_runs": {
                        "state": {
                            "type": {
                                "not_any_": [
                                    "COMPLETED",
                                    "FAILED",
                                    "CANCELLED",
                                    "CRASHED"
                                ]
                            }
                        }
                    }
                })
                .to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body("3")
            .expect(1)
            .create_async()
            .await;

        let count = test_client(&server)
            .count_nonterminal_flow_runs_for_work_pool("docker-secure")
            .await
            .unwrap();

        assert_eq!(count, 3);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn lists_credential_safe_workers_for_exact_work_pool() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/work_pools/docker-secure/workers/filter")
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({
                    "workers": null,
                    "offset": 0,
                    "limit": 200
                })
                .to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[{"name":"worker-1","status":"ONLINE","last_heartbeat_time":"2026-08-09T12:00:00Z","heartbeat_interval_seconds":30,"metadata":{"token":"SENTINEL_SECRET"}}]"#,
            )
            .expect(1)
            .create_async()
            .await;

        let workers = test_client(&server)
            .list_workers_for_work_pool("docker-secure")
            .await
            .unwrap();

        assert_eq!(workers.len(), 1);
        assert_eq!(workers[0].name, "worker-1");
        assert!(!serde_json::to_string(&workers)
            .unwrap()
            .contains("SENTINEL_SECRET"));
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn updates_and_verifies_work_pool_pause_state() {
        let mut server = mockito::Server::new_async().await;
        let patch = server
            .mock("PATCH", "/work_pools/docker-secure")
            .match_header("authorization", "Basic dGVzdDp0ZXN0")
            .match_body(mockito::Matcher::JsonString(
                serde_json::json!({"is_paused": false}).to_string(),
            ))
            .with_status(204)
            .expect(1)
            .create_async()
            .await;
        let get = server
            .mock("GET", "/work_pools/docker-secure")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"name":"docker-secure","type":"docker","is_paused":false,"status":"READY"}"#,
            )
            .expect(1)
            .create_async()
            .await;

        let pool = test_client(&server)
            .set_work_pool_paused("docker-secure", false)
            .await
            .unwrap();

        assert!(!pool.is_paused);
        patch.assert_async().await;
        get.assert_async().await;
    }

    #[tokio::test]
    async fn rejects_unverified_work_pool_pause_state() {
        let mut server = mockito::Server::new_async().await;
        let patch = server
            .mock("PATCH", "/work_pools/docker-secure")
            .with_status(204)
            .expect(1)
            .create_async()
            .await;
        let get = server
            .mock("GET", "/work_pools/docker-secure")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"name":"docker-secure","is_paused":true,"status":"PAUSED"}"#)
            .expect(1)
            .create_async()
            .await;

        let error = test_client(&server)
            .set_work_pool_paused("docker-secure", false)
            .await
            .unwrap_err();

        assert!(
            matches!(error, PfpError::Api(ref message) if message.contains("did not converge"))
        );
        patch.assert_async().await;
        get.assert_async().await;
    }

    #[tokio::test]
    async fn rejects_invalid_work_pool_name_before_http() {
        let server = mockito::Server::new_async().await;
        let client = test_client(&server);

        for name in ["", "pool/queue"] {
            let error = client.get_work_pool(name).await.unwrap_err();
            assert!(matches!(error, PfpError::Validation(_)));

            let error = client
                .count_nonterminal_flow_runs_for_work_pool(name)
                .await
                .unwrap_err();
            assert!(matches!(error, PfpError::Validation(_)));

            let error = client.list_workers_for_work_pool(name).await.unwrap_err();
            assert!(matches!(error, PfpError::Validation(_)));
        }
    }

    #[test]
    fn encodes_work_pool_name_as_one_path_segment() {
        for (name, expected) in [
            ("docker-secure", "/work_pools/docker-secure"),
            ("pool\\queue", "/work_pools/pool%5Cqueue"),
            ("pool%2Fqueue", "/work_pools/pool%252Fqueue"),
            ("pool?query", "/work_pools/pool%3Fquery"),
            ("pool#fragment", "/work_pools/pool%23fragment"),
        ] {
            assert_eq!(work_pool_path(name).unwrap(), expected);
        }
    }

    #[tokio::test]
    async fn resume_fails_if_verification_response_omits_pause_state() {
        let mut server = mockito::Server::new_async().await;
        let patch = server
            .mock("PATCH", "/work_pools/docker-secure")
            .with_status(204)
            .expect(1)
            .create_async()
            .await;
        let get = server
            .mock("GET", "/work_pools/docker-secure")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"name":"docker-secure","status":"READY"}"#)
            .expect(1)
            .create_async()
            .await;

        let error = test_client(&server)
            .set_work_pool_paused("docker-secure", false)
            .await
            .unwrap_err();

        assert!(matches!(error, PfpError::Http(_)));
        patch.assert_async().await;
        get.assert_async().await;
    }
}
