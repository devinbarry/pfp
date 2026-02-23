use crate::client::PrefectClient;
use crate::error::Result;
use crate::models::LogEntry;
use crate::output;
use crate::resolve;

const DEFAULT_LIMIT: usize = 10_000;

pub async fn run(
    client: PrefectClient,
    flow_run_id: String,
    limit: Option<usize>,
    json: bool,
) -> Result<()> {
    let resolved_id = resolve::resolve_flow_run(&client, &flow_run_id).await?;
    let effective_limit = limit.unwrap_or(DEFAULT_LIMIT);
    let values = client
        .get_flow_run_logs(&resolved_id, effective_limit)
        .await?;
    let logs: Vec<LogEntry> = values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    if limit.is_none() && logs.len() >= DEFAULT_LIMIT {
        eprintln!(
            "Warning: output capped at {} entries. Use --limit to adjust.",
            DEFAULT_LIMIT
        );
    }

    if json {
        output::print_json(&logs);
    } else if logs.is_empty() {
        println!("No logs found for flow run {}", resolved_id);
    } else {
        output::print_logs(&logs);
    }

    Ok(())
}
