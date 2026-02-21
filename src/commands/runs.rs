use crate::client::PrefectClient;
use crate::error::Result;
use crate::models::FlowRun;
use crate::output;
use crate::resolve;

pub async fn run(client: PrefectClient, query: String, json: bool) -> Result<()> {
    let deployment = resolve::resolve_deployment(&client, &query).await?;
    let values = client.filter_flow_runs(&deployment.id, 10).await?;
    let runs: Vec<FlowRun> = values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    if json {
        output::print_json(&runs);
    } else if runs.is_empty() {
        println!("No flow runs found for {}", deployment.full_name());
    } else {
        output::print_flow_runs_table(&runs);
    }

    Ok(())
}
