//! Debug log commands.

use std::io::Write;
use std::fs::OpenOptions;
use tauri::{AppHandle, Manager};
use crate::models::DiscoveredPeer;

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

/// Discover local peers via simple connectivity probe.
/// Production: use libp2p mDNS or Kademlia. For now, stub.
#[tauri::command]
pub async fn discover_local_peers() -> Vec<DiscoveredPeer> {
    tracing::debug!(target: "soty_cmd", "CMD discover_local_peers start");
    // Production: broadcast ping on 192.168.x.x:9444 or use mDNS
    // For now return empty — real discovery will use soty-p2p DHT
    let peers: Vec<DiscoveredPeer> = vec![];
    tracing::debug!(target: "soty_cmd", "CMD discover_local_peers end");
    peers
}
