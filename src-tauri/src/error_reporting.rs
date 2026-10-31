use crate::health::{Anomaly, AnomalyKind};
use crate::{hardware, health, log_buffer, HardwareData};
use once_cell::sync::{Lazy, OnceCell};
use sentry::protocol::{Attachment, Value};
use sentry::{ClientInitGuard, Level};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

static SENTRY_GUARD: OnceCell<ClientInitGuard> = OnceCell::new();

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The "Send Error Reports" setting. Sentry is initialized either way and
/// this is checked as each event goes out, so switching it needs no restart.
static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

static REPORTED: Lazy<Mutex<HashSet<String>>> = Lazy::new(Default::default);

/// True the first time `key` comes up this session. Failures that recur on
/// every poll go through this so each is sent once, not every second. Keys
/// aren't recorded while reporting is off, so a failure seen then still goes
/// out once if it's switched back on.
pub fn first_report(key: &str) -> bool {
    enabled()
        && REPORTED
            .lock()
            .map(|mut keys| keys.insert(key.to_string()))
            .unwrap_or(false)
}

/// Only these log tags go out with a report: they explain sensor problems,
/// while the rest (debug-server client addresses, window activity) doesn't
/// belong in someone else's Sentry.
const REPORT_LOG_TAGS: &[&str] = &["Hardware", "PawnIO", "Health", "macOS"];
const REPORT_LOG_LINES: usize = 300;

fn hardware_log_attachment() -> Attachment {
    let lines: Vec<String> = log_buffer::get_all()
        .into_iter()
        .filter(|e| REPORT_LOG_TAGS.contains(&e.tag.as_str()))
        .map(|e| format!("[{}] [{}] [{}] {}", e.timestamp, e.level, e.tag, e.message))
        .collect();
    let start = lines.len().saturating_sub(REPORT_LOG_LINES);
    Attachment {
        buffer: lines[start..].join("\n").into_bytes(),
        filename: "hardware-log.txt".to_string(),
        content_type: Some("text/plain".to_string()),
        ..Default::default()
    }
}

fn client_options() -> sentry::ClientOptions {
    // ClientOptions is #[non_exhaustive] as of sentry 0.48.5, so build it by
    // mutation instead of a struct literal. sample_rate defaults to 1.0.
    let mut options = sentry::ClientOptions::default();
    options.release = Some(format!("ondo@{}", APP_VERSION).into());
    options.environment = Some(
        if cfg!(debug_assertions) {
            "development"
        } else {
            "production"
        }
        .into(),
    );
    // Every event passes through here, panics included. Besides the
    // setting, it drops what the SDK adds on its own: the hostname (often
    // the owner's name) and loaded module paths (the per-user install lives
    // under the Windows user folder). Release builds are stripped, so the
    // module list wouldn't help symbolicate anything.
    options.before_send = Some(Arc::new(|mut event| {
        if !enabled() {
            return None;
        }
        event.server_name = None;
        event.debug_meta = Default::default();
        Some(event)
    }));
    options
}

/// Initialize Sentry for error reporting
pub fn init_sentry() {
    // Get DSN from environment variable (set during build)
    let dsn = option_env!("SENTRY_DSN").unwrap_or("");

    if dsn.is_empty() {
        crate::log_info!("Sentry", "DSN not configured, error reporting disabled");
        return;
    }

    let guard = sentry::init((dsn, client_options()));

    if SENTRY_GUARD.set(guard).is_err() {
        crate::log_warn!("Sentry", "Already initialized");
    } else {
        crate::log_info!("Sentry", "Initialized successfully");
    }
}

/// Capture an error with context
pub fn capture_error(error: &str, source: &str, extra: Option<&[(&str, &str)]>) {
    if !enabled() {
        return;
    }
    sentry::with_scope(
        |scope| {
            scope.set_tag("source", source);
            scope.set_tag("platform", std::env::consts::OS);
            scope.set_tag("version", APP_VERSION);

            if let Some(extras) = extra {
                for (key, value) in extras {
                    scope.set_extra(key, sentry::protocol::Value::String(value.to_string()));
                }
            }
        },
        || {
            sentry::capture_message(error, sentry::Level::Error);
        },
    );
}

/// Capture a hardware-related error
pub fn capture_hardware_error(error: &str, hardware_type: &str) {
    capture_error(
        &format!("[Hardware] {}", error),
        "hardware",
        Some(&[("hardware_type", hardware_type)]),
    );
}

/// Capture a settings-related error
pub fn capture_settings_error(error: &str, operation: &str) {
    capture_error(
        &format!("[Settings] {}", error),
        "settings",
        Some(&[("operation", operation)]),
    );
}

/// Capture a window-related error
pub fn capture_window_error(error: &str, operation: &str) {
    capture_error(
        &format!("[Window] {}", error),
        "window",
        Some(&[("operation", operation)]),
    );
}

/// Report hardware data the HUD shows as missing or wrong (see health.rs),
/// with what was on screen, the sensor backend's state and its recent log.
pub fn capture_health_anomaly(anomaly: &Anomaly, data: &HardwareData) {
    let title = anomaly.title();
    crate::log_warn!(
        "Health",
        "{}{}{}",
        title,
        anomaly
            .subject
            .as_deref()
            .map(|s| format!(" [{}]", s))
            .unwrap_or_default(),
        anomaly
            .value
            .map(|v| format!(" (value {})", v))
            .unwrap_or_default()
    );
    if !enabled() {
        return;
    }

    // "Never available" is often expected (no PawnIO, Macs without SMC
    // access), so it doesn't get the error level that alerts by default
    let level = match anomaly.kind {
        AnomalyKind::NeverAvailable => Level::Warning,
        AnomalyKind::Lost | AnomalyKind::Implausible => Level::Error,
    };
    let pawnio = crate::get_pawnio_detailed_status();
    let lhm = hardware::lhm_daemon_status();

    sentry::with_scope(
        |scope| {
            scope.set_tag("source", "health");
            scope.set_tag("reading", anomaly.reading.key());
            scope.set_tag("anomaly", anomaly.kind.key());
            scope.set_tag("platform", std::env::consts::OS);
            scope.set_tag("version", APP_VERSION);
            scope.set_tag(
                "pawnio",
                pawnio.service_state.as_deref().unwrap_or("unknown"),
            );
            if let Some(subject) = &anomaly.subject {
                scope.set_extra("subject", Value::String(subject.clone()));
            }
            if let Some(value) = anomaly.value {
                scope.set_extra("value", Value::from(value));
            }
            scope.set_extra("hardware", Value::String(health::summary(data)));
            scope.set_extra("lhm", serde_json::to_value(&lhm).unwrap_or(Value::Null));
            scope.set_extra(
                "pawnio_status",
                serde_json::to_value(&pawnio).unwrap_or(Value::Null),
            );
            scope.add_attachment(hardware_log_attachment());
        },
        || {
            sentry::capture_message(&format!("[Health] {}", title), level);
        },
    );
}

/// Report an LHM daemon failure, once per `title` per session since they
/// tend to repeat on every poll. `title` must be stable for Sentry to group
/// it; what varies (exit codes, messages) goes in `details`. Doesn't query
/// the daemon's status, as the callers hold its lock.
#[cfg(target_os = "windows")]
pub fn capture_lhm_event(title: &str, details: &str, level: Level) {
    if !first_report(&format!("lhm:{}", title)) {
        return;
    }
    sentry::with_scope(
        |scope| {
            scope.set_tag("source", "lhm_daemon");
            scope.set_tag("platform", std::env::consts::OS);
            scope.set_tag("version", APP_VERSION);
            scope.set_extra("details", Value::String(details.to_string()));
            scope.add_attachment(hardware_log_attachment());
        },
        || {
            sentry::capture_message(&format!("[LHM] {}", title), level);
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentry::integrations::debug_images::DebugImagesIntegration;
    use sentry::test::with_captured_events_options;

    // ENABLED is process-wide; tests that depend on it hold this
    static ENABLED_LOCK: Mutex<()> = Mutex::new(());

    fn lock_enabled() -> std::sync::MutexGuard<'static, ()> {
        ENABLED_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn send_both_ways() {
        capture_error("boom", "test", None);
        // What the panic integration and any direct SDK call go through
        sentry::capture_message("direct", Level::Error);
    }

    #[test]
    fn nothing_is_sent_while_off_and_sending_resumes_without_reinit() {
        let _enabled = lock_enabled();
        set_enabled(false);
        let off = with_captured_events_options(send_both_ways, client_options());
        set_enabled(true);
        let on = with_captured_events_options(send_both_ways, client_options());

        assert!(off.is_empty(), "sent while off: {:?}", off);
        assert_eq!(on.len(), 2);
    }

    #[test]
    fn failures_seen_while_off_are_still_reported_once_after() {
        let _enabled = lock_enabled();
        let key = "test:seen-while-off";
        set_enabled(false);
        let while_off = first_report(key);
        set_enabled(true);

        assert!(!while_off);
        assert!(first_report(key));
        assert!(!first_report(key));
    }

    #[test]
    fn events_leave_without_hostname_or_module_paths() {
        let _enabled = lock_enabled();
        let with_sdk_extras = |mut options: sentry::ClientOptions| {
            options.server_name = Some("alices-pc".into());
            options
                .integrations
                .push(Arc::new(DebugImagesIntegration::new()));
            options
        };
        let send = || {
            sentry::capture_message("boom", Level::Error);
        };

        let plain = with_captured_events_options(send, with_sdk_extras(Default::default()));
        let ours = with_captured_events_options(send, with_sdk_extras(client_options()));

        // The SDK does add both, so the stripping below is real
        assert!(plain[0].server_name.is_some());
        assert!(!plain[0].debug_meta.is_empty());
        assert_eq!(ours[0].server_name, None);
        assert!(ours[0].debug_meta.is_empty());
    }
}
