use crate::client::PrefectClient;
use crate::error::Result;

pub async fn run(client: PrefectClient, flow_run_id: String) -> Result<()> {
    client.cancel_flow_run(&flow_run_id).await?;
    eprintln!("Cancelled flow run: {}", flow_run_id);
    Ok(())
}
