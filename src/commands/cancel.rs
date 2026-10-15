use crate::client::PrefectClient;
use crate::error::{PfpError, Result};
use crate::resolve;

pub async fn run(client: PrefectClient, flow_run_id: String) -> Result<()> {
    let resolved_id = resolve::resolve_flow_run(&client, &flow_run_id).await?;
    let result = client.cancel_flow_run(&resolved_id).await?;
    let status = result["status"].as_str().unwrap_or("UNKNOWN");
    // Prefect substitutes CANCELLED for scheduled/suspended runs with no
    // infrastructure. That successful transition is reported as REJECT.
    let cancelled_instead = status == "REJECT"
        && matches!(
            result["state"]["type"].as_str(),
            Some("CANCELLING" | "CANCELLED")
        );
    if status != "ACCEPT" && !cancelled_instead {
        let reason = result["details"]["reason"]
            .as_str()
            .unwrap_or("no reason given");
        return Err(PfpError::Api(format!(
            "Prefect did not accept cancellation for flow run {resolved_id}: {status}: {reason}"
        )));
    }
    eprintln!("Cancellation requested for flow run: {resolved_id}");
    Ok(())
}
