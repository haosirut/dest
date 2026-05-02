//! Keeper/network commands: toggle, stats, relay, shutdown.

use tauri::{State, AppHandle};
use crate::state::AppState;
use crate::models::*;
use crate::logic::*;

#[tauri::command]
pub async fn toggle_keeper_mode(state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    tracing::debug!(target: "soty_cmd", "CMD toggle_keeper_mode start");
    *state.is_keeper.write() = enabled;
    tracing::debug!(target: "soty_cmd", "CMD toggle_keeper_mode end");
    Ok(enabled)
}

#[tauri::command]
pub async fn get_keeper_stats(state: State<'_, AppState>) -> Result<KeeperStatsResponse, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_keeper_stats start");
    let rp = *state.rating_pay.read();
    let ra = *state.rating_alloc.read();
    let relay_on = *state.relay_enabled.read();
    let relay_ban = *state.relay_banned_until_tick.read();
    let cur_tick = *state.current_tick.read();
    let gray = *state.relay_gray_clients.read();
    let fail_pct = *state.relay_fail_pct_60min.read();
    let white_ip = *state.has_white_ip.read();
    let relay_eligible = relay_on && relay_ban <= cur_tick && check_relay_eligibility(white_ip, gray, fail_pct);

    let resp = KeeperStatsResponse {
        is_active: *state.is_keeper.read(),
        rating_pay: rp,
        rating_alloc: ra,
        storage_provided_gb: *state.keeper_storage_gb.read(),
        earnings_total: *state.keeper_earnings_total.read(),
        connected_peers: *state.connected_peers.read(),
        has_white_ip: white_ip,
        is_bootstrap: *state.is_bootstrap.read(),
        credit_storage_keeper: *state.credit_storage_keeper.read(),
        relay_enabled: relay_eligible,
        relay_banned: relay_ban > cur_tick,
        relay_gray_clients: gray,
        relay_fail_pct: fail_pct,
        relay_bonus_note: if relay_eligible {
            Some("\u{0410}\u{043a}\u{0442}\u{0438}\u{0432}\u{0435}\u{043d} relay +2% \u{043a} \u{0432}\u{044b}\u{043f}\u{043b}\u{0430}\u{0442}\u{0435}".to_string())
        } else {
            None
        },
        bootstrap_note: if *state.is_bootstrap.read() {
            Some("Bootstrap-\u{0443}\u{0437}\u{0435}\u{043b} +1% \u{043a} \u{0432}\u{044b}\u{043f}\u{043b}\u{0430}\u{0442}\u{0435}".to_string())
        } else {
            None
        },
        warnings: state.active_warnings.read().iter()
            .filter(|w| w.expires_at_tick > cur_tick)
            .map(|w| w.message.clone())
            .collect(),
    };
    tracing::debug!(target: "soty_cmd", "CMD get_keeper_stats end");
    Ok(resp)
}

#[tauri::command]
pub async fn get_client_stats(state: State<'_, AppState>) -> Result<ClientStatsResponse, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_client_stats start");
    let resp = ClientStatsResponse {
        storage_used_gb: *state.storage_used_gb.read(),
        files_count: state.files.read().len() as u32,
        connected_peers: *state.connected_peers.read(),
        monthly_cost: state.files.read().iter().map(|f| f.cost_per_month).sum(),
        credit_storage_enabled: *state.credit_storage_enabled.read(),
    };
    tracing::debug!(target: "soty_cmd", "CMD get_client_stats end");
    Ok(resp)
}

#[tauri::command]
pub async fn get_network_status(state: State<'_, AppState>) -> Result<NetworkStatusResponse, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_network_status start");
    let resp = NetworkStatusResponse {
        connected_peers: *state.connected_peers.read(),
        status: "connected".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        current_tick: *state.current_tick.read(),
    };
    tracing::debug!(target: "soty_cmd", "CMD get_network_status end");
    Ok(resp)
}

#[tauri::command]
pub async fn toggle_relay(state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    tracing::debug!(target: "soty_cmd", "CMD toggle_relay start");
    if enabled && !*state.has_white_ip.read() {
        return Err("Relay \u{0442}\u{0440}\u{0435}\u{0431}\u{0443}\u{0435}\u{0442} \u{0431}\u{0435}\u{043b}\u{043e}\u{0433}\u{043e} IP-\u{0430}\u{0434}\u{0440}\u{0435}\u{0441}\u{0430}".to_string());
    }
    *state.relay_enabled.write() = enabled;
    tracing::debug!(target: "soty_cmd", "CMD toggle_relay end");
    Ok(enabled)
}

#[tauri::command]
pub async fn initiate_shutdown(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    tracing::debug!(target: "soty_cmd", "CMD initiate_shutdown start");
    if !*state.is_keeper.read() {
        return Err("\u{0420}\u{0435}\u{0436}\u{0438}\u{043c} \u{0445}\u{0440}\u{0430}\u{043d}\u{0438}\u{0442}\u{0435}\u{043b}\u{044f} \u{043d}\u{0435} \u{0430}\u{043a}\u{0442}\u{0438}\u{0432}\u{0435}\u{043d}".to_string());
    }
    *state.graceful_shutdown.write() = true;
    let peer_id = state.peer_id.read().clone();
    let app_clone = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(SHUTDOWN_WAIT_SECONDS));
        let _ = app_clone.exit(0);
    });
    tracing::debug!(target: "soty_cmd", "CMD initiate_shutdown end");
    Ok(format!("Graceful shutdown: \u{0443}\u{0437}\u{0435}\u{043b} {} \u{0443}\u{0432}\u{0435}\u{0434}\u{043e}\u{043c}\u{043b}\u{0451}\u{043d}. \u{0417}\u{0430}\u{0432}\u{0435}\u{0440}\u{0448}\u{0435}\u{043d}\u{0438}\u{0435} \u{0447}\u{0435}\u{0440}\u{0435}\u{0437} {} \u{0441}\u{0435}\u{043a}.", peer_id, SHUTDOWN_WAIT_SECONDS))
}

#[tauri::command]
pub async fn send_shutdown_notification(state: State<'_, AppState>) -> Result<(), String> {
    tracing::debug!(target: "soty_cmd", "CMD send_shutdown_notification start");
    let settings = state.settings.read().clone();
    let peer_id = state.peer_id.read().clone();
    std::thread::spawn(move || {
        crate::notifications::notify_shutdown(&settings, &peer_id, SHUTDOWN_WAIT_SECONDS);
    });
    tracing::debug!(target: "soty_cmd", "CMD send_shutdown_notification end");
    Ok(())
}

#[tauri::command]
pub async fn send_low_balance_notification(state: State<'_, AppState>) -> Result<(), String> {
    tracing::debug!(target: "soty_cmd", "CMD send_low_balance_notification start");
    let balance = *state.client_balance.read();
    if balance < 10.0 && *state.storage_used_gb.read() > 0.0 {
        let settings = state.settings.read().clone();
        let peer_id = state.peer_id.read().clone();
        std::thread::spawn(move || {
            crate::notifications::notify_low_balance(&settings, &peer_id, balance);
        });
    }
    tracing::debug!(target: "soty_cmd", "CMD send_low_balance_notification end");
    Ok(())
}
