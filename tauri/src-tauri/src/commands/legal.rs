//! Legal document commands.

use crate::logic;

#[tauri::command]
pub async fn get_legal_offer() -> Result<String, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_legal_offer start");
    let result = logic::get_public_offer_text().to_string();
    tracing::debug!(target: "soty_cmd", "CMD get_legal_offer end");
    Ok(result)
}

#[tauri::command]
pub async fn get_legal_agency() -> Result<String, String> {
    tracing::debug!(target: "soty_cmd", "CMD get_legal_agency start");
    let result = logic::get_agency_contract_text().to_string();
    tracing::debug!(target: "soty_cmd", "CMD get_legal_agency end");
    Ok(result)
}
