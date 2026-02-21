use crate::client::PrefectClient;
use crate::error::Result;
use crate::models::LogEntry;
use crate::output;

pub async fn run(client: PrefectClient, flow_run_id: String, json: bool) -> Result<()> {
    let values = client.get_flow_run_logs(&flow_run_id, 10_000).await?;
    let logs: Vec<LogEntry> = values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    if json {
        output::print_json(&logs);
    } else if logs.is_empty() {
        println!("No logs found for flow run {}", flow_run_id);
    } else {
        output::print_logs(&logs);
    }

    Ok(())
}
