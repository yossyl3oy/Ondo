// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
    cores: Vec<CpuCoreData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuData {
    name: String,
    temperature: f32,
    #[serde(rename = "maxTemperature")]
    max_temperature: f32,
    load: f32,
    #[serde(rename = "memoryUsed")]
    memory_used: f32,
    #[serde(rename = "memoryTotal")]
    memory_total: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareData {
    cpu: Option<CpuData>,
    gpu: Option<GpuData>,
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
    hardware::get_hardware_info().await
}

#[tauri::command]
async fn get_settings(state: State<'_, AppState>) -> Result<settings::AppSettings, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.clone())
}

#[tauri::command]
async fn save_settings(
    state: State<'_, AppState>,
    settings: settings::AppSettings,
) -> Result<(), String> {
    // Update state first, then drop the lock before await
    {
        let mut current = state.settings.lock().map_err(|e| e.to_string())?;
        *current = settings.clone();
    }
    settings::save_settings_to_file(&settings).await
}

#[tauri::command]
async fn set_always_on_top(app: AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_always_on_top(enabled)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn set_window_position(app: AppHandle, position: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let monitor = window
            .current_monitor()
            .map_err(|e| e.to_string())?
            .ok_or("No monitor found")?;

        let monitor_size = monitor.size();
        let window_size = window.outer_size().map_err(|e| e.to_string())?;

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
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn set_auto_start(enabled: bool) -> Result<(), String> {
    settings::set_auto_start(enabled).await
}

#[tauri::command]
async fn set_always_on_back(app: AppHandle, enabled: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        if enabled {
            // Disable always on top first
            window.set_always_on_top(false).map_err(|e| e.to_string())?;
            // Set always on bottom
            window.set_always_on_bottom(true).map_err(|e| e.to_string())?;
        } else {
            window.set_always_on_bottom(false).map_err(|e| e.to_string())?;
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
        let position = window.outer_position().map_err(|e| e.to_string())?;
        let size = window.outer_size().map_err(|e| e.to_string())?;
        Ok(WindowStateData {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        })
    } else {
        Err("Window not found".to_string())
    }
}

#[tauri::command]
async fn restore_window_state(app: AppHandle, state: WindowStateData) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_position(tauri::PhysicalPosition::new(state.x, state.y))
            .map_err(|e| e.to_string())?;
        window
            .set_size(tauri::PhysicalSize::new(state.width, state.height))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn main() {
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
