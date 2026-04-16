#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

// ─── Response types (serializable to frontend) ─────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UploadResult {
    pub success: bool,
    pub file_id: Option<String>,
    pub error: Option<String>,
    pub cost: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DownloadResult {
    pub success: bool,
    pub local_path: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SeedResult {
    pub seed: String,
    pub node_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReputationStatus {
    pub score: String,
    pub consecutive_fails: u32,
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BalanceInfo {
    pub balance: String,
    pub frozen: bool,
    pub subscription: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeInfo {
    pub peer_id: String,
    pub platform: String,
    pub hosting_available: bool,
    pub online: bool,
}

// ─── Application state (managed by Tauri) ──────────────────────────────────

struct AppInner {
    node_id: String,
    initialized: bool,
}

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<AppInner>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AppInner {
                node_id: String::new(),
                initialized: false,
            })),
        }
    }
}

// ─── Secure key storage via OS-native keyring ──────────────────────────────

fn store_seed_securely(seed: &str, node_id: &str) -> Result<(), String> {
    let entry =
        keyring::Entry::new("vaultkeeper", &format!("seed_{}", node_id))
            .map_err(|e| format!("keyring error: {}", e))?;
    entry
        .set_password(seed)
        .map_err(|e| format!("save error: {}", e))?;
    Ok(())
}

fn load_seed_securely(node_id: &str) -> Result<Option<String>, String> {
    let entry =
        keyring::Entry::new("vaultkeeper", &format!("seed_{}", node_id))
            .map_err(|e| format!("keyring error: {}", e))?;
    match entry.get_password() {
        Ok(seed) => Ok(Some(seed)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("load error: {}", e)),
    }
}

// ─── Tauri commands ────────────────────────────────────────────────────────

/// Generate a new 12-word BIP39 mnemonic, derive the node identity, and
/// persist the seed to the OS keyring.
#[tauri::command]
async fn generate_seed(state: tauri::State<'_, AppState>) -> Result<SeedResult, String> {
    // vaultkeeper_core::BIP39Seed::generate(usize) -> anyhow::Result<BIP39Seed>
    let seed = vaultkeeper_core::BIP39Seed::generate(12).map_err(|e| e.to_string())?;

    // vaultkeeper_core::VaultKey::from_seed(&str, &str) -> anyhow::Result<VaultKey>
    let key =
        vaultkeeper_core::VaultKey::from_seed(&seed.phrase, "").map_err(|e| e.to_string())?;

    // vaultkeeper_core::VaultKey::public_id(&self) -> String
    let node_id = key.public_id();

    store_seed_securely(&seed.phrase, &node_id)?;

    let mut inner = state.inner.lock().await;
    inner.node_id = node_id.clone();
    inner.initialized = true;

    Ok(SeedResult {
        seed: seed.phrase,
        node_id,
    })
}

/// Restore identity from an existing BIP39 mnemonic phrase.
#[tauri::command]
async fn recover_from_seed(
    phrase: String,
    state: tauri::State<'_, AppState>,
) -> Result<SeedResult, String> {
    // vaultkeeper_core::BIP39Seed::from_phrase(&str) -> anyhow::Result<BIP39Seed>
    let seed =
        vaultkeeper_core::BIP39Seed::from_phrase(&phrase).map_err(|e| e.to_string())?;

    let key =
        vaultkeeper_core::VaultKey::from_seed(&seed.phrase, "").map_err(|e| e.to_string())?;
    let node_id = key.public_id();

    store_seed_securely(&seed.phrase, &node_id)?;

    let mut inner = state.inner.lock().await;
    inner.node_id = node_id.clone();
    inner.initialized = true;

    Ok(SeedResult {
        seed: seed.phrase,
        node_id,
    })
}

/// Return current balance, frozen status, and subscription tier.
#[tauri::command]
async fn get_balance() -> Result<BalanceInfo, String> {
    // vaultkeeper_billing::BillingEngine::new() -> BillingEngine
    let billing = vaultkeeper_billing::BillingEngine::new();

    let frozen = billing.is_frozen();

    // vaultkeeper_billing::BillingEngine::get_subscription(&self) -> SubscriptionTier
    let sub = match billing.get_subscription() {
        vaultkeeper_billing::SubscriptionTier::Archive => "archive",
        vaultkeeper_billing::SubscriptionTier::Standard => "standard",
        vaultkeeper_billing::SubscriptionTier::Premium => "premium",
    };

    Ok(BalanceInfo {
        // vaultkeeper_billing::BillingEngine::get_current_balance(&self) -> Decimal
        balance: billing.get_current_balance().to_string(),
        frozen,
        subscription: sub.to_string(),
    })
}

/// Return node peer-id, platform type, hosting availability, and online flag.
#[tauri::command]
async fn get_node_info(state: tauri::State<'_, AppState>) -> Result<NodeInfo, String> {
    let inner = state.inner.lock().await;

    // vaultkeeper_storage::platform_type() -> &'static str
    let platform = vaultkeeper_storage::platform_type().to_string();

    // vaultkeeper_storage::is_hosting_available() -> bool
    let hosting_available = vaultkeeper_storage::is_hosting_available();

    Ok(NodeInfo {
        peer_id: inner.node_id.clone(),
        platform,
        hosting_available,
        online: inner.initialized,
    })
}

/// Look up a node's reputation score, consecutive fails, and status.
#[tauri::command]
async fn get_reputation(node_id: String) -> Result<ReputationStatus, String> {
    // vaultkeeper_ledger::ReputationManager::new() -> ReputationManager
    let rep = vaultkeeper_ledger::ReputationManager::new();

    // vaultkeeper_ledger::ReputationManager::get_score(&self, &str) -> Option<Decimal>
    let score = rep
        .get_score(&node_id)
        .unwrap_or(rust_decimal::Decimal::from(5));

    // vaultkeeper_ledger::ReputationManager::get_consecutive_fails(&self, &str) -> Option<u32>
    let fails = rep.get_consecutive_fails(&node_id).unwrap_or(0);

    // vaultkeeper_ledger::ReputationManager::get_status(&self, &str) -> NodeStatus
    let status = match rep.get_status(&node_id) {
        vaultkeeper_ledger::NodeStatus::Active => "active",
        vaultkeeper_ledger::NodeStatus::Warning => "warning",
        vaultkeeper_ledger::NodeStatus::Banned => "banned",
    };

    Ok(ReputationStatus {
        score: score.to_string(),
        consecutive_fails: fails,
        status: status.to_string(),
    })
}

/// Estimate hourly storage cost in RUB for a given file size and parameters.
///
/// `replication` must be 2, 3, or 4.
/// `disk_type` must be "hdd", "ssd", or "nvme".
#[tauri::command]
async fn estimate_cost(
    file_size_bytes: u64,
    replication: u8,
    disk_type: String,
    cushion: bool,
) -> Result<String, String> {
    let billing = vaultkeeper_billing::BillingEngine::new();

    // vaultkeeper_billing::BillingEngine::estimate_upload_cost(
    //     &self, u64, u8, &str, bool
    // ) -> Result<Decimal, BillingError>
    let cost = billing
        .estimate_upload_cost(file_size_bytes, replication, &disk_type, cushion)
        .map_err(|e| e.to_string())?;

    Ok(cost.to_string())
}

/// Change the subscription tier (archive / standard / premium).
#[tauri::command]
async fn subscribe(tier: String) -> Result<bool, String> {
    // vaultkeeper_billing::BillingEngine::set_subscription(&mut self, SubscriptionTier)
    //   -> Result<Decimal, BillingError>
    let mut billing = vaultkeeper_billing::BillingEngine::new();

    let sub = match tier.as_str() {
        "standard" => vaultkeeper_billing::SubscriptionTier::Standard,
        "premium" => vaultkeeper_billing::SubscriptionTier::Premium,
        _ => vaultkeeper_billing::SubscriptionTier::Archive,
    };

    billing
        .set_subscription(sub)
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// Check whether the current platform allows hosting (false on mobile).
#[tauri::command]
async fn check_host_eligibility() -> Result<bool, String> {
    // vaultkeeper_storage::is_hosting_available() -> bool
    Ok(vaultkeeper_storage::is_hosting_available())
}

// ─── Entry point ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            generate_seed,
            recover_from_seed,
            get_balance,
            get_node_info,
            get_reputation,
            estimate_cost,
            subscribe,
            check_host_eligibility,
        ])
        .setup(|app| {
            #[cfg(not(debug_assertions))]
            {
                // Auto-update check placeholder — full implementation needs
                // Ed25519 pubkey in tauri.conf.json plugins.updater.pubkey
                let _handle = app.handle().clone();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
