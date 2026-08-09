use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Deployment {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub flow_id: String,
    #[serde(default)]
    pub flow_name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub work_pool_name: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub parameters: serde_json::Value,
    #[serde(default)]
    pub parameter_openapi_schema: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeploymentSchedule {
    pub id: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WorkPool {
    pub name: String,
    #[serde(default)]
    pub r#type: Option<String>,
    pub is_paused: bool,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub concurrency_limit: Option<u64>,
}

/// Credential-safe worker health returned by Prefect's worker API.
///
/// Keep this model deliberately narrow. Unknown API fields are discarded so
/// `--json` cannot accidentally turn into a raw Prefect response dump.
#[derive(Debug, Deserialize, Serialize)]
pub struct WorkerDiagnostic {
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub last_heartbeat_time: Option<String>,
    #[serde(default)]
    pub heartbeat_interval_seconds: Option<u64>,
}

impl Deployment {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.flow_name, self.name)
    }

    pub fn status_str(&self) -> &str {
        if self.paused {
            "paused"
        } else {
            "active"
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct FlowRun {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub state_type: String,
    #[serde(default)]
    pub state_name: String,
    #[serde(default)]
    pub deployment_id: Option<String>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub total_run_time: f64,
    #[serde(default)]
    pub estimated_run_time: f64,
    #[serde(default)]
    pub parameters: serde_json::Value,
}

/// An allowlisted diagnostic view of a flow run.
///
/// Do not add parameters, context, labels, job variables, state data, or raw
/// API payloads here. Those fields can contain credentials. This type exists
/// specifically so `pfp diagnose --json` remains safe to share.
#[derive(Debug, Deserialize, Serialize)]
pub struct FlowRunDiagnostic {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub state_type: Option<String>,
    #[serde(default)]
    pub state_name: Option<String>,
    #[serde(default)]
    pub deployment_id: Option<String>,
    #[serde(default)]
    pub deployment_version: Option<String>,
    #[serde(default)]
    pub work_pool_name: Option<String>,
    #[serde(default)]
    pub work_queue_name: Option<String>,
    #[serde(default)]
    pub infrastructure_pid: Option<String>,
    #[serde(default)]
    pub run_count: u64,
    #[serde(default)]
    pub expected_start_time: Option<String>,
    #[serde(default)]
    pub next_scheduled_start_time: Option<String>,
    #[serde(default)]
    pub start_time: Option<String>,
    #[serde(default)]
    pub end_time: Option<String>,
    #[serde(default)]
    pub total_run_time: f64,
    #[serde(default)]
    pub estimated_run_time: f64,
    #[serde(default)]
    pub estimated_start_time_delta: f64,
}

impl FlowRun {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state_type.as_str(),
            "COMPLETED" | "FAILED" | "CANCELLED" | "CRASHED"
        )
    }

    pub fn is_success(&self) -> bool {
        self.state_type == "COMPLETED"
    }

    pub fn duration_str(&self) -> String {
        let secs = self.estimated_run_time as u64;
        if secs < 60 {
            format!("{}s", secs)
        } else {
            format!("{}m {:02}s", secs / 60, secs % 60)
        }
    }

    pub fn short_id(&self) -> &str {
        if self.id.len() >= 8 {
            &self.id[..8]
        } else {
            &self.id
        }
    }

    pub fn start_time_short(&self) -> String {
        match &self.start_time {
            Some(t) => {
                if t.len() >= 16 {
                    t[..16].replace('T', " ")
                } else {
                    t.clone()
                }
            }
            None => "-".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LogEntry {
    pub level: u32,
    pub message: String,
    pub timestamp: String,
}

impl LogEntry {
    pub fn level_name(&self) -> &str {
        match self.level {
            0..=9 => "TRACE",
            10 => "DEBUG",
            20 => "INFO",
            30 => "WARNING",
            40 => "ERROR",
            _ => "CRITICAL",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deployment_full_name() {
        let d: Deployment = serde_json::from_value(json!({
            "id": "abc",
            "name": "my-deploy-prod",
            "flow_name": "my_flow"
        }))
        .unwrap();
        assert_eq!(d.full_name(), "my_flow/my-deploy-prod");
    }

    #[test]
    fn deployment_status_active() {
        let d: Deployment = serde_json::from_value(json!({
            "id": "abc", "name": "d", "flow_name": "f", "paused": false
        }))
        .unwrap();
        assert_eq!(d.status_str(), "active");
    }

    #[test]
    fn deployment_status_paused() {
        let d: Deployment = serde_json::from_value(json!({
            "id": "abc", "name": "d", "flow_name": "f", "paused": true
        }))
        .unwrap();
        assert_eq!(d.status_str(), "paused");
    }

    #[test]
    fn flow_run_terminal_states() {
        for state in &["COMPLETED", "FAILED", "CANCELLED", "CRASHED"] {
            let fr: FlowRun = serde_json::from_value(json!({
                "id": "abc", "name": "run", "state_type": state, "state_name": state
            }))
            .unwrap();
            assert!(fr.is_terminal(), "{} should be terminal", state);
        }
    }

    #[test]
    fn flow_run_non_terminal_states() {
        for state in &["SCHEDULED", "PENDING", "RUNNING"] {
            let fr: FlowRun = serde_json::from_value(json!({
                "id": "abc", "name": "run", "state_type": state, "state_name": state
            }))
            .unwrap();
            assert!(!fr.is_terminal(), "{} should not be terminal", state);
        }
    }

    #[test]
    fn flow_run_duration_seconds_only() {
        let fr: FlowRun = serde_json::from_value(json!({
            "id": "a", "name": "r", "total_run_time": 45.0, "estimated_run_time": 45.0
        }))
        .unwrap();
        assert_eq!(fr.duration_str(), "45s");
    }

    #[test]
    fn flow_run_duration_with_minutes() {
        let fr: FlowRun = serde_json::from_value(json!({
            "id": "a", "name": "r", "total_run_time": 125.0, "estimated_run_time": 125.0
        }))
        .unwrap();
        assert_eq!(fr.duration_str(), "2m 05s");
    }

    #[test]
    fn flow_run_duration_uses_estimated_run_time_while_running() {
        let fr: FlowRun = serde_json::from_value(json!({
            "id": "a", "name": "r", "state_type": "RUNNING",
            "total_run_time": 0.0, "estimated_run_time": 42.0
        }))
        .unwrap();
        assert_eq!(fr.duration_str(), "42s");
    }

    #[test]
    fn flow_run_short_id() {
        let fr: FlowRun = serde_json::from_value(json!({
            "id": "171a3f55-e9a5-4100-a2dd-efe5c711f847", "name": "r"
        }))
        .unwrap();
        assert_eq!(fr.short_id(), "171a3f55");
    }

    #[test]
    fn flow_run_start_time_formatting() {
        let fr: FlowRun = serde_json::from_value(json!({
            "id": "a", "name": "r", "start_time": "2026-02-21T17:34:05.301Z"
        }))
        .unwrap();
        assert_eq!(fr.start_time_short(), "2026-02-21 17:34");
    }

    #[test]
    fn flow_run_start_time_none() {
        let fr: FlowRun = serde_json::from_value(json!({
            "id": "a", "name": "r", "start_time": null
        }))
        .unwrap();
        assert_eq!(fr.start_time_short(), "-");
    }

    #[test]
    fn log_entry_deserializes() {
        let entry: LogEntry = serde_json::from_value(json!({
            "level": 20, "message": "Flow started", "timestamp": "2026-02-21T17:34:05.301Z"
        }))
        .unwrap();
        assert_eq!(entry.level, 20);
        assert_eq!(entry.level_name(), "INFO");
    }

    #[test]
    fn deployment_deserializes_parameter_openapi_schema() {
        let d: Deployment = serde_json::from_value(json!({
            "id": "abc",
            "name": "d",
            "flow_name": "f",
            "parameter_openapi_schema": {
                "type": "object",
                "properties": {
                    "config": { "$ref": "#/definitions/FlowConfig" }
                },
                "definitions": {
                    "FlowConfig": {
                        "type": "object",
                        "properties": {
                            "dry_run": { "type": "boolean" }
                        }
                    }
                }
            }
        }))
        .unwrap();
        assert!(d.parameter_openapi_schema.is_some());
        let schema = d.parameter_openapi_schema.unwrap();
        assert!(schema.get("properties").is_some());
    }

    #[test]
    fn deployment_missing_schema_is_none() {
        let d: Deployment = serde_json::from_value(json!({
            "id": "abc", "name": "d", "flow_name": "f"
        }))
        .unwrap();
        assert!(d.parameter_openapi_schema.is_none());
    }

    #[test]
    fn work_pool_deserializes_status() {
        let pool: WorkPool = serde_json::from_value(json!({
            "name": "docker-secure",
            "type": "docker",
            "is_paused": true,
            "status": "PAUSED",
            "concurrency_limit": 1
        }))
        .unwrap();

        assert_eq!(pool.name, "docker-secure");
        assert_eq!(pool.r#type.as_deref(), Some("docker"));
        assert!(pool.is_paused);
        assert_eq!(pool.status.as_deref(), Some("PAUSED"));
        assert_eq!(pool.concurrency_limit, Some(1));
    }

    #[test]
    fn worker_diagnostic_discards_unknown_fields() {
        let worker: WorkerDiagnostic = serde_json::from_value(json!({
            "name": "worker-1",
            "status": "ONLINE",
            "last_heartbeat_time": "2026-08-09T12:00:00Z",
            "heartbeat_interval_seconds": 30,
            "metadata": {"token": "SENTINEL_SECRET"}
        }))
        .unwrap();

        let rendered = serde_json::to_string(&worker).unwrap();
        assert!(!rendered.contains("SENTINEL_SECRET"));
        assert_eq!(worker.name, "worker-1");
        assert_eq!(worker.status, "ONLINE");
    }

    #[test]
    fn flow_run_diagnostic_discards_secret_bearing_fields() {
        let diagnostic: FlowRunDiagnostic = serde_json::from_value(json!({
            "id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            "name": "safe-run",
            "state_type": "RUNNING",
            "state_name": "Running",
            "work_pool_name": "docker-secure",
            "work_queue_name": "default",
            "infrastructure_pid": "container:abc123",
            "parameters": {"token": "SENTINEL_SECRET"},
            "context": {"token": "SENTINEL_SECRET"},
            "labels": {"token": "SENTINEL_SECRET"},
            "job_variables": {"token": "SENTINEL_SECRET"},
            "state": {"data": "SENTINEL_SECRET", "message": "SENTINEL_SECRET"}
        }))
        .unwrap();

        let rendered = serde_json::to_string(&diagnostic).unwrap();
        assert!(!rendered.contains("SENTINEL_SECRET"));
        assert!(!rendered.contains("parameters"));
        assert!(!rendered.contains("job_variables"));
        assert_eq!(diagnostic.work_pool_name.as_deref(), Some("docker-secure"));
    }
}
