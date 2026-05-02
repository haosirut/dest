//! Settings commands: get/save settings, geo, credit toggles.

use tauri::State;
use crate::state::AppState;
use crate::models::*;

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_settings start");
    let result = state.settings.read().clone();
    tracing::debug!(target: "soty_cmd", "CMD get_settings end");
    Ok(result)
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    tracing::debug!(target: "soty_cmd", "CMD save_settings start");
    *state.credit_storage_enabled.write() = settings.credit_storage_client;
    *state.credit_storage_keeper.write() = settings.credit_storage_keeper;
    *state.settings.write() = settings;
    tracing::debug!(target: "soty_cmd", "CMD save_settings end");
    Ok(())
}

#[tauri::command]
pub async fn check_geo(state: State<'_, AppState>) -> Result<GeoResponse, String> {
    tracing::debug!(target: "soty_cmd", "CMD check_geo start");
    let verified = *state.geo_verified.read();
    let resp = GeoResponse {
        verified,
        country: if verified { "RU".to_string() } else { "\u{041d}\u{0435} \u{043e}\u{043f}\u{0440}\u{0435}\u{0434}\u{0435}\u{043b}\u{0435}\u{043d}\u{043e}".to_string() },
        ip: "185.xx.xx.xx".to_string(),
        has_white_ip: *state.has_white_ip.read(),
        installation_id: state.installation_id.read().clone(),
    };
    tracing::debug!(target: "soty_cmd", "CMD check_geo end");
    Ok(resp)
}

#[tauri::command]
pub async fn toggle_credit_storage_client(state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    tracing::debug!(target: "soty_cmd", "CMD toggle_credit_storage_client start");
    *state.credit_storage_enabled.write() = enabled;
    let mut settings = state.settings.write();
    settings.credit_storage_client = enabled;
    tracing::debug!(target: "soty_cmd", "CMD toggle_credit_storage_client end");
    Ok(enabled)
}

#[tauri::command]
pub async fn toggle_credit_storage_keeper(state: State<'_, AppState>, enabled: bool) -> Result<bool, String> {
    tracing::debug!(target: "soty_cmd", "CMD toggle_credit_storage_keeper start");
    *state.credit_storage_keeper.write() = enabled;
    let mut settings = state.settings.write();
    settings.credit_storage_keeper = enabled;
    tracing::debug!(target: "soty_cmd", "CMD toggle_credit_storage_keeper end");
    Ok(enabled)
}
