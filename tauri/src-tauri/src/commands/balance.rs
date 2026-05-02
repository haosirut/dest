//! Balance commands: client/keeper balance, payments, topup, payout.

use tauri::State;
use crate::state::AppState;
use crate::models::*;
use crate::logic::*;

#[tauri::command]
pub async fn get_client_balance(state: State<'_, AppState>) -> Result<ClientBalanceResponse, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_client_balance start");
    let credit_on = *state.credit_storage_enabled.read();
    let zero_ticks = *state.zero_balance_ticks.read();
    let client_bal = *state.client_balance.read();
    let storage_gb = *state.storage_used_gb.read();
    let (credit_action, credit_remaining) = credit_storage_state(client_bal, credit_on, zero_ticks);

    let resp = ClientBalanceResponse {
        balance: client_bal,
        bonus: *state.client_bonus.read(),
        currency: "RUB".to_string(),
        credit_storage_enabled: credit_on,
        credit_action,
        credit_ticks_remaining: credit_remaining,
        low_balance_warning: client_bal < 10.0 && storage_gb > 0.0,
    };
    tracing::debug!(target: "soty_cmd", "CMD get_client_balance end");
    Ok(resp)
}

#[tauri::command]
pub async fn get_keeper_balance(state: State<'_, AppState>) -> Result<KeeperBalanceResponse, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_keeper_balance start");
    let pending = *state.keeper_pending.read();
    let resp = KeeperBalanceResponse {
        balance: *state.keeper_balance.read(),
        pending,
        can_withdraw: pending >= PAYOUT_MIN,
        currency: "RUB".to_string(),
        payout_note: if pending < PAYOUT_MIN {
            Some(format!("\u{041c}\u{0438}\u{043d}\u{0438}\u{043c}\u{0430}\u{043b}\u{044c}\u{043d}\u{0430}\u{044f} \u{0441}\u{0443}\u{043c}\u{043c}\u{0430} \u{0432}\u{044b}\u{0432}\u{043e}\u{0434}\u{0430}: {} \u{20bd}. \u{0415}\u{0449}\u{0451} \u{043d}\u{0443}\u{0436}\u{043d}\u{043e}: {:.2} \u{20bd}", PAYOUT_MIN, PAYOUT_MIN - pending))
        } else {
            Some("\u{0412}\u{044b}\u{0432}\u{043e}\u{0434} \u{0434}\u{043e}\u{0441}\u{0442}\u{0443}\u{043f}\u{0435}\u{043d} (\u{0431}\u{0443}\u{0434}\u{043d}\u{0438} 10:00\u{2013}18:00 \u{041c}\u{0421}\u{041a})".to_string())
        },
    };
    tracing::debug!(target: "soty_cmd", "CMD get_keeper_balance end");
    Ok(resp)
}

#[tauri::command]
pub async fn get_payment_history(state: State<'_, AppState>) -> Result<Vec<PaymentRecord>, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_payment_history start");
    let result = state.payment_history.read().clone();
    tracing::debug!(target: "soty_cmd", "CMD get_payment_history end");
    Ok(result)
}

#[tauri::command]
pub async fn topup_client(state: State<'_, AppState>, amount: f64, method: String) -> Result<TopupResponse, String> {
    tracing::debug!(target: "soty_cmd", "CMD topup_client start");
    let commission = if method == "card" { amount * 0.025 } else { 0.0 };
    let total = amount + commission;
    *state.client_balance.write() += amount;
    *state.zero_balance_ticks.write() = 0;

    state.payment_history.write().push(PaymentRecord {
        id: uuid_str(),
        kind: "deposit".to_string(),
        amount,
        wallet: "client".to_string(),
        description: format!("\u{041f}\u{043e}\u{043f}\u{043e}\u{043b}\u{043d}\u{0435}\u{043d}\u{0438}\u{0435} ({}, \u{043a}\u{043e}\u{043c}\u{0438}\u{0441}\u{0441}\u{0438}\u{044f} {:.2} \u{20bd})",
            if method == "card" { "\u{043a}\u{0430}\u{0440}\u{0442}\u{0430}" } else { "\u{0421}\u{0411}\u{041f}" }, commission),
        timestamp: now_str(),
    });

    tracing::debug!(target: "soty_cmd", "CMD topup_client end");
    Ok(TopupResponse { amount, commission, total })
}

#[tauri::command]
pub async fn request_payout(state: State<'_, AppState>) -> Result<String, String> {
    tracing::debug!(target: "soty_cmd", "CMD request_payout start");
    let balance = *state.keeper_balance.read();
    if balance < PAYOUT_MIN {
        return Err(format!("\u{041c}\u{0438}\u{043d}\u{0438}\u{043c}\u{0430}\u{043b}\u{044c}\u{043d}\u{0430}\u{044f} \u{0441}\u{0443}\u{043c}\u{043c}\u{0430} \u{0432}\u{044b}\u{0432}\u{043e}\u{0434}\u{0430}: {} \u{20bd}. \u{0422}\u{0435}\u{043a}\u{0443}\u{0449}\u{0438}\u{0439} \u{0431}\u{0430}\u{043b}\u{0430}\u{043d}\u{0441}: {:.2} \u{20bd}", PAYOUT_MIN, balance));
    }
    validate_withdraw_time()?;

    *state.keeper_balance.write() = 0.0;
    *state.keeper_pending.write() = 0.0;

    state.payment_history.write().push(PaymentRecord {
        id: uuid_str(), kind: "payout".to_string(), amount: -balance,
        wallet: "keeper".to_string(), description: "\u{0412}\u{044b}\u{0432}\u{043e}\u{0434} \u{0441}\u{0440}\u{0435}\u{0434}\u{0441}\u{0442}\u{0432}".to_string(), timestamp: now_str(),
    });
    tracing::debug!(target: "soty_cmd", "CMD request_payout end");
    Ok(format!("\u{0417}\u{0430}\u{044f}\u{0432}\u{043a}\u{0430} \u{043d}\u{0430} \u{0432}\u{044b}\u{0432}\u{043e}\u{0434} {:.2} \u{20bd} \u{0441}\u{043e}\u{0437}\u{0434}\u{0430}\u{043d}\u{0430}", balance))
}

#[tauri::command]
pub async fn close_client_account(state: State<'_, AppState>) -> Result<CloseAccountResponse, String> {
    tracing::debug!(target: "soty_cmd", "CMD close_client_account start");
    let balance = *state.client_balance.read();
    let bonus = *state.client_bonus.read();
    let (refund, forfeited) = calculate_close_account_refund(balance, bonus);
    let files_count = state.files.read().len();

    state.files.write().clear();
    *state.storage_used_gb.write() = 0.0;
    *state.client_balance.write() = 0.0;
    *state.client_bonus.write() = 0.0;
    *state.zero_balance_ticks.write() = 0;
    state.bonus_entries.write().clear();

    state.payment_history.write().push(PaymentRecord {
        id: uuid_str(), kind: "account_close".to_string(), amount: -refund,
        wallet: "client".to_string(),
        description: format!("\u{0417}\u{0430}\u{043a}\u{0440}\u{044b}\u{0442}\u{0438}\u{0435} \u{0430}\u{043a}\u{043a}\u{0430}\u{0443}\u{043d}\u{0442}\u{0430}: \u{0432}\u{043e}\u{0437}\u{0432}\u{0440}\u{0430}\u{0442} {:.2} \u{20bd}", refund),
        timestamp: now_str(),
    });

    let settings = state.settings.read().clone();
    let pid = state.peer_id.read().clone();
    std::thread::spawn(move || {
        crate::notifications::notify_data_delete(&settings, &pid, "\u{0410}\u{043a}\u{043a}\u{0430}\u{0443}\u{043d}\u{0442} \u{0437}\u{0430}\u{043a}\u{0440}\u{044b}\u{0442} \u{043f}\u{043e}\u{043b}\u{044c}\u{0437}\u{043e}\u{0432}\u{0430}\u{0442}\u{0435}\u{043b}\u{0435}\u{043c}");
    });

    tracing::debug!(target: "soty_cmd", "CMD close_client_account end");
    Ok(CloseAccountResponse {
        refund_amount: refund,
        forfeited_bonus: forfeited,
        files_deleted: files_count as u32,
    })
}
