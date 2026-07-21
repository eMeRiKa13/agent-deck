#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod domain;
mod providers;
mod utils;

use domain::DashboardSnapshot;
use tauri_plugin_opener::OpenerExt;

const ENCHANTER_EN_URL: &str = "https://enchanter.gg/en/";
const ENCHANTER_FR_URL: &str = "https://enchanter.gg/fr/";
const GITHUB_URL: &str = "https://github.com/eMeRiKa13/agent-deck";
const CORSAIR_EN_URL: &str = "https://www.corsair.com/us/en/p/monitors/cc-9011311-ww/corsair-xeneon-edge-14-5-lcd-touchscreen-white-cc-9011311-ww";
const CORSAIR_FR_URL: &str = "https://www.corsair.com/fr/fr/p/monitors/cc-9011311-ww/corsair-xeneon-edge-14-5-lcd-touchscreen-white-cc-9011311-ww";

#[tauri::command]
async fn get_dashboard_snapshot(include_quotas: Option<bool>) -> DashboardSnapshot {
    providers::dashboard_snapshot(include_quotas.unwrap_or(true)).await
}

fn external_url(link_id: &str, locale: &str) -> Option<&'static str> {
    match (link_id, locale) {
        ("enchanter", "en") => Some(ENCHANTER_EN_URL),
        ("enchanter", "fr") => Some(ENCHANTER_FR_URL),
        ("github", "en" | "fr") => Some(GITHUB_URL),
        ("corsair", "en") => Some(CORSAIR_EN_URL),
        ("corsair", "fr") => Some(CORSAIR_FR_URL),
        _ => None,
    }
}

#[tauri::command]
fn open_external_link(
    app: tauri::AppHandle,
    link_id: String,
    locale: String,
) -> Result<(), String> {
    let url = external_url(&link_id, &locale)
        .ok_or_else(|| format!("external link is not allowed: {link_id} ({locale})"))?;

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_dashboard_snapshot,
            open_external_link
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Agent Deck");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_links_are_exactly_allow_listed() {
        assert_eq!(external_url("enchanter", "en"), Some(ENCHANTER_EN_URL));
        assert_eq!(external_url("enchanter", "fr"), Some(ENCHANTER_FR_URL));
        assert_eq!(external_url("github", "en"), Some(GITHUB_URL));
        assert_eq!(external_url("github", "fr"), Some(GITHUB_URL));
        assert_eq!(external_url("corsair", "en"), Some(CORSAIR_EN_URL));
        assert_eq!(external_url("corsair", "fr"), Some(CORSAIR_FR_URL));
    }

    #[test]
    fn arbitrary_external_links_are_rejected() {
        assert_eq!(external_url("unknown", "en"), None);
        assert_eq!(external_url("https://example.com", "fr"), None);
        assert_eq!(external_url("corsair", "de"), None);
    }
}
