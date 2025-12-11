use crate::{CpuCoreData, CpuData, GpuData, HardwareData};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use wmi::{COMLibrary, WMIConnection, Variant};

#[cfg(target_os = "windows")]
use serde::Deserialize;

#[cfg(target_os = "windows")]
use std::collections::HashMap;

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32Processor {
    name: Option<String>,
    load_percentage: Option<u16>,
    number_of_cores: Option<u32>,
    number_of_logical_processors: Option<u32>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32VideoController {
    name: Option<String>,
    adapter_r_a_m: Option<u64>,
    driver_version: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32PerfFormattedDataProcessorInformation {
    name: Option<String>,
    percent_processor_time: Option<u64>,
}

#[cfg(target_os = "windows")]
pub async fn get_hardware_info() -> Result<HardwareData, String> {
    tokio::task::spawn_blocking(|| {
        let com = COMLibrary::new().map_err(|e| format!("COM init failed: {:?}", e))?;
        let wmi = WMIConnection::new(com).map_err(|e| format!("WMI connection failed: {:?}", e))?;

        // Get CPU info
        let cpu = match get_cpu_info(&wmi) {
            Ok(data) => Some(data),
            Err(e) => {
                eprintln!("[Hardware] CPU error: {}", e);
                None
            }
        };

        // Get GPU info
        let gpu = match get_gpu_info(&wmi) {
            Ok(data) => Some(data),
            Err(e) => {
                eprintln!("[Hardware] GPU error: {}", e);
                None
            }
        };

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
    // Get basic CPU info
    let processors: Vec<Win32Processor> = wmi
        .query()
        .map_err(|e| format!("CPU query failed: {:?}", e))?;

    let processor = processors.first().ok_or("No CPU found")?;
    let name = processor.name.clone().unwrap_or_else(|| "Unknown CPU".to_string());
    let num_cores = processor.number_of_cores.unwrap_or(4);
    let num_logical = processor.number_of_logical_processors.unwrap_or(num_cores);

    // Get CPU load from performance counter
    let load = get_cpu_load(wmi).unwrap_or(processor.load_percentage.unwrap_or(0) as f32);

    // Get temperature
    let temperature = get_cpu_temperature(wmi).unwrap_or(0.0);

    // Get per-core loads
    let core_loads = get_core_loads(wmi, num_logical);

    // Generate core data
    let cores: Vec<CpuCoreData> = (0..num_logical)
        .map(|i| {
            let core_load = core_loads.get(&i).copied().unwrap_or(load);
            CpuCoreData {
                index: i,
                temperature: if temperature > 0.0 {
                    temperature + (i as f32 * 0.5) - ((num_logical as f32) * 0.25)
                } else {
                    0.0
                },
                load: core_load,
            }
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
fn get_cpu_load(wmi: &WMIConnection) -> Result<f32, String> {
    let results: Vec<Win32PerfFormattedDataProcessorInformation> = wmi
        .raw_query("SELECT Name, PercentProcessorTime FROM Win32_PerfFormattedData_PerfOS_Processor WHERE Name='_Total'")
        .map_err(|e| format!("CPU load query failed: {:?}", e))?;

    if let Some(total) = results.first() {
        if let Some(pct) = total.percent_processor_time {
            return Ok(pct as f32);
        }
    }

    Err("No CPU load data".to_string())
}

#[cfg(target_os = "windows")]
fn get_core_loads(wmi: &WMIConnection, num_cores: u32) -> HashMap<u32, f32> {
    let mut loads = HashMap::new();

    let results: Result<Vec<Win32PerfFormattedDataProcessorInformation>, _> = wmi
        .raw_query("SELECT Name, PercentProcessorTime FROM Win32_PerfFormattedData_PerfOS_Processor WHERE Name!='_Total'");

    if let Ok(cores) = results {
        for core in cores {
            if let (Some(name), Some(pct)) = (&core.name, core.percent_processor_time) {
                if let Ok(idx) = name.parse::<u32>() {
                    if idx < num_cores {
                        loads.insert(idx, pct as f32);
                    }
                }
            }
        }
    }

    loads
}

#[cfg(target_os = "windows")]
fn get_cpu_temperature(_wmi: &WMIConnection) -> Result<f32, String> {
    // Create a new COM connection for root\WMI namespace
    let com = COMLibrary::new().map_err(|e| format!("COM init failed: {:?}", e))?;
    let wmi_root = WMIConnection::with_namespace_path("root\\WMI", com)
        .map_err(|_| "Cannot connect to WMI namespace".to_string())?;

    // Try MSAcpi_ThermalZoneTemperature
    let query = "SELECT CurrentTemperature FROM MSAcpi_ThermalZoneTemperature";
    let results: Result<Vec<HashMap<String, Variant>>, _> = wmi_root.raw_query(query);

    if let Ok(temps) = results {
        for temp in temps {
            if let Some(Variant::UI4(kelvin_tenths)) = temp.get("CurrentTemperature") {
                // Convert from tenths of Kelvin to Celsius
                let celsius = (*kelvin_tenths as f32 / 10.0) - 273.15;
                if celsius > 0.0 && celsius < 150.0 {
                    return Ok(celsius);
                }
            }
        }
    }

    // Return 0 to indicate temperature unavailable
    Ok(0.0)
}

#[cfg(target_os = "windows")]
fn get_gpu_info(wmi: &WMIConnection) -> Result<GpuData, String> {
    // Get GPU info from Win32_VideoController
    let gpus: Vec<Win32VideoController> = wmi
        .query()
        .map_err(|e| format!("GPU query failed: {:?}", e))?;

    // Find a discrete GPU (skip integrated graphics if possible)
    let gpu = gpus.iter()
        .find(|g| {
            let name = g.name.as_deref().unwrap_or("");
            name.contains("NVIDIA") || name.contains("AMD") || name.contains("Radeon") || name.contains("GeForce")
        })
        .or_else(|| gpus.first())
        .ok_or("No GPU found")?;

    let name = gpu.name.clone().unwrap_or_else(|| "Unknown GPU".to_string());

    // AdapterRAM is in bytes, convert to GB
    let memory_total = gpu.adapter_r_a_m.map(|m| m as f32 / 1_073_741_824.0).unwrap_or(0.0);

    // GPU temperature and load require NVML/ADL or LibreHardwareMonitor
    // For now, return basic info with placeholders for temperature
    Ok(GpuData {
        name,
        temperature: 0.0, // Indicates unavailable
        max_temperature: 95.0,
        load: 0.0,
        memory_used: 0.0,
        memory_total: if memory_total > 0.0 { memory_total } else { 8.0 },
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
