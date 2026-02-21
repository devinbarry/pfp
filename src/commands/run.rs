use crate::client::PrefectClient;
use crate::error::{PfpError, Result};
use crate::models::FlowRun;
use crate::output;
use crate::params;
use crate::resolve;

pub async fn run(
    client: PrefectClient,
    query: String,
    watch: bool,
    sets: Vec<String>,
    json: bool,
) -> Result<()> {
    let deployment = resolve::resolve_deployment(&client, &query).await?;
    eprintln!("Resolved: {}", deployment.full_name());

    // Build parameters: merge deployment defaults with --set overrides
    let overrides = if sets.is_empty() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        params::build_params(&sets).map_err(PfpError::Config)?
    };
    let parameters = params::merge_params(&deployment.parameters, &overrides);

    // Create flow run
    let run_value = client.create_flow_run(&deployment.id, parameters).await?;
    let flow_run: FlowRun =
        serde_json::from_value(run_value.clone()).map_err(|e| PfpError::Api(e.to_string()))?;

    if json && !watch {
        output::print_json(&run_value);
        return Ok(());
    }

    eprintln!(
        "Created flow run '{}' ({})",
        flow_run.name,
        flow_run.short_id()
    );

    if !watch {
        return Ok(());
    }

    // Watch: poll until terminal state
    let mut last_state = String::new();
    loop {
        let current: FlowRun = serde_json::from_value(client.get_flow_run(&flow_run.id).await?)
            .map_err(|e| PfpError::Api(e.to_string()))?;

        if current.state_name != last_state {
            let ts = current
                .start_time
                .as_deref()
                .or(Some(&current.id[..8]))
                .unwrap_or("-");
            if json {
                output::print_json(&serde_json::json!({
                    "state": current.state_name,
                    "state_type": current.state_type,
                    "timestamp": ts
                }));
            } else {
                output::print_watch_state(&current.state_name, ts);
            }
            last_state = current.state_name.clone();
        }

        if current.is_terminal() {
            if current.is_success() {
                return Ok(());
            } else {
                return Err(PfpError::FlowRunFailed(format!(
                    "{} ({})",
                    current.state_name,
                    current.short_id()
                )));
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}
