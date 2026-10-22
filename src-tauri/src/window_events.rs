//! Window geometry event log.
//!
//! Records every change to the main window's position / size together with
//! what caused it, plus the system events that typically move windows
//! around (DPI / display / work-area changes, sleep and resume). Entries go
//! to the in-memory log (tag "WindowEvent") and are appended to
//! `<config dir>/Ondo/window-events.log`, which survives restarts — the
//! window tends to change size while nobody is watching (monitor sleep,
//! overnight), long before anyone opens the debug server.
//!
//! The subclass only observes; it never alters a message.

// The recorder is only wired up to window messages on Windows
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

use std::cell::RefCell;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, OnceLock};

const TAG: &str = "WindowEvent";
const LOG_FILE_NAME: &str = "window-events.log";
/// Rotate to `.old` past this size so the file can't grow unbounded
const LOG_FILE_MAX_BYTES: u64 = 512 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Who changed the window.
#[derive(Debug, PartialEq, Eq)]
enum Cause {
    /// Inside the OS move/size loop (user dragging an edge or the window)
    User,
    /// Ondo itself, with the reason passed to `with_app_reason`
    App(String),
    /// Anything else: Windows, another program, a display change...
    External,
}

impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cause::User => write!(f, "user"),
            Cause::App(reason) => write!(f, "app:{reason}"),
            Cause::External => write!(f, "EXTERNAL"),
        }
    }
}

struct State {
    last: Option<Rect>,
    in_size_move: bool,
    size_move_start: Option<Rect>,
}

static STATE: Mutex<State> = Mutex::new(State {
    last: None,
    in_size_move: false,
    size_move_start: None,
});

thread_local! {
    /// Set while the app itself is moving / resizing the window
    static APP_REASON: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Runs `f`, attributing every geometry change it causes to the app.
///
/// Must run on the UI (main) thread: there Tauri applies window ops
/// synchronously and Windows delivers WM_WINDOWPOSCHANGED inside the
/// SetWindowPos call, so the attribution is exact — no timing guesses.
/// Commands reach it via `AppHandle::run_on_main_thread`.
pub fn with_app_reason<R>(reason: &str, f: impl FnOnce() -> R) -> R {
    let previous = APP_REASON.with(|r| r.replace(Some(reason.to_string())));
    let result = f();
    APP_REASON.with(|r| *r.borrow_mut() = previous);
    result
}

fn current_app_reason() -> Option<String> {
    APP_REASON.with(|r| r.borrow().clone())
}

fn classify(in_size_move: bool, app_reason: Option<String>) -> Cause {
    if in_size_move {
        return Cause::User;
    }
    match app_reason {
        Some(reason) => Cause::App(reason),
        None => Cause::External,
    }
}

fn format_rect(r: Rect) -> String {
    format!("{},{} {}x{}", r.x, r.y, r.w, r.h)
}

/// "x,y wxh (move dx,dy) (size dw,dh)" or None when nothing changed.
fn describe_change(prev: Option<Rect>, new: Rect) -> Option<String> {
    let Some(prev) = prev else {
        return Some(format_rect(new));
    };
    if prev == new {
        return None;
    }
    let mut out = format!("{} -> {}", format_rect(prev), format_rect(new));
    if (prev.w, prev.h) != (new.w, new.h) {
        out.push_str(&format!(" [size {:+},{:+}]", new.w - prev.w, new.h - prev.h));
    }
    if (prev.x, prev.y) != (new.x, new.y) {
        out.push_str(&format!(" [move {:+},{:+}]", new.x - prev.x, new.y - prev.y));
    }
    Some(out)
}

/// `YYYY-MM-DD HH:MM:SS.mmm` (UTC) for the persistent file.
fn format_utc(epoch_ms: u64) -> String {
    let secs = epoch_ms / 1000;
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    // Civil-from-days (Howard Hinnant), avoids a chrono dependency
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}.{:03}",
        tod / 3600,
        (tod / 60) % 60,
        tod % 60,
        epoch_ms % 1000
    )
}

/// Hands entries to a background writer: called from the window procedure,
/// which must not block on file IO (slow profile disk, antivirus scans) or
/// it would stall moving / resizing the window.
static SINK: OnceLock<Sender<(u64, String)>> = OnceLock::new();

fn now_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn record(message: String) {
    if let Some(sink) = SINK.get() {
        // A send error only means the writer is gone; nothing to do then
        let _ = sink.send((now_epoch_ms(), message));
    }
}

fn start_writer() {
    SINK.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<(u64, String)>();
        let _ = std::thread::Builder::new()
            .name("window-events-log".into())
            .spawn(move || {
                for (epoch_ms, message) in rx {
                    crate::log_info!(TAG, "{}", message);
                    append_to_file(epoch_ms, &message);
                }
            });
        tx
    });
}

fn log_dir() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("Ondo"))
}

/// Last `max_lines` lines of the persistent log (older rotated file first),
/// for the debug server.
pub fn read_log_tail(max_lines: usize) -> String {
    let Some(dir) = log_dir() else {
        return String::new();
    };
    let mut text = String::new();
    for name in [format!("{LOG_FILE_NAME}.old"), LOG_FILE_NAME.to_string()] {
        if let Ok(content) = std::fs::read_to_string(dir.join(name)) {
            text.push_str(&content);
        }
    }
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

fn append_to_file(epoch_ms: u64, message: &str) {
    use std::io::Write;
    let Some(dir) = log_dir() else {
        return;
    };
    let path = dir.join(LOG_FILE_NAME);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > LOG_FILE_MAX_BYTES) {
        let _ = std::fs::rename(&path, dir.join(format!("{LOG_FILE_NAME}.old")));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{} UTC {}", format_utc(epoch_ms), message);
    }
}

/// Records a geometry update. Changes inside the user's move/size loop are
/// summarised once when the loop ends instead of per mouse move.
fn on_geometry(rect: Rect) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    if state.in_size_move {
        state.last = Some(rect);
        return;
    }
    let Some(change) = describe_change(state.last, rect) else {
        return;
    };
    state.last = Some(rect);
    drop(state);
    let cause = classify(false, current_app_reason());
    record(format!("geometry {change} by {cause}"));
}

fn on_size_move(start: bool) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    if start {
        state.in_size_move = true;
        state.size_move_start = state.last;
        return;
    }
    state.in_size_move = false;
    let start_rect = state.size_move_start.take();
    let end_rect = state.last;
    drop(state);
    if let Some(end_rect) = end_rect {
        if let Some(change) = describe_change(start_rect, end_rect) {
            record(format!("geometry {change} by {}", Cause::User));
        }
    }
}

// ── Windows message hook ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod hook {
    use super::{on_geometry, on_size_move, record, Rect, STATE};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowRect, SWP_NOMOVE, SWP_NOSIZE, WINDOWPOS,
    };

    const SUBCLASS_ID: usize = 0x4F4E_444F; // "ONDO"

    const WM_SIZE: u32 = 0x0005;
    const WM_WINDOWPOSCHANGED: u32 = 0x0047;
    const WM_SETTINGCHANGE: u32 = 0x001A;
    const WM_DISPLAYCHANGE: u32 = 0x007E;
    const WM_POWERBROADCAST: u32 = 0x0218;
    const WM_ENTERSIZEMOVE: u32 = 0x0231;
    const WM_EXITSIZEMOVE: u32 = 0x0232;
    const WM_DPICHANGED: u32 = 0x02E0;

    const SPI_SETWORKAREA: usize = 0x002F;
    const SIZE_MINIMIZED: usize = 1;
    const SIZE_MAXIMIZED: usize = 2;
    const PBT_APMSUSPEND: usize = 0x0004;
    const PBT_APMRESUMESUSPEND: usize = 0x0007;
    const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;

    fn loword(v: usize) -> u32 {
        (v & 0xFFFF) as u32
    }
    fn hiword(v: usize) -> u32 {
        ((v >> 16) & 0xFFFF) as u32
    }

    pub fn install(hwnd: isize) {
        super::start_writer();
        let hwnd = HWND(hwnd as _);
        unsafe {
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_ok() {
                if let Ok(mut state) = STATE.lock() {
                    state.last = Some(Rect {
                        x: rect.left,
                        y: rect.top,
                        w: rect.right - rect.left,
                        h: rect.bottom - rect.top,
                    });
                }
            }
            let ok = SetWindowSubclass(hwnd, Some(subclass_proc), SUBCLASS_ID, 0).as_bool();
            let initial = STATE.lock().ok().and_then(|s| s.last);
            record(format!(
                "monitor {} (initial {})",
                if ok { "installed" } else { "FAILED to install" },
                initial.map(super::format_rect).unwrap_or_else(|| "unknown".into())
            ));
        }
    }

    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        observe(msg, wparam.0, lparam.0);
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    fn observe(msg: u32, wparam: usize, lparam: isize) {
        match msg {
            WM_WINDOWPOSCHANGED if lparam != 0 => {
                let pos = unsafe { &*(lparam as *const WINDOWPOS) };
                if pos.flags.contains(SWP_NOMOVE) && pos.flags.contains(SWP_NOSIZE) {
                    return;
                }
                on_geometry(Rect {
                    x: pos.x,
                    y: pos.y,
                    w: pos.cx,
                    h: pos.cy,
                });
            }
            WM_ENTERSIZEMOVE => on_size_move(true),
            WM_EXITSIZEMOVE => on_size_move(false),
            WM_DPICHANGED => {
                let suggested = if lparam != 0 {
                    let r = unsafe { &*(lparam as *const RECT) };
                    format!(
                        "{},{} {}x{}",
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top
                    )
                } else {
                    "?".into()
                };
                record(format!(
                    "DPI changed to {} (scale {:.0}%), Windows suggests {suggested}",
                    loword(wparam),
                    f64::from(loword(wparam)) / 96.0 * 100.0
                ));
            }
            WM_DISPLAYCHANGE => record(format!(
                "display changed: {}x{} {}bpp",
                loword(lparam as usize),
                hiword(lparam as usize),
                wparam
            )),
            WM_SETTINGCHANGE if wparam == SPI_SETWORKAREA => {
                record("work area changed (taskbar / display layout)".into())
            }
            WM_POWERBROADCAST => match wparam {
                PBT_APMSUSPEND => record("system suspending".into()),
                PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => record("system resumed".into()),
                _ => {}
            },
            WM_SIZE => match wparam {
                SIZE_MINIMIZED => record("minimized".into()),
                SIZE_MAXIMIZED => record("MAXIMIZED".into()),
                _ => {}
            },
            _ => {}
        }
    }
}

/// Starts recording the main window's geometry changes (Windows only).
#[cfg(target_os = "windows")]
pub fn install(hwnd: isize) {
    hook::install(hwnd);
}

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn install(_hwnd: isize) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn user_drag_wins_over_an_app_reason() {
        assert_eq!(classify(true, Some("fit".into())), Cause::User);
    }

    #[test]
    fn app_reason_only_inside_its_scope() {
        assert_eq!(classify(false, current_app_reason()), Cause::External);
        let inside = with_app_reason("mini-exit", || {
            // Nested calls report the innermost reason, then restore
            let nested = with_app_reason("fit", current_app_reason);
            (nested, current_app_reason())
        });
        assert_eq!(inside, (Some("fit".into()), Some("mini-exit".into())));
        assert_eq!(current_app_reason(), None);
        assert_eq!(classify(false, None), Cause::External);
    }

    #[test]
    fn describes_size_and_move_deltas() {
        assert_eq!(describe_change(Some(rect(10, 20, 300, 900)), rect(10, 20, 300, 900)), None);
        assert_eq!(
            describe_change(Some(rect(10, 20, 300, 900)), rect(10, 20, 180, 900)).unwrap(),
            "10,20 300x900 -> 10,20 180x900 [size -120,+0]"
        );
        assert_eq!(
            describe_change(Some(rect(10, 20, 300, 900)), rect(5, 40, 300, 900)).unwrap(),
            "10,20 300x900 -> 5,40 300x900 [move -5,+20]"
        );
        assert_eq!(describe_change(None, rect(1, 2, 3, 4)).unwrap(), "1,2 3x4");
    }

    #[test]
    fn formats_utc_timestamps() {
        assert_eq!(format_utc(0), "1970-01-01 00:00:00.000");
        // 2026-10-22 08:17:05.684 UTC
        assert_eq!(format_utc(1_792_657_025_684), "2026-10-22 08:17:05.684");
        // Leap day
        assert_eq!(format_utc(1_709_164_800_000), "2024-02-29 00:00:00.000");
    }
}
