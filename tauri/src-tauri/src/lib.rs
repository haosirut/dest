//! Tauri v2 backend commands for VaultKeeper desktop app.

use tauri::State;
use std::sync::Mutex;
use serde::Serialize;

struct AppState {
    balance: Mutex<f64>,
    connected_peers: Mutex<u32>,
}

#[tauri::command]
fn get_status(state: State<AppState>) -> StatusResponse {
    StatusResponse {
        status: "running".to_string(),
        balance: *state.balance.lock().unwrap(),
        connected_peers: *state.connected_peers.lock().unwrap(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[tauri::command]
fn get_balance(state: State<AppState>) -> BalanceResponse {
    BalanceResponse {
        balance: *state.balance.lock().unwrap(),
        currency: "RUB".to_string(),
    }
}

#[tauri::command]
fn update_balance(state: State<AppState>, amount: f64) -> Result<(), String> {
    let mut balance = state.balance.lock().map_err(|e| e.to_string())?;
    *balance += amount;
    Ok(())
}

#[derive(Serialize)]
struct StatusResponse {
    status: String,
    balance: f64,
    connected_peers: u32,
    version: String,
}

#[derive(Serialize)]
struct BalanceResponse {
    balance: f64,
    currency: String,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            balance: Mutex::new(0.0),
            connected_peers: Mutex::new(0),
        })
        .invoke_handler(tauri::generate_handler![get_status, get_balance, update_balance])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
