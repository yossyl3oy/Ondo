use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::Mutex;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMetrics {
    pub current_pid: u32,
    pub total_system_process_count: usize,
    pub app_process_count: usize,
    pub app_memory_bytes: u64,
    pub app_memory_mb: f64,
    pub app_virtual_memory_bytes: u64,
    pub app_cpu_percent: f32,
    pub processes: Vec<ProcessMetrics>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetrics {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub memory_bytes: u64,
    pub memory_mb: f64,
    pub virtual_memory_bytes: u64,
    pub cpu_percent: f32,
    pub run_time_seconds: u64,
}

struct MetricsState {
    system: System,
}

static METRICS_STATE: Lazy<Mutex<MetricsState>> = Lazy::new(|| {
    Mutex::new(MetricsState {
        system: System::new(),
    })
});

pub fn snapshot(extra_pids: &[u32]) -> Result<RuntimeMetrics, String> {
    let current_pid = Pid::from_u32(std::process::id());
    let mut state = METRICS_STATE.lock().map_err(|e| e.to_string())?;

    state.system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().without_tasks(),
    );

    let app_pids = collect_app_pids(&state.system, current_pid, extra_pids);
    let app_pid_vec: Vec<Pid> = app_pids.iter().copied().collect();

    state.system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&app_pid_vec),
        true,
        ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .without_tasks(),
    );

    let processes: Vec<ProcessMetrics> = app_pid_vec
        .iter()
        .filter_map(|pid| state.system.process(*pid).map(process_metrics))
        .collect();

    let app_memory_bytes = processes.iter().map(|p| p.memory_bytes).sum();
    let app_virtual_memory_bytes = processes.iter().map(|p| p.virtual_memory_bytes).sum();
    let app_cpu_percent = processes.iter().map(|p| p.cpu_percent).sum();

    Ok(RuntimeMetrics {
        current_pid: current_pid.as_u32(),
        total_system_process_count: state.system.processes().len(),
        app_process_count: processes.len(),
        app_memory_bytes,
        app_memory_mb: bytes_to_mb(app_memory_bytes),
        app_virtual_memory_bytes,
        app_cpu_percent,
        processes,
    })
}

fn collect_app_pids(system: &System, current_pid: Pid, extra_pids: &[u32]) -> BTreeSet<Pid> {
    let mut pids = BTreeSet::from([current_pid]);

    for process in system.processes().values() {
        if is_descendant_of(system, process.pid(), current_pid) {
            pids.insert(process.pid());
        }
    }

    for pid in extra_pids {
        pids.insert(Pid::from_u32(*pid));
    }

    pids
}

fn is_descendant_of(system: &System, pid: Pid, ancestor: Pid) -> bool {
    let mut next = system.process(pid).and_then(|process| process.parent());
    while let Some(parent) = next {
        if parent == ancestor {
            return true;
        }
        next = system.process(parent).and_then(|process| process.parent());
    }
    false
}

fn process_metrics(process: &sysinfo::Process) -> ProcessMetrics {
    let memory_bytes = process.memory();

    ProcessMetrics {
        pid: process.pid().as_u32(),
        parent_pid: process.parent().map(|pid| pid.as_u32()),
        name: process.name().to_string_lossy().to_string(),
        memory_bytes,
        memory_mb: bytes_to_mb(memory_bytes),
        virtual_memory_bytes: process.virtual_memory(),
        cpu_percent: process.cpu_usage(),
        run_time_seconds: process.run_time(),
    }
}

fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

#[cfg(test)]
mod tests {
    use super::bytes_to_mb;

    #[test]
    fn converts_bytes_to_mebibytes() {
        assert_eq!(bytes_to_mb(0), 0.0);
        assert_eq!(bytes_to_mb(1024 * 1024), 1.0);
        assert_eq!(bytes_to_mb(1536 * 1024), 1.5);
    }
}
