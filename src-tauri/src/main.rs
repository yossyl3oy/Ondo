// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app_metrics;
mod audio;
mod debug_server;
mod error_reporting;
mod fps_monitor;
mod hardware;
mod log_buffer;
mod settings;
mod tray;
mod window_debug;
mod window_events;
mod window_monitor;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuCoreData {
    index: u32,
    temperature: f32,
    load: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuData {
    name: String,
    temperature: f32,
    #[serde(rename = "maxTemperature")]
    max_temperature: f32,
    load: f32,
    frequency: f32, // Current frequency in GHz
    cores: Vec<CpuCoreData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuData {
    name: String,
    temperature: f32,
    #[serde(rename = "maxTemperature")]
    max_temperature: f32,
    load: f32,
    frequency: f32, // Current frequency in GHz
    #[serde(rename = "memoryUsed")]
    memory_used: f32,
    #[serde(rename = "memoryTotal")]
    memory_total: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageData {
    name: String,
    temperature: f32,
    #[serde(rename = "usedSpace")]
    used_space: f32, // in GB
    #[serde(rename = "totalSpace")]
    total_space: f32, // in GB
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanData {
    name: String,
    speed: u32, // RPM
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotherboardData {
    name: String,
    temperature: f32,
    fans: Vec<FanData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterfaceData {
    name: String,
    #[serde(rename = "receivedPerSec")]
    received_per_sec: f64, // bytes/sec
    #[serde(rename = "sentPerSec")]
    sent_per_sec: f64, // bytes/sec
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayData {
    name: Option<String>, // Monitor model name
    #[serde(rename = "refreshRate")]
    refresh_rate: u32, // Hz
    fps: Option<u32>,
    #[serde(rename = "fpsProcessName")]
    fps_process_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareData {
    cpu: Option<CpuData>,
    gpu: Option<GpuData>,
    storage: Option<Vec<StorageData>>,
    motherboard: Option<MotherboardData>,
    network: Option<Vec<NetworkInterfaceData>>,
    display: Option<DisplayData>,
    timestamp: u64,
    #[serde(rename = "cpuError")]
    cpu_error: Option<String>,
    #[serde(rename = "gpuError")]
    gpu_error: Option<String>,
}

pub struct AppState {
    pub settings: Mutex<settings::AppSettings>,
    pub debug_server_running: AtomicBool,
    pub debug_server_shutdown: Mutex<Option<oneshot::Sender<()>>>,
}

impl AppState {
    /// Start or stop the debug HTTP listener. No-op when already in the
    /// requested state. Used by both the Tauri command and the tray menu.
    pub fn set_debug_server(&self, enabled: bool) -> Result<(), String> {
        let was_running = self.debug_server_running.load(Ordering::SeqCst);
        if enabled == was_running {
            return Ok(());
        }
        if enabled {
            self.debug_server_running.store(true, Ordering::SeqCst);
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let mut shutdown = self
                .debug_server_shutdown
                .lock()
                .map_err(|e| e.to_string())?;
            *shutdown = Some(shutdown_tx);
            drop(shutdown);
            tauri::async_runtime::spawn(debug_server::start_debug_server(shutdown_rx));
        } else {
            self.debug_server_running.store(false, Ordering::SeqCst);
            let shutdown = self
                .debug_server_shutdown
                .lock()
                .map_err(|e| e.to_string())?
                .take();
            if let Some(shutdown_tx) = shutdown {
                let _ = shutdown_tx.send(());
            }
        }
        Ok(())
    }
}

#[tauri::command]
async fn get_hardware_data() -> Result<HardwareData, String> {
    match hardware::get_hardware_info().await {
        Ok(data) => {
            // Report if both CPU and GPU are null (indicates a problem)
            if data.cpu.is_none() && data.gpu.is_none() {
                let error_detail = data
                    .cpu_error
                    .as_deref()
                    .or(data.gpu_error.as_deref())
                    .unwrap_or("Unknown error");
                error_reporting::capture_hardware_error(
                    &format!("Both CPU and GPU data unavailable: {}", error_detail),
                    "both",
                );
            }
            Ok(data)
        }
        Err(e) => {
            error_reporting::capture_hardware_error(&e, "get_hardware_info");
            Err(e)
        }
    }
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<settings::AppSettings, String> {
    state.settings.lock().map(|s| s.clone()).map_err(|e| {
        let err = e.to_string();
        error_reporting::capture_settings_error(&err, "get_settings");
        err
    })
}

#[tauri::command]
async fn save_settings(
    state: State<'_, AppState>,
    settings: settings::AppSettings,
) -> Result<(), String> {
    // Update state first, then drop the lock before await
    {
        let mut current = state.settings.lock().map_err(|e| {
            let err = e.to_string();
            error_reporting::capture_settings_error(&err, "save_settings_lock");
            err
        })?;
        *current = settings.clone();
    }
    settings::save_settings_to_file(&settings)
        .await
        .map_err(|e| {
            error_reporting::capture_settings_error(&e, "save_settings_file");
            e
        })
}

#[tauri::command]
async fn set_always_on_top(app: AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.set_always_on_top(enabled).map_err(|e| {
            let err = e.to_string();
            error_reporting::capture_window_error(&err, "set_always_on_top");
            err
        })?;
    }
    Ok(())
}

/// Sets WebView2's memory usage target. "Low" drops caches and trims the
/// renderer's working set (possibly paging it out), which suits the HUD
/// while nobody touches it: a monitor eating hundreds of MB defeats its own
/// purpose. It can make interaction sluggish though, so the frontend asks
/// for "normal" while the pointer is over the HUD or a panel is open.
#[cfg(target_os = "windows")]
fn set_webview_memory_target(window: &tauri::WebviewWindow, low: bool) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    };
    use windows_core::Interface;

    let level = if low {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
    } else {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
    };
    let result = window.with_webview(move |webview| unsafe {
        // ICoreWebView2_19 needs WebView2 Runtime 114+; older ones just skip
        let target = webview
            .controller()
            .CoreWebView2()
            .and_then(|core| core.cast::<ICoreWebView2_19>())
            .and_then(|core| core.SetMemoryUsageTargetLevel(level));
        if let Err(e) = target {
            crate::log_warn!("WebView", "Could not set WebView2 memory target: {}", e);
        }
    });
    if let Err(e) = result {
        crate::log_warn!("WebView", "with_webview failed: {}", e);
    }
}

#[tauri::command]
fn set_webview_memory_low(app: AppHandle, low: bool) {
    #[cfg(target_os = "windows")]
    if let Some(window) = app.get_webview_window("main") {
        set_webview_memory_target(&window, low);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (app, low);
}

/// Maps the `backgroundEffect` setting to a native backdrop. Unknown values
/// (and "none") clear the effect rather than erroring, so a stale setting
/// from a newer build degrades to the plain translucent panel.
fn window_effects_for(effect: &str) -> Option<tauri::utils::config::WindowEffectsConfig> {
    use tauri::window::{Effect, EffectsBuilder};
    let effect = match effect {
        "acrylic" => Effect::Acrylic,
        "mica" => Effect::Mica,
        _ => return None,
    };
    Some(EffectsBuilder::new().effect(effect).build())
}

#[tauri::command]
async fn set_background_effect(app: AppHandle, effect: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        // Tauri's Windows backend applies a new effect without clearing the
        // previous one (Acrylic and Mica use separate DWM attributes), so
        // always reset first or switching Acrylic → Mica can leave both on.
        let effects = window_effects_for(&effect);
        window
            .set_effects(None)
            .and_then(|()| match effects {
                Some(effects) => window.set_effects(effects),
                None => Ok(()),
            })
            .map_err(|e| {
                let err = e.to_string();
                error_reporting::capture_window_error(&err, "set_background_effect");
                err
            })?;
    }
    Ok(())
}

/// Runs a window operation on the UI thread with its geometry changes
/// attributed to `reason` in the window event log, and waits for it. On
/// that thread Tauri applies window ops synchronously, which both makes the
/// attribution exact and keeps consecutive commands in order.
async fn on_main_thread<R: Send + 'static>(
    app: &AppHandle,
    reason: String,
    f: impl FnOnce() -> Result<R, String> + Send + 'static,
) -> Result<R, String> {
    let (tx, rx) = oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(window_events::with_app_reason(&reason, f));
    })
    .map_err(|e| e.to_string())?;
    rx.await
        .map_err(|_| "window operation was dropped".to_string())?
}

#[tauri::command]
async fn set_window_position(
    app: AppHandle,
    position: String,
    reason: Option<String>,
) -> Result<(), String> {
    let handle = app.clone();
    let reason = reason.unwrap_or_else(|| "position".into());
    on_main_thread(&app, reason, move || apply_window_position(&handle, &position)).await
}

fn apply_window_position(app: &AppHandle, position: &str) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let monitor = window
            .current_monitor()
            .map_err(|e| {
                let err = e.to_string();
                error_reporting::capture_window_error(&err, "set_window_position_monitor");
                err
            })?
            .ok_or_else(|| {
                error_reporting::capture_window_error("No monitor found", "set_window_position");
                "No monitor found".to_string()
            })?;

        let monitor_size = monitor.size();
        let window_size = window.outer_size().map_err(|e| {
            let err = e.to_string();
            error_reporting::capture_window_error(&err, "set_window_position_size");
            err
        })?;
        let position =
            anchored_window_position(*monitor.position(), *monitor_size, window_size, &position);

        window.set_position(position).map_err(|e| {
            let err = e.to_string();
            error_reporting::capture_window_error(&err, "set_window_position_set");
            err
        })?;
    }
    Ok(())
}

#[tauri::command]
async fn set_auto_start(enabled: bool) -> Result<(), String> {
    settings::set_auto_start(enabled).await.map_err(|e| {
        error_reporting::capture_settings_error(&e, "set_auto_start");
        e
    })
}

// PawnIO driver check and installation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PawnIOStatus {
    installed: bool,
    checking: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    driver_file_exists: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[cfg(target_os = "windows")]
pub fn get_pawnio_detailed_status() -> PawnIOStatus {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Get driver binary path from service config (sc qc)
    let driver_file_exists = {
        let qc_output = Command::new("sc")
            .args(["qc", "PawnIO"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        if let Ok(qc) = qc_output {
            let qc_stdout = String::from_utf8_lossy(&qc.stdout);
            // Extract BINARY_PATH_NAME from sc qc output and check if file exists
            if let Some(line) = qc_stdout.lines().find(|l| l.contains("BINARY_PATH_NAME")) {
                let path_str = line
                    .split(':')
                    .skip(1)
                    .collect::<Vec<_>>()
                    .join(":")
                    .trim()
                    .to_string();
                // Remove leading \??\ prefix if present
                let clean_path = path_str.trim_start_matches(r"\??\");
                let exists = std::path::Path::new(clean_path).exists();
                crate::log_debug!(
                    "PawnIO",
                    "Driver binary path: {} (exists={})",
                    clean_path,
                    exists
                );
                exists
            } else {
                false
            }
        } else {
            false
        }
    };

    // Check if PawnIO service/driver is registered and its state
    let output = Command::new("sc")
        .args(["query", "PawnIO"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            let service_state = if stdout.contains("RUNNING") {
                "RUNNING".to_string()
            } else if stdout.contains("STOPPED") {
                "STOPPED".to_string()
            } else if stdout.contains("START_PENDING") {
                "START_PENDING".to_string()
            } else if stdout.contains("STOP_PENDING") {
                "STOP_PENDING".to_string()
            } else if !stderr.is_empty() || stdout.contains("1060") {
                "NOT_FOUND".to_string()
            } else {
                format!("UNKNOWN: {}", stdout.trim())
            };

            let installed = service_state == "RUNNING";

            crate::log_info!(
                "PawnIO",
                "Driver status: service={}, driver_file={}",
                service_state,
                driver_file_exists
            );
            if !installed {
                crate::log_warn!(
                    "PawnIO",
                    "Driver is not running. CPU temperature may not be available. sc output: {}",
                    stdout.trim()
                );
            }

            PawnIOStatus {
                installed,
                checking: false,
                service_state: Some(service_state),
                driver_file_exists: Some(driver_file_exists),
                error: None,
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to query PawnIO service: {}", e);
            crate::log_error!("PawnIO", "{}", error_msg);

            PawnIOStatus {
                installed: false,
                checking: false,
                service_state: None,
                driver_file_exists: Some(driver_file_exists),
                error: Some(error_msg),
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_pawnio_detailed_status() -> PawnIOStatus {
    PawnIOStatus {
        installed: true,
        checking: false,
        service_state: Some("N/A (non-Windows)".to_string()),
        driver_file_exists: None,
        error: None,
    }
}

#[tauri::command]
async fn check_pawnio_status() -> Result<PawnIOStatus, String> {
    Ok(get_pawnio_detailed_status())
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn download_and_install_pawnio() -> Result<String, String> {
    // Get the bundled PawnIO installer path
    let exe_path = std::env::current_exe().map_err(|e| format!("Failed to get exe path: {}", e))?;
    let exe_dir = exe_path.parent().ok_or("Failed to get exe directory")?;

    // In development, the resources are in src-tauri/resources
    // In production, they're next to the exe
    let installer_path = exe_dir.join("resources").join("PawnIO_setup.exe");
    let installer_path = if installer_path.exists() {
        installer_path
    } else {
        // Fallback to direct path (production build)
        exe_dir.join("PawnIO_setup.exe")
    };

    if !installer_path.exists() {
        return Err(format!(
            "PawnIO installer not found at {:?}",
            installer_path
        ));
    }

    // Run the installer with UAC elevation (ShellExecuteW with "runas")
    // Use -install -silent for PawnIO silent installation
    tokio::task::spawn_blocking(move || {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let path_wide: Vec<u16> = OsStr::new(installer_path.to_str().unwrap_or_default())
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let verb_wide: Vec<u16> = OsStr::new("runas")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // Silent install parameter (PawnIO uses -install -silent, not NSIS /S)
        let params_wide: Vec<u16> = OsStr::new("-install -silent")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            use windows::core::PCWSTR;
            use windows::Win32::UI::Shell::ShellExecuteW;
            use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

            let result = ShellExecuteW(
                None,
                PCWSTR(verb_wide.as_ptr()),
                PCWSTR(path_wide.as_ptr()),
                PCWSTR(params_wide.as_ptr()),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );

            // ShellExecuteW returns a value > 32 on success
            if result.0 as isize <= 32 {
                return Err(format!(
                    "Failed to launch installer (error code: {})",
                    result.0 as isize
                ));
            }
        }

        Ok(
            "PawnIO driver is being installed. Please restart Ondo after installation completes."
                .to_string(),
        )
    })
    .await
    .map_err(|e| format!("Install task failed: {}", e))?
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn download_and_install_pawnio() -> Result<String, String> {
    Ok("PawnIO is only required on Windows.".to_string())
}

#[tauri::command]
async fn set_always_on_back(app: AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        if enabled {
            // Disable always on top first
            window.set_always_on_top(false).map_err(|e| {
                let err = e.to_string();
                error_reporting::capture_window_error(&err, "set_always_on_back_top");
                err
            })?;
            // Set always on bottom
            window.set_always_on_bottom(true).map_err(|e| {
                let err = e.to_string();
                error_reporting::capture_window_error(&err, "set_always_on_back_bottom");
                err
            })?;
        } else {
            window.set_always_on_bottom(false).map_err(|e| {
                let err = e.to_string();
                error_reporting::capture_window_error(&err, "set_always_on_back_disable");
                err
            })?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowStateData {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[tauri::command]
async fn get_window_state(app: AppHandle) -> Result<WindowStateData, String> {
    if let Some(window) = app.get_webview_window("main") {
        let position = window.outer_position().map_err(|e| {
            let err = e.to_string();
            error_reporting::capture_window_error(&err, "get_window_state_position");
            err
        })?;
        let size = window.inner_size().map_err(|e| {
            let err = e.to_string();
            error_reporting::capture_window_error(&err, "get_window_state_size");
            err
        })?;
        Ok(WindowStateData {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        })
    } else {
        let err = "Window not found".to_string();
        error_reporting::capture_window_error(&err, "get_window_state");
        Err(err)
    }
}

#[tauri::command]
async fn restore_window_state(
    app: AppHandle,
    state: WindowStateData,
    reason: Option<String>,
) -> Result<(), String> {
    let handle = app.clone();
    let reason = reason.unwrap_or_else(|| "restore".into());
    on_main_thread(&app, reason, move || apply_window_state(&handle, &state)).await
}

fn apply_window_state(app: &AppHandle, state: &WindowStateData) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_position(tauri::PhysicalPosition::new(state.x, state.y))
            .map_err(|e| {
                let err = e.to_string();
                error_reporting::capture_window_error(&err, "restore_window_state_position");
                err
            })?;
        window
            .set_size(tauri::PhysicalSize::new(state.width, state.height))
            .map_err(|e| {
                let err = e.to_string();
                error_reporting::capture_window_error(&err, "restore_window_state_size");
                err
            })?;
    }
    Ok(())
}

#[tauri::command]
async fn get_audio_devices() -> Result<Vec<audio::AudioDevice>, String> {
    audio::get_audio_devices()
}

#[tauri::command]
async fn set_default_audio_device(device_id: String, device_type: String) -> Result<(), String> {
    audio::set_default_audio_device(&device_id, &device_type)
}

#[tauri::command]
async fn toggle_debug_server(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.set_debug_server(enabled)
}

#[derive(Clone, Copy)]
enum AxisAnchor {
    Start,
    Center,
    End,
}

fn anchored_axis(origin: i32, monitor_len: u32, window_len: u32, anchor: AxisAnchor) -> i32 {
    let origin = i64::from(origin);
    let available = (i64::from(monitor_len) - i64::from(window_len)).max(0);
    let coordinate = match anchor {
        AxisAnchor::Start => origin,
        AxisAnchor::Center => origin + available / 2,
        AxisAnchor::End => origin + available,
    };

    coordinate.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

/// (horizontal, vertical) anchors for a `position` setting value.
fn position_anchors(position: &str) -> (AxisAnchor, AxisAnchor) {
    match position {
        "left" => (AxisAnchor::Start, AxisAnchor::Center),
        "right" => (AxisAnchor::End, AxisAnchor::Center),
        "top-left" => (AxisAnchor::Start, AxisAnchor::Start),
        "top-right" => (AxisAnchor::End, AxisAnchor::Start),
        "bottom-left" => (AxisAnchor::Start, AxisAnchor::End),
        "bottom-right" => (AxisAnchor::End, AxisAnchor::End),
        _ => (AxisAnchor::End, AxisAnchor::Center),
    }
}

/// New (y, height) when the window is resized to `wanted_h`. The edge the
/// position setting anchors to stays put (top / center / bottom), and the
/// result is kept inside the monitor work area, capping the height there
/// (the HUD content scrolls beyond that).
fn fitted_vertical_span(
    cur_y: i32,
    cur_h: u32,
    wanted_h: u32,
    anchor: AxisAnchor,
    area_top: i32,
    area_h: u32,
) -> (i32, u32) {
    let h = wanted_h.min(area_h).max(1);
    let (cur_y, cur_h, h64) = (i64::from(cur_y), i64::from(cur_h), i64::from(h));
    let y = match anchor {
        AxisAnchor::Start => cur_y,
        AxisAnchor::Center => cur_y + (cur_h - h64) / 2,
        AxisAnchor::End => cur_y + cur_h - h64,
    };
    let top = i64::from(area_top);
    let max_y = (top + i64::from(area_h) - h64).max(top);
    (y.clamp(top, max_y) as i32, h)
}

/// Minimum window width in CSS px (matches `minWidth` in tauri.conf.json).
const MIN_WINDOW_WIDTH: f64 = 180.0;

/// Sizes the window to the HUD content (`height` in CSS px) and locks the
/// height there so only the width stays user-resizable — the panel is
/// transparent, so any extra height would be an invisible, click-blocking
/// strip. `None` drops the lock (mini mode sizes the window itself).
#[tauri::command]
async fn fit_window_height(
    app: AppHandle,
    state: State<'_, AppState>,
    height: Option<f64>,
) -> Result<(), String> {
    let position = state
        .settings
        .lock()
        .map(|s| s.position.clone())
        .unwrap_or_default();
    let handle = app.clone();
    let reason = if height.is_some() { "fit" } else { "fit-unlock" };
    on_main_thread(&app, reason.into(), move || apply_fit(&handle, height, &position)).await
}

fn apply_fit(app: &AppHandle, height: Option<f64>, position: &str) -> Result<(), String> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    let fail = |e: tauri::Error| {
        let err = e.to_string();
        error_reporting::capture_window_error(&err, "fit_window_height");
        err
    };

    // Drop the previous lock first so the new height isn't clamped by it
    window.set_min_size(None::<tauri::Size>).map_err(fail)?;
    window.set_max_size(None::<tauri::Size>).map_err(fail)?;
    let Some(css_height) = height else {
        return Ok(());
    };

    let scale = window.scale_factor().map_err(fail)?;
    let pos = window.outer_position().map_err(fail)?;
    let size = window.outer_size().map_err(fail)?;
    let Some(monitor) = window.current_monitor().map_err(fail)? else {
        return Ok(());
    };
    let area = monitor.work_area();
    let (_, vertical) = position_anchors(position);

    // Round up: at fractional scales rounding to nearest can land a hair
    // short of the content and flash a scrollbar on every value update
    let wanted = (css_height * scale).ceil().max(1.0) as u32;
    let (y, h) = fitted_vertical_span(
        pos.y,
        size.height,
        wanted,
        vertical,
        area.position.y,
        area.size.height,
    );

    window
        .set_size(tauri::PhysicalSize::new(size.width, h))
        .map_err(fail)?;
    if y != pos.y {
        window
            .set_position(tauri::PhysicalPosition::new(pos.x, y))
            .map_err(fail)?;
    }
    let min_w = (MIN_WINDOW_WIDTH * scale).round() as u32;
    window
        .set_min_size(Some(tauri::PhysicalSize::new(min_w, h)))
        .map_err(fail)?;
    window
        .set_max_size(Some(tauri::PhysicalSize::new(area.size.width.max(min_w), h)))
        .map_err(fail)?;
    Ok(())
}

fn anchored_window_position(
    monitor_position: tauri::PhysicalPosition<i32>,
    monitor_size: tauri::PhysicalSize<u32>,
    window_size: tauri::PhysicalSize<u32>,
    position: &str,
) -> tauri::PhysicalPosition<i32> {
    let (horizontal, vertical) = position_anchors(position);

    tauri::PhysicalPosition::new(
        anchored_axis(
            monitor_position.x,
            monitor_size.width,
            window_size.width,
            horizontal,
        ),
        anchored_axis(
            monitor_position.y,
            monitor_size.height,
            window_size.height,
            vertical,
        ),
    )
}

fn main() {
    // Capture all `log` crate output (including Tauri internals) into the debug server buffer
    log_buffer::init_logger();

    // Initialize Sentry for error reporting
    error_reporting::init_sentry();

    let initial_settings =
        settings::load_settings_from_file().unwrap_or_else(|_| settings::AppSettings::default());

    // Clone values we need for setup before moving into AppState
    let startup_position = initial_settings.position.clone();
    let startup_always_on_top = initial_settings.always_on_top;
    let startup_always_on_back = initial_settings.always_on_back;
    let startup_debug_server = initial_settings.debug_server;
    let startup_window_state = initial_settings.window_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState {
            settings: Mutex::new(initial_settings),
            debug_server_running: AtomicBool::new(false),
            debug_server_shutdown: Mutex::new(None),
        })
        .setup(move |app| {
            // Start debug HTTP server first so tray::setup_tray can read the
            // running state into the "Debug server" check item.
            if startup_debug_server {
                let _ = app.state::<AppState>().set_debug_server(true);
            }

            // Setup system tray
            tray::setup_tray(app)?;

            // Start window monitor for mini mode (detects maximized foreground windows)
            window_monitor::start_monitoring(app.handle().clone());

            // Start FPS monitoring (ETW-based, requires admin on Windows)
            fps_monitor::start();

            // Position window on startup
            if let Some(window) = app.get_webview_window("main") {
                // Restore saved window state if available
                // setup runs on the UI thread, so this attributes directly
                window_events::with_app_reason("startup", || {
                    if let Some(ref state) = startup_window_state {
                        let _ = window.set_position(tauri::PhysicalPosition::new(state.x, state.y));
                        let _ = window.set_size(tauri::PhysicalSize::new(state.width, state.height));
                    } else {
                        // Set initial position based on setting
                        let _ = set_initial_position(&window, &startup_position);
                    }
                });

                // Set always on top/back
                if startup_always_on_back {
                    let _ = window.set_always_on_bottom(true);
                } else if startup_always_on_top {
                    let _ = window.set_always_on_top(true);
                }

                // Register HWND for the window-debug endpoints (Windows only)
                #[cfg(target_os = "windows")]
                {
                    if let Ok(hwnd) = window.hwnd() {
                        window_debug::set_main_hwnd_raw(hwnd.0 as isize);
                        // Record every geometry change and what caused it
                        window_events::install(hwnd.0 as isize);
                    }
                    set_webview_memory_target(&window, true);
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_hardware_data,
            get_settings,
            save_settings,
            set_always_on_top,
            set_always_on_back,
            set_background_effect,
            fit_window_height,
            set_webview_memory_low,
            set_window_position,
            set_auto_start,
            get_window_state,
            restore_window_state,
            window_monitor::set_mini_interactive_zones,
            window_monitor::wait_cursor_leaves_edge,
            check_pawnio_status,
            download_and_install_pawnio,
            get_audio_devices,
            set_default_audio_device,
            toggle_debug_server,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if window.label() == "main" {
                    // Shutdown LHM daemon when main window is destroyed
                    hardware::shutdown_lhm_daemon();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    // Shutdown LHM daemon and FPS monitor on app exit
    fps_monitor::stop();
    hardware::shutdown_lhm_daemon();
}

fn set_initial_position(
    window: &tauri::WebviewWindow,
    position: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let monitor = window.current_monitor()?.ok_or("No monitor")?;
    let window_size = window.outer_size()?;
    let position =
        anchored_window_position(*monitor.position(), *monitor.size(), window_size, position);
    window.set_position(position)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::window::Effect;

    #[test]
    fn background_effect_maps_known_values() {
        assert_eq!(window_effects_for("acrylic").unwrap().effects, vec![Effect::Acrylic]);
        assert_eq!(window_effects_for("mica").unwrap().effects, vec![Effect::Mica]);
    }

    const AREA_TOP: i32 = 0;
    const AREA_H: u32 = 1040; // 1080p minus taskbar

    #[test]
    fn fit_keeps_top_edge_for_top_anchors() {
        assert_eq!(fitted_vertical_span(100, 800, 500, AxisAnchor::Start, AREA_TOP, AREA_H), (100, 500));
    }

    #[test]
    fn fit_keeps_bottom_edge_for_bottom_anchors() {
        // bottom stays at 900
        assert_eq!(fitted_vertical_span(100, 800, 500, AxisAnchor::End, AREA_TOP, AREA_H), (400, 500));
        assert_eq!(fitted_vertical_span(400, 500, 700, AxisAnchor::End, AREA_TOP, AREA_H), (200, 700));
    }

    #[test]
    fn fit_keeps_center_for_centered_anchors() {
        // center stays at 520
        assert_eq!(fitted_vertical_span(120, 800, 400, AxisAnchor::Center, AREA_TOP, AREA_H), (320, 400));
    }

    #[test]
    fn fit_caps_height_to_work_area() {
        assert_eq!(fitted_vertical_span(0, 500, 5000, AxisAnchor::Start, AREA_TOP, AREA_H), (0, AREA_H));
    }

    #[test]
    fn fit_stays_inside_work_area() {
        // Growing a top-anchored window near the bottom pushes it up
        assert_eq!(fitted_vertical_span(900, 100, 400, AxisAnchor::Start, AREA_TOP, AREA_H), (640, 400));
        // Growing a bottom-anchored window near the top pushes it down
        assert_eq!(fitted_vertical_span(10, 100, 400, AxisAnchor::End, AREA_TOP, AREA_H), (0, 400));
        // Secondary monitor above the primary (negative origin)
        assert_eq!(fitted_vertical_span(-1000, 300, 600, AxisAnchor::Start, -1080, AREA_H), (-1000, 600));
    }

    #[test]
    fn background_effect_none_or_unknown_clears() {
        assert!(window_effects_for("none").is_none());
        assert!(window_effects_for("frosted-glass").is_none());
    }
}
