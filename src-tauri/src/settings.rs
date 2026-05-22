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

// `#[serde(default)]` at the struct level makes every missing field fall
// back to `Default::default()`, so old settings.json files from earlier
// versions keep loading cleanly when we add new fields. Field-level
// `default = "..."` overrides are kept where the helper value diverges from
// what `Default::default()` alone would produce (it doesn't here, but the
// explicit form documents intent).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub position: String,
    pub opacity: u32,
    pub always_on_top: bool,
    pub always_on_back: bool,
    pub auto_start: bool,
    pub update_interval: u32,
    pub theme: String,
    pub temperature_unit: String,
    pub compact_mode: bool,
    pub debug_server: bool,
    pub section_order: Vec<String>,
    pub hidden_sections: Vec<String>,
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
            update_interval: 1000,
            theme: "auto".to_string(),
            temperature_unit: "celsius".to_string(),
            compact_mode: false,
            debug_server: false,
            section_order: default_section_order(),
            hidden_sections: Vec::new(),
            window_state: None,
        }
    }
}

fn default_section_order() -> Vec<String> {
    vec![
        "cpu".to_string(),
        "gpu".to_string(),
        "storage".to_string(),
        "motherboard".to_string(),
        "network".to_string(),
        "audio".to_string(),
        "display".to_string(),
    ]
}

fn get_settings_path() -> PathBuf {
    let app_data = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    let ondo_dir = app_data.join("Ondo");
    fs::create_dir_all(&ondo_dir).ok();
    ondo_dir.join("settings.json")
}

pub fn load_settings_from_file() -> Result<AppSettings, String> {
    let path = get_settings_path();
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    match serde_json::from_str::<AppSettings>(&content) {
        Ok(settings) => Ok(settings),
        Err(parse_err) => {
            // Rename the broken file aside so the next save() doesn't
            // overwrite the user's last known config with defaults.
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let backup = path.with_extension(format!("json.broken-{}", ts));
            let rename_result = fs::rename(&path, &backup);
            crate::log_error!(
                "Settings",
                "Failed to parse settings.json: {}. Backup={:?} (rename ok={}). Starting with defaults.",
                parse_err,
                backup,
                rename_result.is_ok()
            );
            Ok(AppSettings::default())
        }
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
