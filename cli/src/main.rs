//! VaultKeeper CLI Daemon — headless node management.
//!
//! Usage: vaultkeeperd [COMMAND] [OPTIONS]
//!
//! Commands: init, start, stop, status, upload, download, balance, keys, recover

mod api;
mod commands;
mod config;

use anyhow::Result;
use clap::Parser;
use commands::{CliCommand, handle_command};
use tracing::{info, Level};

#[derive(Parser, Debug)]
#[command(name = "vaultkeeperd", version, about = "VaultKeeper P2P Storage Node Daemon")]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, global = true, default_value = "info")]
    log_level: String,

    /// Configuration file path
    #[arg(long, global = true)]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let level = match cli.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_target(false)
        .with_thread_ids(false)
        .init();

    info!("VaultKeeper Daemon v{}", env!("CARGO_PKG_VERSION"));

    handle_command(cli.command, cli.config).await
}
