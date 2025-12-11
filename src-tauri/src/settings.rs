use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub position: String,
    pub opacity: u32,
    pub always_on_top: bool,
    #[serde(default)]
    pub always_on_back: bool,
    pub auto_start: bool,
    pub show_cpu_cores: bool,
    pub update_interval: u32,
    pub theme: String,
    pub compact_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_state: Option<WindowState>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            position: "right".to_string(),
            opacity: 95,
            always_on_top: false,
            always_on_back: false,
            auto_start: false,
            show_cpu_cores: false,
            update_interval: 1000,
            theme: "auto".to_string(),
            compact_mode: false,
            window_state: None,
        }
    }
}

fn get_settings_path() -> PathBuf {
    let app_data = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let ondo_dir = app_data.join("Ondo");
    fs::create_dir_all(&ondo_dir).ok();
    ondo_dir.join("settings.json")
}

pub fn load_settings_from_file() -> Result<AppSettings, String> {
    let path = get_settings_path();
    if path.exists() {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str(&content).map_err(|e| e.to_string())
    } else {
        Ok(AppSettings::default())
    }
}

pub async fn save_settings_to_file(settings: &AppSettings) -> Result<(), String> {
    let path = get_settings_path();
    let content = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
pub async fn set_auto_start(enabled: bool) -> Result<(), String> {
    use std::process::Command;

    let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_path_str = exe_path.to_string_lossy();

    if enabled {
        // Add to registry for auto-start
        let output = Command::new("reg")
            .args([
                "add",
                "HKCU\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "Ondo",
                "/t",
                "REG_SZ",
                "/d",
                &format!("\"{}\"", exe_path_str),
                "/f",
            ])
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err("Failed to add registry key".to_string());
        }
    } else {
        // Remove from registry
        let _ = Command::new("reg")
            .args([
                "delete",
                "HKCU\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Run",
                "/v",
                "Ondo",
                "/f",
            ])
            .output();
    }

    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub async fn set_auto_start(_enabled: bool) -> Result<(), String> {
    // Auto-start is Windows-specific
    Ok(())
}
