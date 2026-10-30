//! Spots hardware readings that are missing or wrong in what the app shows,
//! so they reach Sentry instead of only showing up as "N/A" on someone's
//! desk. The monitor is pure (time is passed in); `check` wires it to the
//! real clock for the `get_hardware_data` command.

use crate::HardwareData;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Instant;

/// A reading missing (or implausible) this long after it had been fine is
/// reported. The sidecar's GPU watchdog rebuilds the GPU devices after 10 s
/// and again 20 s later, so a lost GPU temperature is only reported when
/// the first rebuild didn't bring it back.
const LOST_AFTER_MS: u64 = 30_000;
/// Readings expected from the start that haven't shown up in this long.
const NEVER_AFTER_MS: u64 = 60_000;
/// A longer gap between two observations (sleep, paused polling) is not
/// counted, and restarts every timer.
const MAX_GAP_MS: u64 = 10_000;
/// No sensor the HUD shows gets anywhere near this; values above it are
/// garbage readings (e.g. 127 / 255 from an unconnected sensor).
const MAX_PLAUSIBLE_TEMPERATURE: f32 = 150.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reading {
    Cpu,
    CpuTemperature,
    Gpu,
    GpuTemperature,
    Motherboard,
    MotherboardTemperature,
    Storage,
    StorageTemperature,
}

impl Reading {
    pub fn key(self) -> &'static str {
        match self {
            Reading::Cpu => "cpu",
            Reading::CpuTemperature => "cpu.temperature",
            Reading::Gpu => "gpu",
            Reading::GpuTemperature => "gpu.temperature",
            Reading::Motherboard => "motherboard",
            Reading::MotherboardTemperature => "motherboard.temperature",
            Reading::Storage => "storage",
            Reading::StorageTemperature => "storage.temperature",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Reading::Cpu => "CPU data",
            Reading::CpuTemperature => "CPU temperature",
            Reading::Gpu => "GPU data",
            Reading::GpuTemperature => "GPU temperature",
            Reading::Motherboard => "Motherboard data",
            Reading::MotherboardTemperature => "Motherboard temperature",
            Reading::Storage => "Storage data",
            Reading::StorageTemperature => "Storage temperature",
        }
    }

    /// Readings every supported machine should have. Motherboard and drive
    /// sensors are often legitimately absent, so for those only losing a
    /// reading that had worked is reported.
    fn expected_from_start(self) -> bool {
        matches!(
            self,
            Reading::Cpu | Reading::CpuTemperature | Reading::Gpu | Reading::GpuTemperature
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnomalyKind {
    /// Was available this session, then went missing
    Lost,
    /// Never showed up since the app started
    NeverAvailable,
    /// Reports a value no real sensor gives
    Implausible,
}

impl AnomalyKind {
    pub fn key(self) -> &'static str {
        match self {
            AnomalyKind::Lost => "lost",
            AnomalyKind::NeverAvailable => "never_available",
            AnomalyKind::Implausible => "implausible",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Anomaly {
    pub reading: Reading,
    pub kind: AnomalyKind,
    /// The device, for per-device readings (drive name)
    pub subject: Option<String>,
    /// The offending value, for implausible readings
    pub value: Option<f32>,
}

impl Anomaly {
    /// Stable per reading and kind (no device names or values), so Sentry
    /// groups every occurrence into one issue.
    pub fn title(&self) -> String {
        let label = self.reading.label();
        match self.kind {
            AnomalyKind::Lost => format!("{} stopped reporting", label),
            AnomalyKind::NeverAvailable => format!("{} not available since startup", label),
            AnomalyKind::Implausible => format!("{} reports an implausible value", label),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Status {
    Ok,
    Missing,
    Implausible(f32),
}

fn temperature_status(celsius: f32) -> Status {
    if celsius <= 0.0 {
        // The backends use 0 for "no reading"
        Status::Missing
    } else if celsius > MAX_PLAUSIBLE_TEMPERATURE {
        Status::Implausible(celsius)
    } else {
        Status::Ok
    }
}

fn presence(present: bool) -> Status {
    if present {
        Status::Ok
    } else {
        Status::Missing
    }
}

/// One reading in a snapshot. `key` tells devices apart and never leaves
/// the process; `subject` is what a report says about the device.
struct Observed {
    reading: Reading,
    key: Option<String>,
    subject: Option<String>,
    status: Status,
}

fn whole(reading: Reading, status: Status) -> Observed {
    Observed {
        reading,
        key: None,
        subject: None,
        status,
    }
}

/// The readings in one snapshot. A section's own readings are only listed
/// while the section exists, so a lost GPU is one report, not two.
fn readings(data: &HardwareData) -> Vec<Observed> {
    let mut out = vec![whole(Reading::Cpu, presence(data.cpu.is_some()))];
    if let Some(cpu) = &data.cpu {
        out.push(whole(
            Reading::CpuTemperature,
            temperature_status(cpu.temperature),
        ));
    }
    out.push(whole(Reading::Gpu, presence(data.gpu.is_some())));
    if let Some(gpu) = &data.gpu {
        out.push(whole(
            Reading::GpuTemperature,
            temperature_status(gpu.temperature),
        ));
    }
    out.push(whole(
        Reading::Motherboard,
        presence(data.motherboard.is_some()),
    ));
    if let Some(mb) = &data.motherboard {
        out.push(whole(
            Reading::MotherboardTemperature,
            temperature_status(mb.temperature),
        ));
    }
    let drives = data.storage.as_deref().unwrap_or_default();
    out.push(whole(Reading::Storage, presence(!drives.is_empty())));
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for (i, drive) in drives.iter().enumerate() {
        // Identical drives share a model name (LHM), so the nth one of a
        // name is its own device
        let nth = seen.entry(drive.name.as_str()).or_default();
        *nth += 1;
        out.push(Observed {
            reading: Reading::StorageTemperature,
            key: Some(format!("{}#{}", drive.name, nth)),
            subject: Some(drive_label(i, drives.len(), drive.total_space)),
            status: temperature_status(drive.temperature),
        });
    }
    out
}

/// Drive names can be volume names the user chose (macOS, and Windows
/// without LHM), so reports only say which drive and how big.
fn drive_label(index: usize, count: usize, total_gb: f32) -> String {
    format!("drive {} of {} ({:.0} GB)", index + 1, count, total_gb)
}

#[derive(Debug, Default)]
struct Track {
    ever_ok: bool,
    missing_ms: u64,
    implausible_ms: u64,
}

#[derive(Debug, Default)]
pub struct HealthMonitor {
    last_at_ms: Option<u64>,
    tracks: HashMap<(Reading, Option<String>), Track>,
    reported: HashSet<(Reading, Option<String>, AnomalyKind)>,
}

impl HealthMonitor {
    /// Records one snapshot taken at `now_ms` (any monotonic clock) and
    /// returns the anomalies to report. Each anomaly is returned once per
    /// session per device, however long it lasts.
    pub fn observe(&mut self, data: &HardwareData, now_ms: u64) -> Vec<Anomaly> {
        let step = self
            .last_at_ms
            .map_or(0, |last| now_ms.saturating_sub(last));
        self.last_at_ms = Some(now_ms);

        let step = if step > MAX_GAP_MS {
            for track in self.tracks.values_mut() {
                track.missing_ms = 0;
                track.implausible_ms = 0;
            }
            0
        } else {
            step
        };

        let mut anomalies = Vec::new();
        for Observed {
            reading,
            key,
            subject,
            status,
        } in readings(data)
        {
            let track = self.tracks.entry((reading, key.clone())).or_default();
            let found = match status {
                Status::Ok => {
                    track.ever_ok = true;
                    track.missing_ms = 0;
                    track.implausible_ms = 0;
                    None
                }
                Status::Missing => {
                    track.missing_ms += step;
                    track.implausible_ms = 0;
                    if track.ever_ok {
                        (track.missing_ms >= LOST_AFTER_MS).then_some((AnomalyKind::Lost, None))
                    } else {
                        (reading.expected_from_start() && track.missing_ms >= NEVER_AFTER_MS)
                            .then_some((AnomalyKind::NeverAvailable, None))
                    }
                }
                Status::Implausible(value) => {
                    track.implausible_ms += step;
                    track.missing_ms = 0;
                    (track.implausible_ms >= LOST_AFTER_MS)
                        .then_some((AnomalyKind::Implausible, Some(value)))
                }
            };

            if let Some((kind, value)) = found {
                if self.reported.insert((reading, key, kind)) {
                    anomalies.push(Anomaly {
                        reading,
                        kind,
                        subject,
                        value,
                    });
                }
            }
        }
        anomalies
    }
}

static MONITOR: Lazy<Mutex<HealthMonitor>> = Lazy::new(Default::default);
static CLOCK: Lazy<Instant> = Lazy::new(Instant::now);

/// Feeds the snapshot the HUD is about to show into the session's monitor.
pub fn check(data: &HardwareData) -> Vec<Anomaly> {
    let now_ms = CLOCK.elapsed().as_millis() as u64;
    MONITOR
        .lock()
        .map(|mut monitor| monitor.observe(data, now_ms))
        .unwrap_or_default()
}

/// What the HUD showed, for reports. Leaves out network interfaces, the FPS
/// process name and drive names, which say more about the user than the
/// hardware.
pub fn summary(data: &HardwareData) -> String {
    let mut lines = Vec::new();
    match &data.cpu {
        Some(c) => lines.push(format!(
            "cpu: {} temp={:.1} max={:.0} load={:.1} freq={:.2}GHz cores={}",
            c.name,
            c.temperature,
            c.max_temperature,
            c.load,
            c.frequency,
            c.cores.len()
        )),
        None => lines.push(format!("cpu: none (error: {:?})", data.cpu_error)),
    }
    match &data.gpu {
        Some(g) => lines.push(format!(
            "gpu: {} temp={:.1} max={:.0} load={:.1} freq={:.2}GHz mem={:.1}/{:.1}GB",
            g.name,
            g.temperature,
            g.max_temperature,
            g.load,
            g.frequency,
            g.memory_used,
            g.memory_total
        )),
        None => lines.push(format!("gpu: none (error: {:?})", data.gpu_error)),
    }
    match &data.motherboard {
        Some(m) => {
            let fans: Vec<String> = m
                .fans
                .iter()
                .map(|f| format!("{}={}rpm", f.name, f.speed))
                .collect();
            lines.push(format!(
                "motherboard: {} temp={:.1} fans=[{}]",
                m.name,
                m.temperature,
                fans.join(", ")
            ));
        }
        None => lines.push("motherboard: none".to_string()),
    }
    match data.storage.as_deref() {
        Some(drives) if !drives.is_empty() => {
            for (i, d) in drives.iter().enumerate() {
                lines.push(format!(
                    "storage: {} temp={:.1} used={:.1}%",
                    drive_label(i, drives.len(), d.total_space),
                    d.temperature,
                    d.used_space
                ));
            }
        }
        _ => lines.push("storage: none".to_string()),
    }
    if let Some(d) = &data.display {
        lines.push(format!(
            "display: {} {}Hz fps={:?}",
            d.name.as_deref().unwrap_or("?"),
            d.refresh_rate,
            d.fps
        ));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CpuData, GpuData, MotherboardData, StorageData};

    const TICK: u64 = 1_000;

    fn cpu(temperature: f32) -> CpuData {
        CpuData {
            name: "Test CPU".into(),
            temperature,
            max_temperature: 100.0,
            load: 10.0,
            frequency: 4.0,
            cores: Vec::new(),
        }
    }

    fn gpu(temperature: f32) -> GpuData {
        GpuData {
            name: "Test GPU".into(),
            temperature,
            max_temperature: 95.0,
            load: 25.0,
            frequency: 1.0,
            memory_used: 1.0,
            memory_total: 16.0,
        }
    }

    fn drive(name: &str, temperature: f32) -> StorageData {
        StorageData {
            name: name.into(),
            temperature,
            used_space: 40.0,
            total_space: 1024.0,
        }
    }

    /// A healthy machine; tests break one reading at a time.
    fn snapshot() -> HardwareData {
        HardwareData {
            cpu: Some(cpu(50.0)),
            gpu: Some(gpu(40.0)),
            storage: Some(vec![drive("SSD", 40.0)]),
            motherboard: Some(MotherboardData {
                name: "Test Board".into(),
                temperature: 35.0,
                fans: Vec::new(),
            }),
            network: None,
            display: None,
            timestamp: 0,
            cpu_error: None,
            gpu_error: None,
        }
    }

    /// Polls once a second from `*now`, returning every anomaly reported.
    fn poll(
        monitor: &mut HealthMonitor,
        now: &mut u64,
        ticks: u64,
        data: &HardwareData,
    ) -> Vec<Anomaly> {
        let mut found = Vec::new();
        for _ in 0..ticks {
            *now += TICK;
            found.extend(monitor.observe(data, *now));
        }
        found
    }

    fn kinds(anomalies: &[Anomaly]) -> Vec<(Reading, AnomalyKind)> {
        anomalies.iter().map(|a| (a.reading, a.kind)).collect()
    }

    #[test]
    fn a_healthy_machine_reports_nothing() {
        let mut monitor = HealthMonitor::default();
        let mut now = 0;
        assert!(poll(&mut monitor, &mut now, 600, &snapshot()).is_empty());
    }

    #[test]
    fn reports_a_temperature_lost_after_it_worked() {
        let mut monitor = HealthMonitor::default();
        let mut now = 0;
        poll(&mut monitor, &mut now, 5, &snapshot());

        let mut broken = snapshot();
        broken.gpu = Some(gpu(0.0));

        // Not before the threshold...
        assert!(poll(&mut monitor, &mut now, 29, &broken).is_empty());
        // ...then once, however long it lasts
        let found = poll(&mut monitor, &mut now, 300, &broken);
        assert_eq!(
            kinds(&found),
            [(Reading::GpuTemperature, AnomalyKind::Lost)]
        );
        assert_eq!(found[0].title(), "GPU temperature stopped reporting");
    }

    #[test]
    fn a_short_dropout_is_not_reported() {
        let mut monitor = HealthMonitor::default();
        let mut now = 0;
        let mut broken = snapshot();
        broken.gpu = Some(gpu(0.0));

        for _ in 0..10 {
            poll(&mut monitor, &mut now, 5, &snapshot());
            assert!(poll(&mut monitor, &mut now, 20, &broken).is_empty());
        }
    }

    #[test]
    fn reports_expected_readings_that_never_showed_up() {
        let mut monitor = HealthMonitor::default();
        let mut now = 0;
        let mut data = snapshot();
        data.cpu = Some(cpu(0.0));

        assert!(poll(&mut monitor, &mut now, 59, &data).is_empty());
        let found = poll(&mut monitor, &mut now, 300, &data);
        assert_eq!(
            kinds(&found),
            [(Reading::CpuTemperature, AnomalyKind::NeverAvailable)]
        );
    }

    #[test]
    fn optional_readings_that_never_showed_up_are_not_reported() {
        let mut monitor = HealthMonitor::default();
        let mut now = 0;
        let mut data = snapshot();
        data.motherboard = None;
        data.storage = Some(vec![drive("USB stick", 0.0)]);

        assert!(poll(&mut monitor, &mut now, 600, &data).is_empty());
    }

    #[test]
    fn a_lost_section_is_one_report() {
        let mut monitor = HealthMonitor::default();
        let mut now = 0;
        poll(&mut monitor, &mut now, 5, &snapshot());

        let mut broken = snapshot();
        broken.gpu = None;

        let found = poll(&mut monitor, &mut now, 300, &broken);
        assert_eq!(kinds(&found), [(Reading::Gpu, AnomalyKind::Lost)]);
    }

    #[test]
    fn storage_temperatures_are_tracked_per_drive() {
        let mut monitor = HealthMonitor::default();
        let mut now = 0;
        let both = |a: f32, b: f32| HardwareData {
            storage: Some(vec![drive("A", a), drive("B", b)]),
            ..snapshot()
        };
        poll(&mut monitor, &mut now, 5, &both(40.0, 0.0));

        // B never had a temperature: fine. A losing its own is reported.
        let found = poll(&mut monitor, &mut now, 300, &both(0.0, 0.0));
        assert_eq!(
            kinds(&found),
            [(Reading::StorageTemperature, AnomalyKind::Lost)]
        );
        assert_eq!(found[0].subject.as_deref(), Some("drive 1 of 2 (1024 GB)"));
    }

    #[test]
    fn identical_drives_are_tracked_apart() {
        let mut monitor = HealthMonitor::default();
        let mut now = 0;
        let twins = |a: f32, b: f32| HardwareData {
            storage: Some(vec![drive("Same SSD", a), drive("Same SSD", b)]),
            ..snapshot()
        };
        poll(&mut monitor, &mut now, 5, &twins(40.0, 40.0));

        // The healthy twin must not keep resetting the other one's count
        let found = poll(&mut monitor, &mut now, 300, &twins(40.0, 0.0));
        assert_eq!(
            kinds(&found),
            [(Reading::StorageTemperature, AnomalyKind::Lost)]
        );
        assert_eq!(found[0].subject.as_deref(), Some("drive 2 of 2 (1024 GB)"));
    }

    #[test]
    fn an_unplugged_drive_is_not_a_lost_temperature() {
        let mut monitor = HealthMonitor::default();
        let mut now = 0;
        let with_usb = HardwareData {
            storage: Some(vec![drive("SSD", 40.0), drive("USB", 30.0)]),
            ..snapshot()
        };
        poll(&mut monitor, &mut now, 5, &with_usb);

        assert!(poll(&mut monitor, &mut now, 300, &snapshot()).is_empty());
    }

    #[test]
    fn reports_a_sustained_implausible_value_with_the_value() {
        let mut monitor = HealthMonitor::default();
        let mut now = 0;
        let mut data = snapshot();
        data.motherboard.as_mut().unwrap().temperature = 255.0;

        assert!(poll(&mut monitor, &mut now, 29, &data).is_empty());
        let found = poll(&mut monitor, &mut now, 300, &data);
        assert_eq!(
            kinds(&found),
            [(Reading::MotherboardTemperature, AnomalyKind::Implausible)]
        );
        assert_eq!(found[0].value, Some(255.0));
    }

    #[test]
    fn time_across_a_gap_is_not_counted() {
        let mut monitor = HealthMonitor::default();
        let mut now = 0;
        poll(&mut monitor, &mut now, 5, &snapshot());

        let mut broken = snapshot();
        broken.gpu = Some(gpu(0.0));
        poll(&mut monitor, &mut now, 20, &broken);

        // Sleep: the next poll comes an hour later, still without a value
        now += 3_600_000;
        assert!(monitor.observe(&broken, now).is_empty());
        // The 30 s count starts over after the gap
        assert!(poll(&mut monitor, &mut now, 29, &broken).is_empty());
        assert_eq!(poll(&mut monitor, &mut now, 1, &broken).len(), 1);
    }

    #[test]
    fn summary_leaves_out_network_fps_process_and_drive_names() {
        let data = HardwareData {
            storage: Some(vec![drive("Taro's Backup", 30.0)]),
            network: Some(vec![crate::NetworkInterfaceData {
                name: "Home Wi-Fi".into(),
                received_per_sec: 1.0,
                sent_per_sec: 1.0,
            }]),
            display: Some(crate::DisplayData {
                name: Some("Monitor".into()),
                refresh_rate: 144,
                fps: Some(120),
                fps_process_name: Some("secret-game.exe".into()),
            }),
            ..snapshot()
        };

        let text = summary(&data);
        assert!(text.contains("gpu: Test GPU temp=40.0"));
        assert!(text.contains("144Hz"));
        assert!(!text.contains("Wi-Fi"));
        assert!(!text.contains("secret-game"));
        assert!(!text.contains("Taro"));
        assert!(text.contains("storage: drive 1 of 1 (1024 GB) temp=30.0"));
    }
}
