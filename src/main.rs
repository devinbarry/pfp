mod client;
mod commands;
mod config;
mod error;
mod models;
mod output;
mod params;
mod resolve;

use clap::{Parser, Subcommand};
use client::PrefectClient;
use config::Config;
use error::Result;

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
        /// Flow run ID
        flow_run_id: String,
        /// Maximum number of log entries to fetch
        #[arg(long)]
        limit: Option<usize>,
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
        /// Flow run ID
        flow_run_id: String,
    },
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        std::process::exit(e.exit_code());
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

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
            json,
        } => {
            let config = Config::load()?;
            let client = PrefectClient::new(config);
            commands::logs::run(client, flow_run_id, limit, json).await
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
