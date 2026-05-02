//! Соты — Pure business logic (unit-testable, no State, no Tauri).

use crate::models::BonusEntry;

// ═══════════════════════════════════════════════════════════════
//  CONSTANTS
// ═══════════════════════════════════════════════════════════════

pub const PRICE_HDD: f64  = 0.30;
pub const PRICE_SSD: f64  = 0.60;
pub const PRICE_NVME: f64 = 0.90;
pub const REPLICATION: u32 = 4;
pub const CREDIT_MULTIPLIER: f64 = 1.5;
pub const RELAY_BONUS_PCT: f64 = 0.02;
pub const BOOTSTRAP_BONUS_PCT: f64 = 0.01;
pub const REFERRAL_PCT: f64 = 0.03;
pub const KEEPER_BASE_PCT: f64 = 0.80;
pub const MAX_KEEPER_TOTAL_PCT: f64 = 0.89;
pub const SPEED_REFERENCE_MBPS: f64 = 100.0;
pub const TICKS_PER_MONTH: f64 = 30.0 * 24.0 * 12.0;
pub const PAYOUT_MIN: f64 = 100.0;
pub const CREDIT_PERIOD_TICKS: u32 = (72.0 * 12.0) as u32;
pub const BONUS_EXPIRY_TICKS: u32 = (365.0 * 24.0 * 12.0) as u32;
pub const RELAY_MAX_GRAY_CLIENTS: u32 = 7;
pub const RELAY_MAX_FAIL_PCT: f64 = 0.30;
pub const SHUTDOWN_WAIT_SECONDS: u64 = 30;

// ═══════════════════════════════════════════════════════════════
//  PRICING
// ═══════════════════════════════════════════════════════════════

pub fn get_price(disk_type: &str) -> f64 {
    match disk_type {
        "nvme" => PRICE_NVME,
        "ssd"  => PRICE_SSD,
        _      => PRICE_HDD,
    }
}

pub fn cost_gb_month(gb: f64, disk_type: &str) -> f64 {
    gb * get_price(disk_type)
}

pub fn cost_gb_tick(gb: f64, disk_type: &str) -> f64 {
    cost_gb_month(gb, disk_type) / TICKS_PER_MONTH
}

pub fn cost_gb_tick_credit(gb: f64, disk_type: &str) -> f64 {
    cost_gb_tick(gb, disk_type) * CREDIT_MULTIPLIER
}

// ═══════════════════════════════════════════════════════════════
//  RATINGS
// ═══════════════════════════════════════════════════════════════

pub fn calc_rating_pay(availability: f64, avg_speed_mbps: f64) -> f64 {
    let speed_factor = (avg_speed_mbps / SPEED_REFERENCE_MBPS).min(1.0);
    availability * speed_factor
}

pub fn calc_rating_alloc(
    availability: f64,
    avg_speed_mbps: f64,
    has_white_ip: bool,
    is_bootstrap: bool,
    credit_storage: bool,
) -> f64 {
    let speed_factor     = avg_speed_mbps / SPEED_REFERENCE_MBPS;
    let ip_factor        = if has_white_ip { 1.05 } else { 1.0 };
    let bootstrap_factor = if is_bootstrap { 1.02 } else { 1.0 };
    let credit_factor    = if credit_storage { 1.02 } else { 1.0 };
    availability * speed_factor * ip_factor * bootstrap_factor * credit_factor
}

// ═══════════════════════════════════════════════════════════════
//  PAYMENT DISTRIBUTION
// ═══════════════════════════════════════════════════════════════

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

    let mut relay_bonus     = 0.0;
    let mut bootstrap_bonus = 0.0;

    if has_relay {
        relay_bonus = payment * RELAY_BONUS_PCT;
    }
    if has_bootstrap {
        bootstrap_bonus = payment * BOOTSTRAP_BONUS_PCT;
    }

    let total_keeper = keeper_base + relay_bonus + bootstrap_bonus + ref_keeper_amt;
    let max_keeper = payment * MAX_KEEPER_TOTAL_PCT;
    let excess = (total_keeper - max_keeper).max(0.0);

    let keeper_pool = if total_keeper > 0.0 {
        keeper_base - excess * (keeper_base / total_keeper)
    } else {
        keeper_base
    };
    let relay_bonus     = relay_bonus.max(0.0) - excess * if total_keeper > 0.0 { relay_bonus.max(0.0) / total_keeper } else { 0.0 };
    let bootstrap_bonus = bootstrap_bonus.max(0.0) - excess * if total_keeper > 0.0 { bootstrap_bonus.max(0.0) / total_keeper } else { 0.0 };
    let ref_keeper_amt  = ref_keeper_amt - excess * if total_keeper > 0.0 { ref_keeper_amt / total_keeper } else { 0.0 };

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

// ═══════════════════════════════════════════════════════════════
//  PENALTIES
// ═══════════════════════════════════════════════════════════════

pub fn assess_keeper_penalty(
    pos_failures: u32,
    avail_flips_24h: u32,
    has_graceful_shutdown: bool,
    is_spoofing: bool,
    is_sybil: bool,
    repeat_offense: bool,
) -> Option<(String, String)> {
    if is_spoofing || is_sybil {
        if repeat_offense {
            return Some(("critical".into(), "permanent_ban".into()));
        }
        return Some(("high".into(), "reset_rating_confiscate".into()));
    }
    if pos_failures == 1 {
        return Some(("low".into(), "warning_freeze_1h".into()));
    }
    if avail_flips_24h > 3 && !has_graceful_shutdown {
        return Some(("medium".into(), "penalty_10pct".into()));
    }
    None
}

pub fn assess_client_penalty(
    replica_rebalance_hour: u32,
    geo_changes_30min: u32,
    total_rebalance_6h_gb: f64,
    is_xss: bool,
) -> Option<(String, String)> {
    if is_xss {
        return Some(("high".into(), "reset_rating_block_7d".into()));
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

// ═══════════════════════════════════════════════════════════════
//  CREDIT STORAGE
// ═══════════════════════════════════════════════════════════════

pub fn credit_storage_state(
    balance: f64,
    credit_enabled: bool,
    zero_ticks: u32,
) -> (String, u32) {
    if balance > 0.0 {
        return ("none".into(), CREDIT_PERIOD_TICKS);
    }
    if !credit_enabled {
        return ("delete_data".into(), 0);
    }
    if zero_ticks >= CREDIT_PERIOD_TICKS {
        return ("delete_data".into(), 0);
    }
    ("block_uploads".into(), CREDIT_PERIOD_TICKS - zero_ticks)
}

// ═══════════════════════════════════════════════════════════════
//  MNEMONIC & WALLET
// ═══════════════════════════════════════════════════════════════

pub const MNEMONIC_WORDS: &[&str] = &[
    "абрикос","банан","вишня","гранат","дыня","ежевика","земляника",
    "инжир","клубника","лимон","малина","нектарин","облепиха","персик",
    "рябина","смородина","тыква","финик","хурма","черешня","яблоко",
    "айва","груша","слива",
];

pub fn verify_mnemonic_words(mnemonic: &str, positions: &[u32], expected: &[String]) -> bool {
    let words: Vec<&str> = mnemonic.split_whitespace().collect();
    if positions.len() != expected.len() {
        return false;
    }
    for (i, &pos) in positions.iter().enumerate() {
        let idx = pos as usize;
        if idx >= words.len() {
            return false;
        }
        if words[idx] != expected[i] {
            return false;
        }
    }
    true
}

pub fn validate_withdraw_time() -> Result<(), String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let day_of_week = (secs / 86400 + 4) % 7;
    if day_of_week >= 5 {
        return Err("\u{0412}\u{044b}\u{0432}\u{043e}\u{0434} \u{0441}\u{0440}\u{0435}\u{0434}\u{0441}\u{0442}\u{0432} \u{0434}\u{043e}\u{0441}\u{0442}\u{0443}\u{043f}\u{0435}\u{043d} \u{0442}\u{043e}\u{043b}\u{044c}\u{043a}\u{043e} \u{0432} \u{0431}\u{0443}\u{0434}\u{043d}\u{0438\u{0435} \u{0434\u{043d}\u{0438} (\u{041f}\u{043d}\u{2013}\u{041f}\u{0442})".to_string());
    }
    let seconds_since_midnight_utc = secs % 86400;
    let msk_offset = 3 * 3600;
    let seconds_since_midnight_msk = (seconds_since_midnight_utc as i64 + msk_offset) % 86400;
    let hour_msk = (seconds_since_midnight_msk / 3600) as u32;
    if hour_msk < 10 || hour_msk >= 18 {
        return Err(format!(
            "\u{0412}\u{044b}\u{0432}\u{043e}\u{0434} \u{0441}\u{0440}\u{0435}\u{0434}\u{0441}\u{0442}\u{0432} \u{0434}\u{043e}\u{0441}\u{0442}\u{0443}\u{043f}\u{0435}\u{043d} \u{0441} 10:00 \u{0434\u{043e} 18:00 \u{041c}\u{0421}\u{041a}. \u{0421}\u{0435}\u{0439}\u{0447}\u{0430}\u{0441} {}:{:02} \u{041c}\u{0421}\u{041a}",
            hour_msk,
            (seconds_since_midnight_msk % 3600) / 60
        ));
    }
    Ok(())
}

pub fn calculate_close_account_refund(balance: f64, bonus: f64) -> (f64, f64) {
    (balance.max(0.0), bonus)
}

// ═══════════════════════════════════════════════════════════════
//  BONUS MANAGEMENT
// ═══════════════════════════════════════════════════════════════

pub(crate) fn add_bonus(bonus_entries: &mut Vec<BonusEntry>, amount: f64, current_tick: u32, source: &str) {
    bonus_entries.push(BonusEntry {
        id: uuid_str(),
        amount,
        created_at_tick: current_tick,
        expiry_tick: current_tick + BONUS_EXPIRY_TICKS,
        source: source.to_string(),
    });
}

pub fn expire_bonuses(bonus_entries: &mut Vec<BonusEntry>, current_tick: u32) -> f64 {
    let expired_sum: f64 = bonus_entries.iter()
        .filter(|b| b.expiry_tick <= current_tick)
        .map(|b| b.amount)
        .sum();
    bonus_entries.retain(|b| b.expiry_tick > current_tick);
    expired_sum
}

// ═══════════════════════════════════════════════════════════════
//  RELAY
// ═══════════════════════════════════════════════════════════════

pub fn check_relay_eligibility(
    has_white_ip: bool,
    gray_clients: u32,
    fail_pct: f64,
) -> bool {
    has_white_ip && gray_clients <= RELAY_MAX_GRAY_CLIENTS && fail_pct <= RELAY_MAX_FAIL_PCT
}

// ═══════════════════════════════════════════════════════════════
//  LEGAL DOCUMENTS
// ═══════════════════════════════════════════════════════════════

pub fn get_public_offer_text() -> &'static str {
    include_str!("../legal/oferta.txt")
}

pub fn get_agency_contract_text() -> &'static str {
    include_str!("../legal/agency_contract.txt")
}

// ═══════════════════════════════════════════════════════════════
//  HELPERS
// ═══════════════════════════════════════════════════════════════

pub fn uuid_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{:x}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis())
}

pub fn now_str() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!("{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs())
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
