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
