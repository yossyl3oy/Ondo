use crate::{CpuCoreData, CpuData, GpuData, HardwareData, StorageData, MotherboardData};

#[cfg(target_os = "windows")]
use crate::FanData;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use crate::error_reporting;

#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
use serde::Deserialize;

// LHM JSON response structures
#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug, Clone)]
struct LhmResponse {
    cpu: Option<LhmCpuData>,
    gpu: Option<LhmGpuData>,
    storage: Option<Vec<LhmStorageData>>,
    motherboard: Option<LhmMotherboardData>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug, Clone)]
struct LhmCpuData {
    name: String,
    temperature: f32,
    max_temperature: f32,
    load: f32,
    frequency: f32,
    cores: Option<Vec<LhmCpuCoreData>>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug, Clone)]
struct LhmCpuCoreData {
    index: u32,
    temperature: f32,
    load: f32,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug, Clone)]
struct LhmGpuData {
    name: String,
    temperature: f32,
    max_temperature: f32,
    load: f32,
    frequency: f32,
    memory_used: f32,
    memory_total: f32,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug, Clone)]
struct LhmStorageData {
    name: String,
    temperature: f32,
    used_percent: f32,
    total_space: f32,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug, Clone)]
struct LhmMotherboardData {
    name: String,
    temperature: f32,
    fans: Option<Vec<LhmFanData>>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug, Clone)]
struct LhmFanData {
    name: String,
    speed: u32,
}

// LHM daemon process state
#[cfg(target_os = "windows")]
use std::process::{Child, ChildStdout};
#[cfg(target_os = "windows")]
use std::io::{BufReader, BufRead};

#[cfg(target_os = "windows")]
struct LhmDaemon {
    process: Child,
    reader: BufReader<ChildStdout>,
    latest_data: Option<LhmResponse>,
}

#[cfg(target_os = "windows")]
static LHM_DAEMON: Mutex<Option<LhmDaemon>> = Mutex::new(None);

#[cfg(target_os = "windows")]
pub async fn get_hardware_info() -> Result<HardwareData, String> {
    tokio::task::spawn_blocking(|| {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        // Try to get data from LHM daemon
        let lhm_data = get_lhm_data();

        // Get WMI data for fallback/supplement
        let wmi_data = get_hardware_from_wmi(timestamp).ok();

        if let Some(lhm) = lhm_data {
            // Use LHM data, supplement with WMI where needed
            let cpu = lhm.cpu.map(|c| CpuData {
                name: c.name,
                temperature: c.temperature,
                max_temperature: c.max_temperature,
                load: c.load,
                frequency: c.frequency,
                cores: c.cores.map(|cores| {
                    cores.into_iter().map(|core| CpuCoreData {
                        index: core.index,
                        temperature: core.temperature,
                        load: core.load,
                    }).collect()
                }).unwrap_or_default(),
            });

            let gpu = lhm.gpu.map(|g| GpuData {
                name: g.name,
                temperature: g.temperature,
                max_temperature: g.max_temperature,
                load: g.load,
                frequency: g.frequency,
                memory_used: g.memory_used,
                memory_total: g.memory_total,
            });

            // For storage: use LHM data, but supplement with WMI if LHM data is incomplete
            let storage = lhm.storage.map(|storages| {
                let wmi_storage = wmi_data.as_ref().and_then(|w| w.storage.as_ref());
                storages.into_iter().map(|s| {
                    // Find matching WMI storage for supplementing missing data
                    let wmi_match = wmi_storage.and_then(|ws| {
                        ws.iter().find(|w| {
                            w.name.contains(&s.name) || s.name.contains(&w.name) ||
                            // Also match by partial name (e.g., "Samsung" matches "Samsung SSD 980")
                            w.name.split_whitespace().next().map(|first| s.name.contains(first)).unwrap_or(false)
                        })
                    });

                    // If LHM doesn't have total_space, try to find it from WMI
                    let total_space = if s.total_space > 0.0 {
                        s.total_space
                    } else {
                        wmi_match.map(|w| w.total_space).unwrap_or(0.0)
                    };

                    // If LHM doesn't have used_percent, try to find it from WMI
                    let used_space = if s.used_percent > 0.0 {
                        s.used_percent
                    } else {
                        wmi_match.map(|w| w.used_space).unwrap_or(0.0)
                    };

                    // If LHM doesn't have temperature, try to find it from WMI (PowerShell fallback)
                    let temperature = if s.temperature > 0.0 {
                        s.temperature
                    } else {
                        wmi_match.map(|w| w.temperature).unwrap_or(0.0)
                    };

                    StorageData {
                        name: s.name,
                        temperature,
                        used_space,
                        total_space,
                    }
                }).collect()
            });

            // For motherboard: use LHM data, supplement with WMI for name if needed
            let motherboard = lhm.motherboard.map(|m| {
                let wmi_mb = wmi_data.as_ref().and_then(|w| w.motherboard.as_ref());
                let name = if m.name.is_empty() || m.name == "Unknown" {
                    wmi_mb.map(|w| w.name.clone()).unwrap_or(m.name)
                } else {
                    m.name
                };
                // Use WMI fans if LHM doesn't have any
                let fans = m.fans.map(|fans| {
                    if fans.is_empty() {
                        wmi_mb.map(|w| w.fans.clone()).unwrap_or_default()
                    } else {
                        fans.into_iter().map(|f| FanData {
                            name: f.name,
                            speed: f.speed,
                        }).collect()
                    }
                }).unwrap_or_else(|| {
                    wmi_mb.map(|w| w.fans.clone()).unwrap_or_default()
                });
                MotherboardData {
                    name,
                    temperature: m.temperature,
                    fans,
                }
            }).or_else(|| wmi_data.as_ref().and_then(|w| w.motherboard.clone()));

            Ok(HardwareData {
                cpu,
                gpu,
                storage,
                motherboard,
                timestamp,
                cpu_error: None,
                gpu_error: None,
            })
        } else if let Some(wmi) = wmi_data {
            // Full fallback to WMI
            Ok(wmi)
        } else {
            Ok(HardwareData {
                cpu: None,
                gpu: None,
                storage: None,
                motherboard: None,
                timestamp,
                cpu_error: Some("Failed to get hardware data".to_string()),
                gpu_error: Some("Failed to get hardware data".to_string()),
            })
        }
    })
    .await
    .map_err(|e| format!("Task failed: {:?}", e))?
}

#[cfg(target_os = "windows")]
fn get_lhm_data() -> Option<LhmResponse> {
    let mut daemon_guard = LHM_DAEMON.lock().ok()?;

    // Start daemon if not running
    if daemon_guard.is_none() {
        match start_lhm_daemon() {
            Ok(daemon) => {
                *daemon_guard = Some(daemon);
            }
            Err(e) => {
                crate::log_error!("Hardware", "Failed to start LHM daemon: {}", e);
                error_reporting::capture_lhm_error(&format!("Failed to start daemon: {}", e));
                return None;
            }
        }
    }

    let daemon = daemon_guard.as_mut()?;

    // Check if process is still running
    match daemon.process.try_wait() {
        Ok(Some(status)) => {
            // Process exited, restart it
            crate::log_warn!("Hardware", "LHM daemon exited with status: {}, restarting...", status);
            *daemon_guard = None;
            return None;
        }
        Ok(None) => {
            // Process still running, read latest line
        }
        Err(e) => {
            crate::log_error!("Hardware", "Failed to check LHM daemon status: {}", e);
            *daemon_guard = None;
            return None;
        }
    }

    // Read all available lines and keep the latest one
    // Use PeekNamedPipe to check for available data before reading,
    // because PIPE_NOWAIT may not work reliably on anonymous pipes.
    let mut latest_line = None;
    loop {
        // Check if there's data available before attempting to read
        let has_data = {
            use std::os::windows::io::AsRawHandle;
            use windows::Win32::System::Pipes::PeekNamedPipe;
            use windows::Win32::Foundation::HANDLE;

            let handle = HANDLE(daemon.reader.get_ref().as_raw_handle() as *mut std::ffi::c_void);
            let mut bytes_available: u32 = 0;
            let ok = unsafe {
                PeekNamedPipe(handle, None, 0, None, Some(&mut bytes_available), None)
            };
            ok.is_ok() && bytes_available > 0
        };

        if !has_data {
            break;
        }

        let mut line = String::new();
        match daemon.reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    latest_line = Some(trimmed.to_string());
                }
            }
            Err(e) => {
                crate::log_error!("Hardware", "Failed to read from LHM daemon: {}", e);
                break;
            }
        }
    }

    // Parse the latest line if available
    if let Some(line) = latest_line {
        match serde_json::from_str::<LhmResponse>(&line) {
            Ok(data) => {
                daemon.latest_data = Some(data.clone());
                return Some(data);
            }
            Err(e) => {
                crate::log_warn!("Hardware", "Failed to parse LHM JSON: {}", e);
            }
        }
    }

    // Return cached data if no new data available
    daemon.latest_data.clone()
}

#[cfg(target_os = "windows")]
fn start_lhm_daemon() -> Result<LhmDaemon, String> {
    use std::process::{Command, Stdio};
    use std::os::windows::process::CommandExt;
    use std::env;

    // CREATE_NO_WINDOW flag to prevent console window from appearing
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let exe_path = env::current_exe().map_err(|e| format!("Cannot get exe path: {}", e))?;
    let exe_dir = exe_path.parent().ok_or("Cannot get exe directory")?;
    let lhm_path = exe_dir.join("ondo-hwmon.exe");

    if !lhm_path.exists() {
        return Err(format!("LHM CLI not found at {:?}", lhm_path));
    }

    let mut child = Command::new(&lhm_path)
        .args(["--daemon", "1000"]) // 1 second interval
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start LHM daemon: {}", e))?;

    let stdout = child.stdout.take()
        .ok_or("Failed to capture LHM daemon stdout")?;

    let reader = BufReader::new(stdout);

    crate::log_info!("Hardware", "LHM daemon started successfully");

    Ok(LhmDaemon {
        process: child,
        reader,
        latest_data: None,
    })
}

// Shutdown LHM daemon when app exits
#[cfg(target_os = "windows")]
pub fn shutdown_lhm_daemon() {
    if let Ok(mut daemon_guard) = LHM_DAEMON.lock() {
        if let Some(mut daemon) = daemon_guard.take() {
            let _ = daemon.process.kill();
            crate::log_info!("Hardware", "LHM daemon stopped");
        }
    }
}

// WMI fallback implementation
#[cfg(target_os = "windows")]
use wmi::{COMLibrary, WMIConnection, Variant};

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
    current_clock_speed: Option<u32>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32VideoController {
    name: Option<String>,
    adapter_r_a_m: Option<u64>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32PerfFormattedDataProcessorInformation {
    name: Option<String>,
    percent_processor_time: Option<u64>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32DiskDrive {
    model: Option<String>,
    size: Option<u64>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32LogicalDisk {
    device_i_d: Option<String>,
    size: Option<u64>,
    free_space: Option<u64>,
    volume_name: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32BaseBoard {
    manufacturer: Option<String>,
    product: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32Fan {
    name: Option<String>,
    #[serde(rename = "DesiredSpeed")]
    desired_speed: Option<u64>,
}

#[cfg(target_os = "windows")]
fn get_hardware_from_wmi(timestamp: u64) -> Result<HardwareData, String> {
    let com = match COMLibrary::new() {
        Ok(c) => c,
        Err(e) => {
            let error_msg = format!("COM init failed: {:?}", e);
            error_reporting::capture_wmi_error(&error_msg, "com_init");
            return Ok(HardwareData {
                cpu: None,
                gpu: None,
                storage: None,
                motherboard: None,
                timestamp,
                cpu_error: Some(error_msg.clone()),
                gpu_error: Some(error_msg),
            });
        }
    };

    let wmi = match WMIConnection::new(com) {
        Ok(w) => w,
        Err(e) => {
            let error_msg = format!("WMI connection failed: {:?}", e);
            error_reporting::capture_wmi_error(&error_msg, "wmi_connection");
            return Ok(HardwareData {
                cpu: None,
                gpu: None,
                storage: None,
                motherboard: None,
                timestamp,
                cpu_error: Some(error_msg.clone()),
                gpu_error: Some(error_msg),
            });
        }
    };

    let (cpu, cpu_error) = match get_cpu_info(&wmi) {
        Ok(data) => (Some(data), None),
        Err(e) => {
            error_reporting::capture_wmi_error(&e, "cpu_info");
            (None, Some(e))
        }
    };

    let (gpu, gpu_error) = match get_gpu_info(&wmi) {
        Ok(data) => (Some(data), None),
        Err(e) => {
            error_reporting::capture_wmi_error(&e, "gpu_info");
            (None, Some(e))
        }
    };

    let storage = get_storage_info(&wmi).ok();
    let motherboard = get_motherboard_info(&wmi).ok();

    Ok(HardwareData {
        cpu,
        gpu,
        storage,
        motherboard,
        timestamp,
        cpu_error,
        gpu_error,
    })
}

#[cfg(target_os = "windows")]
fn get_cpu_info(wmi: &WMIConnection) -> Result<CpuData, String> {
    let processors: Vec<Win32Processor> = wmi
        .raw_query("SELECT Name, LoadPercentage, NumberOfCores, NumberOfLogicalProcessors, CurrentClockSpeed FROM Win32_Processor")
        .map_err(|e| format!("CPU query failed: {:?}", e))?;

    let processor = processors.first().ok_or("No CPU found")?;
    let name = processor.name.clone().unwrap_or_else(|| "Unknown CPU".to_string());
    let num_cores = processor.number_of_cores.unwrap_or(4);
    let num_logical = processor.number_of_logical_processors.unwrap_or(num_cores);
    let frequency = processor.current_clock_speed.map(|mhz| mhz as f32 / 1000.0).unwrap_or(0.0);
    let load = get_cpu_load(wmi).unwrap_or(processor.load_percentage.unwrap_or(0) as f32);
    let temperature = get_cpu_temperature(wmi).unwrap_or(0.0);
    let core_loads = get_core_loads(wmi, num_logical);

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
        frequency,
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
    // Try MSAcpi_ThermalZoneTemperature first
    if let Ok(temp) = get_cpu_temp_from_thermal_zone() {
        if temp > 0.0 {
            return Ok(temp);
        }
    }

    // Fallback: Try PowerShell with Get-WmiObject
    if let Some(temp) = get_cpu_temp_from_powershell() {
        return Ok(temp);
    }

    Ok(0.0)
}

#[cfg(target_os = "windows")]
fn get_cpu_temp_from_thermal_zone() -> Result<f32, String> {
    let com = COMLibrary::new().map_err(|e| format!("COM init failed: {:?}", e))?;
    let wmi_root = WMIConnection::with_namespace_path("root\\WMI", com)
        .map_err(|_| "Cannot connect to WMI namespace".to_string())?;

    let query = "SELECT CurrentTemperature FROM MSAcpi_ThermalZoneTemperature";
    let results: Result<Vec<HashMap<String, Variant>>, _> = wmi_root.raw_query(query);

    if let Ok(temps) = results {
        for temp in temps {
            if let Some(Variant::UI4(kelvin_tenths)) = temp.get("CurrentTemperature") {
                let celsius = (*kelvin_tenths as f32 / 10.0) - 273.15;
                if celsius > 0.0 && celsius < 150.0 {
                    return Ok(celsius);
                }
            }
        }
    }
    Ok(0.0)
}

#[cfg(target_os = "windows")]
fn get_cpu_temp_from_powershell() -> Option<f32> {
    use std::process::{Command, Stdio};
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Use PowerShell to query thermal zone temperature
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "Get-CimInstance -Namespace root/WMI -ClassName MSAcpi_ThermalZoneTemperature 2>$null | Select-Object -ExpandProperty CurrentTemperature | Select-Object -First 1"
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if let Ok(kelvin_tenths) = stdout.trim().parse::<f32>() {
            let celsius = (kelvin_tenths / 10.0) - 273.15;
            if celsius > 0.0 && celsius < 150.0 {
                return Some(celsius);
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn get_gpu_info(wmi: &WMIConnection) -> Result<GpuData, String> {
    let gpus: Vec<Win32VideoController> = wmi
        .raw_query("SELECT Name, AdapterRAM FROM Win32_VideoController")
        .map_err(|e| format!("GPU query failed: {:?}", e))?;

    let gpu = gpus.iter()
        .find(|g| {
            let name = g.name.as_deref().unwrap_or("");
            name.contains("NVIDIA") || name.contains("AMD") || name.contains("Radeon") || name.contains("GeForce")
        })
        .or_else(|| gpus.first())
        .ok_or("No GPU found")?;

    let name = gpu.name.clone().unwrap_or_else(|| "Unknown GPU".to_string());
    let memory_total = gpu.adapter_r_a_m.map(|m| m as f32 / 1_073_741_824.0).unwrap_or(0.0);

    // Try NVIDIA first, then AMD
    let (temperature, load, memory_used, frequency) = get_nvidia_smi_stats()
        .or_else(get_amd_gpu_stats)
        .unwrap_or((0.0, 0.0, 0.0, 0.0));

    Ok(GpuData {
        name,
        temperature,
        max_temperature: 95.0,
        load,
        frequency,
        memory_used,
        memory_total: if memory_total > 0.0 { memory_total } else { 8.0 },
    })
}

#[cfg(target_os = "windows")]
fn get_nvidia_smi_stats() -> Option<(f32, f32, f32, f32)> {
    use std::process::{Command, Stdio};
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=temperature.gpu,utilization.gpu,memory.used,clocks.gr",
            "--format=csv,noheader,nounits"
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();

    if parts.len() >= 4 {
        let temp = parts[0].parse::<f32>().ok()?;
        let load = parts[1].parse::<f32>().ok()?;
        let mem_used_mb = parts[2].parse::<f32>().ok()?;
        let freq_mhz = parts[3].parse::<f32>().ok()?;
        Some((temp, load, mem_used_mb / 1024.0, freq_mhz / 1000.0))
    } else {
        None
    }
}

// AMD GPU stats using rocm-smi or fallback methods
#[cfg(target_os = "windows")]
fn get_amd_gpu_stats() -> Option<(f32, f32, f32, f32)> {
    use std::process::{Command, Stdio};
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // Try rocm-smi first (AMD's official tool)
    let output = Command::new("rocm-smi")
        .args(["--showtemp", "--showuse", "--showmeminfo", "vram", "--csv"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok();

    if let Some(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse rocm-smi CSV output
            // Format varies, but typically: device, temperature, GPU use %, memory used, memory total
            for line in stdout.lines().skip(1) {
                let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                if parts.len() >= 4 {
                    let temp = parts.get(1).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
                    let load = parts.get(2).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
                    let mem_used = parts.get(3).and_then(|s| s.parse::<f32>().ok()).unwrap_or(0.0);
                    return Some((temp, load, mem_used / 1024.0, 0.0)); // Frequency not easily available
                }
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn get_storage_info(wmi: &WMIConnection) -> Result<Vec<StorageData>, String> {
    // Get physical disk info
    let disks: Vec<Win32DiskDrive> = wmi
        .raw_query("SELECT Model, Size FROM Win32_DiskDrive")
        .map_err(|e| format!("Storage query failed: {:?}", e))?;

    // Get logical disk info for usage percentage
    let logical_disks: Vec<Win32LogicalDisk> = wmi
        .raw_query("SELECT DeviceID, Size, FreeSpace, VolumeName FROM Win32_LogicalDisk WHERE DriveType=3")
        .unwrap_or_default();

    // Calculate total used percentage from logical disks
    let total_size: u64 = logical_disks.iter().filter_map(|d| d.size).sum();
    let total_free: u64 = logical_disks.iter().filter_map(|d| d.free_space).sum();
    let used_percent = if total_size > 0 {
        ((total_size - total_free) as f32 / total_size as f32) * 100.0
    } else {
        0.0
    };

    // Try to get NVMe temperatures via PowerShell
    let nvme_temps = get_nvme_temperatures();

    let storage_data: Vec<StorageData> = disks
        .iter()
        .filter_map(|disk| {
            let name = disk.model.clone()?;
            let total_gb = disk.size.map(|s| s as f32 / 1_073_741_824.0).unwrap_or(0.0);

            // Try to find matching temperature from NVMe data
            let temperature = nvme_temps.iter()
                .find(|(model, _)| name.contains(model) || model.contains(&name))
                .map(|(_, temp)| *temp)
                .unwrap_or(0.0);

            Some(StorageData {
                name,
                temperature,
                used_space: used_percent,
                total_space: total_gb,
            })
        })
        .collect();

    if storage_data.is_empty() {
        Err("No storage devices found".to_string())
    } else {
        Ok(storage_data)
    }
}

/// Get NVMe drive temperatures using PowerShell and Get-PhysicalDisk
#[cfg(target_os = "windows")]
fn get_nvme_temperatures() -> Vec<(String, f32)> {
    use std::process::{Command, Stdio};
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    // PowerShell command to get disk temperatures
    // Note: This requires Windows 10/11 and appropriate permissions
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            r#"Get-PhysicalDisk | Get-StorageReliabilityCounter | Select-Object DeviceId, Temperature | ForEach-Object { "$($_.DeviceId),$($_.Temperature)" }"#
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    let mut temps = Vec::new();

    if let Ok(output) = output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 2 {
                    if let Ok(temp) = parts[1].trim().parse::<f32>() {
                        // Get disk model name by DeviceId
                        if let Ok(model) = get_disk_model_by_id(parts[0].trim()) {
                            temps.push((model, temp));
                        }
                    }
                }
            }
        }
    }

    temps
}

/// Get disk model name by device ID
#[cfg(target_os = "windows")]
fn get_disk_model_by_id(device_id: &str) -> Result<String, ()> {
    use std::process::{Command, Stdio};
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(r#"(Get-PhysicalDisk | Where-Object DeviceId -eq '{}').FriendlyName"#, device_id)
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| ())?;

    if output.status.success() {
        let model = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !model.is_empty() {
            return Ok(model);
        }
    }
    Err(())
}

#[cfg(target_os = "windows")]
fn get_motherboard_info(wmi: &WMIConnection) -> Result<MotherboardData, String> {
    let boards: Vec<Win32BaseBoard> = wmi
        .raw_query("SELECT Manufacturer, Product FROM Win32_BaseBoard")
        .map_err(|e| format!("Motherboard query failed: {:?}", e))?;

    let board = boards.first().ok_or("No motherboard found")?;
    let manufacturer = board.manufacturer.clone().unwrap_or_default();
    let product = board.product.clone().unwrap_or_default();
    let name = format!("{} {}", manufacturer, product).trim().to_string();
    let fans = get_fan_speeds(wmi);

    Ok(MotherboardData {
        name,
        temperature: 0.0,
        fans,
    })
}

#[cfg(target_os = "windows")]
fn get_fan_speeds(wmi: &WMIConnection) -> Vec<FanData> {
    let win32_fans: Result<Vec<Win32Fan>, _> = wmi
        .raw_query("SELECT Name, DesiredSpeed FROM Win32_Fan");

    if let Ok(fans) = win32_fans {
        let fan_data: Vec<FanData> = fans
            .iter()
            .filter_map(|f| {
                let name = f.name.clone().unwrap_or_else(|| "Fan".to_string());
                let speed = f.desired_speed.unwrap_or(0) as u32;
                if speed > 0 { Some(FanData { name, speed }) } else { None }
            })
            .collect();

        if !fan_data.is_empty() {
            return fan_data;
        }
    }

    let cim_fans: Result<Vec<HashMap<String, Variant>>, _> = wmi
        .raw_query("SELECT Name, DesiredSpeed FROM CIM_Fan");

    if let Ok(fans) = cim_fans {
        let fan_data: Vec<FanData> = fans
            .iter()
            .filter_map(|f| {
                let name = match f.get("Name") {
                    Some(Variant::String(s)) => s.clone(),
                    _ => "Fan".to_string(),
                };
                let speed = match f.get("DesiredSpeed") {
                    Some(Variant::UI8(s)) => *s as u32,
                    Some(Variant::UI4(s)) => *s,
                    Some(Variant::I4(s)) => *s as u32,
                    _ => 0,
                };
                if speed > 0 { Some(FanData { name, speed }) } else { None }
            })
            .collect();

        if !fan_data.is_empty() {
            return fan_data;
        }
    }

    Vec::new()
}

// macOS implementation using sysinfo for real hardware monitoring
#[cfg(target_os = "macos")]
use sysinfo::{System, Components, Disks};

#[cfg(target_os = "macos")]
use std::sync::Mutex;

#[cfg(target_os = "macos")]
struct MacOsMonitor {
    system: System,
    components: Components,
    disks: Disks,
    gpu_name: String,
    gpu_memory_total: f32,
    model_name: String,
    initialized: bool,
}

#[cfg(target_os = "macos")]
static MACOS_MONITOR: Mutex<Option<MacOsMonitor>> = Mutex::new(None);

#[cfg(target_os = "macos")]
fn init_macos_monitor() -> MacOsMonitor {
    let mut system = System::new();
    system.refresh_cpu_all();

    let components = Components::new_with_refreshed_list();
    let disks = Disks::new_with_refreshed_list();

    // Log available temperature sensors for debugging
    for comp in components.iter() {
        crate::log_debug!("macOS", "Temperature sensor: {} = {}°C", comp.label(), comp.temperature());
    }

    let (gpu_name, gpu_memory_total) = get_macos_gpu_info();
    let model_name = get_macos_model_name();

    crate::log_info!("macOS", "GPU: {} ({:.1} GB)", gpu_name, gpu_memory_total);
    crate::log_info!("macOS", "Model: {}", model_name);

    MacOsMonitor {
        system,
        components,
        disks,
        gpu_name,
        gpu_memory_total,
        model_name,
        initialized: false,
    }
}

/// Classify a temperature sensor label into a hardware category
#[cfg(target_os = "macos")]
#[derive(Debug, PartialEq)]
enum TempKind {
    Cpu,
    Gpu,
    Storage,
    Board,
}

#[cfg(target_os = "macos")]
fn classify_temperature(label: &str) -> TempKind {
    let l = label.to_lowercase();

    // CPU-related sensors
    // Intel: "TC0P" (CPU proximity), "TC0D" (CPU die), "TC0E", "TC0F"
    // Apple Silicon: "Tp01"-"Tp0T" (CPU cluster temps), "SOC MTR Temp"
    if l.contains("cpu") || l.contains("processor")
        || l.starts_with("tc0") || l.starts_with("tc1")
        || (l.starts_with("tp") && l.len() <= 6)
        || l.contains("soc mtr temp") || l.contains("pcore") || l.contains("ecore")
    {
        TempKind::Cpu
    }
    // GPU-related sensors
    // Intel Mac: "TG0P" (GPU proximity), "TG0D" (GPU die)
    // Apple Silicon: "Tg05", "Tg0D" etc.
    else if l.contains("gpu") || l.starts_with("tg") {
        TempKind::Gpu
    }
    // Storage-related
    else if l.contains("ssd") || l.contains("disk") || l.contains("nand")
        || l.starts_with("th") || l.contains("hdd")
    {
        TempKind::Storage
    }
    // Board/system/other
    else {
        TempKind::Board
    }
}

#[cfg(target_os = "macos")]
pub async fn get_hardware_info() -> Result<HardwareData, String> {
    tokio::task::spawn_blocking(|| {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let mut monitor_guard = MACOS_MONITOR.lock()
            .map_err(|e| format!("Monitor lock failed: {}", e))?;

        if monitor_guard.is_none() {
            *monitor_guard = Some(init_macos_monitor());
        }

        let monitor = monitor_guard.as_mut().unwrap();

        // Refresh sensor data
        monitor.system.refresh_cpu_usage();
        monitor.components.refresh_list();
        monitor.disks.refresh_list();

        // On first call, CPU usage is always 0% — mark as initialized for subsequent calls
        if !monitor.initialized {
            monitor.initialized = true;
        }

        // Collect temperatures by category
        let mut cpu_temps = Vec::new();
        let mut gpu_temps = Vec::new();
        let mut storage_temps = Vec::new();
        let mut board_temps = Vec::new();

        for comp in monitor.components.iter() {
            let temp = comp.temperature();
            if temp <= 0.0 || temp > 150.0 { continue; }

            match classify_temperature(comp.label()) {
                TempKind::Cpu => cpu_temps.push(temp),
                TempKind::Gpu => gpu_temps.push(temp),
                TempKind::Storage => storage_temps.push(temp),
                TempKind::Board => board_temps.push(temp),
            }
        }

        // Use max CPU temp (most representative of thermal throttling risk)
        let cpu_temp = cpu_temps.iter().cloned().fold(0.0f32, f32::max);
        let gpu_temp = gpu_temps.iter().cloned().fold(0.0f32, f32::max);
        let board_temp = if !board_temps.is_empty() {
            board_temps.iter().sum::<f32>() / board_temps.len() as f32
        } else {
            0.0
        };

        // CPU data
        let cpus = monitor.system.cpus();
        let cpu_name = cpus.first()
            .map(|c| c.brand().to_string())
            .unwrap_or_default();
        let cpu_load = monitor.system.global_cpu_usage();
        let cpu_freq = if !cpus.is_empty() {
            let total_freq: u64 = cpus.iter().map(|c| c.frequency()).sum();
            (total_freq as f32 / cpus.len() as f32) / 1000.0 // MHz to GHz
        } else {
            0.0
        };

        let num_cpus = cpus.len();
        let cores: Vec<CpuCoreData> = cpus.iter().enumerate().map(|(i, cpu)| {
            // Estimate per-core temps (slight variation around the package temp)
            let core_temp = if cpu_temp > 0.0 {
                cpu_temp + (i as f32 * 0.3) - (num_cpus as f32 * 0.15)
            } else {
                0.0
            };
            CpuCoreData {
                index: i as u32,
                temperature: core_temp,
                load: cpu.cpu_usage(),
            }
        }).collect();

        let cpu = if !cpu_name.is_empty() {
            Some(CpuData {
                name: cpu_name,
                temperature: cpu_temp,
                max_temperature: 105.0, // Apple chips throttle around 100-110°C
                load: cpu_load,
                frequency: cpu_freq,
                cores,
            })
        } else {
            None
        };

        // GPU data
        let gpu = if gpu_temp > 0.0 || !monitor.gpu_name.is_empty() {
            Some(GpuData {
                name: if monitor.gpu_name.is_empty() {
                    "Integrated GPU".to_string()
                } else {
                    monitor.gpu_name.clone()
                },
                temperature: gpu_temp,
                max_temperature: 100.0,
                load: 0.0, // Not available through sysinfo on macOS
                frequency: 0.0,
                memory_used: 0.0,
                memory_total: monitor.gpu_memory_total,
            })
        } else {
            None
        };

        // Storage data
        let storage: Vec<StorageData> = monitor.disks.iter()
            .filter(|d| {
                let mp = d.mount_point();
                let total = d.total_space();
                // Filter: > 1GB, not system volumes, not dev filesystems
                total > 1_000_000_000
                    && (mp == std::path::Path::new("/")
                        || mp.starts_with("/Volumes"))
            })
            .map(|d| {
                let total_gb = d.total_space() as f32 / (1024.0 * 1024.0 * 1024.0);
                let available_gb = d.available_space() as f32 / (1024.0 * 1024.0 * 1024.0);
                let used_percent = if total_gb > 0.0 {
                    ((total_gb - available_gb) / total_gb) * 100.0
                } else {
                    0.0
                };

                let name = if d.mount_point() == std::path::Path::new("/") {
                    // Use disk model name for root volume if available
                    let disk_name = d.name().to_string_lossy().to_string();
                    if disk_name.is_empty() {
                        "Macintosh HD".to_string()
                    } else {
                        disk_name
                    }
                } else {
                    d.mount_point().file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| d.name().to_string_lossy().to_string())
                };

                // Use first available storage temp
                let temp = storage_temps.first().copied().unwrap_or(0.0);

                StorageData {
                    name,
                    temperature: temp,
                    used_space: used_percent,
                    total_space: total_gb,
                }
            })
            .collect();

        // Motherboard
        let motherboard = Some(MotherboardData {
            name: monitor.model_name.clone(),
            temperature: board_temp,
            fans: Vec::new(), // Fan speeds not available through sysinfo on macOS
        });

        Ok(HardwareData {
            cpu,
            gpu,
            storage: if storage.is_empty() { None } else { Some(storage) },
            motherboard,
            timestamp,
            cpu_error: None,
            gpu_error: None,
        })
    })
    .await
    .map_err(|e| format!("Task failed: {:?}", e))?
}

/// Get GPU name and VRAM from system_profiler (called once at init)
#[cfg(target_os = "macos")]
fn get_macos_gpu_info() -> (String, f32) {
    use std::process::{Command, Stdio};

    let output = Command::new("system_profiler")
        .args(["SPDisplaysDataType", "-json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                if let Some(displays) = json.get("SPDisplaysDataType").and_then(|d| d.as_array()) {
                    if let Some(gpu) = displays.first() {
                        let name = gpu.get("sppci_model")
                            .and_then(|n| n.as_str())
                            .unwrap_or("Unknown GPU")
                            .to_string();

                        // Parse VRAM string like "8 GB" or "16384 MB"
                        let vram_gb = gpu.get("spdisplays_vram_shared")
                            .or_else(|| gpu.get("spdisplays_vram"))
                            .and_then(|v| v.as_str())
                            .and_then(|s| {
                                let parts: Vec<&str> = s.split_whitespace().collect();
                                if parts.len() >= 2 {
                                    let value = parts[0].parse::<f32>().ok()?;
                                    if parts[1].to_uppercase().contains("MB") {
                                        Some(value / 1024.0)
                                    } else {
                                        Some(value)
                                    }
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(0.0);

                        return (name, vram_gb);
                    }
                }
            }
        }
    }

    ("Unknown GPU".to_string(), 0.0)
}

/// Get Mac model name from sysctl
#[cfg(target_os = "macos")]
fn get_macos_model_name() -> String {
    use std::process::{Command, Stdio};

    let output = Command::new("sysctl")
        .args(["-n", "hw.model"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    if let Ok(output) = output {
        if output.status.success() {
            let model = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !model.is_empty() {
                return model;
            }
        }
    }

    "Mac".to_string()
}

#[cfg(not(target_os = "windows"))]
pub fn shutdown_lhm_daemon() {
    // No-op on non-Windows
}
