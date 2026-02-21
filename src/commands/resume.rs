use crate::client::PrefectClient;
use crate::error::Result;
use crate::resolve;

pub async fn run(client: PrefectClient, query: String) -> Result<()> {
    let deployment = resolve::resolve_deployment(&client, &query).await?;
    client.set_deployment_paused(&deployment.id, false).await?;
    eprintln!("Resumed: {}", deployment.full_name());
    Ok(())
}
