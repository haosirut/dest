//! CLI command handlers.

use crate::api::run_api_server;
use crate::config::NodeConfig;
use anyhow::{Context, Result};
use clap::Subcommand;
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Subcommand, Debug)]
pub enum CliCommand {
    /// Initialize a new node (generate identity, create directories)
    Init {
        /// Node data directory
        #[arg(long, default_value = "~/.vaultkeeper")]
        data_dir: String,
    },
    /// Start the daemon
    Start {
        /// Node data directory
        #[arg(long, default_value = "~/.vaultkeeper")]
        data_dir: String,
        /// API listen address
        #[arg(long, default_value = "127.0.0.1:8080")]
        api_addr: String,
    },
    /// Stop the running daemon
    Stop,
    /// Show node status
    Status {
        /// Node data directory
        #[arg(long, default_value = "~/.vaultkeeper")]
        data_dir: String,
    },
    /// Upload a file
    Upload {
        /// File path to upload
        file: String,
        /// Replication factor
        #[arg(long, default_value = "3")]
        replication: u32,
    },
    /// Download a file by chunk ID
    Download {
        /// Chunk ID to download
        chunk_id: String,
        /// Output file path
        #[arg(long)]
        output: String,
    },
    /// Show account balance
    Balance,
    /// Manage encryption keys
    Keys {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// Recover from mnemonic
    Recover {
        /// Mnemonic phrase (space-separated words)
        #[arg(long)]
        mnemonic: String,
        /// Passphrase for key derivation
        #[arg(long, default_value = "")]
        passphrase: String,
    },
    /// Run as systemd service (called by systemd)
    Daemon {
        /// Node data directory
        #[arg(long, default_value = "~/.vaultkeeper")]
        data_dir: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum KeyAction {
    /// Generate a new key pair
    Generate,
    /// Show public key
    Show,
    /// Backup key via BIP39 mnemonic
    Backup,
}

pub async fn handle_command(command: CliCommand, _config_path: Option<String>) -> Result<()> {
    match command {
        CliCommand::Init { data_dir } => cmd_init(&data_dir).await,
        CliCommand::Start { data_dir, api_addr } => cmd_start(&data_dir, &api_addr).await,
        CliCommand::Stop => cmd_stop(),
        CliCommand::Status { data_dir } => cmd_status(&data_dir),
        CliCommand::Upload { file, replication } => cmd_upload(&file, replication).await,
        CliCommand::Download { chunk_id, output } => cmd_download(&chunk_id, &output).await,
        CliCommand::Balance => cmd_balance(),
        CliCommand::Keys { action } => cmd_keys(action).await,
        CliCommand::Recover { mnemonic, passphrase } => cmd_recover(&mnemonic, &passphrase),
        CliCommand::Daemon { data_dir } => cmd_daemon(&data_dir).await,
    }
}

async fn cmd_init(data_dir: &str) -> Result<()> {
    let expanded = shellexpand::tilde(data_dir).to_string();
    let path = PathBuf::from(&expanded);

    std::fs::create_dir_all(&path)
        .with_context(|| format!("Failed to create data directory: {}", expanded))?;
    std::fs::create_dir_all(path.join("shards"))
        .with_context(|| "Failed to create shards directory")?;
    std::fs::create_dir_all(path.join("ledger"))
        .with_context(|| "Failed to create ledger directory")?;

    // Generate node identity
    let config = NodeConfig::generate(&path)?;

    info!("Node initialized successfully at: {}", expanded);
    info!("Peer ID: {}", config.peer_id);
    info!("Public key: {}", hex::encode(&config.public_key));

    Ok(())
}

async fn cmd_start(data_dir: &str, api_addr: &str) -> Result<()> {
    let expanded = shellexpand::tilde(data_dir).to_string();
    info!("Starting VaultKeeper daemon...");
    info!("Data directory: {}", expanded);
    info!("API address: {}", api_addr);

    // Load config
    let _config = NodeConfig::load(&expanded)?;

    // Initialize P2P node
    let p2p_config = vaultkeeper_p2p::P2pConfig::default();
    let _node = vaultkeeper_p2p::P2pNode::new(p2p_config).await?;
    info!("P2P node started: {}", _node.peer_id_str());

    // Initialize ledger
    let ledger_path = PathBuf::from(&expanded).join("ledger").join("vaultkeeper.db");
    let _ledger = vaultkeeper_ledger::LedgerStore::open(&ledger_path)?;
    info!("Ledger opened: {}", ledger_path.display());

    // Initialize billing
    let _billing = vaultkeeper_billing::BillingAccount::new(rust_decimal::Decimal::ZERO);
    info!("Billing initialized");

    // Initialize storage
    let shard_path = PathBuf::from(&expanded).join("shards");
    let _storage = vaultkeeper_storage::shard_store::ShardStore::new(&shard_path)?;
    info!("Storage initialized at: {}", shard_path.display());

    // Start API server
    info!("Starting API server on {}", api_addr);
    run_api_server(api_addr).await?;

    Ok(())
}

fn cmd_stop() -> Result<()> {
    info!("Attempting to stop VaultKeeper daemon...");
    // In production, this would send SIGTERM to the running process
    // or communicate via a UNIX socket / PID file
    warn!("Daemon stop: use 'systemctl stop vaultkeeperd' for systemd-managed nodes");
    Ok(())
}

fn cmd_status(data_dir: &str) -> Result<()> {
    let expanded = shellexpand::tilde(data_dir).to_string();
    let path = PathBuf::from(&expanded);
    let config_path = path.join("config.json");

    if !config_path.exists() {
        anyhow::bail!("Node not initialized at: {}", expanded);
    }

    let config = NodeConfig::load(&expanded)?;
    println!("=== VaultKeeper Node Status ===");
    println!("Peer ID:       {}", config.peer_id);
    println!("Data directory: {}", expanded);
    println!("Initialized:    Yes");
    println!("Version:        {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

async fn cmd_upload(file: &str, replication: u32) -> Result<()> {
    let path = PathBuf::from(file);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file);
    }

    let data = std::fs::read(&path)?;
    info!("Uploading {} bytes with replication factor {}", data.len(), replication);

    // Chunk the data
    let chunks = vaultkeeper_core::chunking::chunk_data(&data);
    let chunk_ids = vaultkeeper_core::chunking::generate_chunk_ids(&chunks);

    info!("Split into {} chunks", chunks.len());
    for (i, id) in chunk_ids.iter().enumerate() {
        println!("  Chunk {}: {} ({} bytes)", i, id, chunks[i].len());
    }

    // In production: encrypt, erasure code, distribute to peers
    info!("Upload complete (simulation mode)");
    Ok(())
}

async fn cmd_download(chunk_id: &str, _output: &str) -> Result<()> {
    let id = vaultkeeper_core::ChunkId::from_hex(chunk_id)
        .with_context(|| format!("Invalid chunk ID: {}", chunk_id))?;

    info!("Downloading chunk: {}", id);
    // In production: request shards from peers, reassemble, decrypt
    warn!("Download not yet connected to P2P network (simulation mode)");
    Ok(())
}

fn cmd_balance() -> Result<()> {
    // In production: read balance from local ledger / billing state
    println!("=== Account Balance ===");
    println!("Balance: 0.00 RUB");
    println!("Status: Active");
    Ok(())
}

async fn cmd_keys(action: KeyAction) -> Result<()> {
    match action {
        KeyAction::Generate => {
            let mnemonic = vaultkeeper_core::bip39_recovery::generate_mnemonic(
                vaultkeeper_core::MnemonicWordCount::Twelve,
            )?;
            println!("=== New Recovery Key ===");
            println!("Mnemonic: {}", mnemonic);
            println!("");
            println!("IMPORTANT: Write down these words and store them securely.");
            println!("This is the ONLY way to recover your data.");
        }
        KeyAction::Show => {
            println!("Public key: (not yet implemented for key display)");
        }
        KeyAction::Backup => {
            println!("Backup: Use 'vaultkeeperd keys generate' to create a new recovery mnemonic.");
        }
    }
    Ok(())
}

fn cmd_recover(mnemonic: &str, passphrase: &str) -> Result<()> {
    let parsed = vaultkeeper_core::bip39_recovery::parse_mnemonic(mnemonic)?;
    let _key = vaultkeeper_core::bip39_recovery::recover_key_from_mnemonic(&parsed, passphrase)?;

    println!("=== Recovery Successful ===");
    println!("Mnemonic validated: {} words", parsed.word_count());
    println!("Key derived successfully");
    println!("");
    println!("NOTE: This validated the mnemonic but did not restore data.");
    println!("Run 'vaultkeeperd start' to resume operation with recovered key.");
    Ok(())
}

async fn cmd_daemon(data_dir: &str) -> Result<()> {
    cmd_start(data_dir, "127.0.0.1:8080").await
}
