//! Соты — Data structures (DTOs, request/response types).

use serde::{Serialize, Deserialize};

// ═══════════════════════════════════════════════════════════════
//  APP SETTINGS
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSettings {
    pub bootstrap_nodes: Vec<String>,
    pub storage_limit_gb: u32,
    pub auto_start: bool,
    pub russia_only: bool,
    pub credit_storage_keeper: bool,
    pub credit_storage_client: bool,
    // SMTP notification settings
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_login: String,
    pub smtp_password_encrypted: String,
    pub notify_enabled: bool,
    pub notify_email: String,
    pub notify_low_balance: bool,
    pub notify_penalty: bool,
    pub notify_data_delete: bool,
    pub notify_shutdown: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            bootstrap_nodes: vec![
                "/dns4/bootstrap1.soty.net/tcp/9444/p2p/QmExample1".to_string(),
                "/dns4/bootstrap2.soty.net/tcp/9444/p2p/QmExample2".to_string(),
            ],
            storage_limit_gb: 100,
            auto_start: false,
            russia_only: true,
            credit_storage_keeper: false,
            credit_storage_client: false,
            smtp_host: String::new(),
            smtp_port: 587,
            smtp_login: String::new(),
            smtp_password_encrypted: String::new(),
            notify_enabled: false,
            notify_email: String::new(),
            notify_low_balance: true,
            notify_penalty: true,
            notify_data_delete: true,
            notify_shutdown: true,
        }
    }
}

// ═══════════════════════════════════════════════════════════════
//  CORE DATA STRUCTURES
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub replicas: u32,
    pub disk_type: String,
    pub uploaded_at: String,
    pub cost_per_month: f64,
    pub is_credit: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRecord {
    pub id: String,
    pub kind: String,
    pub amount: f64,
    pub wallet: String,
    pub description: String,
    pub timestamp: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PenaltyEntry {
    pub id: String,
    pub level: String,
    pub target: String,
    pub reason: String,
    pub action: String,
    pub tick: u32,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WarningEntry {
    pub id: String,
    pub target: String,
    pub message: String,
    pub tick_issued: u32,
    pub expires_at_tick: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct BonusEntry {
    pub id: String,
    pub amount: f64,
    pub created_at_tick: u32,
    pub expiry_tick: u32,
    pub source: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct _NotificationEvent {
    kind: String,
    message: String,
    timestamp: String,
}

// ═══════════════════════════════════════════════════════════════
//  RESPONSE TYPES
// ═══════════════════════════════════════════════════════════════

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletInfo {
    pub peer_id: String,
    pub mnemonic: String,
    pub referral_code: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientBalanceResponse {
    pub balance: f64,
    pub bonus: f64,
    pub currency: String,
    pub credit_storage_enabled: bool,
    pub credit_action: String,
    pub credit_ticks_remaining: u32,
    pub low_balance_warning: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeeperBalanceResponse {
    pub balance: f64,
    pub pending: f64,
    pub can_withdraw: bool,
    pub currency: String,
    pub payout_note: Option<String>,
}

#[derive(Serialize)]
pub struct TopupResponse {
    pub amount: f64,
    pub commission: f64,
    pub total: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalcResponse {
    pub gb: f64,
    pub disk_type: String,
    pub replication: u32,
    pub price_per_gb: f64,
    pub cost_per_month: f64,
    pub cost_per_5min: f64,
    pub credit_multiplier: f64,
    pub comparison: CalcComparison,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalcComparison {
    pub hdd_monthly: f64,
    pub ssd_monthly: f64,
    pub nvme_monthly: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeeperStatsResponse {
    pub is_active: bool,
    pub rating_pay: f64,
    pub rating_alloc: f64,
    pub storage_provided_gb: f64,
    pub earnings_total: f64,
    pub connected_peers: u32,
    pub has_white_ip: bool,
    pub is_bootstrap: bool,
    pub credit_storage_keeper: bool,
    pub relay_enabled: bool,
    pub relay_banned: bool,
    pub relay_gray_clients: u32,
    pub relay_fail_pct: f64,
    pub relay_bonus_note: Option<String>,
    pub bootstrap_note: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatsResponse {
    pub storage_used_gb: f64,
    pub files_count: u32,
    pub connected_peers: u32,
    pub monthly_cost: f64,
    pub credit_storage_enabled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatusResponse {
    pub connected_peers: u32,
    pub status: String,
    pub version: String,
    pub current_tick: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferralInfoResponse {
    pub referral_code: String,
    pub referral_link: String,
    pub invited_count: u32,
    pub total_earnings: f64,
    pub note: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoResponse {
    pub verified: bool,
    pub country: String,
    pub ip: String,
    pub has_white_ip: bool,
    pub installation_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NextCalcResponse {
    pub seconds_left: u32,
    pub interval_minutes: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickResponse {
    pub client_balance: f64,
    pub client_bonus: f64,
    pub keeper_balance: f64,
    pub rating_pay: f64,
    pub rating_alloc: f64,
    pub current_tick: u32,
    pub zero_balance_ticks: u32,
    pub credit_action: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseAccountResponse {
    pub refund_amount: f64,
    pub forfeited_bonus: f64,
    pub files_deleted: u32,
}
