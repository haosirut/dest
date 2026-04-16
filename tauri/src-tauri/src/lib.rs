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
// Seed is NEVER saved to SQLite, JSON, or files. Only through OS keyring:
//   macOS  → Security Framework (Keychain)
//   Windows → DPAPI (Credential Manager)
//   Linux  → libsecret / Secret Service API

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
    let seed = vaultkeeper_core::BIP39Seed::generate(12).map_err(|e| e.to_string())?;
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

/// Restore identity from an existing BIP39 mnemonic phrase.
#[tauri::command]
async fn recover_from_seed(
    phrase: String,
    state: tauri::State<'_, AppState>,
) -> Result<SeedResult, String> {
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
    let billing = vaultkeeper_billing::BillingEngine::new();
    let frozen = billing.is_frozen();

    let sub = match billing.get_subscription() {
        vaultkeeper_billing::SubscriptionTier::Archive => "archive",
        vaultkeeper_billing::SubscriptionTier::Standard => "standard",
        vaultkeeper_billing::SubscriptionTier::Premium => "premium",
    };

    Ok(BalanceInfo {
        balance: billing.get_current_balance().to_string(),
        frozen,
        subscription: sub.to_string(),
    })
}

/// Return node peer-id, platform type, hosting availability, and online flag.
#[tauri::command]
async fn get_node_info(state: tauri::State<'_, AppState>) -> Result<NodeInfo, String> {
    let inner = state.inner.lock().await;
    let platform = vaultkeeper_storage::platform_type().to_string();
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
    let rep = vaultkeeper_ledger::ReputationManager::new();

    let score = rep
        .get_score(&node_id)
        .unwrap_or(rust_decimal::Decimal::from(5));

    let fails = rep.get_consecutive_fails(&node_id).unwrap_or(0);

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
#[tauri::command]
async fn estimate_cost(
    file_size_bytes: u64,
    replication: u8,
    disk_type: String,
    cushion: bool,
) -> Result<String, String> {
    let billing = vaultkeeper_billing::BillingEngine::new();

    let cost = billing
        .estimate_upload_cost(file_size_bytes, replication, &disk_type, cushion)
        .map_err(|e| e.to_string())?;

    Ok(cost.to_string())
}

/// Change the subscription tier (archive / standard / premium).
#[tauri::command]
async fn subscribe(tier: String) -> Result<bool, String> {
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

/// Check whether the current platform allows hosting.
/// On mobile (android/ios): always returns false (hosting prohibited).
/// On desktop: checks Wi-Fi connection (stub — use network-info in production).
#[tauri::command]
async fn check_host_eligibility() -> Result<bool, String> {
    // Mobile: hosting is ALWAYS disabled
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        return Ok(false);
    }

    // Desktop: check platform availability + Wi-Fi requirement
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        if !vaultkeeper_storage::is_hosting_available() {
            return Ok(false);
        }
        // TODO: In production, use `network-info` or `nix` crate to detect
        // connection type. Only allow hosting on Wi-Fi / Ethernet.
        // Mobile data should be rejected to prevent unexpected data charges.
        let _is_wifi = true; // placeholder
        Ok(true)
    }
}

/// Upload file data through the P2P layer. Returns file_id on success.
#[tauri::command]
async fn upload_file(
    file_name: String,
    file_data: Vec<u8>,
    replication: u8,
    disk_type: String,
) -> Result<UploadResult, String> {
    let mut node = vaultkeeper_p2p::P2PNode::new_with_nat_support()
        .await
        .map_err(|e| e.to_string())?;

    let disk = disk_type.parse::<vaultkeeper_p2p::DiskType>()
        .map_err(|e| e.to_string())?;

    let params = vaultkeeper_p2p::UploadParams {
        replication,
        disk_type: disk,
        cushion_enabled: true,
        max_cost: rust_decimal::Decimal::ZERO,
    };

    let file_id = node
        .upload_chunks(&file_data, params)
        .await
        .map_err(|e| e.to_string())?;

    Ok(UploadResult {
        success: true,
        file_id: Some(file_id),
        error: None,
        cost: None,
    })
}

/// Download file data by file_id through the P2P layer.
#[tauri::command]
async fn download_file(file_id: String) -> Result<DownloadResult, String> {
    let mut node = vaultkeeper_p2p::P2PNode::new_with_nat_support()
        .await
        .map_err(|e| e.to_string())?;

    let data = node
        .download_file(&file_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(DownloadResult {
        success: true,
        local_path: None,
        error: None,
    })
}

// ─── Entry point ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            upload_file,
            download_file,
        ])
        .setup(|app| {
            // Auto-update check in release builds
            #[cfg(not(debug_assertions))]
            {
                use tauri_plugin_updater::UpdaterExt;
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    match app_handle.updater_builder().build() {
                        Ok(updater) => {
                            if let Err(e) = updater.check_and_install().await {
                                tracing::error!("Auto-update failed: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Updater builder failed: {}", e);
                        }
                    }
                });
            }

            #[cfg(debug_assertions)]
            {
                tracing::info!("Running in debug mode — auto-updater disabled");
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
