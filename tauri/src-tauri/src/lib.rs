//! Соты — Tauri v2 backend: decentralized P2P storage.
//!
//! Architecture:
//!   - state.rs     — AppState (parking_lot::RwLock)
//!   - models.rs    — DTOs, request/response types
//!   - logic.rs     — Pure business logic (unit-tested)
//!   - commands/    — Tauri invoke handlers (async)
//!   - notifications.rs — SMTP notifications

use parking_lot::RwLock;
use tauri::Manager;

mod state;
mod models;
mod logic;
pub(crate) mod notifications;
mod commands;

use state::AppState;
use models::AppSettings;
use std::io::Write;
use std::fs::OpenOptions;

// ═══════════════════════════════════════════════════════════════
//  APP ENTRY
// ═══════════════════════════════════════════════════════════════

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // ── File-based tracing ──
    let log_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let file_appender = tracing_appender::rolling::never(&log_dir, "soty-debug.log");
    let (non_blocking_writer, _log_guard) = tracing_appender::non_blocking(file_appender);
    let _ = tracing_subscriber::fmt()
        .with_writer(non_blocking_writer)
        .with_ansi(false)
        .with_target(true)
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    tracing::info!("=== Соты v{} started ===", env!("CARGO_PKG_VERSION"));
    tracing::info!("Log dir: {:?}", log_dir);
    // Note: _log_guard must stay alive for the duration of run()

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Create JS debug log file at app start
            if let Some(app_dir) = app.path().app_log_dir().ok() {
                let _ = std::fs::create_dir_all(&app_dir);
                let log_path = app_dir.join("soty_debug.log");
                if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&log_path) {
                    let _ = writeln!(f, "[START] === Соты запущен ===");
                }
                eprintln!("[SOTY] Debug log: {:?}", log_path);
            }
            Ok(())
        })
        .manage(AppState {
            initialized: RwLock::new(false),
            peer_id: RwLock::new(String::new()),
            mnemonic: RwLock::new(String::new()),
            mnemonic_shown_once: RwLock::new(false),
            client_balance: RwLock::new(0.0),
            client_bonus: RwLock::new(0.0),
            credit_storage_enabled: RwLock::new(false),
            zero_balance_ticks: RwLock::new(0),
            keeper_balance: RwLock::new(0.0),
            keeper_pending: RwLock::new(0.0),
            is_keeper: RwLock::new(false),
            credit_storage_keeper: RwLock::new(false),
            rating_pay: RwLock::new(0.0),
            rating_alloc: RwLock::new(0.0),
            availability_72h: RwLock::new(0.0),
            avg_speed_mbps: RwLock::new(0.0),
            has_white_ip: RwLock::new(false),
            is_bootstrap: RwLock::new(false),
            keeper_storage_gb: RwLock::new(0.0),
            keeper_earnings_total: RwLock::new(0.0),
            relay_enabled: RwLock::new(false),
            relay_fail_pct_60min: RwLock::new(0.0),
            _relay_incidents_24h: RwLock::new(0),
            relay_banned_until_tick: RwLock::new(0),
            relay_gray_clients: RwLock::new(0),
            connected_peers: RwLock::new(0),
            storage_used_gb: RwLock::new(0.0),
            current_tick: RwLock::new(0),
            graceful_shutdown: RwLock::new(false),
            files: RwLock::new(Vec::new()),
            payment_history: RwLock::new(Vec::new()),
            penalty_log: RwLock::new(Vec::new()),
            active_warnings: RwLock::new(Vec::new()),
            referral_code: RwLock::new(String::new()),
            referral_count: RwLock::new(0),
            referral_earnings: RwLock::new(0.0),
            has_referrer: RwLock::new(false),
            bonus_entries: RwLock::new(Vec::new()),
            geo_verified: RwLock::new(false),
            installation_id: RwLock::new(String::new()),
            settings: RwLock::new(AppSettings::default()),
        })
        .invoke_handler(tauri::generate_handler![
            commands::wallet::create_wallet,
            commands::wallet::restore_wallet,
            commands::wallet::verify_mnemonic_words_cmd,
            commands::wallet::confirm_mnemonic_shown,
            commands::wallet::get_wallet_info,
            commands::wallet::sync_files_after_restore,
            commands::balance::get_client_balance,
            commands::balance::get_keeper_balance,
            commands::balance::get_payment_history,
            commands::balance::topup_client,
            commands::balance::request_payout,
            commands::balance::close_client_account,
            commands::files::get_files,
            commands::files::upload_file,
            commands::files::delete_file,
            commands::files::download_file,
            commands::keeper::toggle_keeper_mode,
            commands::keeper::get_keeper_stats,
            commands::keeper::get_client_stats,
            commands::keeper::get_network_status,
            commands::keeper::toggle_relay,
            commands::keeper::initiate_shutdown,
            commands::keeper::send_shutdown_notification,
            commands::keeper::send_low_balance_notification,
            commands::settings::get_settings,
            commands::settings::save_settings,
            commands::settings::check_geo,
            commands::settings::toggle_credit_storage_client,
            commands::settings::toggle_credit_storage_keeper,
            commands::penalties::get_penalties,
            commands::penalties::get_warnings,
            commands::legal::get_legal_offer,
            commands::legal::get_legal_agency,
            commands::calc::calculate_storage_cost,
            commands::calc::get_next_calc_time,
            commands::calc::simulate_tick,
            commands::debug::debug_log,
            commands::debug::get_debug_log_path,
            commands::debug::discover_local_peers,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ═══════════════════════════════════════════════════════════════
//  UNIT TESTS
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::logic::*;

    #[test]
    fn test_get_price() {
        assert!((get_price("hdd") - 0.30).abs() < 1e-9);
        assert!((get_price("ssd") - 0.60).abs() < 1e-9);
        assert!((get_price("nvme") - 0.90).abs() < 1e-9);
    }

    #[test]
    fn test_cost_gb_month() {
        assert!((cost_gb_month(10.0, "hdd") - 3.0).abs() < 1e-9);
        assert!((cost_gb_month(10.0, "ssd") - 6.0).abs() < 1e-9);
    }

    #[test]
    fn test_cost_gb_tick() {
        let tick_cost = cost_gb_tick(1.0, "hdd");
        let monthly = cost_gb_month(1.0, "hdd");
        assert!((tick_cost - monthly / TICKS_PER_MONTH).abs() < 1e-12);
    }

    #[test]
    fn test_cost_gb_tick_credit() {
        let normal = cost_gb_tick(1.0, "hdd");
        let credit = cost_gb_tick_credit(1.0, "hdd");
        assert!((credit - normal * CREDIT_MULTIPLIER).abs() < 1e-12);
    }

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
    fn test_rating_pay_slow_capped() {
        let rp = calc_rating_pay(1.0, 200.0);
        assert!((rp - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_rating_pay_slow() {
        let rp = calc_rating_pay(0.9, 50.0);
        assert!((rp - 0.9 * 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_rating_alloc_base() {
        let ra = calc_rating_alloc(1.0, 100.0, false, false, false);
        assert!((ra - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_rating_alloc_white_ip() {
        let ra = calc_rating_alloc(1.0, 100.0, true, false, false);
        assert!((ra - 1.05).abs() < 1e-9);
    }

    #[test]
    fn test_rating_alloc_all_bonuses() {
        let ra = calc_rating_alloc(1.0, 100.0, true, true, true);
        let expected = 1.0 * 1.05 * 1.02 * 1.02;
        assert!((ra - expected).abs() < 1e-9);
    }

    #[test]
    fn test_distribute_payment_base() {
        let (keeper, relay, bootstrap, rk, rc, platform) = distribute_payment(100.0, false, false, false, false);
        assert!((keeper - 80.0).abs() < 1e-9);
        assert!((relay).abs() < 1e-9);
        assert!((bootstrap).abs() < 1e-9);
        assert!((rk).abs() < 1e-9);
        assert!((rc).abs() < 1e-9);
        assert!((platform - 20.0).abs() < 1e-9);
    }

    #[test]
    fn test_distribute_payment_full() {
        let (keeper, relay, bootstrap, rk, rc, platform) = distribute_payment(100.0, true, true, true, true);
        assert!((keeper + relay + bootstrap + rk + rc + platform - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_credit_storage_state_positive_balance() {
        let (action, remaining) = credit_storage_state(50.0, true, 0);
        assert_eq!(action, "none");
        assert_eq!(remaining, CREDIT_PERIOD_TICKS);
    }

    #[test]
    fn test_credit_storage_state_no_credit() {
        let (action, remaining) = credit_storage_state(0.0, false, 0);
        assert_eq!(action, "delete_data");
        assert_eq!(remaining, 0);
    }

    #[test]
    fn test_credit_storage_state_block_uploads() {
        let (action, remaining) = credit_storage_state(0.0, true, 100);
        assert_eq!(action, "block_uploads");
        assert_eq!(remaining, CREDIT_PERIOD_TICKS - 100);
    }

    #[test]
    fn test_credit_storage_state_expired() {
        let (action, remaining) = credit_storage_state(0.0, true, CREDIT_PERIOD_TICKS);
        assert_eq!(action, "delete_data");
    }

    #[test]
    fn test_verify_mnemonic_words_correct() {
        let mnemonic = "абрикос банан вишня гранат дыня ежевика земляника инжир клубника лимон малина нектарин облепиха персик рябина смородина тыква финик хурма черешня яблока айва груша слива";
        assert!(verify_mnemonic_words(mnemonic, &[0, 5, 23], &["абрикос".into(), "ежевика".into(), "слива".into()]));
    }

    #[test]
    fn test_verify_mnemonic_words_wrong() {
        let mnemonic = "абрикос банан вишня гранат дыня ежевика земляника инжир клубника лимон малина нектарин облепиха персик рябина смородина тыква финик хурма черешня яблока айва груша слива";
        assert!(!verify_mnemonic_words(mnemonic, &[0, 1], &["абрикос".into(), "вишня".into()]));
    }

    #[test]
    fn test_assess_keeper_penalty_spoofing() {
        let result = assess_keeper_penalty(0, 0, false, true, false, false);
        assert_eq!(result, Some(("high".into(), "reset_rating_confiscate".into())));
    }

    #[test]
    fn test_assess_keeper_penalty_no_issue() {
        let result = assess_keeper_penalty(0, 0, true, false, false, false);
        assert_eq!(result, None);
    }

    #[test]
    fn test_assess_client_penalty_xss() {
        let result = assess_client_penalty(0, 0, 0.0, true);
        assert_eq!(result, Some(("high".into(), "reset_rating_block_7d".into())));
    }

    #[test]
    fn test_check_relay_eligibility() {
        assert!(check_relay_eligibility(true, 3, 0.2));
        assert!(!check_relay_eligibility(false, 3, 0.2)); // no white IP
        assert!(!check_relay_eligibility(true, 10, 0.2)); // too many gray
        assert!(!check_relay_eligibility(true, 3, 0.5)); // too high fail %
    }

    #[test]
    fn test_calculate_close_account() {
        let (refund, forfeited) = calculate_close_account_refund(100.0, 50.0);
        assert!((refund - 100.0).abs() < 1e-9);
        assert!((forfeited - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_expire_bonuses() {
        let mut entries = vec![
            crate::models::BonusEntry { id: "1".into(), amount: 10.0, created_at_tick: 0, expiry_tick: 5, source: "referral".into() },
            crate::models::BonusEntry { id: "2".into(), amount: 20.0, created_at_tick: 0, expiry_tick: 15, source: "referral".into() },
        ];
        let expired = expire_bonuses(&mut entries, 10);
        assert!((expired - 10.0).abs() < 1e-9);
        assert_eq!(entries.len(), 1);
    }
}
