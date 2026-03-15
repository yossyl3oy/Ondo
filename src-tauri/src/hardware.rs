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

        // Get sysinfo data for fallback/supplement (no WMI)
        let sysinfo_data = get_fallback_from_sysinfo(timestamp);

        if let Some(lhm) = lhm_data {
            // Use LHM data, supplement with sysinfo where needed
            let cpu = lhm.cpu.map(|c| {
                // sysinfo cannot provide CPU temperature on Windows, so use LHM value (0 if unavailable)
                let temperature = c.temperature;

                // If LHM doesn't have CPU frequency, try sysinfo fallback
                let frequency = if c.frequency > 0.0 {
                    c.frequency
                } else {
                    sysinfo_data.cpu.as_ref()
                        .map(|cpu| cpu.frequency)
                        .unwrap_or(0.0)
                };

                CpuData {
                    name: c.name,
                    temperature,
                    max_temperature: c.max_temperature,
                    load: c.load,
                    frequency,
                    cores: c.cores.map(|cores| {
                        cores.into_iter().map(|core| {
                            // If per-core temperature is 0, use the package temperature
                            let core_temp = if core.temperature > 0.0 {
                                core.temperature
                            } else {
                                temperature
                            };
                            CpuCoreData {
                                index: core.index,
                                temperature: core_temp,
                                load: core.load,
                            }
                        }).collect()
                    }).unwrap_or_default(),
                }
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

            // For storage: use LHM data, supplement with sysinfo if LHM data is incomplete
            let storage = lhm.storage.map(|storages| {
                let sysinfo_storage = sysinfo_data.storage.as_ref();
                storages.into_iter().map(|s| {
                    // Find matching sysinfo storage for supplementing missing data
                    let sysinfo_match = sysinfo_storage.and_then(|ss| {
                        ss.iter().find(|si| {
                            si.name.contains(&s.name) || s.name.contains(&si.name) ||
                            si.name.split_whitespace().next().map(|first| s.name.contains(first)).unwrap_or(false)
                        })
                    });

                    let total_space = if s.total_space > 0.0 {
                        s.total_space
                    } else {
                        sysinfo_match.map(|si| si.total_space).unwrap_or(0.0)
                    };

                    let used_space = if s.used_percent > 0.0 {
                        s.used_percent
                    } else {
                        sysinfo_match.map(|si| si.used_space).unwrap_or(0.0)
                    };

                    StorageData {
                        name: s.name,
                        temperature: s.temperature,
                        used_space,
                        total_space,
                    }
                }).collect()
            });

            // For motherboard: use LHM data only (sysinfo cannot provide this)
            let motherboard = lhm.motherboard.map(|m| {
                let fans = m.fans.map(|fans| {
                    fans.into_iter().map(|f| FanData {
                        name: f.name,
                        speed: f.speed,
                    }).collect()
                }).unwrap_or_default();
                MotherboardData {
                    name: m.name,
                    temperature: m.temperature,
                    fans,
                }
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
            // Full fallback to sysinfo (LHM not available)
            crate::log_warn!("Hardware", "LHM unavailable, using sysinfo fallback");
            Ok(HardwareData {
                cpu: sysinfo_data.cpu,
                gpu: sysinfo_data.gpu,
                storage: sysinfo_data.storage,
                motherboard: None,
                timestamp,
                cpu_error: None,
                gpu_error: None,
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

    let mut reader = BufReader::new(stdout);

    crate::log_info!("Hardware", "LHM daemon started, waiting for first data...");

    // Wait for the first line of data from the daemon (up to 10 seconds)
    let mut initial_data = None;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        let mut line = String::new();
        // Use a short sleep + peek approach to avoid blocking forever
        {
            use std::os::windows::io::AsRawHandle;
            use windows::Win32::System::Pipes::PeekNamedPipe;
            use windows::Win32::Foundation::HANDLE;

            let handle = HANDLE(reader.get_ref().as_raw_handle() as *mut std::ffi::c_void);
            let mut bytes_available: u32 = 0;
            let ok = unsafe {
                PeekNamedPipe(handle, None, 0, None, Some(&mut bytes_available), None)
            };
            if !ok.is_ok() || bytes_available == 0 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
        }

        match reader.read_line(&mut line) {
            Ok(0) => break, // EOF - daemon exited
            Ok(_) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    match serde_json::from_str::<LhmResponse>(trimmed) {
                        Ok(data) => {
                            crate::log_info!("Hardware", "LHM daemon ready - first data received");
                            initial_data = Some(data);
                            break;
                        }
                        Err(e) => {
                            crate::log_warn!("Hardware", "LHM initial data parse error: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                crate::log_error!("Hardware", "Failed to read initial LHM data: {}", e);
                break;
            }
        }
    }

    if initial_data.is_none() {
        crate::log_warn!("Hardware", "LHM daemon started but no initial data received within timeout");
    }

    Ok(LhmDaemon {
        process: child,
        reader,
        latest_data: initial_data,
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

// sysinfo for hardware data (replaces WMI on Windows to avoid application control policy blocks)
use sysinfo::{System, Disks};

/// Fallback data source using sysinfo crate (no WMI dependency)
#[cfg(target_os = "windows")]
struct SysinfoFallback {
    cpu: Option<CpuData>,
    gpu: Option<GpuData>,
    storage: Option<Vec<StorageData>>,
}

#[cfg(target_os = "windows")]
fn get_fallback_from_sysinfo(timestamp: u64) -> SysinfoFallback {
    let mut sys = System::new();
    sys.refresh_cpu_all();
    // Brief pause then refresh again for accurate CPU usage readings
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_all();

    let cpus = sys.cpus();

    let cpu = if !cpus.is_empty() {
        let name = cpus[0].brand().to_string();
        let total_load: f32 = cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32;
        let avg_freq = cpus.iter().map(|c| c.frequency()).sum::<u64>() as f32 / cpus.len() as f32 / 1000.0;

        let cores: Vec<CpuCoreData> = cpus.iter().enumerate().map(|(i, c)| {
            CpuCoreData {
                index: i as u32,
                temperature: 0.0, // sysinfo does not provide CPU temperature on Windows
                load: c.cpu_usage(),
            }
        }).collect();

        Some(CpuData {
            name,
            temperature: 0.0, // Not available via sysinfo on Windows
            max_temperature: 100.0,
            load: total_load,
            frequency: avg_freq,
            cores,
        })
    } else {
        None
    };

    // GPU: use nvidia-smi / rocm-smi (no WMI)
    let gpu = get_gpu_info_without_wmi();

    // Storage: use sysinfo Disks
    let disks = Disks::new_with_refreshed_list();
    let storage_data: Vec<StorageData> = disks.iter()
        .filter(|d| d.total_space() > 1_073_741_824) // > 1GB
        .map(|d| {
            let total_gb = d.total_space() as f32 / 1_073_741_824.0;
            let used_gb = (d.total_space() - d.available_space()) as f32 / 1_073_741_824.0;
            let used_percent = if total_gb > 0.0 { (used_gb / total_gb) * 100.0 } else { 0.0 };
            let name = d.name().to_string_lossy().to_string();
            let name = if name.is_empty() {
                d.mount_point().to_string_lossy().to_string()
            } else {
                name
            };
            StorageData {
                name,
                temperature: 0.0,
                used_space: used_percent,
                total_space: total_gb,
            }
        })
        .collect();

    let storage = if storage_data.is_empty() { None } else { Some(storage_data) };

    SysinfoFallback { cpu, gpu, storage }
}

/// Get GPU info without WMI - uses nvidia-smi / rocm-smi CLI tools
#[cfg(target_os = "windows")]
fn get_gpu_info_without_wmi() -> Option<GpuData> {
    // Try NVIDIA first, then AMD
    let (temperature, load, memory_used, frequency) = get_nvidia_smi_stats()
        .or_else(get_amd_gpu_stats)?;

    // Get GPU name from nvidia-smi if available
    let name = get_nvidia_gpu_name().unwrap_or_else(|| "Unknown GPU".to_string());

    Some(GpuData {
        name,
        temperature,
        max_temperature: 95.0,
        load,
        frequency,
        memory_used,
        memory_total: get_nvidia_memory_total().unwrap_or(8.0),
    })
}

/// Get NVIDIA GPU name via nvidia-smi
#[cfg(target_os = "windows")]
fn get_nvidia_gpu_name() -> Option<String> {
    use std::process::{Command, Stdio};
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader,nounits"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !name.is_empty() { return Some(name); }
    }
    None
}

/// Get NVIDIA GPU total memory via nvidia-smi
#[cfg(target_os = "windows")]
fn get_nvidia_memory_total() -> Option<f32> {
    use std::process::{Command, Stdio};
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mb = stdout.trim().parse::<f32>().ok()?;
        return Some(mb / 1024.0); // MB to GB
    }
    None
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

// macOS implementation using sysinfo for real hardware monitoring
#[cfg(target_os = "macos")]
use sysinfo::Components;

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
