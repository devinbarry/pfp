use crate::client::PrefectClient;
use crate::error::Result;
use crate::models::Deployment;
use crate::output;

pub async fn run(client: PrefectClient, json: bool) -> Result<()> {
    let values = client.list_deployments().await?;
    let mut deployments: Vec<Deployment> = values
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    deployments.sort_by_key(|a| a.full_name());

    if json {
        output::print_json(&deployments);
    } else {
        output::print_deployments_table(&deployments);
    }

    Ok(())
}
