//! Соты — Application state (all fields use parking_lot::RwLock).

use parking_lot::RwLock;
use crate::models::*;

pub(crate) struct AppState {
    // ── Onboarding ──
    pub initialized: RwLock<bool>,
    pub peer_id: RwLock<String>,
    pub mnemonic: RwLock<String>,
    pub mnemonic_shown_once: RwLock<bool>,

    // ── Client wallet ──
    pub client_balance: RwLock<f64>,
    pub client_bonus: RwLock<f64>,
    pub credit_storage_enabled: RwLock<bool>,
    pub zero_balance_ticks: RwLock<u32>,

    // ── Keeper wallet ──
    pub keeper_balance: RwLock<f64>,
    pub keeper_pending: RwLock<f64>,
    pub is_keeper: RwLock<bool>,
    pub credit_storage_keeper: RwLock<bool>,

    // ── Keeper metrics ──
    pub rating_pay: RwLock<f64>,
    pub rating_alloc: RwLock<f64>,
    pub availability_72h: RwLock<f64>,
    pub avg_speed_mbps: RwLock<f64>,
    pub has_white_ip: RwLock<bool>,
    pub is_bootstrap: RwLock<bool>,
    pub keeper_storage_gb: RwLock<f64>,
    pub keeper_earnings_total: RwLock<f64>,
    pub relay_enabled: RwLock<bool>,
    pub relay_fail_pct_60min: RwLock<f64>,
    pub _relay_incidents_24h: RwLock<u32>,
    pub relay_banned_until_tick: RwLock<u32>,
    pub relay_gray_clients: RwLock<u32>,

    // ── Network ──
    pub connected_peers: RwLock<u32>,
    pub storage_used_gb: RwLock<f64>,
    pub current_tick: RwLock<u32>,
    pub graceful_shutdown: RwLock<bool>,

    // ── Files & payments ──
    pub files: RwLock<Vec<FileEntry>>,
    pub payment_history: RwLock<Vec<PaymentRecord>>,
    pub penalty_log: RwLock<Vec<PenaltyEntry>>,
    pub active_warnings: RwLock<Vec<WarningEntry>>,

    // ── Referral ──
    pub referral_code: RwLock<String>,
    pub referral_count: RwLock<u32>,
    pub referral_earnings: RwLock<f64>,
    pub has_referrer: RwLock<bool>,

    // ── Bonus entries with expiry ──
    pub bonus_entries: RwLock<Vec<BonusEntry>>,

    // ── Settings ──
    pub geo_verified: RwLock<bool>,
    pub installation_id: RwLock<String>,
    pub settings: RwLock<AppSettings>,
}
