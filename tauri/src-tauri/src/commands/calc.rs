//! Calculator, timer, and simulate_tick commands.

use tauri::State;
use crate::state::AppState;
use crate::models::*;
use crate::logic::*;

#[tauri::command]
pub async fn calculate_storage_cost(gb: f64, disk_type: String, credit: bool) -> Result<CalcResponse, String> {
    tracing::debug!(target: "soty_cmd", "CMD calculate_storage_cost start");
    let mult = if credit { CREDIT_MULTIPLIER } else { 1.0 };
    let monthly = cost_gb_month(gb, &disk_type) * mult;
    let five_min = monthly / TICKS_PER_MONTH;
    let resp = CalcResponse {
        gb, disk_type: disk_type.clone(), replication: REPLICATION,
        price_per_gb: get_price(&disk_type),
        cost_per_month: monthly, cost_per_5min: five_min,
        credit_multiplier: mult,
        comparison: CalcComparison {
            hdd_monthly: cost_gb_month(gb, "hdd") * mult,
            ssd_monthly: cost_gb_month(gb, "ssd") * mult,
            nvme_monthly: cost_gb_month(gb, "nvme") * mult,
        },
    };
    tracing::debug!(target: "soty_cmd", "CMD calculate_storage_cost end");
    Ok(resp)
}

#[tauri::command]
pub async fn get_next_calc_time() -> Result<NextCalcResponse, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_next_calc_time start");
    let resp = NextCalcResponse { seconds_left: 300, interval_minutes: 5 };
    tracing::debug!(target: "soty_cmd", "CMD get_next_calc_time end");
    Ok(resp)
}

#[tauri::command]
pub async fn simulate_tick(state: State<'_, AppState>) -> Result<TickResponse, String> {
    tracing::debug!(target: "soty_cmd", "CMD simulate_tick start");

    // Advance tick
    let tick = {
        let cur = *state.current_tick.read();
        let new_tick = cur + 1;
        *state.current_tick.write() = new_tick;
        new_tick
    };

    let is_keeper = *state.is_keeper.read();

    // ── Update keeper ratings ──
    if is_keeper {
        {
            let mut avail = state.availability_72h.write();
            *avail = (*avail + 0.002).min(1.0);
        }
        {
            let mut speed = state.avg_speed_mbps.write();
            *speed = (*speed + 0.5).min(150.0);
        }

        let avail = *state.availability_72h.read();
        let speed = *state.avg_speed_mbps.read();
        let white_ip = *state.has_white_ip.read();
        let bootstrap = *state.is_bootstrap.read();
        let credit_keeper = *state.credit_storage_keeper.read();

        let rp = calc_rating_pay(avail, speed);
        let ra = calc_rating_alloc(avail, speed, white_ip, bootstrap, credit_keeper);
        *state.rating_pay.write() = rp;
        *state.rating_alloc.write() = ra;

        let storage_gb = *state.keeper_storage_gb.read();
        if rp >= 1.0 && storage_gb >= 1000.0 && white_ip {
            *state.is_bootstrap.write() = true;
        } else if rp < 1.0 && bootstrap {
            *state.is_bootstrap.write() = false;
        }

        // Relay auto-disable
        let relay_on = *state.relay_enabled.read();
        if relay_on {
            let gray = *state.relay_gray_clients.read();
            let fail_pct = *state.relay_fail_pct_60min.read();
            if !check_relay_eligibility(white_ip, gray, fail_pct) {
                *state.relay_enabled.write() = false;
                state.active_warnings.write().push(WarningEntry {
                    id: uuid_str(),
                    target: "keeper".into(),
                    message: "Реле автоматически отключено: превышение лимитов".into(),
                    tick_issued: tick,
                    expires_at_tick: tick + 144,
                });
            }
        }
    }

    // ── Keeper earnings ──
    if is_keeper && !*state.graceful_shutdown.read() {
        let storage_gb = *state.keeper_storage_gb.read();
        let rp = *state.rating_pay.read();
        let base_price = get_price("hdd");
        let mut earning = storage_gb * base_price * KEEPER_BASE_PCT * rp / TICKS_PER_MONTH;

        let relay_on = *state.relay_enabled.read();
        let relay_ban = *state.relay_banned_until_tick.read();
        if relay_on && relay_ban <= tick {
            earning += storage_gb * base_price * RELAY_BONUS_PCT * rp / TICKS_PER_MONTH;
        }
        if *state.is_bootstrap.read() {
            earning += storage_gb * base_price * BOOTSTRAP_BONUS_PCT * rp / TICKS_PER_MONTH;
        }

        *state.keeper_balance.write() += earning;
        *state.keeper_pending.write() += earning;
        *state.keeper_earnings_total.write() += earning;
    }

    // ── Client charges ──
    let total_charge = {
        let files = state.files.read();
        let mut total = 0.0;
        for f in files.iter() {
            let gb = f.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
            let mult = if f.is_credit { CREDIT_MULTIPLIER } else { 1.0 };
            total += cost_gb_month(gb, &f.disk_type) * mult / TICKS_PER_MONTH;
        }
        total
    };

    if total_charge > 0.0 {
        {
            let mut bonus = state.client_bonus.write();
            let mut bal = state.client_balance.write();
            let from_bonus = total_charge.min(*bonus);
            *bonus -= from_bonus;
            *bal -= total_charge - from_bonus;
        }
    }

    // ── Low-balance notification ──
    {
        let bal_notify = *state.client_balance.read();
        let storage_gb = *state.storage_used_gb.read();
        if bal_notify < 10.0 && storage_gb > 0.0 {
            let settings = state.settings.read().clone();
            let pid = state.peer_id.read().clone();
            std::thread::spawn(move || {
                crate::notifications::notify_low_balance(&settings, &pid, bal_notify);
            });
        }
    }

    // ── Zero-balance handling ──
    {
        let bal = *state.client_balance.read();
        if bal <= 0.0 {
            let credit_on = *state.credit_storage_enabled.read();
            if credit_on {
                let mut zt = state.zero_balance_ticks.write();
                *zt += 1;
                if *zt >= CREDIT_PERIOD_TICKS {
                    state.files.write().clear();
                    *state.storage_used_gb.write() = 0.0;
                    state.penalty_log.write().push(PenaltyEntry {
                        id: uuid_str(), level: "high".into(), target: "client".into(),
                        reason: "Кредитный период (72 ч) истёк".into(),
                        action: "data_deleted".into(), tick,
                    });
                    let settings = state.settings.read().clone();
                    let pid = state.peer_id.read().clone();
                    std::thread::spawn(move || {
                        crate::notifications::notify_data_delete(&settings, &pid, "Кредитный период (72 ч) истёк");
                    });
                }
            } else {
                let mut files = state.files.write();
                if !files.is_empty() {
                    files.clear();
                    drop(files);
                    *state.storage_used_gb.write() = 0.0;
                    state.penalty_log.write().push(PenaltyEntry {
                        id: uuid_str(), level: "high".into(), target: "client".into(),
                        reason: "Баланс исчерпан, кредит не подключён".into(),
                        action: "data_deleted".into(), tick,
                    });
                    let settings = state.settings.read().clone();
                    let pid = state.peer_id.read().clone();
                    std::thread::spawn(move || {
                        crate::notifications::notify_data_delete(&settings, &pid, "Баланс исчерпан");
                    });
                }
            }
        } else {
            *state.zero_balance_ticks.write() = 0;
        }
    }

    // ── Expire old bonuses ──
    {
        let mut entries = state.bonus_entries.write();
        let _expired = expire_bonuses(&mut entries, tick);
    }

    // Build response
    let bal = *state.client_balance.read();
    let bonus = *state.client_bonus.read();
    let keeper_bal = *state.keeper_balance.read();
    let rp = *state.rating_pay.read();
    let ra = *state.rating_alloc.read();
    let zt = *state.zero_balance_ticks.read();
    let credit_on = *state.credit_storage_enabled.read();
    let credit_action = credit_storage_state(bal, credit_on, zt).0;

    let resp = TickResponse {
        client_balance: bal,
        client_bonus: bonus,
        keeper_balance: keeper_bal,
        rating_pay: rp,
        rating_alloc: ra,
        current_tick: tick,
        zero_balance_ticks: zt,
        credit_action,
    };
    tracing::debug!(target: "soty_cmd", "CMD simulate_tick end");
    Ok(resp)
}
