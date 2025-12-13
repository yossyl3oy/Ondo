// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod error_reporting;
mod hardware;
mod settings;
mod tray;

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

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
pub struct HardwareData {
    cpu: Option<CpuData>,
    gpu: Option<GpuData>,
    storage: Option<Vec<StorageData>>,
    motherboard: Option<MotherboardData>,
    timestamp: u64,
    #[serde(rename = "cpuError")]
    cpu_error: Option<String>,
    #[serde(rename = "gpuError")]
    gpu_error: Option<String>,
}

pub struct AppState {
    settings: Mutex<settings::AppSettings>,
}

#[tauri::command]
async fn get_hardware_data() -> Result<HardwareData, String> {
    match hardware::get_hardware_info().await {
        Ok(data) => {
            // Report if both CPU and GPU are null (indicates a problem)
            if data.cpu.is_none() && data.gpu.is_none() {
                let error_detail = data.cpu_error.as_deref()
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
    state.settings.lock()
        .map(|s| s.clone())
        .map_err(|e| {
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
    settings::save_settings_to_file(&settings).await.map_err(|e| {
        error_reporting::capture_settings_error(&e, "save_settings_file");
        e
    })
}

#[tauri::command]
async fn set_always_on_top(app: AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_always_on_top(enabled)
            .map_err(|e| {
                let err = e.to_string();
                error_reporting::capture_window_error(&err, "set_always_on_top");
                err
            })?;
    }
    Ok(())
}

#[tauri::command]
async fn set_window_position(app: AppHandle, position: String) -> Result<(), String> {
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

        let (x, y) = match position.as_str() {
            "left" => (0, (monitor_size.height - window_size.height) / 2),
            "right" => (
                monitor_size.width - window_size.width,
                (monitor_size.height - window_size.height) / 2,
            ),
            "top-left" => (0, 0),
            "top-right" => (monitor_size.width - window_size.width, 0),
            "bottom-left" => (0, monitor_size.height - window_size.height),
            "bottom-right" => (
                monitor_size.width - window_size.width,
                monitor_size.height - window_size.height,
            ),
            _ => (
                monitor_size.width - window_size.width,
                (monitor_size.height - window_size.height) / 2,
            ),
        };

        window
            .set_position(tauri::PhysicalPosition::new(x as i32, y as i32))
            .map_err(|e| {
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
}

#[cfg(target_os = "windows")]
fn is_pawnio_installed() -> bool {
    use std::process::Command;
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Check if PawnIO driver is loaded by checking if the service exists
    let output = Command::new("sc")
        .args(["query", "PawnIO"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // If the service exists and is running, it will contain "RUNNING" or "STATE"
        return stdout.contains("STATE") || stdout.contains("RUNNING");
    }

    false
}

#[cfg(not(target_os = "windows"))]
fn is_pawnio_installed() -> bool {
    true // Always return true on non-Windows (PawnIO not needed)
}

#[tauri::command]
async fn check_pawnio_status() -> Result<PawnIOStatus, String> {
    Ok(PawnIOStatus {
        installed: is_pawnio_installed(),
        checking: false,
    })
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn download_and_install_pawnio() -> Result<String, String> {
    // Get the bundled PawnIO installer path
    let exe_path = std::env::current_exe()
        .map_err(|e| format!("Failed to get exe path: {}", e))?;
    let exe_dir = exe_path.parent()
        .ok_or("Failed to get exe directory")?;

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
        return Err(format!("PawnIO installer not found at {:?}", installer_path));
    }

    // Run the installer with UAC elevation (ShellExecuteW with "runas")
    // Use /S for silent installation
    tokio::task::spawn_blocking(move || {
        use std::os::windows::ffi::OsStrExt;
        use std::ffi::OsStr;

        let path_wide: Vec<u16> = OsStr::new(installer_path.to_str().unwrap_or_default())
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let verb_wide: Vec<u16> = OsStr::new("runas")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // Silent install parameter
        let params_wide: Vec<u16> = OsStr::new("/S")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        unsafe {
            use windows::Win32::UI::Shell::ShellExecuteW;
            use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
            use windows::core::PCWSTR;

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
                return Err(format!("Failed to launch installer (error code: {})", result.0 as isize));
            }
        }

        Ok("PawnIO driver is being installed. Please restart Ondo after installation completes.".to_string())
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
        let size = window.outer_size().map_err(|e| {
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
async fn restore_window_state(app: AppHandle, state: WindowStateData) -> Result<(), String> {
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

fn main() {
    // Initialize Sentry for error reporting
    error_reporting::init_sentry();

    let initial_settings = settings::load_settings_from_file()
        .unwrap_or_else(|_| settings::AppSettings::default());

    // Clone values we need for setup before moving into AppState
    let startup_position = initial_settings.position.clone();
    let startup_always_on_top = initial_settings.always_on_top;
    let startup_always_on_back = initial_settings.always_on_back;
    let startup_window_state = initial_settings.window_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(AppState {
            settings: Mutex::new(initial_settings),
        })
        .setup(move |app| {
            // Setup system tray
            tray::setup_tray(app)?;

            // Position window on startup
            if let Some(window) = app.get_webview_window("main") {
                // Restore saved window state if available
                if let Some(ref state) = startup_window_state {
                    let _ = window.set_position(tauri::PhysicalPosition::new(state.x, state.y));
                    let _ = window.set_size(tauri::PhysicalSize::new(state.width, state.height));
                } else {
                    // Set initial position based on setting
                    let _ = set_initial_position(&window, &startup_position);
                }

                // Set always on top/back
                if startup_always_on_back {
                    let _ = window.set_always_on_bottom(true);
                } else if startup_always_on_top {
                    let _ = window.set_always_on_top(true);
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
            set_window_position,
            set_auto_start,
            get_window_state,
            restore_window_state,
            check_pawnio_status,
            download_and_install_pawnio,
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

    // Shutdown LHM daemon on app exit
    hardware::shutdown_lhm_daemon();
}

fn set_initial_position(
    window: &tauri::WebviewWindow,
    position: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let monitor = window.current_monitor()?.ok_or("No monitor")?;
    let monitor_size = monitor.size();
    let window_size = window.outer_size()?;

    let (x, y) = match position {
        "left" => (0, (monitor_size.height - window_size.height) / 2),
        "right" => (
            monitor_size.width - window_size.width,
            (monitor_size.height - window_size.height) / 2,
        ),
        "top-left" => (0, 0),
        "top-right" => (monitor_size.width - window_size.width, 0),
        "bottom-left" => (0, monitor_size.height - window_size.height),
        "bottom-right" => (
            monitor_size.width - window_size.width,
            monitor_size.height - window_size.height,
        ),
        _ => (
            monitor_size.width - window_size.width,
            (monitor_size.height - window_size.height) / 2,
        ),
    };

    window.set_position(tauri::PhysicalPosition::new(x as i32, y as i32))?;
    Ok(())
}
