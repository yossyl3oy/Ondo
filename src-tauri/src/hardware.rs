use crate::{CpuCoreData, CpuData, GpuData, HardwareData, StorageData, MotherboardData, FanData};
use std::time::{SystemTime, UNIX_EPOCH};

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

        if let Some(lhm) = lhm_data {
            // Use LHM data
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

            let storage = lhm.storage.map(|storages| {
                storages.into_iter().map(|s| StorageData {
                    name: s.name,
                    temperature: s.temperature,
                    used_space: s.used_percent,
                    total_space: s.total_space,
                }).collect()
            });

            let motherboard = lhm.motherboard.map(|m| MotherboardData {
                name: m.name,
                temperature: m.temperature,
                fans: m.fans.map(|fans| {
                    fans.into_iter().map(|f| FanData {
                        name: f.name,
                        speed: f.speed,
                    }).collect()
                }).unwrap_or_default(),
            });

            Ok(HardwareData {
                cpu,
                gpu,
                storage,
                motherboard,
                timestamp,
                cpu_error: None,
                gpu_error: None,
            })
        } else {
            // Fallback to WMI
            get_hardware_from_wmi(timestamp)
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
                eprintln!("[Hardware] Failed to start LHM daemon: {}", e);
                return None;
            }
        }
    }

    let daemon = daemon_guard.as_mut()?;

    // Check if process is still running
    match daemon.process.try_wait() {
        Ok(Some(status)) => {
            // Process exited, restart it
            eprintln!("[Hardware] LHM daemon exited with status: {}, restarting...", status);
            *daemon_guard = None;
            return None;
        }
        Ok(None) => {
            // Process still running, read latest line
        }
        Err(e) => {
            eprintln!("[Hardware] Failed to check LHM daemon status: {}", e);
            *daemon_guard = None;
            return None;
        }
    }

    // Read all available lines and keep the latest one
    let mut latest_line = None;
    loop {
        let mut line = String::new();
        match daemon.reader.read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    latest_line = Some(trimmed.to_string());
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => {
                eprintln!("[Hardware] Failed to read from LHM daemon: {}", e);
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
                eprintln!("[Hardware] Failed to parse LHM JSON: {}", e);
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

    // Set stdout to non-blocking mode
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::System::Pipes::SetNamedPipeHandleState;
        use windows::Win32::Storage::FileSystem::PIPE_NOWAIT;
        use windows::Win32::Foundation::HANDLE;

        unsafe {
            let handle = HANDLE(stdout.as_raw_handle());
            let mut mode = PIPE_NOWAIT;
            let _ = SetNamedPipeHandleState(handle, Some(&mut mode), None, None);
        }
    }

    let reader = BufReader::new(stdout);

    eprintln!("[Hardware] LHM daemon started successfully");

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
            eprintln!("[Hardware] LHM daemon stopped");
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
        Err(e) => (None, Some(e)),
    };

    let (gpu, gpu_error) = match get_gpu_info(&wmi) {
        Ok(data) => (Some(data), None),
        Err(e) => (None, Some(e)),
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
    let (temperature, load, memory_used, frequency) = get_nvidia_smi_stats().unwrap_or((0.0, 0.0, 0.0, 0.0));

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

    // CREATE_NO_WINDOW flag to prevent console window from appearing
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

#[cfg(target_os = "windows")]
fn get_storage_info(wmi: &WMIConnection) -> Result<Vec<StorageData>, String> {
    let disks: Vec<Win32DiskDrive> = wmi
        .raw_query("SELECT Model, Size FROM Win32_DiskDrive")
        .map_err(|e| format!("Storage query failed: {:?}", e))?;

    let storage_data: Vec<StorageData> = disks
        .iter()
        .filter_map(|disk| {
            let name = disk.model.clone()?;
            let total_gb = disk.size.map(|s| s as f32 / 1_073_741_824.0).unwrap_or(0.0);
            Some(StorageData {
                name,
                temperature: 0.0,
                used_space: 0.0,
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

// Non-Windows fallback implementation
#[cfg(not(target_os = "windows"))]
pub async fn get_hardware_info() -> Result<HardwareData, String> {
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
            frequency: 3.7 + (rand_float() * 1.0),
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
            frequency: 1.7 + (rand_float() * 0.5),
            memory_used: 4.0 + (rand_float() * 4.0),
            memory_total: 10.0,
        }),
        storage: Some(vec![
            StorageData {
                name: "Samsung SSD 980 PRO 1TB".to_string(),
                temperature: 35.0 + (rand_float() * 10.0),
                used_space: 500.0,
                total_space: 1000.0,
            },
        ]),
        motherboard: Some(MotherboardData {
            name: "ASUS ROG STRIX B550-F".to_string(),
            temperature: 40.0 + (rand_float() * 15.0),
            fans: vec![
                FanData {
                    name: "CPU Fan".to_string(),
                    speed: 1200 + (rand_float() * 500.0) as u32,
                },
                FanData {
                    name: "Chassis Fan 1".to_string(),
                    speed: 800 + (rand_float() * 300.0) as u32,
                },
            ],
        }),
        timestamp,
        cpu_error: None,
        gpu_error: None,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn shutdown_lhm_daemon() {
    // No-op on non-Windows
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
