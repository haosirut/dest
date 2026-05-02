//! Debug log commands.

use std::io::Write;
use std::fs::OpenOptions;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub async fn debug_log(app: AppHandle, message: String) {
    tracing::debug!(target: "soty_cmd", "CMD debug_log start");
    if let Some(app_dir) = app.path().app_log_dir().ok() {
        let _ = std::fs::create_dir_all(&app_dir);
        let log_path = app_dir.join("soty_debug.log");
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = writeln!(file, "{}", message);
        }
    }
    tracing::debug!(target: "soty_cmd", "CMD debug_log end");
}

#[tauri::command]
pub async fn get_debug_log_path(app: AppHandle) -> Option<String> {
    tracing::debug!(target: "soty_cmd", "CMD get_debug_log_path start");
    let result = app.path().app_log_dir().ok()
        .map(|d| d.join("soty_debug.log").to_string_lossy().to_string());
    tracing::debug!(target: "soty_cmd", "CMD get_debug_log_path end");
    result
}
