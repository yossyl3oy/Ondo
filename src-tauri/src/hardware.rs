use crate::{CpuCoreData, CpuData, GpuData, HardwareData};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use wmi::{COMLibrary, WMIConnection};

#[cfg(target_os = "windows")]
use serde::Deserialize;

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32Processor {
    name: Option<String>,
    load_percentage: Option<u16>,
    number_of_cores: Option<u32>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct MSAcpiThermalZoneTemperature {
    current_temperature: Option<u32>,
}

#[cfg(target_os = "windows")]
pub async fn get_hardware_info() -> Result<HardwareData, String> {
    tokio::task::spawn_blocking(|| {
        let com = COMLibrary::new().map_err(|e| format!("COM init failed: {:?}", e))?;
        let wmi = WMIConnection::new(com).map_err(|e| format!("WMI connection failed: {:?}", e))?;

        // Get CPU info
        let cpu = get_cpu_info(&wmi).ok();

        // Get GPU info (simplified - Windows doesn't expose GPU temp through WMI easily)
        let gpu = get_gpu_info(&wmi).ok();

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Ok(HardwareData {
            cpu,
            gpu,
            timestamp,
        })
    })
    .await
    .map_err(|e| format!("Task failed: {:?}", e))?
}

#[cfg(target_os = "windows")]
fn get_cpu_info(wmi: &WMIConnection) -> Result<CpuData, String> {
    let processors: Vec<Win32Processor> = wmi
        .query()
        .map_err(|e| format!("CPU query failed: {:?}", e))?;

    let processor = processors.first().ok_or("No CPU found")?;
    let name = processor.name.clone().unwrap_or_else(|| "Unknown CPU".to_string());
    let load = processor.load_percentage.unwrap_or(0) as f32;
    let num_cores = processor.number_of_cores.unwrap_or(4);

    // Try to get temperature from thermal zone
    let temperature = get_cpu_temperature(wmi).unwrap_or(50.0);

    // Generate core data (simulated variation for demo)
    let cores: Vec<CpuCoreData> = (0..num_cores)
        .map(|i| CpuCoreData {
            index: i,
            temperature: temperature + (i as f32 * 0.5) - ((num_cores as f32) * 0.25),
            load: (load + (i as f32 * 2.0) - (num_cores as f32)).max(0.0).min(100.0),
        })
        .collect();

    Ok(CpuData {
        name,
        temperature,
        max_temperature: 100.0,
        load,
        cores,
    })
}

#[cfg(target_os = "windows")]
fn get_cpu_temperature(wmi: &WMIConnection) -> Result<f32, String> {
    // Try MSAcpi_ThermalZoneTemperature
    let temps: Result<Vec<MSAcpiThermalZoneTemperature>, _> = wmi.raw_query(
        "SELECT * FROM MSAcpi_ThermalZoneTemperature"
    );

    if let Ok(temps) = temps {
        if let Some(temp) = temps.first() {
            if let Some(kelvin_tenths) = temp.current_temperature {
                // Convert from tenths of Kelvin to Celsius
                let celsius = (kelvin_tenths as f32 / 10.0) - 273.15;
                if celsius > 0.0 && celsius < 150.0 {
                    return Ok(celsius);
                }
            }
        }
    }

    // Fallback: return a reasonable default
    Ok(50.0)
}

#[cfg(target_os = "windows")]
fn get_gpu_info(_wmi: &WMIConnection) -> Result<GpuData, String> {
    // Note: Windows WMI doesn't provide GPU temperature directly
    // For real temperature monitoring, LibreHardwareMonitor or NVAPI/ADL would be needed

    // Return placeholder data - in production, this would use NVML/ADL
    Ok(GpuData {
        name: "Graphics Card".to_string(),
        temperature: 55.0,
        max_temperature: 95.0,
        load: 20.0,
        memory_used: 2.0,
        memory_total: 8.0,
    })
}

// Non-Windows fallback implementation
#[cfg(not(target_os = "windows"))]
pub async fn get_hardware_info() -> Result<HardwareData, String> {
    // Return mock data for non-Windows platforms (development)
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let base_temp = 45.0 + (rand_float() * 20.0);
    let gpu_temp = 50.0 + (rand_float() * 25.0);

    Ok(HardwareData {
        cpu: Some(CpuData {
            name: "AMD Ryzen 9 5900X".to_string(),
            temperature: base_temp,
            max_temperature: 95.0,
            load: 20.0 + (rand_float() * 40.0),
            cores: (0..12)
                .map(|i| CpuCoreData {
                    index: i,
                    temperature: base_temp + (rand_float() - 0.5) * 10.0,
                    load: rand_float() * 100.0,
                })
                .collect(),
        }),
        gpu: Some(GpuData {
            name: "NVIDIA GeForce RTX 3080".to_string(),
            temperature: gpu_temp,
            max_temperature: 93.0,
            load: 15.0 + (rand_float() * 50.0),
            memory_used: 4.0 + (rand_float() * 4.0),
            memory_total: 10.0,
        }),
        timestamp,
    })
}

#[cfg(not(target_os = "windows"))]
fn rand_float() -> f32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64,
    );
    (hasher.finish() % 1000) as f32 / 1000.0
}
