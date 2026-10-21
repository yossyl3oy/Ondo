/// FPS monitoring via ETW (Event Tracing for Windows).
///
/// Captures DXGI and D3D9 Present events to calculate the framerate of the
/// foreground application.  Requires administrator privileges — when running
/// without elevation the monitor silently returns `None`.

#[cfg(target_os = "windows")]
use std::collections::HashMap;
use std::collections::VecDeque;
#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "windows")]
use std::sync::Mutex;

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

/// Present timestamps per PID, in raw QPC ticks taken from the ETW event
/// header (the session uses the QPC clock). Using the event's own timestamp
/// instead of the callback's arrival time matters: real-time ETW delivers
/// events in buffered batches, so arrival times bunch up and a "count the
/// last second" approach swings between 0 and double the real rate.
#[cfg(target_os = "windows")]
static FRAME_TIMES: Mutex<Option<HashMap<u32, VecDeque<i64>>>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Present event matching + FPS math (platform independent, unit tested)
// ---------------------------------------------------------------------------

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresentProvider {
    Dxgi,
    D3d9,
}

/// Event IDs per PresentMon's PresentData/ETW manifests:
/// DXGI Present_Start = 0x2a, PresentMultiplaneOverlay_Start = 0x37,
/// D3D9 Present_Start = 0x01. Both providers emit plenty of other
/// Start-opcode events, so matching on the opcode alone overcounts.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn is_present_start(provider: PresentProvider, event_id: u16) -> bool {
    match provider {
        PresentProvider::Dxgi => matches!(event_id, 0x2a | 0x37),
        PresentProvider::D3d9 => event_id == 0x01,
    }
}

/// Averaging window: frames within this span before the newest one count.
const WINDOW_SECS: i64 = 1;
/// History kept per PID (must be >= WINDOW_SECS).
const KEEP_SECS: i64 = 2;
/// No presents for this long (relative to now) means the app is idle or
/// gone. Has to exceed the usual ETW delivery lag (FlushTimer = 1s).
const STALE_SECS: i64 = 2;

/// Records one present and drops history older than KEEP_SECS relative to
/// the newest timestamp. Events can arrive slightly out of order across
/// buffers, so trimming is anchored on the max seen, not on `ts`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn record_present(times: &mut VecDeque<i64>, ts: i64, freq: i64) {
    times.push_back(ts);
    let newest = times.iter().copied().max().unwrap_or(ts);
    let cutoff = newest - KEEP_SECS * freq;
    while times.front().is_some_and(|&t| t < cutoff) {
        times.pop_front();
    }
}

/// No present within STALE_SECS of `now` (or no history at all).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn is_stale(times: &VecDeque<i64>, now: i64, freq: i64) -> bool {
    times
        .iter()
        .copied()
        .max()
        .is_none_or(|newest| now - newest > STALE_SECS * freq)
}

/// Average FPS over the last WINDOW_SECS of presents, from the frame
/// intervals themselves: (frames - 1) / (newest - oldest). `now` is only
/// used to detect that presenting has stopped.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn fps_from_timestamps(times: &VecDeque<i64>, now: i64, freq: i64) -> Option<u32> {
    if freq <= 0 {
        return None;
    }
    let newest = times.iter().copied().max()?;
    if now - newest > STALE_SECS * freq {
        return None;
    }
    let window_start = newest - WINDOW_SECS * freq;
    let (count, oldest) = times
        .iter()
        .copied()
        .filter(|&t| t >= window_start)
        .fold((0u64, newest), |(n, lo), t| (n + 1, lo.min(t)));
    let span = newest - oldest;
    if count < 2 || span <= 0 {
        return None;
    }
    let fps = (count - 1) as f64 * freq as f64 / span as f64;
    Some(fps.round() as u32)
}

// ---------------------------------------------------------------------------
// ETW consumer APIs — not available in the `windows` crate 0.62,
// so we declare them via raw FFI from advapi32/sechost.
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
const PROCESS_TRACE_MODE_REAL_TIME: u32 = 0x0000_0100;
#[cfg(target_os = "windows")]
const PROCESS_TRACE_MODE_EVENT_RECORD: u32 = 0x1000_0000;
// Keep EventHeader.TimeStamp in the session clock (QPC, ClientContext = 1)
// instead of letting ProcessTrace convert it to FILETIME system time;
// fps_from_timestamps compares it against QueryPerformanceCounter.
#[cfg(target_os = "windows")]
const PROCESS_TRACE_MODE_RAW_TIMESTAMP: u32 = 0x0000_1000;
#[cfg(target_os = "windows")]
const INVALID_PROCESSTRACE_HANDLE: u64 = u64::MAX;

// EVENT_TRACE_LOGFILEW field offsets on x86-64:
//   0: LogFileName        (ptr,  8)
//   8: LoggerName         (ptr,  8)
//  28: ProcessTraceMode   (u32,  4)  [Anonymous1 union]
// 424: EventRecordCallback(ptr,  8)  [Anonymous2 union]
// Total struct size ≈ 448 bytes; we use 512 for safety.
#[cfg(target_os = "windows")]
const LOGFILE_BUF_SIZE: usize = 512;
#[cfg(target_os = "windows")]
const OFF_LOGGER_NAME: usize = 8;
#[cfg(target_os = "windows")]
const OFF_PROCESS_TRACE_MODE: usize = 28;
#[cfg(target_os = "windows")]
const OFF_EVENT_RECORD_CALLBACK: usize = 424;

#[cfg(target_os = "windows")]
#[link(name = "advapi32")]
extern "system" {
    fn OpenTraceW(logfile: *mut u8) -> u64;
    fn ProcessTrace(
        handle_array: *const u64,
        handle_count: u32,
        start_time: *const i64,
        end_time: *const i64,
    ) -> u32;
    fn CloseTrace(trace_handle: u64) -> u32;
}

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
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

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
        let name_bytes = session_wide.len() * 2;
        let total = props_base + name_bytes;
        let mut buffer = vec![0u8; total];
        let props = &mut *(buffer.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES);

        props.Wnode.BufferSize = total as u32;
        props.Wnode.Flags = WNODE_FLAG_TRACED_GUID;
        props.Wnode.ClientContext = 1; // QPC clock
        props.LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
        // Flush buffers every second so delivery lag stays ~1s (typical, not
        // guaranteed under load), see STALE_SECS
        props.FlushTimer = 1;
        props.LoggerNameOffset = props_base as u32;

        // --- Start trace session ---
        let mut session_handle = CONTROLTRACE_HANDLE::default();
        StartTraceW(&mut session_handle, PCWSTR(session_wide.as_ptr()), props)
            .ok()
            .map_err(|e| format!("StartTraceW failed: {e}"))?;

        // --- Enable providers ---
        enable_provider(session_handle, &DXGI_PROVIDER)?;
        let _ = enable_provider(session_handle, &D3D9_PROVIDER); // D3D9 is optional

        // --- Open trace for real-time consumption (raw FFI) ---
        let mut logfile_buf = vec![0u8; LOGFILE_BUF_SIZE];
        let p = logfile_buf.as_mut_ptr();

        // LoggerName (offset 8): pointer to wide session name
        *(p.add(OFF_LOGGER_NAME) as *mut *const u16) = session_wide.as_ptr();

        // ProcessTraceMode (offset 28)
        *(p.add(OFF_PROCESS_TRACE_MODE) as *mut u32) =
            PROCESS_TRACE_MODE_REAL_TIME
                | PROCESS_TRACE_MODE_EVENT_RECORD
                | PROCESS_TRACE_MODE_RAW_TIMESTAMP;

        // EventRecordCallback (offset 424)
        *(p.add(OFF_EVENT_RECORD_CALLBACK) as *mut usize) =
            event_record_callback as *const () as usize;

        let trace_handle = OpenTraceW(p);
        if trace_handle == INVALID_PROCESSTRACE_HANDLE {
            return Err("OpenTraceW failed (admin privileges may be required)".into());
        }

        // ProcessTrace blocks until the session is stopped
        ProcessTrace(&trace_handle, 1, std::ptr::null(), std::ptr::null());

        CloseTrace(trace_handle);
    }

    Ok(())
}

#[cfg(target_os = "windows")]
unsafe fn enable_provider(session: CONTROLTRACE_HANDLE, provider: &GUID) -> Result<(), String> {
    // EVENT_CONTROL_CODE_ENABLE_PROVIDER = 1, TRACE_LEVEL_INFORMATION = 4
    EnableTraceEx2(
        session,
        provider,
        1,                     // EVENT_CONTROL_CODE_ENABLE_PROVIDER
        4,                     // TRACE_LEVEL_INFORMATION
        0xFFFF_FFFF_FFFF_FFFF, // MatchAnyKeyword – all
        0,
        0,
        None,
    )
    .ok()
    .map_err(|e| format!("EnableTraceEx2 failed: {e}"))
}

// ---------------------------------------------------------------------------
// ETW event callback  (called on the ProcessTrace thread)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
unsafe extern "system" fn event_record_callback(record: *mut EVENT_RECORD) {
    let rec = &*record;

    let provider = if rec.EventHeader.ProviderId == DXGI_PROVIDER {
        PresentProvider::Dxgi
    } else if rec.EventHeader.ProviderId == D3D9_PROVIDER {
        PresentProvider::D3d9
    } else {
        return;
    };
    if !is_present_start(provider, rec.EventHeader.EventDescriptor.Id) {
        return;
    }

    let pid = rec.EventHeader.ProcessId;
    if pid == 0 {
        return;
    }

    let Some(freq) = qpc_frequency() else {
        return;
    };

    if let Ok(mut guard) = FRAME_TIMES.lock() {
        if let Some(ref mut map) = *guard {
            let times = map.entry(pid).or_default();
            record_present(times, rec.EventHeader.TimeStamp, freq);
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn get_fps_for_pid(pid: u32) -> Option<u32> {
    let freq = qpc_frequency()?;
    let now = qpc_now()?;
    let mut guard = FRAME_TIMES.lock().ok()?;
    let map = guard.as_mut()?;
    // Every process that ever presented gets an entry; drop the ones that
    // went quiet so the map doesn't grow for the life of the app
    map.retain(|_, times| !is_stale(times, now, freq));
    fps_from_timestamps(map.get(&pid)?, now, freq)
}

#[cfg(target_os = "windows")]
fn qpc_frequency() -> Option<i64> {
    use std::sync::OnceLock;
    use windows::Win32::System::Performance::QueryPerformanceFrequency;
    static FREQ: OnceLock<Option<i64>> = OnceLock::new();
    *FREQ.get_or_init(|| {
        let mut freq = 0i64;
        unsafe { QueryPerformanceFrequency(&mut freq) }.ok()?;
        (freq > 0).then_some(freq)
    })
}

#[cfg(target_os = "windows")]
fn qpc_now() -> Option<i64> {
    use windows::Win32::System::Performance::QueryPerformanceCounter;
    let mut now = 0i64;
    unsafe { QueryPerformanceCounter(&mut now) }.ok()?;
    Some(now)
}

#[cfg(target_os = "windows")]
fn process_name(pid: u32) -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    const FREQ: i64 = 10_000_000; // typical QPC frequency (100ns ticks)

    fn frames(fps: i64, seconds: i64, start: i64) -> VecDeque<i64> {
        let mut times = VecDeque::new();
        for i in 0..fps * seconds {
            record_present(&mut times, start + i * FREQ / fps, FREQ);
        }
        times
    }

    #[test]
    fn matches_only_present_start_events() {
        assert!(is_present_start(PresentProvider::Dxgi, 0x2a));
        assert!(is_present_start(PresentProvider::Dxgi, 0x37));
        assert!(!is_present_start(PresentProvider::Dxgi, 0x2b)); // Present_Stop
        assert!(!is_present_start(PresentProvider::Dxgi, 0x01));
        assert!(is_present_start(PresentProvider::D3d9, 0x01));
        assert!(!is_present_start(PresentProvider::D3d9, 0x02));
    }

    #[test]
    fn fps_is_derived_from_event_timestamps() {
        for fps in [30, 60, 144, 240] {
            let times = frames(fps, 3, 0);
            let newest = *times.back().unwrap();
            assert_eq!(fps_from_timestamps(&times, newest, FREQ), Some(fps as u32));
        }
    }

    #[test]
    fn delivery_latency_does_not_change_fps() {
        // Batched ETW delivery: we read ~900ms after the last present
        let times = frames(144, 3, 0);
        let newest = *times.back().unwrap();
        assert_eq!(fps_from_timestamps(&times, newest + FREQ * 9 / 10, FREQ), Some(144));
    }

    #[test]
    fn stale_history_reports_nothing() {
        let times = frames(60, 2, 0);
        let newest = *times.back().unwrap();
        assert_eq!(fps_from_timestamps(&times, newest + 3 * FREQ, FREQ), None);
    }

    #[test]
    fn quiet_processes_are_stale() {
        let times = frames(60, 1, 0);
        let newest = *times.back().unwrap();
        assert!(!is_stale(&times, newest + FREQ, FREQ));
        assert!(is_stale(&times, newest + 3 * FREQ, FREQ));
        assert!(is_stale(&VecDeque::new(), 0, FREQ));
    }

    #[test]
    fn single_frame_or_empty_reports_nothing() {
        assert_eq!(fps_from_timestamps(&VecDeque::new(), 0, FREQ), None);
        assert_eq!(fps_from_timestamps(&VecDeque::from([5]), 5, FREQ), None);
    }

    #[test]
    fn history_is_trimmed_to_keep_window() {
        let times = frames(100, 5, 0);
        let newest = *times.back().unwrap();
        assert!(times.iter().all(|&t| t >= newest - KEEP_SECS * FREQ));
    }

    #[test]
    fn out_of_order_events_are_tolerated() {
        // Same 60fps stream, but neighbouring events delivered swapped
        let mut ts: Vec<i64> = (0..120).map(|i| i * FREQ / 60).collect();
        for pair in ts.chunks_mut(2) {
            pair.swap(0, 1);
        }
        let mut times = VecDeque::new();
        for t in &ts {
            record_present(&mut times, *t, FREQ);
        }
        let newest = *ts.iter().max().unwrap();
        assert_eq!(fps_from_timestamps(&times, newest, FREQ), Some(60));
    }
}
