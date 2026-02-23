use crate::client::PrefectClient;
use crate::error::Result;
use crate::resolve;

pub async fn run(client: PrefectClient, flow_run_id: String) -> Result<()> {
    let resolved_id = resolve::resolve_flow_run(&client, &flow_run_id).await?;
    client.cancel_flow_run(&resolved_id).await?;
    eprintln!("Cancelled flow run: {}", resolved_id);
    Ok(())
}
