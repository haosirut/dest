//! Legal document commands.

use crate::logic;

#[tauri::command]
pub async fn get_legal_offer() -> String {
    tracing::debug!(target: "soty_cmd", "CMD get_legal_offer start");
    let result = logic::get_public_offer_text().to_string();
    tracing::debug!(target: "soty_cmd", "CMD get_legal_offer end");
    result
}

#[tauri::command]
pub async fn get_legal_agency() -> String {
    tracing::debug!(target: "soty_cmd", "CMD get_legal_agency start");
    let result = logic::get_agency_contract_text().to_string();
    tracing::debug!(target: "soty_cmd", "CMD get_legal_agency end");
    result
}
