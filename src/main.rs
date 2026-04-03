mod client;
mod commands;
mod config;
mod error;
mod logger;
mod models;
mod output;
mod params;
mod resolve;

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
    /// Show logs for a flow run
    Logs {
        /// Flow run ID or UUID prefix
        flow_run_id: String,
        /// Maximum number of log entries to fetch
        #[arg(long)]
        limit: Option<usize>,
        /// Follow log output (like tail -f)
        #[arg(long, short = 'f')]
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
    let (cmd_name, cmd_args) = describe_command(&cli.command);
    let start = Instant::now();
    let result = run(cli).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    logger::log_invocation(&cmd_name, cmd_args, &result, duration_ms);

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(e.exit_code());
    }
}

/// Extract subcommand name and args for logging.
/// Note: --set values are logged in plaintext (same exposure as shell history).
fn describe_command(cmd: &Commands) -> (String, serde_json::Value) {
    match cmd {
        Commands::Ls { json } => ("ls".into(), serde_json::json!({ "json": json })),
        Commands::Run {
            query,
            watch,
            sets,
            json,
        } => (
            "run".into(),
            serde_json::json!({ "query": query, "watch": watch, "sets": sets, "json": json }),
        ),
        Commands::Runs { query, json } => (
            "runs".into(),
            serde_json::json!({ "query": query, "json": json }),
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

async fn run(cli: Cli) -> Result<()> {
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
        } => {
            let config = Config::load()?;
            let client = PrefectClient::new(config);
            commands::run::run(client, query, watch, sets, json).await
        }
        Commands::Runs { query, json } => {
            let config = Config::load()?;
            let client = PrefectClient::new(config);
            commands::runs::run(client, query, json).await
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
