//! File commands: list, upload, delete, download.

use tauri::State;
use crate::state::AppState;
use crate::models::*;
use crate::logic::*;

#[tauri::command]
pub async fn get_files(state: State<'_, AppState>) -> Vec<FileEntry> {
    tracing::debug!(target: "soty_cmd", "CMD get_files start");
    let result = state.files.read().clone();
    tracing::debug!(target: "soty_cmd", "CMD get_files end");
    result
}

#[tauri::command]
pub async fn upload_file(state: State<'_, AppState>, name: String, size_bytes: u64, disk_type: String) -> Result<FileEntry, String> {
    tracing::debug!(target: "soty_cmd", "CMD upload_file start");
    let credit_on = *state.credit_storage_enabled.read();
    let zero_ticks = *state.zero_balance_ticks.read();
    let bal = *state.client_balance.read();
    if bal <= 0.0 && credit_on && zero_ticks > 0 {
        return Err("\u{0417}\u{0430}\u{0433}\u{0440}\u{0443}\u{0437}\u{043a}\u{0438} \u{0437}\u{0430}\u{0431}\u{043b}\u{043e}\u{043a\u{0438}\u{0440}\u{043e}\u{0432}\u{0430}\u{043d}\u{044b}: \u{043d}\u{0443}\u{043b}\u{0435}\u{0432}\u{043e}\u{0439} \u{0431}\u{0430}\u{043b}\u{0430}\u{043d}\u{0441}.".to_string());
    }
    if bal <= 0.0 && !credit_on {
        return Err("\u{041d}\u{0435}\u{0434}\u{043e}\u{0441}\u{0442}\u{0430\u{0442}\u{043e}\u{0447}\u{043d}\u{043e} \u{0441}\u{0440}\u{0435}\u{0434}\u{0441}\u{0442}\u{0432}".to_string());
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
    state.files.write().push(entry.clone());

    let mut bonus = state.client_bonus.write();
    let mut balance = state.client_balance.write();
    let mut remaining = charge_tick;
    let from_bonus = remaining.min(*bonus);
    *bonus -= from_bonus;
    remaining -= from_bonus;
    *balance -= remaining;
    drop(bonus);
    drop(balance);
    *state.storage_used_gb.write() += gb;

    state.payment_history.write().push(PaymentRecord {
        id: uuid_str(), kind: "storage_fee".to_string(), amount: -charge_tick,
        wallet: "client".to_string(),
        description: format!("\u{0425}\u{0440}\u{0430}\u{043d}\u{0435}\u{043d}\u{0438}\u{0435}: {} ({}){}", entry.name, disk_type, if is_credit { " [\u{043a}\u{0440}\u{0435}\u{0434}\u{0438}\u{0442}]" } else { "" }),
        timestamp: now_str(),
    });

    tracing::debug!(target: "soty_cmd", "CMD upload_file end");
    Ok(entry)
}

#[tauri::command]
pub async fn delete_file(state: State<'_, AppState>, file_id: String) -> Result<(), String> {
    tracing::debug!(target: "soty_cmd", "CMD delete_file start");
    let mut files = state.files.write();
    if let Some(idx) = files.iter().position(|f| f.id == file_id) {
        let file = files.remove(idx);
        let gb = file.size_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        drop(files);
        *state.storage_used_gb.write() -= gb;
        tracing::debug!(target: "soty_cmd", "CMD delete_file end");
        Ok(())
    } else {
        Err("\u{0424}\u{0430}\u{0439}\u{043b} \u{043d}\u{0435} \u{043d}\u{0430}\u{0439}\u{0434}\u{0435}\u{043d}".to_string())
    }
}

#[tauri::command]
pub async fn download_file(_state: State<'_, AppState>, _file_id: String) -> Result<String, String> {
    tracing::debug!(target: "soty_cmd", "CMD download_file start");
    tracing::debug!(target: "soty_cmd", "CMD download_file end");
    Ok("download_started".to_string())
}
