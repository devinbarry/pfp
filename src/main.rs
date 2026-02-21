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
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {}", e);
        std::process::exit(2);
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
    }
}
