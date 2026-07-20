#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod domain;
mod providers;
mod utils;

use domain::DashboardSnapshot;

#[tauri::command]
async fn get_dashboard_snapshot() -> DashboardSnapshot {
    providers::dashboard_snapshot().await
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_dashboard_snapshot])
        .run(tauri::generate_context!())
        .expect("failed to run Agent Deck");
}
