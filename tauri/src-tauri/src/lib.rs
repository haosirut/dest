//! Соты — Tauri v2 backend commands for P2P storage desktop app.

use tauri::{Manager, State};
use std::sync::Mutex;
use serde::{Serialize, Deserialize};

// ─── Application State ───────────────────────────────────────────

struct AppState {
    initialized: Mutex<bool>,
    peer_id: Mutex<String>,
    mnemonic: Mutex<String>,
    client_balance: Mutex<f64>,
    client_bonus: Mutex<f64>,
    keeper_balance: Mutex<f64>,
    keeper_pending: Mutex<f64>,
    is_keeper: Mutex<bool>,
    keeper_rating: Mutex<f64>,
    keeper_storage_gb: Mutex<f64>,
    keeper_earnings_total: Mutex<f64>,
    connected_peers: Mutex<u32>,
    storage_used_gb: Mutex<f64>,
    files: Mutex<Vec<FileEntry>>,
    payment_history: Mutex<Vec<PaymentRecord>>,
    referral_code: Mutex<String>,
    referral_count: Mutex<u32>,
    referral_earnings: Mutex<f64>,
    geo_verified: Mutex<bool>,
    relay_enabled: Mutex<bool>,
    settings: Mutex<AppSettings>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    bootstrap_nodes: Vec<String>,
    storage_limit_gb: u32,
    auto_start: bool,
    russia_only: bool,
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
}

#[derive(Clone, Serialize, Deserialize)]
struct PaymentRecord {
    id: String,
    kind: String,       // "deposit", "storage_fee", "payout", "referral_bonus", "relay_bonus"
    amount: f64,
    wallet: String,     // "client", "keeper"
    description: String,
    timestamp: String,
}

// ─── Pricing Constants ───────────────────────────────────────────

const PRICE_HDD: f64 = 0.30;  // ₽/GB/month (with x4 replication included)
const PRICE_SSD: f64 = 0.60;
const PRICE_NVME: f64 = 0.90;
const REPLICATION: u32 = 4;

fn get_price(disk_type: &str) -> f64 {
    match disk_type {
        "nvme" => PRICE_NVME,
        "ssd" => PRICE_SSD,
        _ => PRICE_HDD,
    }
}

fn calculate_cost_gb_month(gb: f64, disk_type: &str) -> f64 {
    gb * get_price(disk_type)
}

fn calculate_cost_5min(gb: f64, disk_type: &str) -> f64 {
    let monthly = calculate_cost_gb_month(gb, disk_type);
    monthly / (30.0 * 24.0 * 12.0) // 30 days * 24 hours * 12 five-minute intervals
}

// ─── Tauri Commands: Onboarding ──────────────────────────────────

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
    let mnemonic_words = vec![
        "абрикос", "банан", "вишня", "гранат", "дыня", "ежевика", "земляника",
        "инжир", "клубника", "лимон", "малина", "нектарин", "облепиха", "персик",
        "рябина", "смородина", "тыква", " финик", "хурма", "черешня", "яблоко",
        "айва", "груша", "слива",
    ];
    let mnemonic = mnemonic_words.join(" ");

    *state.initialized.lock().unwrap() = true;
    *state.peer_id.lock().unwrap() = peer_id.clone();
    *state.mnemonic.lock().unwrap() = mnemonic.clone();
    *state.referral_code.lock().unwrap() = format!("SOTY-{}", to_hex(&rng_state[..4]).to_uppercase());

    WalletInfo {
        peer_id,
        mnemonic,
        referral_code: state.referral_code.lock().unwrap().clone(),
    }
}

#[tauri::command]
fn restore_wallet(state: State<AppState>, _mnemonic: String) -> Result<WalletInfo, String> {
    // In production: validate mnemonic and derive keypair
    let info = create_wallet(state);
    Ok(info)
}

#[tauri::command]
fn get_wallet_info(state: State<AppState>) -> WalletInfo {
    WalletInfo {
        peer_id: state.peer_id.lock().unwrap().clone(),
        mnemonic: state.mnemonic.lock().unwrap().clone(),
        referral_code: state.referral_code.lock().unwrap().clone(),
    }
}

// ─── Tauri Commands: Balances ────────────────────────────────────

#[tauri::command]
fn get_client_balance(state: State<AppState>) -> ClientBalanceResponse {
    ClientBalanceResponse {
        balance: *state.client_balance.lock().unwrap(),
        bonus: *state.client_bonus.lock().unwrap(),
        currency: "RUB".to_string(),
    }
}

#[tauri::command]
fn get_keeper_balance(state: State<AppState>) -> KeeperBalanceResponse {
    let pending = *state.keeper_pending.lock().unwrap();
    KeeperBalanceResponse {
        balance: *state.keeper_balance.lock().unwrap(),
        pending,
        can_withdraw: pending >= 100.0,
        currency: "RUB".to_string(),
        payout_note: if pending < 100.0 {
            Some(format!("Минимальная сумма вывода: 100 ₽. Ещё нужно: {:.2} ₽", 100.0 - pending))
        } else {
            Some("Вывод доступен (будни 10:00–18:00 МСК)".to_string())
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

    let mut balance = state.client_balance.lock().unwrap();
    *balance += amount;

    let record = PaymentRecord {
        id: uuid_str(),
        kind: "deposit".to_string(),
        amount,
        wallet: "client".to_string(),
        description: format!("Пополнение ({}{}", if method == "card" { "карта, комиссия " } else { "СБП" }, commission),
        timestamp: now_str(),
    };
    state.payment_history.lock().unwrap().push(record);

    TopupResponse {
        amount,
        commission,
        total,
    }
}

#[tauri::command]
fn request_payout(state: State<AppState>) -> Result<String, String> {
    let balance = *state.keeper_balance.lock().unwrap();
    if balance < 100.0 {
        return Err(format!("Минимальная сумма вывода: 100 ₽. Текущий баланс: {:.2} ₽", balance));
    }
    let amount = balance;
    *state.keeper_balance.lock().unwrap() = 0.0;

    let record = PaymentRecord {
        id: uuid_str(),
        kind: "payout".to_string(),
        amount: -amount,
        wallet: "keeper".to_string(),
        description: "Вывод средств".to_string(),
        timestamp: now_str(),
    };
    state.payment_history.lock().unwrap().push(record);

    Ok(format!("Заявка на вывод {:.2} ₽ создана", amount))
}

// ─── Tauri Commands: Calculator ──────────────────────────────────

#[tauri::command]
fn calculate_storage_cost(gb: f64, disk_type: String) -> CalcResponse {
    let monthly = calculate_cost_gb_month(gb, &disk_type);
    let five_min = calculate_cost_5min(gb, &disk_type);
    CalcResponse {
        gb,
        disk_type,
        replication: REPLICATION,
        price_per_gb: get_price(&disk_type),
        cost_per_month: monthly,
        cost_per_5min: five_min,
        comparison: CalcComparison {
            hdd_monthly: calculate_cost_gb_month(gb, "hdd"),
            ssd_monthly: calculate_cost_gb_month(gb, "ssd"),
            nvme_monthly: calculate_cost_gb_month(gb, "nvme"),
        },
    }
}

// ─── Tauri Commands: Files ───────────────────────────────────────

#[tauri::command]
fn get_files(state: State<AppState>) -> Vec<FileEntry> {
    state.files.lock().unwrap().clone()
}

#[tauri::command]
fn upload_file(state: State<AppState>, name: String, size_bytes: u64, disk_type: String) -> FileEntry {
    let gb = size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    let cost_month = calculate_cost_gb_month(gb, &disk_type);

    let entry = FileEntry {
        id: uuid_str(),
        name,
        size_bytes,
        replicas: REPLICATION,
        disk_type: disk_type.clone(),
        uploaded_at: now_str(),
        cost_per_month: cost_month,
    };
    state.files.lock().unwrap().push(entry.clone());

    // Charge client balance
    let charge_5min = calculate_cost_5min(gb, &disk_type);
    let mut bal = state.client_balance.lock().unwrap();
    if *bal >= charge_5min {
        *bal -= charge_5min;
    }
    // Also use bonus first
    let mut bonus = state.client_bonus.lock().unwrap();
    if *bal < 0.0 {
        let diff = -*bal;
        if *bonus >= diff {
            *bonus -= diff;
            *bal = 0.0;
        } else {
            *bal -= *bonus;
            *bonus = 0.0;
        }
    }

    state.storage_used_gb.lock().unwrap() += gb;

    let record = PaymentRecord {
        id: uuid_str(),
        kind: "storage_fee".to_string(),
        amount: -charge_5min,
        wallet: "client".to_string(),
        description: format!("Хранение: {} ({})", entry.name, disk_type),
        timestamp: now_str(),
    };
    state.payment_history.lock().unwrap().push(record);

    entry
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
    // In production: decrypt and save file to downloads
    Ok("download_started".to_string())
}

// ─── Tauri Commands: Keeper / Network ────────────────────────────

#[tauri::command]
fn toggle_keeper_mode(state: State<AppState>, enabled: bool) -> bool {
    *state.is_keeper.lock().unwrap() = enabled;
    enabled
}

#[tauri::command]
fn get_keeper_stats(state: State<AppState>) -> KeeperStatsResponse {
    KeeperStatsResponse {
        is_active: *state.is_keeper.lock().unwrap(),
        rating: *state.keeper_rating.lock().unwrap(),
        storage_provided_gb: *state.keeper_storage_gb.lock().unwrap(),
        earnings_total: *state.keeper_earnings_total.lock().unwrap(),
        connected_peers: *state.connected_peers.lock().unwrap(),
        relay_enabled: *state.relay_enabled.lock().unwrap(),
        relay_bonus_note: if *state.relay_enabled.lock().unwrap() {
            Some("Активен relay +3% к выплате".to_string())
        } else {
            None
        },
    }
}

#[tauri::command]
fn get_client_stats(state: State<AppState>) -> ClientStatsResponse {
    ClientStatsResponse {
        storage_used_gb: *state.storage_used_gb.lock().unwrap(),
        files_count: state.files.lock().unwrap().len() as u32,
        connected_peers: *state.connected_peers.lock().unwrap(),
        monthly_cost: state.files.lock().unwrap().iter()
            .map(|f| f.cost_per_month)
            .sum(),
    }
}

#[tauri::command]
fn get_network_status(state: State<AppState>) -> NetworkStatusResponse {
    NetworkStatusResponse {
        connected_peers: *state.connected_peers.lock().unwrap(),
        status: "connected".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
fn toggle_relay(state: State<AppState>, enabled: bool) -> bool {
    *state.relay_enabled.lock().unwrap() = enabled;
    enabled
}

// ─── Tauri Commands: Referrals ───────────────────────────────────

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

#[tauri::command]
fn copy_to_clipboard(_text: String) -> Result<(), String> {
    // In production: use tauri-plugin-clipboard-manager
    Ok(())
}

// ─── Tauri Commands: Settings ────────────────────────────────────

#[tauri::command]
fn get_settings(state: State<AppState>) -> AppSettings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(state: State<AppState>, settings: AppSettings) -> Result<(), String> {
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
fn check_geo(state: State<AppState>) -> GeoResponse {
    let verified = *state.geo_verified.lock().unwrap();
    GeoResponse {
        verified,
        country: if verified { "RU".to_string() } else { "Не определено".to_string() },
        ip: "185.xx.xx.xx".to_string(),
    }
}

// ─── Tauri Commands: Timer / Simulation ──────────────────────────

#[tauri::command]
fn get_next_calc_time() -> NextCalcResponse {
    // Simulate next calculation time (always within 5 minutes)
    NextCalcResponse {
        seconds_left: 300,
        interval_minutes: 5,
    }
}

#[tauri::command]
fn simulate_tick(state: State<AppState>) -> TickResponse {
    // Simulate a 5-minute tick for demo purposes
    let is_keeper = *state.is_keeper.lock().unwrap();

    // Keeper earns if active
    if is_keeper {
        let storage_gb = *state.keeper_storage_gb.lock().unwrap();
        let rating = *state.keeper_rating.lock().unwrap();
        let earnings_per_5min = storage_gb * get_price("hdd") * 0.8 * rating / (30.0 * 24.0 * 12.0);
        let relay_mult = if *state.relay_enabled.lock().unwrap() { 1.03 } else { 1.0 };

        let earning = earnings_per_5min * relay_mult;
        *state.keeper_balance.lock().unwrap() += earning;
        *state.keeper_pending.lock().unwrap() += earning;
        *state.keeper_earnings_total.lock().unwrap() += earning;
    }

    // Client gets charged
    let files = state.files.lock().unwrap();
    let mut total_charge = 0.0;
    for f in files.iter() {
        let gb = f.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        total_charge += calculate_cost_5min(gb, &f.disk_type);
    }
    drop(files);

    if total_charge > 0.0 {
        let mut bonus = state.client_bonus.lock().unwrap();
        let mut bal = state.client_balance.lock().unwrap();

        let from_bonus = total_charge.min(*bonus);
        *bonus -= from_bonus;
        let remaining = total_charge - from_bonus;
        *bal -= remaining;

        if *bal < 0.0 {
            // Insufficient funds — in production: trigger warning
        }
    }

    // Improve rating slightly
    if is_keeper {
        let mut rating = state.keeper_rating.lock().unwrap();
        *rating = (*rating + 0.002).min(1.0);
    }

    TickResponse {
        client_balance: *state.client_balance.lock().unwrap(),
        client_bonus: *state.client_bonus.lock().unwrap(),
        keeper_balance: *state.keeper_balance.lock().unwrap(),
        keeper_rating: *state.keeper_rating.lock().unwrap(),
    }
}

// ─── Response Types ──────────────────────────────────────────────

#[derive(Serialize)]
struct WalletInfo {
    peer_id: String,
    mnemonic: String,
    referral_code: String,
}

#[derive(Serialize)]
struct ClientBalanceResponse {
    balance: f64,
    bonus: f64,
    currency: String,
}

#[derive(Serialize)]
struct KeeperBalanceResponse {
    balance: f64,
    pending: f64,
    can_withdraw: bool,
    currency: String,
    payout_note: Option<String>,
}

#[derive(Serialize)]
struct TopupResponse {
    amount: f64,
    commission: f64,
    total: f64,
}

#[derive(Serialize)]
struct CalcResponse {
    gb: f64,
    disk_type: String,
    replication: u32,
    price_per_gb: f64,
    cost_per_month: f64,
    cost_per_5min: f64,
    comparison: CalcComparison,
}

#[derive(Serialize)]
struct CalcComparison {
    hdd_monthly: f64,
    ssd_monthly: f64,
    nvme_monthly: f64,
}

#[derive(Serialize)]
struct KeeperStatsResponse {
    is_active: bool,
    rating: f64,
    storage_provided_gb: f64,
    earnings_total: f64,
    connected_peers: u32,
    relay_enabled: bool,
    relay_bonus_note: Option<String>,
}

#[derive(Serialize)]
struct ClientStatsResponse {
    storage_used_gb: f64,
    files_count: u32,
    connected_peers: u32,
    monthly_cost: f64,
}

#[derive(Serialize)]
struct NetworkStatusResponse {
    connected_peers: u32,
    status: String,
    version: String,
}

#[derive(Serialize)]
struct ReferralInfoResponse {
    referral_code: String,
    referral_link: String,
    invited_count: u32,
    total_earnings: f64,
    note: String,
}

#[derive(Serialize)]
struct GeoResponse {
    verified: bool,
    country: String,
    ip: String,
}

#[derive(Serialize)]
struct NextCalcResponse {
    seconds_left: u32,
    interval_minutes: u32,
}

#[derive(Serialize)]
struct TickResponse {
    client_balance: f64,
    client_bonus: f64,
    keeper_balance: f64,
    keeper_rating: f64,
}

// ─── Helpers ─────────────────────────────────────────────────────

fn uuid_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    format!("{:x}", ts)
}

fn now_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    // Simple ISO-like format
    format!("{}", ts)
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

// ─── App Entry ───────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            initialized: Mutex::new(false),
            peer_id: Mutex::new(String::new()),
            mnemonic: Mutex::new(String::new()),
            client_balance: Mutex::new(0.0),
            client_bonus: Mutex::new(0.0),
            keeper_balance: Mutex::new(0.0),
            keeper_pending: Mutex::new(0.0),
            is_keeper: Mutex::new(false),
            keeper_rating: Mutex::new(0.5),
            keeper_storage_gb: Mutex::new(0.0),
            keeper_earnings_total: Mutex::new(0.0),
            connected_peers: Mutex::new(3),
            storage_used_gb: Mutex::new(0.0),
            files: Mutex::new(Vec::new()),
            payment_history: Mutex::new(Vec::new()),
            referral_code: Mutex::new(String::new()),
            referral_count: Mutex::new(0),
            referral_earnings: Mutex::new(0.0),
            geo_verified: Mutex::new(true),
            relay_enabled: Mutex::new(false),
            settings: Mutex::new(AppSettings::default()),
        })
        .invoke_handler(tauri::generate_handler![
            // Onboarding
            check_initialized,
            create_wallet,
            restore_wallet,
            get_wallet_info,
            // Balances
            get_client_balance,
            get_keeper_balance,
            get_payment_history,
            topup_client,
            request_payout,
            // Calculator
            calculate_storage_cost,
            // Files
            get_files,
            upload_file,
            delete_file,
            download_file,
            // Keeper / Network
            toggle_keeper_mode,
            get_keeper_stats,
            get_client_stats,
            get_network_status,
            toggle_relay,
            // Referrals
            get_referral_info,
            copy_to_clipboard,
            // Settings
            get_settings,
            save_settings,
            check_geo,
            // Timer / Simulation
            get_next_calc_time,
            simulate_tick,
        ])
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                println!("[soty] Window acquired, centering and showing...");
                let _ = window.center();
                let _ = window.show();
                let _ = window.set_focus();
                println!("[soty] Window is visible: {}", window.is_visible().unwrap_or(false));
            } else {
                println!("[soty] WARNING: main window not found!");
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
