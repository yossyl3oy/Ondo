use once_cell::sync::Lazy;
use serde::Serialize;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LINES: usize = 5000;

#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub id: u64,
    pub timestamp: String,
    pub epoch_ms: u64,
    pub level: &'static str,
    pub tag: String,
    pub message: String,
}

struct LogState {
    entries: Vec<LogEntry>,
    next_id: u64,
}

static LOG_STATE: Lazy<Mutex<LogState>> = Lazy::new(|| {
    Mutex::new(LogState {
        entries: Vec::with_capacity(MAX_LINES),
        next_id: 0,
    })
});

fn now_iso() -> (String, u64) {
    let epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    // Simple ISO-ish format without chrono dependency
    let secs = epoch_ms / 1000;
    let ms = epoch_ms % 1000;
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    (format!("{:02}:{:02}:{:02}.{:03}", h, m, s, ms), epoch_ms)
}

pub fn push_log(level: &'static str, tag: &str, message: &str) {
    let (timestamp, epoch_ms) = now_iso();

    // Also print to stderr for local visibility
    eprintln!("[{}] [{}] {}", tag, level, message);

    if let Ok(mut state) = LOG_STATE.lock() {
        if state.entries.len() >= MAX_LINES {
            state.entries.remove(0);
        }
        let entry = LogEntry {
            id: state.next_id,
            timestamp,
            epoch_ms,
            level,
            tag: tag.to_string(),
            message: message.to_string(),
        };
        state.next_id += 1;
        state.entries.push(entry);
    }
}

pub fn get_all() -> Vec<LogEntry> {
    LOG_STATE
        .lock()
        .map(|s| s.entries.clone())
        .unwrap_or_default()
}

pub fn get_tail(n: usize) -> Vec<LogEntry> {
    LOG_STATE
        .lock()
        .map(|s| {
            let start = s.entries.len().saturating_sub(n);
            s.entries[start..].to_vec()
        })
        .unwrap_or_default()
}

pub fn get_since(epoch_ms: u64) -> Vec<LogEntry> {
    LOG_STATE
        .lock()
        .map(|s| {
            s.entries
                .iter()
                .filter(|e| e.epoch_ms > epoch_ms)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

pub fn search(pattern: &str) -> Vec<LogEntry> {
    let pattern_lower = pattern.to_lowercase();
    LOG_STATE
        .lock()
        .map(|s| {
            s.entries
                .iter()
                .filter(|e| {
                    e.message.to_lowercase().contains(&pattern_lower)
                        || e.tag.to_lowercase().contains(&pattern_lower)
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

pub fn clear() {
    if let Ok(mut state) = LOG_STATE.lock() {
        state.entries.clear();
    }
}

pub fn count() -> usize {
    LOG_STATE
        .lock()
        .map(|s| s.entries.len())
        .unwrap_or(0)
}

/// Bridge the standard `log` crate into our ring buffer so that Tauri's
/// internal logs (and any other `log::info!()` etc.) are also captured.
struct BufferLogger;

impl log::Log for BufferLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let level = match record.level() {
            log::Level::Error => "error",
            log::Level::Warn => "warn",
            log::Level::Info => "info",
            log::Level::Debug => "debug",
            log::Level::Trace => "trace",
        };
        let tag = record
            .module_path()
            .unwrap_or(record.target());
        push_log(level, tag, &format!("{}", record.args()));
    }

    fn flush(&self) {}
}

static LOGGER: BufferLogger = BufferLogger;

/// Call once at startup (before `tauri::Builder`) to capture all `log` crate output.
pub fn init_logger() {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(log::LevelFilter::Debug))
        .ok();
}

/// Convenience macros for logging with tag and level
#[macro_export]
macro_rules! app_log {
    ($level:expr, $tag:expr, $($arg:tt)*) => {
        $crate::log_buffer::push_log($level, $tag, &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_info {
    ($tag:expr, $($arg:tt)*) => {
        $crate::app_log!("info", $tag, $($arg)*)
    };
}

#[macro_export]
macro_rules! log_error {
    ($tag:expr, $($arg:tt)*) => {
        $crate::app_log!("error", $tag, $($arg)*)
    };
}

#[macro_export]
macro_rules! log_warn {
    ($tag:expr, $($arg:tt)*) => {
        $crate::app_log!("warn", $tag, $($arg)*)
    };
}

#[macro_export]
macro_rules! log_debug {
    ($tag:expr, $($arg:tt)*) => {
        $crate::app_log!("debug", $tag, $($arg)*)
    };
}
