//! CLI command handlers.
//!
//! Each subcommand maps to an async function that performs the requested
//! operation using the vaultkeeper-core, vaultkeeper-p2p, vaultkeeper-storage,
//! vaultkeeper-billing, and vaultkeeper-ledger crates.

use crate::api::run_api_server;
use crate::config::NodeConfig;
use anyhow::{Context, Result};
use clap::Subcommand;
use std::path::PathBuf;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// CLI command definitions
// ---------------------------------------------------------------------------

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
    /// Upload a file to the P2P network
    Upload {
        /// File path to upload
        file: String,
        /// Replication factor (2, 3, or 4)
        #[arg(long, default_value = "3")]
        replication: u8,
    },
    /// Download a file by file_id
    Download {
        /// File ID (blake3 hex) to download
        file_id: String,
        /// Output file path
        #[arg(long)]
        output: String,
    },
    /// Show account balance
    Balance {
        /// Node data directory
        #[arg(long, default_value = "~/.vaultkeeper")]
        data_dir: String,
    },
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
    /// Generate a new BIP39 recovery key
    Generate,
    /// Show the node's public key
    Show {
        /// Node data directory
        #[arg(long, default_value = "~/.vaultkeeper")]
        data_dir: String,
    },
    /// Display BIP39 recovery mnemonic
    Backup {
        /// Node data directory
        #[arg(long, default_value = "~/.vaultkeeper")]
        data_dir: String,
    },
}

// ---------------------------------------------------------------------------
// Command dispatcher
// ---------------------------------------------------------------------------

pub async fn handle_command(command: CliCommand, _config_path: Option<String>) -> Result<()> {
    match command {
        CliCommand::Init { data_dir } => cmd_init(&data_dir).await,
        CliCommand::Start { data_dir, api_addr } => cmd_start(&data_dir, &api_addr).await,
        CliCommand::Stop => cmd_stop(),
        CliCommand::Status { data_dir } => cmd_status(&data_dir),
        CliCommand::Upload { file, replication } => cmd_upload(&file, replication).await,
        CliCommand::Download { file_id, output } => cmd_download(&file_id, &output).await,
        CliCommand::Balance { data_dir } => cmd_balance(&data_dir),
        CliCommand::Keys { action } => cmd_keys(action).await,
        CliCommand::Recover { mnemonic, passphrase } => cmd_recover(&mnemonic, &passphrase),
        CliCommand::Daemon { data_dir } => cmd_daemon(&data_dir).await,
    }
}

// ---------------------------------------------------------------------------
// Helper: tilde expansion without shellexpand dependency
// ---------------------------------------------------------------------------

/// Expand a leading `~` to the value of `$HOME`.
fn expand_tilde(path: &str) -> String {
    if path.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        if path == "~" {
            home
        } else {
            format!("{}{}", home, &path[1..])
        }
    } else {
        path.to_string()
    }
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

async fn cmd_init(data_dir: &str) -> Result<()> {
    let expanded = expand_tilde(data_dir);
    let path = PathBuf::from(&expanded);

    std::fs::create_dir_all(&path)
        .with_context(|| format!("Failed to create data directory: {}", expanded))?;
    std::fs::create_dir_all(path.join("shards"))
        .with_context(|| "Failed to create shards directory")?;
    std::fs::create_dir_all(path.join("ledger"))
        .with_context(|| "Failed to create ledger directory")?;

    // Generate node identity using libp2p
    let config = NodeConfig::generate(&path)?;

    println!("Node initialized successfully at: {}", expanded);
    println!("  Peer ID:   {}", config.peer_id);
    println!("  Public key: {}", hex::encode(&config.public_key));
    println!("  P2P port:   {}", config.p2p_port);
    println!("  API port:   {}", config.api_port);
    println!();
    println!("Run 'vaultkeeperd start --data-dir {}' to start the daemon.", data_dir);

    Ok(())
}

async fn cmd_start(data_dir: &str, api_addr: &str) -> Result<()> {
    let expanded = expand_tilde(data_dir);
    info!("Starting VaultKeeper daemon...");
    info!("Data directory: {}", expanded);
    info!("API address: {}", api_addr);

    // Check mobile guards
    if !vaultkeeper_storage::is_hosting_available() {
        warn!(
            "Hosting is disabled on {} — running in client-only mode",
            vaultkeeper_storage::platform_type()
        );
    }

    // Load config
    let _node_config = NodeConfig::load(&expanded)?;

    // Initialize P2P node with real P2pConfig
    let p2p_config = vaultkeeper_p2p::P2pConfig::default();
    let p2p_node = vaultkeeper_p2p::P2pNode::new(p2p_config).await?;
    p2p_node.initialize().await?;
    info!("P2P node started: {}", p2p_node.peer_id_str());

    // Initialize ledger
    let ledger_path = PathBuf::from(&expanded).join("ledger").join("vaultkeeper.db");
    let _ledger = vaultkeeper_ledger::store::LedgerStore::open(&ledger_path)?;
    info!("Ledger opened: {}", ledger_path.display());

    // Initialize billing engine
    let billing = vaultkeeper_billing::BillingEngine::new();
    info!("Billing engine initialized (balance: {} RUB)", billing.get_current_balance());

    // Initialize storage
    let shard_path = PathBuf::from(&expanded).join("shards");
    let _storage = vaultkeeper_storage::shard_store::ShardStore::new(&shard_path)?;
    info!("Storage initialized at: {}", shard_path.display());

    // Initialize replication manager
    let _replication = vaultkeeper_storage::replication::ReplicationManager::new(3);
    info!("Replication manager initialized (target: 3 replicas)");

    // Start API server
    info!("Starting API server on {}", api_addr);
    run_api_server(api_addr).await?;

    Ok(())
}

fn cmd_stop() -> Result<()> {
    info!("Attempting to stop VaultKeeper daemon...");
    warn!("Daemon stop: use 'systemctl stop vaultkeeperd' for systemd-managed nodes");
    Ok(())
}

fn cmd_status(data_dir: &str) -> Result<()> {
    let expanded = expand_tilde(data_dir);
    let path = PathBuf::from(&expanded);
    let config_path = path.join("config.json");

    if !config_path.exists() {
        anyhow::bail!("Node not initialized at: {}", expanded);
    }

    let config = NodeConfig::load(&expanded)?;

    println!("=== VaultKeeper Node Status ===");
    println!("  Peer ID:        {}", config.peer_id);
    println!("  Public key:     {}", hex::encode(&config.public_key));
    println!("  Data directory: {}", expanded);
    println!("  P2P port:       {}", config.p2p_port);
    println!("  API port:       {}", config.api_port);
    println!("  Created:        {}", config.created_at);
    println!("  Platform:       {}", vaultkeeper_storage::platform_type());
    println!("  Hosting:        {}", if vaultkeeper_storage::is_hosting_available() { "enabled" } else { "disabled (mobile)" });
    println!("  Version:        {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}

async fn cmd_upload(file: &str, replication: u8) -> Result<()> {
    let path = PathBuf::from(file);
    if !path.exists() {
        anyhow::bail!("File not found: {}", file);
    }

    let data = std::fs::read(&path)
        .with_context(|| format!("Failed to read file: {}", file))?;
    let file_size = data.len();

    if !(2..=4).contains(&replication) {
        anyhow::bail!("Replication must be 2, 3, or 4, got {}", replication);
    }

    info!(
        "Uploading {} ({} bytes) with replication factor {}",
        file, file_size, replication
    );

    // Chunk the data using vaultkeeper-core
    let chunks = vaultkeeper_core::chunking::chunk_data(&data);
    let chunk_ids = vaultkeeper_core::chunking::generate_chunk_ids(&chunks);

    info!("Split into {} chunks", chunks.len());
    println!("=== Upload Summary ===");
    println!("  File:        {}", file);
    println!("  Size:        {} bytes", file_size);
    println!("  Chunks:      {}", chunks.len());
    println!("  Replication: {}", replication);
    println!("  Chunk IDs:");

    for (i, id) in chunk_ids.iter().enumerate() {
        println!("    [{}] {} ({} bytes)", i, id, chunks[i].len());
    }

    // Estimate cost using billing engine
    let billing = vaultkeeper_billing::BillingEngine::new();
    let cost = billing.estimate_upload_cost(file_size as u64, replication, "ssd", true);
    match cost {
        Ok(hourly_cost) => {
            println!("  Est. hourly: {} RUB", hourly_cost);
            println!("  Est. daily:  {} RUB", hourly_cost * rust_decimal::Decimal::from(24));
        }
        Err(e) => {
            warn!("Could not estimate cost: {}", e);
        }
    }

    info!("Upload complete (simulation mode — data chunked locally)");
    Ok(())
}

async fn cmd_download(file_id: &str, output: &str) -> Result<()> {
    // Validate file_id is valid hex
    let bytes = hex::decode(file_id)
        .with_context(|| format!("Invalid file_id (must be hex): {}", file_id))?;

    info!("Downloading file: {} -> {}", file_id, output);

    // In production: request shards from peers via P2P, reassemble, decrypt
    // For now, simulate the download
    println!("=== Download Summary ===");
    println!("  File ID: {}", file_id);
    println!("  Output:  {}", output);
    println!("  Size:    {} bytes (hex decoded)", bytes.len());
    warn!("Download requires a connected P2P node (use 'vaultkeeperd start' first)");

    Ok(())
}

fn cmd_balance(data_dir: &str) -> Result<()> {
    let expanded = expand_tilde(data_dir);
    let path = PathBuf::from(&expanded);
    let config_path = path.join("config.json");

    // Initialize billing engine with defaults
    let billing = vaultkeeper_billing::BillingEngine::new();

    println!("=== Account Balance ===");
    println!("  Balance:       {} RUB", billing.get_current_balance());
    println!("  Subscription:  {:?}", billing.get_subscription());
    println!("  Status:        {}", if billing.is_frozen() { "FROZEN" } else { "Active" });

    if config_path.exists() {
        println!("  Node:          initialized ({})", expanded);
    } else {
        println!("  Node:          not initialized");
    }

    Ok(())
}

async fn cmd_keys(action: KeyAction) -> Result<()> {
    match action {
        KeyAction::Generate => {
            let seed = vaultkeeper_core::BIP39Seed::generate(12)?;
            println!("=== New Recovery Key ===");
            println!("  Mnemonic: {}", seed.phrase);
            println!("  Words:    {}", seed.word_count());
            println!();
            println!("IMPORTANT: Write down these words and store them securely.");
            println!("This is the ONLY way to recover your data and identity.");
        }
        KeyAction::Show { data_dir } => {
            let expanded = expand_tilde(&data_dir);
            let config = NodeConfig::load(&expanded)?;
            println!("=== Node Public Key ===");
            println!("  Peer ID:    {}", config.peer_id);
            println!("  Public key: {}", hex::encode(&config.public_key));
        }
        KeyAction::Backup { data_dir } => {
            let expanded = expand_tilde(&data_dir);
            let config = NodeConfig::load(&expanded)?;
            println!("=== Backup Information ===");
            println!("  Peer ID:   {}", config.peer_id);
            println!("  Created:   {}", config.created_at);
            println!("  Data dir:  {}", config.data_dir);
            println!();
            println!("To generate a new recovery mnemonic, run:");
            println!("  vaultkeeperd keys generate");
        }
    }
    Ok(())
}

fn cmd_recover(mnemonic: &str, passphrase: &str) -> Result<()> {
    // Validate the mnemonic
    let seed = vaultkeeper_core::BIP39Seed::from_phrase(mnemonic)
        .with_context(|| "Invalid mnemonic phrase. Check for typos or missing words.")?;

    // Derive the vault key from the mnemonic
    let _vault_key = vaultkeeper_core::VaultKey::from_seed(&seed.phrase, passphrase)?;

    println!("=== Recovery Successful ===");
    println!("  Mnemonic:    validated ({} words)", seed.word_count());
    println!("  Key derived: successfully");
    println!();
    println!("NOTE: This validated the mnemonic and derived the master key,");
    println!("but did not restore data. Run 'vaultkeeperd start' to resume");
    println!("operation with the recovered identity.");

    Ok(())
}

async fn cmd_daemon(data_dir: &str) -> Result<()> {
    cmd_start(data_dir, "127.0.0.1:8080").await
}
