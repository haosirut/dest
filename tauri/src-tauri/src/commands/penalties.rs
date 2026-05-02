//! Penalty and warning commands.

use tauri::State;
use crate::state::AppState;
use crate::models::*;

#[tauri::command]
pub async fn get_penalties(state: State<'_, AppState>) -> Result<Vec<PenaltyEntry>, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_penalties start");
    let result = state.penalty_log.read().clone();
    tracing::debug!(target: "soty_cmd", "CMD get_penalties end");
    Ok(result)
}

#[tauri::command]
pub async fn get_warnings(state: State<'_, AppState>) -> Result<Vec<WarningEntry>, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_warnings start");
    let cur = *state.current_tick.read();
    let result = state.active_warnings.read().iter()
        .filter(|w| w.expires_at_tick > cur)
        .cloned()
        .collect();
    tracing::debug!(target: "soty_cmd", "CMD get_warnings end");
    Ok(result)
}
