use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Deployment {
    pub id: String,
    pub name: String,
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

#[allow(dead_code)]
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
    pub parameters: serde_json::Value,
}

#[allow(dead_code)]
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
        let secs = self.total_run_time as u64;
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

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize)]
pub struct LogEntry {
    pub level: String,
    pub message: String,
    pub timestamp: String,
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
            "id": "a", "name": "r", "total_run_time": 45.0
        }))
        .unwrap();
        assert_eq!(fr.duration_str(), "45s");
    }

    #[test]
    fn flow_run_duration_with_minutes() {
        let fr: FlowRun = serde_json::from_value(json!({
            "id": "a", "name": "r", "total_run_time": 125.0
        }))
        .unwrap();
        assert_eq!(fr.duration_str(), "2m 05s");
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
            "level": "INFO", "message": "Flow started", "timestamp": "2026-02-21T17:34:05.301Z"
        }))
        .unwrap();
        assert_eq!(entry.level, "INFO");
    }
}
