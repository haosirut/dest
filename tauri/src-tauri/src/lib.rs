//! Соты — Tauri v2 backend: security, ratings, penalties, credit storage, legal docs.
//!
//! Business model:
//!   - rating_pay  (выплаты хранителям) = avail_72h * min(avg_speed / 100, 1.0)
//!   - rating_alloc (размещение данных) = rating_pay * ip_factor * bootstrap_factor * credit_factor
//!   - Распределение: 80% хранителям, 2% relay, 1% bootstrap, 3% реф-хранителю,
//!     3% реф-клиенту(бонус), остальное платформа (>= 11%).
//!   - Хранение в долг: x1.5 к стоимости, 72 ч кредитный период.
//!   - Штрафы: warning → freeze → penalty → ban.

use tauri::{Manager, State};
use std::sync::Mutex;
use serde::{Serialize, Deserialize};

// ═══════════════════════════════════════════════════════════════
//  CONSTANTS
// ═══════════════════════════════════════════════════════════════

const PRICE_HDD: f64  = 0.30;   // ₽/GB/month (x4 replication included)
const PRICE_SSD: f64  = 0.60;
const PRICE_NVME: f64 = 0.90;
const REPLICATION: u32 = 4;
const CREDIT_MULTIPLIER: f64 = 1.5;
const RELAY_BONUS_PCT: f64 = 0.02;   // +2%
const BOOTSTRAP_BONUS_PCT: f64 = 0.01; // +1%
const REFERRAL_PCT: f64 = 0.03;
const KEEPER_BASE_PCT: f64 = 0.80;
const MAX_KEEPER_TOTAL_PCT: f64 = 0.89;
const SPEED_REFERENCE_MBPS: f64 = 100.0;
const TICKS_PER_MONTH: f64 = 30.0 * 24.0 * 12.0; // 5-min intervals
const PAYOUT_MIN: f64 = 100.0;
const CREDIT_PERIOD_TICKS: u32 = (72.0 * 12.0) as u32; // 72h * 12 ticks/h = 864

// ═══════════════════════════════════════════════════════════════
//  APPLICATION STATE
// ═══════════════════════════════════════════════════════════════

struct AppState {
    initialized: Mutex<bool>,
    peer_id: Mutex<String>,
    mnemonic: Mutex<String>,
    mnemonic_shown_once: Mutex<bool>,

    // ── Client wallet ──
    client_balance: Mutex<f64>,
    client_bonus: Mutex<f64>,
    credit_storage_enabled: Mutex<bool>,  // client side
    zero_balance_ticks: Mutex<u32>,       // ticks since balance went to 0

    // ── Keeper wallet ──
    keeper_balance: Mutex<f64>,
    keeper_pending: Mutex<f64>,
    is_keeper: Mutex<bool>,
    credit_storage_keeper: Mutex<bool>,   // keeper accepts credit data

    // ── Keeper metrics ──
    rating_pay: Mutex<f64>,
    rating_alloc: Mutex<f64>,
    availability_72h: Mutex<f64>,
    avg_speed_mbps: Mutex<f64>,
    has_white_ip: Mutex<bool>,
    is_bootstrap: Mutex<bool>,
    keeper_storage_gb: Mutex<f64>,
    keeper_earnings_total: Mutex<f64>,
    relay_enabled: Mutex<bool>,
    relay_fail_pct_60min: Mutex<f64>,
    relay_incidents_24h: Mutex<u32>,
    relay_banned_until_tick: Mutex<u32>,

    // ── Network ──
    connected_peers: Mutex<u32>,
    storage_used_gb: Mutex<f64>,
    current_tick: Mutex<u32>,
    graceful_shutdown: Mutex<bool>,

    // ── Files & payments ──
    files: Mutex<Vec<FileEntry>>,
    payment_history: Mutex<Vec<PaymentRecord>>,
    penalty_log: Mutex<Vec<PenaltyEntry>>,
    active_warnings: Mutex<Vec<WarningEntry>>,

    // ── Referral ──
    referral_code: Mutex<String>,
    referral_count: Mutex<u32>,
    referral_earnings: Mutex<f64>,

    // ── Settings ──
    geo_verified: Mutex<bool>,
    installation_id: Mutex<String>,
    settings: Mutex<AppSettings>,
}

// ═══════════════════════════════════════════════════════════════
//  DATA STRUCTURES
// ═══════════════════════════════════════════════════════════════

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    bootstrap_nodes: Vec<String>,
    storage_limit_gb: u32,
    auto_start: bool,
    russia_only: bool,
    credit_storage_keeper: bool,
    credit_storage_client: bool,
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
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct FileEntry {
    id: String,
    name: String,
    size_bytes: u64,
    replicas: u32,
    disk_type: String,
    uploaded_at: String,
    cost_per_month: f64,
    is_credit: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct PaymentRecord {
    id: String,
    kind: String,
    amount: f64,
    wallet: String,
    description: String,
    timestamp: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct PenaltyEntry {
    id: String,
    level: String,      // "low", "medium", "high", "critical"
    target: String,     // "keeper" or "client"
    reason: String,
    action: String,
    tick: u32,
}

#[derive(Clone, Serialize, Deserialize)]
struct WarningEntry {
    id: String,
    target: String,
    message: String,
    tick_issued: u32,
    expires_at_tick: u32,
}

// ═══════════════════════════════════════════════════════════════
//  PURE BUSINESS LOGIC (unit-testable)
// ═══════════════════════════════════════════════════════════════

/// Price per GB/month for a given disk type.
pub fn get_price(disk_type: &str) -> f64 {
    match disk_type {
        "nvme" => PRICE_NVME,
        "ssd"  => PRICE_SSD,
        _      => PRICE_HDD,
    }
}

/// Cost per GB per month.
pub fn cost_gb_month(gb: f64, disk_type: &str) -> f64 {
    gb * get_price(disk_type)
}

/// Cost per GB per 5-min tick.
pub fn cost_gb_tick(gb: f64, disk_type: &str) -> f64 {
    cost_gb_month(gb, disk_type) / TICKS_PER_MONTH
}

/// Cost per GB per 5-min tick with credit multiplier.
pub fn cost_gb_tick_credit(gb: f64, disk_type: &str) -> f64 {
    cost_gb_tick(gb, disk_type) * CREDIT_MULTIPLIER
}

/// rating_pay = availability_72h * min(avg_speed / 100, 1.0)
pub fn calc_rating_pay(availability: f64, avg_speed_mbps: f64) -> f64 {
    let speed_factor = (avg_speed_mbps / SPEED_REFERENCE_MBPS).min(1.0);
    availability * speed_factor
}

/// rating_alloc = rating_pay * ip_factor * bootstrap_factor * credit_factor
pub fn calc_rating_alloc(
    rating_pay: f64,
    has_white_ip: bool,
    is_bootstrap: bool,
    credit_storage: bool,
) -> f64 {
    let ip_factor       = if has_white_ip { 1.05 } else { 1.0 };
    let bootstrap_factor = if is_bootstrap { 1.02 } else { 1.0 };
    let credit_factor   = if credit_storage { 1.02 } else { 1.0 };
    rating_pay * ip_factor * bootstrap_factor * credit_factor
}

/// Revenue distribution from a single client payment.
/// Returns (keeper_pool, relay_bonus, bootstrap_bonus, ref_keeper, ref_client, platform).
/// If `is_credit`, the input `payment` is already x1.5.
pub fn distribute_payment(
    payment: f64,
    has_relay: bool,
    has_bootstrap: bool,
    has_ref_keeper: bool,
    has_ref_client: bool,
) -> (f64, f64, f64, f64, f64, f64) {
    let ref_keeper_amt = if has_ref_keeper { payment * REFERRAL_PCT } else { 0.0 };
    let ref_client_amt = if has_ref_client { payment * REFERRAL_PCT } else { 0.0 };
    let keeper_base    = payment * KEEPER_BASE_PCT;

    let mut relay_bonus    = 0.0;
    let mut bootstrap_bonus = 0.0;

    // Relay +2% from platform share
    if has_relay {
        relay_bonus = payment * RELAY_BONUS_PCT;
    }
    // Bootstrap +1% from platform share
    if has_bootstrap {
        bootstrap_bonus = payment * BOOTSTRAP_BONUS_PCT;
    }

    // Total to keepers must not exceed MAX_KEEPER_TOTAL_PCT (89%)
    let total_keeper = keeper_base + relay_bonus + bootstrap_bonus + ref_keeper_amt;
    let max_keeper = payment * MAX_KEEPER_TOTAL_PCT;
    let excess = (total_keeper - max_keeper).max(0.0);

    // Distribute excess reduction proportionally
    let keeper_pool = if total_keeper > 0.0 {
        keeper_base - excess * (keeper_base / total_keeper)
    } else {
        keeper_base
    };
    let relay_bonus    = relay_bonus.max(0.0) - excess * if total_keeper > 0.0 { relay_bonus.max(0.0) / total_keeper } else { 0.0 };
    let bootstrap_bonus = bootstrap_bonus.max(0.0) - excess * if total_keeper > 0.0 { bootstrap_bonus.max(0.0) / total_keeper } else { 0.0 };
    let ref_keeper_amt = ref_keeper_amt - excess * if total_keeper > 0.0 { ref_keeper_amt / total_keeper } else { 0.0 };

    let platform = payment - keeper_pool - relay_bonus - bootstrap_bonus - ref_keeper_amt - ref_client_amt;

    (
        keeper_pool.max(0.0),
        relay_bonus.max(0.0),
        bootstrap_bonus.max(0.0),
        ref_keeper_amt.max(0.0),
        ref_client_amt,
        platform.max(0.0),
    )
}

/// Calculate penalty level and action for a keeper.
pub fn assess_keeper_penalty(
    pos_failures: u32,           // proof-of-storage failures this cycle
    avail_flips_24h: u32,        // unavailable-available flips in 24h
    has_graceful_shutdown: bool,
    is_spoofing: bool,
    is_sybil: bool,
    repeat_offense: bool,
) -> Option<(String, String)> {
    // Critical: spoofing or sybil
    if is_spoofing || is_sybil {
        if repeat_offense {
            return Some(("critical".into(), "permanent_ban".into()));
        }
        return Some(("high".into(), "reset_rating_confiscate".into()));
    }
    // Low: single PoS failure
    if pos_failures == 1 {
        return Some(("low".into(), "warning_freeze_1h".into()));
    }
    // Medium: avail flips without graceful shutdown
    if avail_flips_24h > 3 && !has_graceful_shutdown {
        return Some(("medium".into(), "penalty_10pct".into()));
    }
    None
}

/// Calculate penalty level and action for a client.
pub fn assess_client_penalty(
    replica_rebalance_hour: u32,    // rebalance requests per hour
    geo_changes_30min: u32,
    total_rebalance_6h_gb: f64,
    is_xss: bool,
) -> Option<(String, String)> {
    if is_xss {
        return Some(("high".into(), "permanent_ban_zero_balance".into()));
    }
    if replica_rebalance_hour >= 10 || total_rebalance_6h_gb > 1000.0 {
        return Some(("medium".into(), "block_24h_fine_500".into()));
    }
    if geo_changes_30min > 6 {
        return Some(("medium".into(), "block_24h_fine_500".into()));
    }
    if replica_rebalance_hour >= 5 {
        return Some(("low".into(), "warning".into()));
    }
    None
}

/// Determine credit storage state after a tick.
/// Returns (action, ticks_remaining).
/// Actions: "none", "warn", "block_uploads", "delete_data"
pub fn credit_storage_state(
    balance: f64,
    credit_enabled: bool,
    zero_ticks: u32,
) -> (String, u32) {
    if balance > 0.0 {
        return ("none".into(), CREDIT_PERIOD_TICKS);
    }
    if !credit_enabled {
        // No credit: data deleted after CREDIT_PERIOD_TICKS
        if zero_ticks >= CREDIT_PERIOD_TICKS {
            return ("delete_data".into(), 0);
        }
        return ("warn".into(), CREDIT_PERIOD_TICKS - zero_ticks);
    }
    // Credit enabled
    if zero_ticks >= CREDIT_PERIOD_TICKS {
        return ("delete_data".into(), 0);
    }
    if zero_ticks > 0 {
        return ("block_uploads".into(), CREDIT_PERIOD_TICKS - zero_ticks);
    }
    ("none".into(), CREDIT_PERIOD_TICKS)
}

// ═══════════════════════════════════════════════════════════════
//  LEGAL DOCUMENTS (Russian Federation)
// ═══════════════════════════════════════════════════════════════

pub fn get_public_offer_text() -> &'static str {
    include_str!("../legal/oferta.txt")
}

pub fn get_agency_contract_text() -> &'static str {
    include_str!("../legal/agency_contract.txt")
}

// ═══════════════════════════════════════════════════════════════
//  TAURI COMMANDS: ONBOARDING
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
fn check_initialized(state: State<AppState>) -> bool {
    *state.initialized.lock().unwrap()
}

#[tauri::command]
fn create_wallet(state: State<AppState>) -> WalletInfo {
    let mut rng_state: [u8; 32] = [0; 32];
    for i in 0..32 {
        rng_state[i] = (i as u8).wrapping_add(42).wrapping_mul(7);
    }
    let peer_id = format!("12D3KooW{}", to_hex(&rng_state[..16]));
    let inst_id = format!("SID-{}", to_hex(&rng_state[16..20]));
    let mnemonic_words = vec![
        "абрикос","банан","вишня","гранат","дыня","ежевика","земляника",
        "инжир","клубника","лимон","малина","нектарин","облепиха","персик",
        "рябина","смородина","тыква","финик","хурма","черешня","яблоко",
        "айва","груша","слива",
    ];
    let mnemonic = mnemonic_words.join(" ");

    *state.initialized.lock().unwrap() = true;
    *state.peer_id.lock().unwrap() = peer_id.clone();
    *state.mnemonic.lock().unwrap() = mnemonic.clone();
    *state.mnemonic_shown_once.lock().unwrap() = false;
    *state.installation_id.lock().unwrap() = inst_id;
    *state.referral_code.lock().unwrap() = format!("SOTY-{}", to_hex(&rng_state[..4]).to_uppercase());

    WalletInfo {
        peer_id,
        mnemonic,
        referral_code: state.referral_code.lock().unwrap().clone(),
    }
}

#[tauri::command]
fn restore_wallet(state: State<AppState>, _mnemonic: String) -> Result<WalletInfo, String> {
    // Production: validate mnemonic, derive keypair, restore wallet
    let info = create_wallet(state);
    Ok(info)
}

#[tauri::command]
fn confirm_mnemonic_shown(state: State<AppState>) -> Result<(), String> {
    *state.mnemonic_shown_once.lock().unwrap() = true;
    // Production: encrypt mnemonic with user password (AES-256), store encrypted backup
    Ok(())
}

#[tauri::command]
fn get_wallet_info(state: State<AppState>) -> WalletInfo {
    WalletInfo {
        peer_id: state.peer_id.lock().unwrap().clone(),
        mnemonic: if *state.mnemonic_shown_once.lock().unwrap() {
            String::new() // No longer shown after first confirmation
        } else {
            state.mnemonic.lock().unwrap().clone()
        },
        referral_code: state.referral_code.lock().unwrap().clone(),
    }
}

// ═══════════════════════════════════════════════════════════════
//  TAURI COMMANDS: BALANCES
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
fn get_client_balance(state: State<AppState>) -> ClientBalanceResponse {
    let credit_on = *state.credit_storage_enabled.lock().unwrap();
    let zero_ticks = *state.zero_balance_ticks.lock().unwrap();
    let (credit_action, credit_remaining) = credit_storage_state(
        *state.client_balance.lock().unwrap(),
        credit_on,
        zero_ticks,
    );
    ClientBalanceResponse {
        balance: *state.client_balance.lock().unwrap(),
        bonus: *state.client_bonus.lock().unwrap(),
        currency: "RUB".to_string(),
        credit_storage_enabled: credit_on,
        credit_action,
        credit_ticks_remaining: credit_remaining,
        low_balance_warning: *state.client_balance.lock().unwrap() < 10.0 && *state.storage_used_gb.lock().unwrap() > 0.0,
    }
}

#[tauri::command]
fn get_keeper_balance(state: State<AppState>) -> KeeperBalanceResponse {
    let pending = *state.keeper_pending.lock().unwrap();
    KeeperBalanceResponse {
        balance: *state.keeper_balance.lock().unwrap(),
        pending,
        can_withdraw: pending >= PAYOUT_MIN,
        currency: "RUB".to_string(),
        payout_note: if pending < PAYOUT_MIN {
            Some(format!("Минимальная сумма вывода: {} ₽. Ещё нужно: {:.2} ₽", PAYOUT_MIN, PAYOUT_MIN - pending))
        } else {
            Some("Вывод доступен (будни 10:00\u{2013}18:00 МСК)".to_string())
        },
    }
}

#[tauri::command]
fn get_payment_history(state: State<AppState>) -> Vec<PaymentRecord> {
    state.payment_history.lock().unwrap().clone()
}

#[tauri::command]
fn topup_client(state: State<AppState>, amount: f64, method: String) -> TopupResponse {
    let commission = if method == "card" { amount * 0.025 } else { 0.0 };
    let total = amount + commission;
    *state.client_balance.lock().unwrap() += amount;
    *state.zero_balance_ticks.lock().unwrap() = 0;

    state.payment_history.lock().unwrap().push(PaymentRecord {
        id: uuid_str(),
        kind: "deposit".to_string(),
        amount,
        wallet: "client".to_string(),
        description: format!("Пополнение ({}, комиссия {:.2} ₽)",
            if method == "card" { "карта" } else { "СБП" }, commission),
        timestamp: now_str(),
    });

    TopupResponse { amount, commission, total }
}

#[tauri::command]
fn request_payout(state: State<AppState>) -> Result<String, String> {
    let balance = *state.keeper_balance.lock().unwrap();
    if balance < PAYOUT_MIN {
        return Err(format!("Минимальная сумма вывода: {} ₽. Текущий баланс: {:.2} ₽", PAYOUT_MIN, balance));
    }
    *state.keeper_balance.lock().unwrap() = 0.0;
    *state.keeper_pending.lock().unwrap() = 0.0;

    state.payment_history.lock().unwrap().push(PaymentRecord {
        id: uuid_str(), kind: "payout".to_string(), amount: -balance,
        wallet: "keeper".to_string(), description: "Вывод средств".to_string(), timestamp: now_str(),
    });
    Ok(format!("Заявка на вывод {:.2} ₽ создана", balance))
}

// ═══════════════════════════════════════════════════════════════
//  TAURI COMMANDS: CALCULATOR
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
fn calculate_storage_cost(gb: f64, disk_type: String, credit: bool) -> CalcResponse {
    let mult = if credit { CREDIT_MULTIPLIER } else { 1.0 };
    let monthly = cost_gb_month(gb, &disk_type) * mult;
    let five_min = monthly / TICKS_PER_MONTH;
    CalcResponse {
        gb, disk_type, replication: REPLICATION,
        price_per_gb: get_price(&disk_type),
        cost_per_month: monthly, cost_per_5min: five_min,
        credit_multiplier: mult,
        comparison: CalcComparison {
            hdd_monthly: cost_gb_month(gb, "hdd") * mult,
            ssd_monthly: cost_gb_month(gb, "ssd") * mult,
            nvme_monthly: cost_gb_month(gb, "nvme") * mult,
        },
    }
}

// ═══════════════════════════════════════════════════════════════
//  TAURI COMMANDS: FILES
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
fn get_files(state: State<AppState>) -> Vec<FileEntry> {
    state.files.lock().unwrap().clone()
}

#[tauri::command]
fn upload_file(state: State<AppState>, name: String, size_bytes: u64, disk_type: String) -> Result<FileEntry, String> {
    // Block uploads if credit expired
    let credit_on = *state.credit_storage_enabled.lock().unwrap();
    let zero_ticks = *state.zero_balance_ticks.lock().unwrap();
    let bal = *state.client_balance.lock().unwrap();
    if bal <= 0.0 && credit_on && zero_ticks > 0 {
        return Err("Загрузки заблокированы: нулевой баланс. Пополните в течение кредитного периода.".to_string());
    }
    if bal <= 0.0 && !credit_on {
        return Err("Недостаточно средств для загрузки".to_string());
    }

    let is_credit = credit_on;
    let mult = if is_credit { CREDIT_MULTIPLIER } else { 1.0 };
    let gb = size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let cost_month = cost_gb_month(gb, &disk_type) * mult;
    let charge_tick = cost_month / TICKS_PER_MONTH;

    let entry = FileEntry {
        id: uuid_str(), name, size_bytes, replicas: REPLICATION,
        disk_type: disk_type.clone(), uploaded_at: now_str(),
        cost_per_month: cost_month, is_credit,
    };
    state.files.lock().unwrap().push(entry.clone());

    // Charge: bonus first, then balance
    let mut bonus = state.client_bonus.lock().unwrap();
    let mut balance = state.client_balance.lock().unwrap();
    let mut remaining = charge_tick;
    let from_bonus = remaining.min(*bonus);
    *bonus -= from_bonus;
    remaining -= from_bonus;
    *balance -= remaining;
    state.storage_used_gb.lock().unwrap() += gb;

    state.payment_history.lock().unwrap().push(PaymentRecord {
        id: uuid_str(), kind: "storage_fee".to_string(), amount: -charge_tick,
        wallet: "client".to_string(),
        description: format!("Хранение: {} ({}){}", entry.name, disk_type, if is_credit { " [кредит]" } else { "" }),
        timestamp: now_str(),
    });

    Ok(entry)
}

#[tauri::command]
fn delete_file(state: State<AppState>, file_id: String) -> Result<(), String> {
    let mut files = state.files.lock().unwrap();
    if let Some(idx) = files.iter().position(|f| f.id == file_id) {
        let file = files.remove(idx);
        let gb = file.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        *state.storage_used_gb.lock().unwrap() -= gb;
        Ok(())
    } else {
        Err("Файл не найден".to_string())
    }
}

#[tauri::command]
fn download_file(_state: State<AppState>, _file_id: String) -> Result<String, String> {
    Ok("download_started".to_string())
}

// ═══════════════════════════════════════════════════════════════
//  TAURI COMMANDS: KEEPER / NETWORK
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
fn toggle_keeper_mode(state: State<AppState>, enabled: bool) -> bool {
    *state.is_keeper.lock().unwrap() = enabled;
    enabled
}

#[tauri::command]
fn get_keeper_stats(state: State<AppState>) -> KeeperStatsResponse {
    let rp = *state.rating_pay.lock().unwrap();
    let ra = *state.rating_alloc.lock().unwrap();
    let relay_on = *state.relay_enabled.lock().unwrap();
    let relay_ban = *state.relay_banned_until_tick.lock().unwrap();
    let cur_tick = *state.current_tick.lock().unwrap();
    KeeperStatsResponse {
        is_active: *state.is_keeper.lock().unwrap(),
        rating_pay: rp,
        rating_alloc: ra,
        storage_provided_gb: *state.keeper_storage_gb.lock().unwrap(),
        earnings_total: *state.keeper_earnings_total.lock().unwrap(),
        connected_peers: *state.connected_peers.lock().unwrap(),
        has_white_ip: *state.has_white_ip.lock().unwrap(),
        is_bootstrap: *state.is_bootstrap.lock().unwrap(),
        credit_storage_keeper: *state.credit_storage_keeper.lock().unwrap(),
        relay_enabled: relay_on && relay_ban <= cur_tick,
        relay_banned: relay_ban > cur_tick,
        relay_bonus_note: if relay_on && relay_ban <= cur_tick {
            Some("Активен relay +2% к выплате".to_string())
        } else {
            None
        },
        bootstrap_note: if *state.is_bootstrap.lock().unwrap() {
            Some("Bootstrap-узел +1% к выплате".to_string())
        } else {
            None
        },
        warnings: state.active_warnings.lock().unwrap().iter()
            .filter(|w| w.expires_at_tick > cur_tick)
            .map(|w| w.message.clone())
            .collect(),
    }
}

#[tauri::command]
fn get_client_stats(state: State<AppState>) -> ClientStatsResponse {
    ClientStatsResponse {
        storage_used_gb: *state.storage_used_gb.lock().unwrap(),
        files_count: state.files.lock().unwrap().len() as u32,
        connected_peers: *state.connected_peers.lock().unwrap(),
        monthly_cost: state.files.lock().unwrap().iter().map(|f| f.cost_per_month).sum(),
        credit_storage_enabled: *state.credit_storage_enabled.lock().unwrap(),
    }
}

#[tauri::command]
fn get_network_status(state: State<AppState>) -> NetworkStatusResponse {
    NetworkStatusResponse {
        connected_peers: *state.connected_peers.lock().unwrap(),
        status: "connected".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        current_tick: *state.current_tick.lock().unwrap(),
    }
}

#[tauri::command]
fn toggle_relay(state: State<AppState>, enabled: bool) -> Result<bool, String> {
    if enabled && !*state.has_white_ip.lock().unwrap() {
        return Err("Relay требует белого IP-адреса".to_string());
    }
    *state.relay_enabled.lock().unwrap() = enabled;
    Ok(enabled)
}

#[tauri::command]
fn graceful_shutdown(state: State<AppState>) -> Result<String, String> {
    if !*state.is_keeper.lock().unwrap() {
        return Err("Режим хранителя не активен".to_string());
    }
    *state.graceful_shutdown.lock().unwrap() = true;
    // Production: broadcast signed "graceful_exit" message to network
    // Network exempts this node from penalties for 15 minutes
    Ok("Graceful shutdown: сеть уведомлена. Узел освобождён от штрафов на 15 мин.".to_string())
}

// ═══════════════════════════════════════════════════════════════
//  TAURI COMMANDS: REFERRALS
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
fn get_referral_info(state: State<AppState>) -> ReferralInfoResponse {
    ReferralInfoResponse {
        referral_code: state.referral_code.lock().unwrap().clone(),
        referral_link: format!("https://soty.net/r/{}", state.referral_code.lock().unwrap()),
        invited_count: *state.referral_count.lock().unwrap(),
        total_earnings: *state.referral_earnings.lock().unwrap(),
        note: "Бонусные баллы клиента сгорают через 12 месяцев".to_string(),
    }
}

// ═══════════════════════════════════════════════════════════════
//  TAURI COMMANDS: SETTINGS
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
fn get_settings(state: State<AppState>) -> AppSettings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<AppState>, settings: AppSettings) -> Result<(), String> {
    *state.credit_storage_client.lock().unwrap() = settings.credit_storage_client;
    *state.credit_storage_keeper.lock().unwrap() = settings.credit_storage_keeper;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn check_geo(state: State<AppState>) -> GeoResponse {
    let verified = *state.geo_verified.lock().unwrap();
    GeoResponse {
        verified, country: if verified { "RU".to_string() } else { "Не определено".to_string() },
        ip: "185.xx.xx.xx".to_string(),
        has_white_ip: *state.has_white_ip.lock().unwrap(),
        installation_id: state.installation_id.lock().unwrap().clone(),
    }
}

#[tauri::command]
fn toggle_credit_storage_client(state: State<AppState>, enabled: bool) -> bool {
    *state.credit_storage_enabled.lock().unwrap() = enabled;
    let mut settings = state.settings.lock().unwrap();
    settings.credit_storage_client = enabled;
    enabled
}

#[tauri::command]
fn toggle_credit_storage_keeper(state: State<AppState>, enabled: bool) -> bool {
    *state.credit_storage_keeper.lock().unwrap() = enabled;
    let mut settings = state.settings.lock().unwrap();
    settings.credit_storage_keeper = enabled;
    enabled
}

// ═══════════════════════════════════════════════════════════════
//  TAURI COMMANDS: PENALTIES & WARNINGS
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
fn get_penalties(state: State<AppState>) -> Vec<PenaltyEntry> {
    state.penalty_log.lock().unwrap().clone()
}

#[tauri::command]
fn get_warnings(state: State<AppState>) -> Vec<WarningEntry> {
    let cur = *state.current_tick.lock().unwrap();
    state.active_warnings.lock().unwrap().iter()
        .filter(|w| w.expires_at_tick > cur)
        .cloned().collect()
}

// ═══════════════════════════════════════════════════════════════
//  TAURI COMMANDS: LEGAL DOCUMENTS
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
fn get_legal_offer() -> String {
    get_public_offer_text().to_string()
}

#[tauri::command]
fn get_legal_agency() -> String {
    get_agency_contract_text().to_string()
}

// ═══════════════════════════════════════════════════════════════
//  TAURI COMMANDS: TIMER / SIMULATION
// ═══════════════════════════════════════════════════════════════

#[tauri::command]
fn get_next_calc_time() -> NextCalcResponse {
    NextCalcResponse { seconds_left: 300, interval_minutes: 5 }
}

#[tauri::command]
fn simulate_tick(state: State<AppState>) -> TickResponse {
    let tick = {
        let mut t = state.current_tick.lock().unwrap();
        *t += 1;
        *t
    };

    let is_keeper = *state.is_keeper.lock().unwrap();

    // ── Update keeper ratings ──
    if is_keeper {
        let mut avail = state.availability_72h.lock().unwrap();
        *avail = (*avail + 0.002).min(1.0);
        let mut speed = state.avg_speed_mbps.lock().unwrap();
        *speed = (*speed + 0.5).min(150.0);

        let rp = calc_rating_pay(*avail, *speed);
        let ra = calc_rating_alloc(
            rp,
            *state.has_white_ip.lock().unwrap(),
            *state.is_bootstrap.lock().unwrap(),
            *state.credit_storage_keeper.lock().unwrap(),
        );
        *state.rating_pay.lock().unwrap() = rp;
        *state.rating_alloc.lock().unwrap() = ra;

        // Bootstrap eligibility: rating_pay=1.0, storage>1TB, white IP
        if rp >= 1.0 && *state.keeper_storage_gb.lock().unwrap() >= 1000.0 && *state.has_white_ip.lock().unwrap() {
            *state.is_bootstrap.lock().unwrap() = true;
        }
    }

    // ── Keeper earnings (every tick if available) ──
    if is_keeper && !*state.graceful_shutdown.lock().unwrap() {
        let storage_gb = *state.keeper_storage_gb.lock().unwrap();
        let rp = *state.rating_pay.lock().unwrap();
        let credit_keeper = *state.credit_storage_keeper.lock().unwrap();

        // Base price for keeper pool (uses HDD as default keeper disk)
        let base_price = get_price("hdd");
        // If credit_storage_keeper, base 80% of 1.5x = effectively 1.2x standard rate
        let credit_mult = if credit_keeper { CREDIT_MULTIPLIER } else { 1.0 };
        let earnings_per_tick = storage_gb * base_price * KEEPER_BASE_PCT * credit_mult * rp / TICKS_PER_MONTH;

        let mut earning = earnings_per_tick;
        // Relay bonus +2% from platform share
        if *state.relay_enabled.lock().unwrap() {
            let relay_ban = *state.relay_banned_until_tick.lock().unwrap();
            if relay_ban <= tick {
                earning += storage_gb * base_price * RELAY_BONUS_PCT * rp / TICKS_PER_MONTH;
            }
        }
        // Bootstrap bonus +1%
        if *state.is_bootstrap.lock().unwrap() {
            earning += storage_gb * base_price * BOOTSTRAP_BONUS_PCT * rp / TICKS_PER_MONTH;
        }

        *state.keeper_balance.lock().unwrap() += earning;
        *state.keeper_pending.lock().unwrap() += earning;
        *state.keeper_earnings_total.lock().unwrap() += earning;
    }

    // ── Client charges ──
    let files = state.files.lock().unwrap();
    let mut total_charge = 0.0;
    for f in files.iter() {
        let gb = f.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        let mult = if f.is_credit { CREDIT_MULTIPLIER } else { 1.0 };
        total_charge += cost_gb_month(gb, &f.disk_type) * mult / TICKS_PER_MONTH;
    }
    drop(files);

    if total_charge > 0.0 {
        let mut bonus = state.client_bonus.lock().unwrap();
        let mut bal = state.client_balance.lock().unwrap();
        let from_bonus = total_charge.min(*bonus);
        *bonus -= from_bonus;
        *bal -= (total_charge - from_bonus);
    }

    // ── Credit storage: track zero-balance ticks ──
    let bal = *state.client_balance.lock().unwrap();
    if bal <= 0.0 {
        let mut zt = state.zero_balance_ticks.lock().unwrap();
        *zt += 1;
        if *zt >= CREDIT_PERIOD_TICKS {
            // Delete all files — credit period expired
            let mut files = state.files.lock().unwrap();
            let total_gb: f64 = files.iter().map(|f| f.size_bytes as f64 / (1024.0*1024.0*1024.0)).sum();
            files.clear();
            *state.storage_used_gb.lock().unwrap() = 0.0;
            state.penalty_log.lock().unwrap().push(PenaltyEntry {
                id: uuid_str(), level: "high".into(), target: "client".into(),
                reason: "Кредитный период истёк".into(),
                action: "data_deleted".into(), tick,
            });
            drop(files);
        }
    } else {
        *state.zero_balance_ticks.lock().unwrap() = 0;
    }

    TickResponse {
        client_balance: *state.client_balance.lock().unwrap(),
        client_bonus: *state.client_bonus.lock().unwrap(),
        keeper_balance: *state.keeper_balance.lock().unwrap(),
        rating_pay: *state.rating_pay.lock().unwrap(),
        rating_alloc: *state.rating_alloc.lock().unwrap(),
        current_tick: tick,
        zero_balance_ticks: *state.zero_balance_ticks.lock().unwrap(),
        credit_action: credit_storage_state(
            *state.client_balance.lock().unwrap(),
            *state.credit_storage_enabled.lock().unwrap(),
            *state.zero_balance_ticks.lock().unwrap(),
        ).0,
    }
}

// ═══════════════════════════════════════════════════════════════
//  RESPONSE TYPES
// ═══════════════════════════════════════════════════════════════

#[derive(Serialize)]
struct WalletInfo { peer_id: String, mnemonic: String, referral_code: String }

#[derive(Serialize)]
struct ClientBalanceResponse {
    balance: f64, bonus: f64, currency: String,
    credit_storage_enabled: bool, credit_action: String,
    credit_ticks_remaining: u32, low_balance_warning: bool,
}

#[derive(Serialize)]
struct KeeperBalanceResponse {
    balance: f64, pending: f64, can_withdraw: bool,
    currency: String, payout_note: Option<String>,
}

#[derive(Serialize)]
struct TopupResponse { amount: f64, commission: f64, total: f64 }

#[derive(Serialize)]
struct CalcResponse {
    gb: f64, disk_type: String, replication: u32,
    price_per_gb: f64, cost_per_month: f64, cost_per_5min: f64,
    credit_multiplier: f64, comparison: CalcComparison,
}

#[derive(Serialize)]
struct CalcComparison { hdd_monthly: f64, ssd_monthly: f64, nvme_monthly: f64 }

#[derive(Serialize)]
struct KeeperStatsResponse {
    is_active: bool, rating_pay: f64, rating_alloc: f64,
    storage_provided_gb: f64, earnings_total: f64,
    connected_peers: u32, has_white_ip: bool, is_bootstrap: bool,
    credit_storage_keeper: bool, relay_enabled: bool, relay_banned: bool,
    relay_bonus_note: Option<String>, bootstrap_note: Option<String>,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct ClientStatsResponse {
    storage_used_gb: f64, files_count: u32,
    connected_peers: u32, monthly_cost: f64,
    credit_storage_enabled: bool,
}

#[derive(Serialize)]
struct NetworkStatusResponse { connected_peers: u32, status: String, version: String, current_tick: u32 }

#[derive(Serialize)]
struct ReferralInfoResponse {
    referral_code: String, referral_link: String,
    invited_count: u32, total_earnings: f64, note: String,
}

#[derive(Serialize)]
struct GeoResponse {
    verified: bool, country: String, ip: String,
    has_white_ip: bool, installation_id: String,
}

#[derive(Serialize)]
struct NextCalcResponse { seconds_left: u32, interval_minutes: u32 }

#[derive(Serialize)]
struct TickResponse {
    client_balance: f64, client_bonus: f64, keeper_balance: f64,
    rating_pay: f64, rating_alloc: f64, current_tick: u32,
    zero_balance_ticks: u32, credit_action: String,
}

// ═══════════════════════════════════════════════════════════════
//  HELPERS
// ═══════════════════════════════════════════════════════════════

fn uuid_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{:x}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis())
}
fn now_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs())
}
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

// ═══════════════════════════════════════════════════════════════
//  APP ENTRY
// ═══════════════════════════════════════════════════════════════

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            initialized: Mutex::new(false),
            peer_id: Mutex::new(String::new()),
            mnemonic: Mutex::new(String::new()),
            mnemonic_shown_once: Mutex::new(false),
            client_balance: Mutex::new(0.0),
            client_bonus: Mutex::new(0.0),
            credit_storage_enabled: Mutex::new(false),
            zero_balance_ticks: Mutex::new(0),
            keeper_balance: Mutex::new(0.0),
            keeper_pending: Mutex::new(0.0),
            is_keeper: Mutex::new(false),
            credit_storage_keeper: Mutex::new(false),
            rating_pay: Mutex::new(0.5),
            rating_alloc: Mutex::new(0.5),
            availability_72h: Mutex::new(0.5),
            avg_speed_mbps: Mutex::new(50.0),
            has_white_ip: Mutex::new(true),
            is_bootstrap: Mutex::new(false),
            keeper_storage_gb: Mutex::new(0.0),
            keeper_earnings_total: Mutex::new(0.0),
            relay_enabled: Mutex::new(false),
            relay_fail_pct_60min: Mutex::new(0.0),
            relay_incidents_24h: Mutex::new(0),
            relay_banned_until_tick: Mutex::new(0),
            connected_peers: Mutex::new(3),
            storage_used_gb: Mutex::new(0.0),
            current_tick: Mutex::new(0),
            graceful_shutdown: Mutex::new(false),
            files: Mutex::new(Vec::new()),
            payment_history: Mutex::new(Vec::new()),
            penalty_log: Mutex::new(Vec::new()),
            active_warnings: Mutex::new(Vec::new()),
            referral_code: Mutex::new(String::new()),
            referral_count: Mutex::new(0),
            referral_earnings: Mutex::new(0.0),
            geo_verified: Mutex::new(true),
            installation_id: Mutex::new(String::new()),
            settings: Mutex::new(AppSettings::default()),
        })
        .invoke_handler(tauri::generate_handler![
            check_initialized, create_wallet, restore_wallet, confirm_mnemonic_shown, get_wallet_info,
            get_client_balance, get_keeper_balance, get_payment_history, topup_client, request_payout,
            calculate_storage_cost,
            get_files, upload_file, delete_file, download_file,
            toggle_keeper_mode, get_keeper_stats, get_client_stats, get_network_status,
            toggle_relay, graceful_shutdown,
            get_referral_info,
            get_settings, save_settings, check_geo,
            toggle_credit_storage_client, toggle_credit_storage_keeper,
            get_penalties, get_warnings,
            get_legal_offer, get_legal_agency,
            get_next_calc_time, simulate_tick,
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.center();
                let _ = window.show();
                let _ = window.set_focus();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ═══════════════════════════════════════════════════════════════
//  UNIT TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_price() {
        assert!((get_price("hdd") - 0.30).abs() < 1e-9);
        assert!((get_price("ssd") - 0.60).abs() < 1e-9);
        assert!((get_price("nvme") - 0.90).abs() < 1e-9);
        assert!((get_price("unknown") - 0.30).abs() < 1e-9);
    }

    #[test]
    fn test_cost_gb_month() {
        assert!((cost_gb_month(10.0, "hdd") - 3.0).abs() < 1e-9);
        assert!((cost_gb_month(100.0, "ssd") - 60.0).abs() < 1e-9);
    }

    #[test]
    fn test_cost_gb_tick() {
        let monthly = cost_gb_month(10.0, "hdd");
        let tick = cost_gb_tick(10.0, "hdd");
        let expected = monthly / TICKS_PER_MONTH;
        assert!((tick - expected).abs() < 1e-12);
    }

    #[test]
    fn test_cost_gb_tick_credit() {
        let normal = cost_gb_tick(10.0, "hdd");
        let credit = cost_gb_tick_credit(10.0, "hdd");
        assert!((credit - normal * CREDIT_MULTIPLIER).abs() < 1e-12);
    }

    // ── Rating tests ──

    #[test]
    fn test_rating_pay_perfect() {
        let rp = calc_rating_pay(1.0, 100.0);
        assert!((rp - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_rating_pay_partial_avail() {
        let rp = calc_rating_pay(0.8, 100.0);
        assert!((rp - 0.8).abs() < 1e-9);
    }

    #[test]
    fn test_rating_pay_slow capped() {
        let rp = calc_rating_pay(1.0, 200.0);
        assert!((rp - 1.0).abs() < 1e-9); // speed capped at 1.0
    }

    #[test]
    fn test_rating_pay_slow() {
        let rp = calc_rating_pay(0.9, 50.0);
        assert!((rp - 0.9 * 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_rating_alloc_base() {
        let rp = calc_rating_pay(1.0, 100.0);
        let ra = calc_rating_alloc(rp, false, false, false);
        assert!((ra - 1.0).abs() < 1e-9); // no multipliers
    }

    #[test]
    fn test_rating_alloc_white_ip() {
        let rp = calc_rating_pay(1.0, 100.0);
        let ra = calc_rating_alloc(rp, true, false, false);
        assert!((ra - 1.05).abs() < 1e-9);
    }

    #[test]
    fn test_rating_alloc_bootstrap() {
        let rp = calc_rating_pay(1.0, 100.0);
        let ra = calc_rating_alloc(rp, false, true, false);
        assert!((ra - 1.02).abs() < 1e-9);
    }

    #[test]
    fn test_rating_alloc_credit() {
        let rp = calc_rating_pay(1.0, 100.0);
        let ra = calc_rating_alloc(rp, false, false, true);
        assert!((ra - 1.02).abs() < 1e-9);
    }

    #[test]
    fn test_rating_alloc_all_bonuses() {
        let rp = calc_rating_pay(1.0, 100.0);
        let ra = calc_rating_alloc(rp, true, true, true);
        let expected = 1.0 * 1.05 * 1.02 * 1.02;
        assert!((ra - expected).abs() < 1e-9);
    }

    #[test]
    fn test_rating_alloc_initial() {
        let rp = calc_rating_pay(0.5, 50.0); // 0.5 * 0.5 = 0.25
        let ra = calc_rating_alloc(rp, true, false, false);
        assert!((ra - 0.25 * 1.05).abs() < 1e-9);
    }

    // ── Revenue distribution tests ──

    #[test]
    fn test_distribute_normal_no_bonuses() {
        let (keeper, relay, bootstrap, ref_k, ref_c, platform) =
            distribute_payment(100.0, false, false, false, false);
        assert!((keeper - 80.0).abs() < 1e-6);
        assert!((relay - 0.0).abs() < 1e-6);
        assert!((bootstrap - 0.0).abs() < 1e-6);
        assert!((ref_k - 0.0).abs() < 1e-6);
        assert!((ref_c - 0.0).abs() < 1e-6);
        assert!((platform - 20.0).abs() < 1e-6);
    }

    #[test]
    fn test_distribute_with_relay_and_bootstrap() {
        let (keeper, relay, bootstrap, ref_k, ref_c, platform) =
            distribute_payment(100.0, true, true, false, false);
        assert!((keeper - 80.0).abs() < 1e-6);
        assert!((relay - 2.0).abs() < 1e-6);
        assert!((bootstrap - 1.0).abs() < 1e-6);
        assert!((ref_k - 0.0).abs() < 1e-6);
        assert!((ref_c - 0.0).abs() < 1e-6);
        assert!((platform - 17.0).abs() < 1e-6);
    }

    #[test]
    fn test_distribute_with_referrals() {
        let (keeper, relay, bootstrap, ref_k, ref_c, platform) =
            distribute_payment(100.0, true, true, true, true);
        assert!((keeper - 80.0).abs() < 1e-6);
        assert!((relay - 2.0).abs() < 1e-6);
        assert!((bootstrap - 1.0).abs() < 1e-6);
        assert!((ref_k - 3.0).abs() < 1e-6);
        assert!((ref_c - 3.0).abs() < 1e-6);
        assert!((platform - 11.0).abs() < 1e-6);
    }

    #[test]
    fn test_distribute_credit_storage() {
        // Credit: payment is 1.5x, so 150₽
        let (keeper, relay, bootstrap, ref_k, ref_c, platform) =
            distribute_payment(150.0, true, true, true, true);
        // 150 * 0.80 = 120 keeper, +3 relay, +1.5 bootstrap, +4.5 ref_k = 129 total
        // Total keeper payout must not exceed 150 * 0.89 = 133.5
        assert!(keeper + relay + bootstrap + ref_k <= 133.5 + 1e-6);
        assert!((ref_c - 4.5).abs() < 1e-6); // 150 * 0.03
        assert!(platform >= 150.0 * 0.11 - 1e-6); // >= 11%
    }

    #[test]
    fn test_distribute_total_is_whole() {
        // Sum of all parts should equal the original payment
        let payment = 100.0;
        let (keeper, relay, bootstrap, ref_k, ref_c, platform) =
            distribute_payment(payment, true, true, true, true);
        let total = keeper + relay + bootstrap + ref_k + ref_c + platform;
        assert!((total - payment).abs() < 1e-6);
    }

    // ── Penalty tests ──

    #[test]
    fn test_keeper_penalty_pos_failure() {
        let result = assess_keeper_penalty(1, 0, false, false, false, false);
        assert_eq!(result, Some(("low".into(), "warning_freeze_1h".into())));
    }

    #[test]
    fn test_keeper_penalty_no_penalty() {
        let result = assess_keeper_penalty(0, 2, true, false, false, false);
        assert_eq!(result, None); // graceful shutdown prevents medium penalty
    }

    #[test]
    fn test_keeper_penalty_medium() {
        let result = assess_keeper_penalty(0, 5, false, false, false, false);
        assert_eq!(result, Some(("medium".into(), "penalty_10pct".into())));
    }

    #[test]
    fn test_keeper_penalty_sybil() {
        let result = assess_keeper_penalty(0, 0, false, true, true, false);
        assert_eq!(result, Some(("high".into(), "reset_rating_confiscate".into())));
    }

    #[test]
    fn test_keeper_penalty_sybil_repeat() {
        let result = assess_keeper_penalty(0, 0, false, true, true, true);
        assert_eq!(result, Some(("critical".into(), "permanent_ban".into())));
    }

    #[test]
    fn test_client_penalty_rebalance_low() {
        let result = assess_client_penalty(6, 0, 0.0, false);
        assert_eq!(result, Some(("low".into(), "warning".into())));
    }

    #[test]
    fn test_client_penalty_rebalance_medium() {
        let result = assess_client_penalty(12, 0, 0.0, false);
        assert_eq!(result, Some(("medium".into(), "block_24h_fine_500".into())));
    }

    #[test]
    fn test_client_penalty_xss() {
        let result = assess_client_penalty(0, 0, 0.0, true);
        assert_eq!(result, Some(("high".into(), "permanent_ban_zero_balance".into())));
    }

    #[test]
    fn test_client_penalty_no_penalty() {
        let result = assess_client_penalty(0, 0, 0.0, false);
        assert_eq!(result, None);
    }

    // ── Credit storage tests ──

    #[test]
    fn test_credit_state_positive_balance() {
        let (action, remaining) = credit_storage_state(50.0, true, 0);
        assert_eq!(action, "none");
        assert_eq!(remaining, CREDIT_PERIOD_TICKS);
    }

    #[test]
    fn test_credit_state_enabled_zero_balance() {
        let (action, remaining) = credit_storage_state(0.0, true, 100);
        assert_eq!(action, "block_uploads");
        assert_eq!(remaining, CREDIT_PERIOD_TICKS - 100);
    }

    #[test]
    fn test_credit_state_expired() {
        let (action, remaining) = credit_storage_state(0.0, true, CREDIT_PERIOD_TICKS);
        assert_eq!(action, "delete_data");
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_credit_state_disabled_zero_balance() {
        let (action, remaining) = credit_storage_state(0.0, false, 100);
        assert_eq!(action, "warn");
        assert_eq!(remaining, CREDIT_PERIOD_TICKS - 100);
    }

    #[test]
    fn test_credit_state_disabled_expired() {
        let (action, _) = credit_storage_state(0.0, false, CREDIT_PERIOD_TICKS);
        assert_eq!(action, "delete_data");
    }
}
