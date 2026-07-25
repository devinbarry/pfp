use crate::client::PrefectClient;
use crate::error::Result;
use crate::output;

pub async fn status(client: PrefectClient, name: String, json: bool) -> Result<()> {
    let pool = client.get_work_pool(&name).await?;
    if json {
        output::print_json(&pool);
    } else {
        println!("{:<30} {:<12} {:<8} TYPE", "WORK POOL", "STATUS", "PAUSED");
        output::print_work_pool(&pool);
    }
    Ok(())
}

pub async fn set_paused(client: PrefectClient, name: String, paused: bool) -> Result<()> {
    let pool = client.set_work_pool_paused(&name, paused).await?;
    let action = if paused { "Paused" } else { "Resumed" };
    eprintln!("{action}: {}", pool.name);
    Ok(())
}
