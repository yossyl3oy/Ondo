/// FPS monitoring via ETW (Event Tracing for Windows).
///
/// Captures DXGI and D3D9 Present events to calculate the framerate of the
/// foreground application.  Requires administrator privileges — when running
/// without elevation the monitor silently returns `None`.

#[cfg(target_os = "windows")]
use std::collections::{HashMap, VecDeque};
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use windows::core::{GUID, PCWSTR, PWSTR};
#[cfg(target_os = "windows")]
use windows::Win32::System::Diagnostics::Etw::*;

// Microsoft-Windows-DXGI  {CA11C036-0102-4A2D-A6AD-F03CFED5D3C9}
#[cfg(target_os = "windows")]
const DXGI_PROVIDER: GUID = GUID::from_values(
    0xCA11C036,
    0x0102,
    0x4A2D,
    [0xA6, 0xAD, 0xF0, 0x3C, 0xFE, 0xD5, 0xD3, 0xC9],
);

// Microsoft-Windows-D3D9  {783ACA0A-790E-4D7F-8451-AA850511C6B9}
#[cfg(target_os = "windows")]
const D3D9_PROVIDER: GUID = GUID::from_values(
    0x783ACA0A,
    0x790E,
    0x4D7F,
    [0x84, 0x51, 0xAA, 0x85, 0x05, 0x11, 0xC6, 0xB9],
);

#[cfg(target_os = "windows")]
const SESSION_NAME: &str = "OndoFpsMonitor";

#[cfg(target_os = "windows")]
static RUNNING: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "windows")]
static FRAME_TIMES: Mutex<Option<HashMap<u32, VecDeque<Instant>>>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub fn start() {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return; // already running
    }

    *FRAME_TIMES.lock().unwrap() = Some(HashMap::new());

    std::thread::Builder::new()
        .name("fps-etw".into())
        .spawn(|| {
            if let Err(e) = run_etw_session() {
                log::warn!("FPS monitoring unavailable: {}", e);
            }
            RUNNING.store(false, Ordering::SeqCst);
        })
        .ok();
}

#[cfg(target_os = "windows")]
pub fn stop() {
    let session_wide = to_wide(SESSION_NAME);
    stop_session_by_name(&session_wide);
    if let Ok(mut guard) = FRAME_TIMES.lock() {
        *guard = None;
    }
}

/// Returns the FPS of the current foreground application and its process name.
#[cfg(target_os = "windows")]
pub fn get_foreground_fps() -> Option<(u32, String)> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };

    let pid = unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid
    };

    if pid == 0 {
        return None;
    }

    let fps = get_fps_for_pid(pid)?;
    if fps == 0 {
        return None;
    }

    let name = process_name(pid).unwrap_or_default();
    Some((fps, name))
}

// ---------------------------------------------------------------------------
// ETW session
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn run_etw_session() -> Result<(), String> {
    let session_wide = to_wide(SESSION_NAME);

    // Stop stale session from a previous crash
    stop_session_by_name(&session_wide);

    unsafe {
        // --- Allocate EVENT_TRACE_PROPERTIES + trailing session name buffer ---
        let props_base = std::mem::size_of::<EVENT_TRACE_PROPERTIES>();
        let name_bytes = session_wide.len() * 2; // u16 → bytes
        let total = props_base + name_bytes;
        let mut buffer = vec![0u8; total];
        let props = &mut *(buffer.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES);

        props.Wnode.BufferSize = total as u32;
        props.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        props.Wnode.ClientContext = 1; // QPC clock
        props.LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
        props.LoggerNameOffset = props_base as u32;

        // --- Start trace session ---
        let mut session_handle = CONTROLTRACE_HANDLE::default();
        StartTraceW(
            &mut session_handle,
            PCWSTR(session_wide.as_ptr()),
            props,
        )
        .map_err(|e| format!("StartTraceW failed: {e}"))?;

        // --- Enable providers ---
        enable_provider(session_handle, &DXGI_PROVIDER)?;
        let _ = enable_provider(session_handle, &D3D9_PROVIDER); // D3D9 is optional

        // --- Open trace for real-time consumption ---
        let mut logfile: EVENT_TRACE_LOGFILEW = std::mem::zeroed();
        logfile.LoggerName = PWSTR(session_wide.as_ptr() as *mut u16);
        logfile.Anonymous1.ProcessTraceMode =
            PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
        logfile.Anonymous2.EventRecordCallback = Some(event_record_callback);

        let trace_handle = OpenTraceW(&mut logfile)
            .map_err(|e| format!("OpenTraceW failed: {e}"))?;

        // ProcessTrace blocks until the session is stopped
        let _ = ProcessTrace(&[trace_handle], None, None);

        let _ = CloseTrace(trace_handle);
    }

    Ok(())
}

#[cfg(target_os = "windows")]
unsafe fn enable_provider(
    session: CONTROLTRACE_HANDLE,
    provider: &GUID,
) -> Result<(), String> {
    EnableTraceEx2(
        session,
        provider,
        EVENT_CONTROL_CODE_ENABLE_PROVIDER.0,
        TRACE_LEVEL_INFORMATION.0 as u8,
        0xFFFF_FFFF_FFFF_FFFF, // MatchAnyKeyword – all
        0,
        0,
        None,
    )
    .map_err(|e| format!("EnableTraceEx2 failed: {e}"))
}

// ---------------------------------------------------------------------------
// ETW event callback  (called on the ProcessTrace thread)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
unsafe extern "system" fn event_record_callback(record: *mut EVENT_RECORD) {
    let rec = &*record;

    // Only count Start events (Opcode 1) to avoid double-counting
    if rec.EventHeader.EventDescriptor.Opcode != 1 {
        return;
    }

    let pid = rec.EventHeader.ProcessId;
    if pid == 0 {
        return;
    }

    let now = Instant::now();

    if let Ok(mut guard) = FRAME_TIMES.lock() {
        if let Some(ref mut map) = *guard {
            let times = map.entry(pid).or_insert_with(VecDeque::new);
            times.push_back(now);

            // Trim entries older than 2 seconds
            let cutoff = now - Duration::from_secs(2);
            while let Some(&front) = times.front() {
                if front < cutoff {
                    times.pop_front();
                } else {
                    break;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn get_fps_for_pid(pid: u32) -> Option<u32> {
    let guard = FRAME_TIMES.lock().ok()?;
    let map = guard.as_ref()?;
    let times = map.get(&pid)?;

    let now = Instant::now();
    let one_sec_ago = now - Duration::from_secs(1);
    let count = times.iter().filter(|&&t| t >= one_sec_ago).count() as u32;

    if count == 0 { None } else { Some(count) }
}

#[cfg(target_os = "windows")]
fn process_name(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle =
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);
        if ok.is_ok() {
            let path = String::from_utf16_lossy(&buf[..size as usize]);
            // Return just the filename without extension
            let file = path.rsplit('\\').next().unwrap_or(&path);
            let name = file.strip_suffix(".exe").unwrap_or(file);
            Some(name.to_string())
        } else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
fn stop_session_by_name(session_wide: &[u16]) {
    unsafe {
        let props_base = std::mem::size_of::<EVENT_TRACE_PROPERTIES>();
        let total = props_base + session_wide.len() * 2;
        let mut buffer = vec![0u8; total];
        let props = &mut *(buffer.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES);
        props.Wnode.BufferSize = total as u32;
        props.LoggerNameOffset = props_base as u32;

        let _ = ControlTraceW(
            CONTROLTRACE_HANDLE::default(),
            PCWSTR(session_wide.as_ptr()),
            props,
            EVENT_TRACE_CONTROL_STOP,
        );
    }
}

#[cfg(target_os = "windows")]
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------------------
// macOS / Linux stubs
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "windows"))]
pub fn start() {}

#[cfg(not(target_os = "windows"))]
pub fn stop() {}

#[cfg(not(target_os = "windows"))]
pub fn get_foreground_fps() -> Option<(u32, String)> {
    None
}
