use crate::client::PrefectClient;
use crate::error::{PfpError, Result};
use crate::models::{FlowRun, LogEntry};
use crate::output;
use crate::resolve;

const DEFAULT_LIMIT: usize = 10_000;
const FOLLOW_POLL_SECS: u64 = 3;

pub async fn run(
    client: PrefectClient,
    flow_run_id: String,
    limit: Option<usize>,
    follow: bool,
    json: bool,
) -> Result<()> {
    let resolved_id = resolve::resolve_flow_run(&client, &flow_run_id).await?;

    // Initial fetch
    let effective_limit = limit.unwrap_or(DEFAULT_LIMIT);
    let values = client
        .get_flow_run_logs(&resolved_id, effective_limit, 0)
        .await?;
    let logs: Vec<LogEntry> = values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    let mut total_seen = logs.len();

    if limit.is_none() && logs.len() >= DEFAULT_LIMIT && !follow {
        eprintln!(
            "Warning: output capped at {} entries. Use --limit to adjust.",
            DEFAULT_LIMIT
        );
    }

    if json {
        output::print_json(&logs);
    } else if logs.is_empty() && !follow {
        println!("No logs found for flow run {}", resolved_id);
    } else {
        output::print_logs(&logs);
    }

    if !follow {
        return Ok(());
    }

    // Follow mode: poll for new logs until flow run is terminal
    eprintln!("Following logs (Ctrl+C to stop)...");

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(FOLLOW_POLL_SECS)).await;

        // Fetch new logs from where we left off
        let new_values = client
            .get_flow_run_logs(&resolved_id, DEFAULT_LIMIT, total_seen)
            .await?;
        let new_logs: Vec<LogEntry> = new_values
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();

        if !new_logs.is_empty() {
            total_seen += new_logs.len();
            if json {
                output::print_json(&new_logs);
            } else {
                output::print_logs(&new_logs);
            }
        }

        // Check if flow run reached a terminal state
        let flow_run_value = client.get_flow_run(&resolved_id).await?;
        let flow_run: FlowRun =
            serde_json::from_value(flow_run_value).map_err(|e| PfpError::Api(e.to_string()))?;

        if flow_run.is_terminal() {
            // Final fetch to catch any stragglers
            let final_values = client
                .get_flow_run_logs(&resolved_id, DEFAULT_LIMIT, total_seen)
                .await?;
            let final_logs: Vec<LogEntry> = final_values
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect();

            if !final_logs.is_empty() {
                if json {
                    output::print_json(&final_logs);
                } else {
                    output::print_logs(&final_logs);
                }
            }

            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn test_client(server: &mockito::Server) -> PrefectClient {
        let config = Config {
            api_url: server.url(),
            auth_header: Some("Basic dGVzdDp0ZXN0".to_string()),
        };
        PrefectClient::new(config)
    }

    #[tokio::test]
    async fn follow_polls_and_stops_on_terminal() {
        let mut server = mockito::Server::new_async().await;
        let flow_run_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

        // Poll 1: initial fetch returns 2 logs
        let logs_mock_1 = server
            .mock("POST", "/logs/filter")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"offset":0}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                {"level":20,"message":"Starting","timestamp":"2026-01-01T00:00:00Z"},
                {"level":20,"message":"Working","timestamp":"2026-01-01T00:00:01Z"}
            ]"#,
            )
            .expect(1)
            .create_async()
            .await;

        // Poll 2: new logs from offset 2
        let logs_mock_2 = server
            .mock("POST", "/logs/filter")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"offset":2}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                {"level":20,"message":"Almost done","timestamp":"2026-01-01T00:00:02Z"}
            ]"#,
            )
            .expect(1)
            .create_async()
            .await;

        // Poll 3 + final fetch after terminal: no new logs from offset 3 (hit twice)
        let logs_mock_3 = server
            .mock("POST", "/logs/filter")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"offset":3}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .expect(2)
            .create_async()
            .await;

        // Flow run state: first check returns RUNNING, second returns COMPLETED
        let state_mock_running = server
            .mock("GET", format!("/flow_runs/{}", flow_run_id).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{"id":"{}","name":"test-run","state_type":"RUNNING","state_name":"Running"}}"#,
                flow_run_id
            ))
            .expect(1)
            .create_async()
            .await;

        let state_mock_completed = server
            .mock("GET", format!("/flow_runs/{}", flow_run_id).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{"id":"{}","name":"test-run","state_type":"COMPLETED","state_name":"Completed"}}"#,
                flow_run_id
            ))
            .expect(1)
            .create_async()
            .await;

        let client = test_client(&server);

        // Run with follow=true — should complete without hanging
        let result = run(client, flow_run_id.to_string(), None, true, false).await;

        assert!(result.is_ok());
        logs_mock_1.assert_async().await;
        logs_mock_2.assert_async().await;
        logs_mock_3.assert_async().await;
        state_mock_running.assert_async().await;
        state_mock_completed.assert_async().await;
    }

    #[tokio::test]
    async fn follow_stops_immediately_if_already_terminal() {
        let mut server = mockito::Server::new_async().await;
        let flow_run_id = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

        // Initial fetch returns logs
        let logs_mock_initial = server
            .mock("POST", "/logs/filter")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"offset":0}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"[
                {"level":20,"message":"Done","timestamp":"2026-01-01T00:00:00Z"}
            ]"#,
            )
            .expect(1)
            .create_async()
            .await;

        // Follow poll: no new logs at offset 1
        let logs_mock_poll = server
            .mock("POST", "/logs/filter")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"offset":1}"#.to_string(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[]"#)
            .expect(2) // once for poll, once for final fetch
            .create_async()
            .await;

        // Flow run is already completed
        let state_mock = server
            .mock("GET", format!("/flow_runs/{}", flow_run_id).as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{"id":"{}","name":"test-run","state_type":"COMPLETED","state_name":"Completed"}}"#,
                flow_run_id
            ))
            .expect(1)
            .create_async()
            .await;

        let client = test_client(&server);

        let result = run(client, flow_run_id.to_string(), None, true, false).await;

        assert!(result.is_ok());
        logs_mock_initial.assert_async().await;
        logs_mock_poll.assert_async().await;
        state_mock.assert_async().await;
    }
}
