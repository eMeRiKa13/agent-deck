use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
use tauri::{
    AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, Runtime, WebviewWindow, Window,
    WindowEvent,
};

const MAIN_WINDOW_LABEL: &str = "main";
const PLACEMENT_STATE_FILENAME: &str = ".window-placement.json";
const LEGACY_STATE_FILENAME: &str = ".window-state.json";
const MONITOR_POLL_INTERVAL: Duration = Duration::from_millis(250);
const MONITOR_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const MONITOR_SWITCH_DELAY: Duration = Duration::from_millis(100);

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
        let same_characteristics = self.width == candidate.width
            && self.height == candidate.height
            && (self.scale_factor - candidate.scale_factor).abs() <= 0.001;
        let same_name = self
            .name
            .as_ref()
            .zip(candidate.name.as_ref())
            .is_some_and(|(saved, current)| saved == current);

        same_characteristics || same_name
    }

    fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.scale_factor.is_finite()
            && self.scale_factor > 0.0
    }

    fn normalize(&mut self) {
        if self
            .name
            .as_ref()
            .is_some_and(|name| name.trim().is_empty())
        {
            self.name = None;
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct WindowPlacement {
    monitor: MonitorFingerprint,
    relative_x: i32,
    relative_y: i32,
    width: u32,
    height: u32,
    fullscreen: bool,
}

impl WindowPlacement {
    fn is_valid(&self) -> bool {
        self.monitor.is_valid() && self.width > 0 && self.height > 0
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct LegacyWindowState {
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    #[serde(default)]
    fullscreen: bool,
}

impl LegacyWindowState {
    fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }
}

#[derive(Debug, Deserialize)]
struct LegacyWindowStateFile {
    main: LegacyWindowState,
}

#[derive(Clone, Debug, PartialEq)]
enum SavedWindowState {
    Placement(WindowPlacement),
    Legacy(LegacyWindowState),
}

#[derive(Clone, Debug, PartialEq)]
struct MonitorGeometry {
    fingerprint: MonitorFingerprint,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl MonitorGeometry {
    fn from_monitor(monitor: &Monitor) -> Self {
        Self {
            fingerprint: MonitorFingerprint::from_monitor(monitor),
            x: monitor.position().x,
            y: monitor.position().y,
            width: monitor.size().width,
            height: monitor.size().height,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ResolvedPlacement {
    monitor: MonitorFingerprint,
    relative_x: i32,
    relative_y: i32,
    width: u32,
    height: u32,
    fullscreen: bool,
}

pub fn restore_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let saved_state = load_saved_window_state(app);

    tauri::async_runtime::spawn(async move {
        let Some(saved_state) = saved_state else {
            show_window(&window);
            return;
        };
        let started_at = Instant::now();

        loop {
            let monitors = available_monitor_geometry(&window);
            if let Some(placement) = resolve_saved_state(&saved_state, &monitors) {
                apply_placement(&window, placement).await;
                return;
            }

            if started_at.elapsed() >= MONITOR_WAIT_TIMEOUT {
                apply_fallback(&window, &saved_state).await;
                return;
            }

            tokio::time::sleep(MONITOR_POLL_INTERVAL).await;
        }
    });
}

pub fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() == MAIN_WINDOW_LABEL && matches!(event, WindowEvent::CloseRequested { .. }) {
        save_window(
            window.app_handle(),
            window.outer_position(),
            window.inner_size(),
            window.available_monitors(),
            window.is_fullscreen(),
        );
    }
}

pub fn save_main_window<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };

    save_window(
        app,
        window.outer_position(),
        window.inner_size(),
        window.available_monitors(),
        window.is_fullscreen(),
    );
}

async fn apply_placement<R: Runtime>(window: &WebviewWindow<R>, placement: ResolvedPlacement) {
    let _ = window.unmaximize();
    let _ = window.set_fullscreen(false);

    #[cfg(target_os = "macos")]
    let positioned = apply_macos_placement(window, placement.clone()).await;
    #[cfg(not(target_os = "macos"))]
    let positioned = false;

    if !positioned {
        let monitors = available_monitor_geometry(window);
        if let Some(monitor) = monitors
            .iter()
            .find(|monitor| placement.monitor.matches(&monitor.fingerprint))
        {
            let _ = window.set_size(PhysicalSize::new(placement.width, placement.height));
            let _ = window.set_position(PhysicalPosition::new(
                monitor.x + placement.relative_x,
                monitor.y + placement.relative_y,
            ));
        }
    }

    if placement.fullscreen {
        // macOS needs one event-loop turn to associate a hidden window with
        // its destination display before entering native fullscreen.
        tokio::time::sleep(MONITOR_SWITCH_DELAY).await;
        let _ = window.set_fullscreen(true);
    }
    show_window(window);
}

async fn apply_fallback<R: Runtime>(window: &WebviewWindow<R>, saved_state: &SavedWindowState) {
    let (saved_width, saved_height, fullscreen) = state_dimensions(saved_state);
    if let Ok(Some(monitor)) = window.primary_monitor() {
        let width = saved_width.min(monitor.size().width);
        let height = saved_height.min(monitor.size().height);
        let _ = window.set_size(PhysicalSize::new(width, height));
        let _ = window.center();
        tokio::time::sleep(MONITOR_SWITCH_DELAY).await;

        if fullscreen {
            let _ = window.set_fullscreen(true);
        }
    }
    show_window(window);
}

fn show_window<R: Runtime>(window: &WebviewWindow<R>) {
    let _ = window.show();
    let _ = window.set_focus();
}

fn state_dimensions(saved_state: &SavedWindowState) -> (u32, u32, bool) {
    match saved_state {
        SavedWindowState::Placement(placement) => {
            (placement.width, placement.height, placement.fullscreen)
        }
        SavedWindowState::Legacy(legacy) => (legacy.width, legacy.height, legacy.fullscreen),
    }
}

fn resolve_saved_state(
    saved_state: &SavedWindowState,
    monitors: &[MonitorGeometry],
) -> Option<ResolvedPlacement> {
    match saved_state {
        SavedWindowState::Placement(placement) => {
            let monitor = monitors
                .iter()
                .find(|monitor| placement.monitor.matches(&monitor.fingerprint))?;
            Some(resolve_relative_placement(placement, monitor))
        }
        SavedWindowState::Legacy(legacy) => {
            let monitor =
                select_monitor(monitors, legacy.x, legacy.y, legacy.width, legacy.height)?;
            Some(resolve_relative_placement(
                &WindowPlacement {
                    monitor: monitor.fingerprint.clone(),
                    relative_x: legacy.x - monitor.x,
                    relative_y: legacy.y - monitor.y,
                    width: legacy.width,
                    height: legacy.height,
                    fullscreen: legacy.fullscreen,
                },
                monitor,
            ))
        }
    }
}

fn resolve_relative_placement(
    placement: &WindowPlacement,
    monitor: &MonitorGeometry,
) -> ResolvedPlacement {
    let width = placement.width.min(monitor.width);
    let height = placement.height.min(monitor.height);
    let max_relative_x = monitor.width.saturating_sub(width) as i32;
    let max_relative_y = monitor.height.saturating_sub(height) as i32;

    ResolvedPlacement {
        monitor: monitor.fingerprint.clone(),
        relative_x: placement.relative_x.clamp(0, max_relative_x),
        relative_y: placement.relative_y.clamp(0, max_relative_y),
        width,
        height,
        fullscreen: placement.fullscreen,
    }
}

fn save_window<R: Runtime>(
    app: &AppHandle<R>,
    position: tauri::Result<PhysicalPosition<i32>>,
    size: tauri::Result<PhysicalSize<u32>>,
    monitors: tauri::Result<Vec<Monitor>>,
    fullscreen: tauri::Result<bool>,
) {
    #[cfg(target_os = "macos")]
    if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
        if let (Ok(pointer), Some(fullscreen)) = (window.ns_window(), fullscreen.as_ref().ok()) {
            if let Some(placement) = capture_macos_placement(pointer, *fullscreen) {
                save_window_placement(app, &placement);
                return;
            }
        }
    }

    let (Ok(position), Ok(size), Ok(monitors), Ok(fullscreen)) =
        (position, size, monitors, fullscreen)
    else {
        return;
    };
    let monitor_geometry = monitors
        .iter()
        .map(MonitorGeometry::from_monitor)
        .collect::<Vec<_>>();
    let Some(monitor) = select_monitor(
        &monitor_geometry,
        position.x,
        position.y,
        size.width,
        size.height,
    ) else {
        return;
    };
    let placement = WindowPlacement {
        monitor: monitor.fingerprint.clone(),
        relative_x: position.x - monitor.x,
        relative_y: position.y - monitor.y,
        width: size.width,
        height: size.height,
        fullscreen,
    };

    save_window_placement(app, &placement);
}

#[cfg(target_os = "macos")]
async fn apply_macos_placement<R: Runtime>(
    window: &WebviewWindow<R>,
    placement: ResolvedPlacement,
) -> bool {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let native_window = window.clone();
    let scheduled = window.run_on_main_thread(move || {
        let applied = native_window
            .ns_window()
            .ok()
            .is_some_and(|pointer| set_macos_window_frame(pointer, &placement));
        let _ = sender.send(applied);
    });

    if scheduled.is_err() {
        return false;
    }

    receiver.await.unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn set_macos_window_frame(pointer: *mut std::ffi::c_void, placement: &ResolvedPlacement) -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSScreen, NSWindow};

    let Some(main_thread) = MainThreadMarker::new() else {
        return false;
    };
    let screens = NSScreen::screens(main_thread);
    let screen = (0..screens.count())
        .map(|index| screens.objectAtIndex(index))
        .find(|screen| placement.monitor.matches(&macos_screen_fingerprint(screen)));
    let Some(screen) = screen else {
        return false;
    };
    let Some(frame) =
        macos_frame_for_placement(placement, screen.frame(), screen.backingScaleFactor())
    else {
        return false;
    };

    // SAFETY: Tauri owns this NSWindow for the lifetime of `window`; the
    // pointer is used synchronously on the AppKit main thread and is not retained.
    let native_window = unsafe { &*pointer.cast::<NSWindow>() };
    native_window.setFrame_display(frame, false);
    true
}

#[cfg(target_os = "macos")]
fn capture_macos_placement(
    pointer: *mut std::ffi::c_void,
    fullscreen: bool,
) -> Option<WindowPlacement> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSWindow;

    MainThreadMarker::new()?;
    // SAFETY: Tauri owns this NSWindow and invokes lifecycle callbacks on the
    // AppKit main thread. The borrowed object never escapes this function.
    let window = unsafe { &*pointer.cast::<NSWindow>() };
    let screen = window.screen()?;
    let screen_frame = screen.frame();
    let window_frame = window.frame();
    let scale = screen.backingScaleFactor();
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }

    let relative_x = ((window_frame.origin.x - screen_frame.origin.x) * scale).round();
    let relative_y = ((screen_frame.origin.y + screen_frame.size.height
        - window_frame.origin.y
        - window_frame.size.height)
        * scale)
        .round();

    Some(WindowPlacement {
        monitor: macos_screen_fingerprint(&screen),
        relative_x: f64_to_i32(relative_x)?,
        relative_y: f64_to_i32(relative_y)?,
        width: f64_to_u32((window_frame.size.width * scale).round())?,
        height: f64_to_u32((window_frame.size.height * scale).round())?,
        fullscreen,
    })
}

#[cfg(target_os = "macos")]
fn macos_screen_fingerprint(screen: &objc2_app_kit::NSScreen) -> MonitorFingerprint {
    let frame = screen.frame();
    let scale = screen.backingScaleFactor();

    MonitorFingerprint {
        name: Some(screen.localizedName().to_string()),
        width: f64_to_u32((frame.size.width * scale).round()).unwrap_or_default(),
        height: f64_to_u32((frame.size.height * scale).round()).unwrap_or_default(),
        scale_factor: scale,
    }
}

#[cfg(target_os = "macos")]
fn macos_frame_for_placement(
    placement: &ResolvedPlacement,
    screen_frame: objc2_foundation::NSRect,
    scale: f64,
) -> Option<objc2_foundation::NSRect> {
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    let width = placement.width as f64 / scale;
    let height = placement.height as f64 / scale;
    let relative_x = placement.relative_x as f64 / scale;
    let relative_y = placement.relative_y as f64 / scale;
    let x = screen_frame.origin.x + relative_x;
    let y = screen_frame.origin.y + screen_frame.size.height - relative_y - height;

    Some(NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)))
}

#[cfg(target_os = "macos")]
fn f64_to_i32(value: f64) -> Option<i32> {
    (value.is_finite() && value >= i32::MIN as f64 && value <= i32::MAX as f64)
        .then_some(value as i32)
}

#[cfg(target_os = "macos")]
fn f64_to_u32(value: f64) -> Option<u32> {
    (value.is_finite() && value >= 0.0 && value <= u32::MAX as f64).then_some(value as u32)
}

fn select_monitor(
    monitors: &[MonitorGeometry],
    window_x: i32,
    window_y: i32,
    window_width: u32,
    window_height: u32,
) -> Option<&MonitorGeometry> {
    monitors
        .iter()
        .map(|monitor| {
            (
                monitor,
                intersection_area(
                    (window_x, window_y, window_width, window_height),
                    (monitor.x, monitor.y, monitor.width, monitor.height),
                ),
            )
        })
        .filter(|(_, area)| *area > 0)
        .max_by_key(|(_, area)| *area)
        .map(|(monitor, _)| monitor)
}

fn intersection_area(first: (i32, i32, u32, u32), second: (i32, i32, u32, u32)) -> u64 {
    let first_right = first.0 as i64 + first.2 as i64;
    let first_bottom = first.1 as i64 + first.3 as i64;
    let second_right = second.0 as i64 + second.2 as i64;
    let second_bottom = second.1 as i64 + second.3 as i64;
    let width = (first_right.min(second_right) - (first.0 as i64).max(second.0 as i64)).max(0);
    let height = (first_bottom.min(second_bottom) - (first.1 as i64).max(second.1 as i64)).max(0);

    (width * height) as u64
}

fn available_monitor_geometry<R: Runtime>(window: &WebviewWindow<R>) -> Vec<MonitorGeometry> {
    window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(MonitorGeometry::from_monitor)
        .collect()
}

fn save_window_placement<R: Runtime>(app: &AppHandle<R>, placement: &WindowPlacement) {
    let Ok(path) = state_path(app, PLACEMENT_STATE_FILENAME) else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    let Ok(json) = serde_json::to_vec_pretty(placement) else {
        return;
    };
    let temporary_path = path.with_extension("json.tmp");

    if fs::create_dir_all(parent).is_ok()
        && fs::write(&temporary_path, json).is_ok()
        && fs::rename(&temporary_path, path).is_err()
    {
        let _ = fs::remove_file(temporary_path);
    }
}

fn load_saved_window_state<R: Runtime>(app: &AppHandle<R>) -> Option<SavedWindowState> {
    let placement_path = state_path(app, PLACEMENT_STATE_FILENAME).ok()?;
    if let Ok(bytes) = fs::read(placement_path) {
        if let Some(placement) = decode_window_placement(&bytes) {
            return Some(SavedWindowState::Placement(placement));
        }
    }

    let legacy_path = state_path(app, LEGACY_STATE_FILENAME).ok()?;
    let bytes = fs::read(legacy_path).ok()?;
    decode_legacy_window_state(&bytes).map(SavedWindowState::Legacy)
}

fn state_path<R: Runtime>(app: &AppHandle<R>, filename: &str) -> tauri::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join(filename))
}

fn decode_window_placement(bytes: &[u8]) -> Option<WindowPlacement> {
    let mut placement = serde_json::from_slice::<WindowPlacement>(bytes).ok()?;
    placement.monitor.normalize();
    placement.is_valid().then_some(placement)
}

fn decode_legacy_window_state(bytes: &[u8]) -> Option<LegacyWindowState> {
    let state = serde_json::from_slice::<LegacyWindowStateFile>(bytes)
        .ok()?
        .main;
    state.is_valid().then_some(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fingerprint(
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

    fn monitor(
        name: Option<&str>,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        scale_factor: f64,
    ) -> MonitorGeometry {
        MonitorGeometry {
            fingerprint: fingerprint(name, width, height, scale_factor),
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn display_characteristics_survive_an_unstable_macos_monitor_name() {
        let saved = fingerprint(Some("Monitor #40634"), 2560, 720, 1.0);
        let renamed = fingerprint(Some("Monitor #91234"), 2560, 720, 1.0);
        let other_display = fingerprint(Some("Monitor #40634"), 3840, 2160, 2.0);

        assert!(saved.matches(&renamed));
        assert!(saved.matches(&other_display));
        assert!(!saved.matches(&fingerprint(Some("Different display"), 3840, 2160, 2.0)));
    }

    #[test]
    fn monitor_selection_uses_the_largest_window_intersection() {
        let xeneon = monitor(Some("XENEON"), -2560, 1200, 2560, 720, 1.0);
        let ultrafine = monitor(Some("LG"), 0, 0, 3840, 2160, 2.0);
        let monitors = [xeneon.clone(), ultrafine];

        let selected = select_monitor(&monitors, -2550, 1210, 2540, 690)
            .expect("the window intersects the XENEON");

        assert_eq!(selected, &xeneon);
    }

    #[test]
    fn relative_placement_survives_a_monitor_origin_change() {
        let placement = WindowPlacement {
            monitor: fingerprint(Some("XENEON"), 2560, 720, 1.0),
            relative_x: 6,
            relative_y: 30,
            width: 2548,
            height: 690,
            fullscreen: false,
        };
        let moved_monitor = monitor(Some("XENEON"), 3840, -720, 2560, 720, 1.0);

        assert_eq!(
            resolve_relative_placement(&placement, &moved_monitor),
            ResolvedPlacement {
                monitor: fingerprint(Some("XENEON"), 2560, 720, 1.0),
                relative_x: 6,
                relative_y: 30,
                width: 2548,
                height: 690,
                fullscreen: false,
            }
        );
    }

    #[test]
    fn oversized_or_offscreen_placement_is_clamped_to_the_monitor() {
        let placement = WindowPlacement {
            monitor: fingerprint(Some("XENEON"), 2560, 720, 1.0),
            relative_x: 4000,
            relative_y: -100,
            width: 3000,
            height: 900,
            fullscreen: false,
        };
        let xeneon = monitor(Some("XENEON"), -2560, 1200, 2560, 720, 1.0);

        assert_eq!(
            resolve_relative_placement(&placement, &xeneon),
            ResolvedPlacement {
                monitor: fingerprint(Some("XENEON"), 2560, 720, 1.0),
                relative_x: 0,
                relative_y: 0,
                width: 2560,
                height: 720,
                fullscreen: false,
            }
        );
    }

    #[test]
    fn legacy_plugin_state_is_migrated_without_its_virtual_monitor_hint() {
        let legacy = decode_legacy_window_state(
            br#"{
                "main": {
                    "width": 2548,
                    "height": 690,
                    "x": -1055,
                    "y": 1448,
                    "maximized": true,
                    "fullscreen": false,
                    "visible": true
                }
            }"#,
        )
        .expect("legacy state should be readable");

        assert_eq!(
            legacy,
            LegacyWindowState {
                width: 2548,
                height: 690,
                x: -1055,
                y: 1448,
                fullscreen: false,
            }
        );
    }

    #[test]
    fn invalid_current_and_legacy_state_are_ignored() {
        assert_eq!(decode_window_placement(b"not json"), None);
        assert_eq!(decode_legacy_window_state(b"not json"), None);
        assert_eq!(
            decode_legacy_window_state(br#"{"main":{"width":0,"height":720,"x":0,"y":0}}"#),
            None
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn appkit_frame_uses_the_target_displays_scale_and_coordinate_system() {
        use objc2_foundation::{NSPoint, NSRect, NSSize};

        let placement = ResolvedPlacement {
            monitor: fingerprint(Some("XENEON EDGE"), 2560, 720, 1.0),
            relative_x: 0,
            relative_y: 30,
            width: 2560,
            height: 690,
            fullscreen: false,
        };
        let screen = NSRect::new(NSPoint::new(-1055.0, -10.0), NSSize::new(2560.0, 720.0));

        assert_eq!(
            macos_frame_for_placement(&placement, screen, 1.0),
            Some(NSRect::new(
                NSPoint::new(-1055.0, -10.0),
                NSSize::new(2560.0, 690.0),
            ))
        );
    }
}
