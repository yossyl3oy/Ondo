use once_cell::sync::OnceCell;
use sentry::ClientInitGuard;

static SENTRY_GUARD: OnceCell<ClientInitGuard> = OnceCell::new();

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Initialize Sentry for error reporting
pub fn init_sentry() {
    // Get DSN from environment variable (set during build)
    let dsn = option_env!("SENTRY_DSN").unwrap_or("");

    if dsn.is_empty() {
        eprintln!("[Sentry] DSN not configured, error reporting disabled");
        return;
    }

    let guard = sentry::init((
        dsn,
        sentry::ClientOptions {
            release: Some(format!("ondo@{}", APP_VERSION).into()),
            environment: Some(
                if cfg!(debug_assertions) {
                    "development"
                } else {
                    "production"
                }
                .into(),
            ),
            sample_rate: 1.0,
            ..Default::default()
        },
    ));

    if SENTRY_GUARD.set(guard).is_err() {
        eprintln!("[Sentry] Already initialized");
    } else {
        eprintln!("[Sentry] Initialized successfully");
    }
}

/// Capture an error with context
pub fn capture_error(error: &str, source: &str, extra: Option<&[(&str, &str)]>) {
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

/// Capture an LHM daemon error (Windows only)
#[cfg(target_os = "windows")]
pub fn capture_lhm_error(error: &str) {
    capture_error(
        &format!("[LHM] {}", error),
        "lhm_daemon",
        None,
    );
}

/// Capture a WMI error (Windows only)
#[cfg(target_os = "windows")]
pub fn capture_wmi_error(error: &str, query_type: &str) {
    capture_error(
        &format!("[WMI] {}", error),
        "wmi",
        Some(&[("query_type", query_type)]),
    );
}
