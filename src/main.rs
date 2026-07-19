mod client;
mod commands;
mod config;
mod error;
mod logger;
mod models;
mod output;
mod params;
mod resolve;
mod validate;

use clap::{Parser, Subcommand};
use client::PrefectClient;
use config::Config;
use error::Result;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "pfp", version, about = "Prefect CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List deployments
    Ls {
        #[arg(long)]
        json: bool,
    },
    /// Run a deployment
    Run {
        /// Deployment name (substring match)
        query: String,
        #[arg(long)]
        watch: bool,
        #[arg(long = "set", num_args = 1)]
        sets: Vec<String>,
        /// Read flow-run parameters as JSON from a file, or "-" for stdin.
        /// Merged under any --set overrides (--set wins).
        #[arg(long = "params-file")]
        params_file: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show recent flow runs for a deployment
    Runs {
        /// Deployment name (substring match)
        query: String,
        #[arg(long)]
        json: bool,
    },
    /// Inspect one flow run by its full UUID
    Inspect {
        /// Full flow run UUID
        flow_run_id: String,
        #[arg(long)]
        json: bool,
    },
    /// Show logs for a flow run
    Logs {
        /// Flow run ID or UUID prefix
        flow_run_id: String,
        /// Maximum number of log entries to fetch
        #[arg(long)]
        limit: Option<usize>,
        /// Follow log output (like tail -f)
        #[arg(long, short = 'f', visible_alias = "tail")]
        follow: bool,
        #[arg(long)]
        json: bool,
    },
    /// Pause a deployment
    Pause {
        /// Deployment name (substring match)
        query: String,
    },
    /// Resume a deployment
    Resume {
        /// Deployment name (substring match)
        query: String,
    },
    /// Cancel a running flow run
    Cancel {
        /// Flow run ID or UUID prefix
        flow_run_id: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Resolve the --params-file payload exactly once so the stdin ("-") stream
    // is not consumed twice (once for logging, once for execution).
    let params_payload: Option<Result<serde_json::Value>> = match &cli.command {
        Commands::Run {
            params_file: Some(path),
            ..
        } => Some(commands::run::load_params_file(path)),
        _ => None,
    };

    let (cmd_name, cmd_args) = describe_command(&cli.command, params_payload.as_ref());
    let start = Instant::now();
    let result = run(cli, params_payload).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    logger::log_invocation(&cmd_name, cmd_args, &result, duration_ms);

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(e.exit_code());
    }
}

/// Extract subcommand name and args for logging.
/// Note: --set values and --params-file payloads are logged in plaintext
/// (same exposure as shell history).
fn describe_command(
    cmd: &Commands,
    params_payload: Option<&Result<serde_json::Value>>,
) -> (String, serde_json::Value) {
    match cmd {
        Commands::Ls { json } => ("ls".into(), serde_json::json!({ "json": json })),
        Commands::Run {
            query,
            watch,
            sets,
            params_file,
            json,
        } => {
            let params_log = params_file.as_ref().map(|p| match params_payload {
                Some(Ok(v)) => serde_json::json!({ "path": p, "payload": v }),
                _ => serde_json::json!({ "path": p }),
            });
            (
                "run".into(),
                serde_json::json!({
                    "query": query,
                    "watch": watch,
                    "sets": sets,
                    "params_file": params_log,
                    "json": json,
                }),
            )
        }
        Commands::Runs { query, json } => (
            "runs".into(),
            serde_json::json!({ "query": query, "json": json }),
        ),
        Commands::Inspect { flow_run_id, json } => (
            "inspect".into(),
            serde_json::json!({ "flow_run_id": flow_run_id, "json": json }),
        ),
        Commands::Logs {
            flow_run_id,
            limit,
            follow,
            json,
        } => (
            "logs".into(),
            serde_json::json!({ "flow_run_id": flow_run_id, "limit": limit, "follow": follow, "json": json }),
        ),
        Commands::Pause { query } => ("pause".into(), serde_json::json!({ "query": query })),
        Commands::Resume { query } => ("resume".into(), serde_json::json!({ "query": query })),
        Commands::Cancel { flow_run_id } => (
            "cancel".into(),
            serde_json::json!({ "flow_run_id": flow_run_id }),
        ),
    }
}

async fn run(cli: Cli, params_payload: Option<Result<serde_json::Value>>) -> Result<()> {
    match cli.command {
        Commands::Ls { json } => {
            let config = Config::load()?;
            let client = PrefectClient::new(config);
            commands::ls::run(client, json).await
        }
        Commands::Run {
            query,
            watch,
            sets,
            json,
            ..
        } => {
            // Surface a bad --params-file before any config/network work.
            let params_base = params_payload.transpose()?;
            let config = Config::load()?;
            let client = PrefectClient::new(config);
            commands::run::run(client, query, watch, sets, params_base, json).await
        }
        Commands::Runs { query, json } => {
            let config = Config::load()?;
            let client = PrefectClient::new(config);
            commands::runs::run(client, query, json).await
        }
        Commands::Inspect { flow_run_id, json } => {
            let config = Config::load()?;
            let client = PrefectClient::new(config);
            commands::inspect::run(client, flow_run_id, json).await
        }
        Commands::Logs {
            flow_run_id,
            limit,
            follow,
            json,
        } => {
            let config = Config::load()?;
            let client = PrefectClient::new(config);
            commands::logs::run(client, flow_run_id, limit, follow, json).await
        }
        Commands::Pause { query } => {
            let config = Config::load()?;
            let client = PrefectClient::new(config);
            commands::pause::run(client, query).await
        }
        Commands::Resume { query } => {
            let config = Config::load()?;
            let client = PrefectClient::new(config);
            commands::resume::run(client, query).await
        }
        Commands::Cancel { flow_run_id } => {
            let config = Config::load()?;
            let client = PrefectClient::new(config);
            commands::cancel::run(client, flow_run_id).await
        }
    }
}
