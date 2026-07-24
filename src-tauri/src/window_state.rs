use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, Monitor, Runtime, WebviewWindow, Window, WindowEvent};
use tauri_plugin_window_state::{StateFlags, WindowExt};

const MAIN_WINDOW_LABEL: &str = "main";
const MONITOR_STATE_FILENAME: &str = ".window-monitor.json";
const MONITOR_POLL_INTERVAL: Duration = Duration::from_millis(250);
const MONITOR_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct MonitorFingerprint {
    name: Option<String>,
    width: u32,
    height: u32,
    scale_factor: f64,
}

impl MonitorFingerprint {
    fn from_monitor(monitor: &Monitor) -> Self {
        Self {
            name: monitor
                .name()
                .filter(|name| !name.trim().is_empty())
                .cloned(),
            width: monitor.size().width,
            height: monitor.size().height,
            scale_factor: monitor.scale_factor(),
        }
    }

    fn matches(&self, candidate: &Self) -> bool {
        match (&self.name, &candidate.name) {
            (Some(saved), Some(current)) => saved == current,
            (Some(_), None) => false,
            (None, _) => {
                self.width == candidate.width
                    && self.height == candidate.height
                    && (self.scale_factor - candidate.scale_factor).abs() <= 0.001
            }
        }
    }
}

pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_window_state::Builder::default()
        .with_state_flags(state_flags())
        .skip_initial_state(MAIN_WINDOW_LABEL)
        .build()
}

pub fn restore_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let saved_monitor = load_monitor_fingerprint(app);

    tauri::async_runtime::spawn(async move {
        wait_for_saved_monitor(&window, saved_monitor.as_ref()).await;

        // Restoration failures must never leave the main window inaccessible.
        let _ = window.restore_state(state_flags());
        let _ = window.show();
        let _ = window.set_focus();
    });
}

pub fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() == MAIN_WINDOW_LABEL && matches!(event, WindowEvent::CloseRequested { .. }) {
        save_current_monitor(window);
    }
}

pub fn save_main_monitor<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };

    save_monitor_fingerprint(app, &MonitorFingerprint::from_monitor(&monitor));
}

fn state_flags() -> StateFlags {
    StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED | StateFlags::FULLSCREEN
}

async fn wait_for_saved_monitor<R: Runtime>(
    window: &WebviewWindow<R>,
    saved_monitor: Option<&MonitorFingerprint>,
) {
    let Some(saved_monitor) = saved_monitor else {
        return;
    };
    let started_at = Instant::now();

    loop {
        let available_monitors = window
            .available_monitors()
            .unwrap_or_default()
            .iter()
            .map(MonitorFingerprint::from_monitor)
            .collect::<Vec<_>>();

        if should_restore(saved_monitor, &available_monitors, started_at.elapsed()) {
            return;
        }

        tokio::time::sleep(MONITOR_POLL_INTERVAL).await;
    }
}

fn should_restore(
    saved_monitor: &MonitorFingerprint,
    available_monitors: &[MonitorFingerprint],
    elapsed: Duration,
) -> bool {
    elapsed >= MONITOR_WAIT_TIMEOUT
        || available_monitors
            .iter()
            .any(|monitor| saved_monitor.matches(monitor))
}

fn save_current_monitor<R: Runtime>(window: &Window<R>) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };

    save_monitor_fingerprint(
        window.app_handle(),
        &MonitorFingerprint::from_monitor(&monitor),
    );
}

fn save_monitor_fingerprint<R: Runtime>(app: &AppHandle<R>, fingerprint: &MonitorFingerprint) {
    let Ok(path) = monitor_state_path(app) else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    let Ok(json) = serde_json::to_vec_pretty(fingerprint) else {
        return;
    };

    if fs::create_dir_all(parent).is_ok() {
        let _ = fs::write(path, json);
    }
}

fn load_monitor_fingerprint<R: Runtime>(app: &AppHandle<R>) -> Option<MonitorFingerprint> {
    let path = monitor_state_path(app).ok()?;
    let bytes = fs::read(path).ok();
    decode_monitor_fingerprint(bytes.as_deref())
}

fn monitor_state_path<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join(MONITOR_STATE_FILENAME))
}

fn decode_monitor_fingerprint(bytes: Option<&[u8]>) -> Option<MonitorFingerprint> {
    let mut fingerprint = serde_json::from_slice::<MonitorFingerprint>(bytes?).ok()?;
    if fingerprint.width == 0
        || fingerprint.height == 0
        || !fingerprint.scale_factor.is_finite()
        || fingerprint.scale_factor <= 0.0
    {
        return None;
    }
    if fingerprint
        .name
        .as_ref()
        .is_some_and(|name| name.trim().is_empty())
    {
        fingerprint.name = None;
    }

    Some(fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(
        name: Option<&str>,
        width: u32,
        height: u32,
        scale_factor: f64,
    ) -> MonitorFingerprint {
        MonitorFingerprint {
            name: name.map(str::to_owned),
            width,
            height,
            scale_factor,
        }
    }

    #[test]
    fn monitor_name_has_priority_over_display_characteristics() {
        let saved = monitor(Some("XENEON EDGE"), 2560, 720, 1.0);
        let renamed_dimensions = monitor(Some("XENEON EDGE"), 1920, 1080, 2.0);
        let same_dimensions = monitor(Some("Other display"), 2560, 720, 1.0);

        assert!(saved.matches(&renamed_dimensions));
        assert!(!saved.matches(&same_dimensions));
    }

    #[test]
    fn dimensions_and_scale_match_a_monitor_without_a_saved_name() {
        let saved = monitor(None, 2560, 720, 1.0);

        assert!(saved.matches(&monitor(Some("XENEON EDGE"), 2560, 720, 1.0)));
        assert!(!saved.matches(&monitor(None, 2560, 720, 2.0)));
        assert!(!saved.matches(&monitor(None, 1920, 1080, 1.0)));
    }

    #[test]
    fn restoration_waits_for_the_saved_monitor_until_the_deadline() {
        let saved = monitor(Some("XENEON EDGE"), 2560, 720, 1.0);
        let internal = monitor(Some("Built-in Retina Display"), 3024, 1964, 2.0);

        assert!(!should_restore(
            &saved,
            &[internal.clone()],
            Duration::from_secs(4)
        ));
        assert!(should_restore(&saved, &[internal], MONITOR_WAIT_TIMEOUT));
        assert!(should_restore(
            &saved,
            &[monitor(Some("XENEON EDGE"), 2560, 720, 1.0)],
            Duration::ZERO
        ));
    }

    #[test]
    fn missing_or_invalid_monitor_state_is_ignored() {
        assert_eq!(decode_monitor_fingerprint(None), None);
        assert_eq!(decode_monitor_fingerprint(Some(b"not json")), None);
        assert_eq!(
            decode_monitor_fingerprint(Some(br#"{"name":"","width":2560}"#)),
            None
        );
        assert_eq!(
            decode_monitor_fingerprint(Some(
                br#"{"name":"XENEON EDGE","width":0,"height":720,"scale_factor":1.0}"#
            )),
            None
        );
    }

    #[test]
    fn an_empty_monitor_name_falls_back_to_display_characteristics() {
        let decoded = decode_monitor_fingerprint(Some(
            br#"{"name":"  ","width":2560,"height":720,"scale_factor":1.0}"#,
        ))
        .expect("the display characteristics are valid");

        assert_eq!(decoded.name, None);
        assert!(decoded.matches(&monitor(Some("XENEON EDGE"), 2560, 720, 1.0)));
    }
}
