use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

static LAST_STATE: AtomicBool = AtomicBool::new(false);
static LAST_CURSOR_NEAR: AtomicBool = AtomicBool::new(false);
static LAST_INTERACTIVE: AtomicBool = AtomicBool::new(false);

/// Regions of the webview (logical CSS px, relative to the window's top-left)
/// that stay clickable while mini mode is otherwise click-through.
static INTERACTIVE_ZONES: Mutex<Vec<InteractiveZone>> = Mutex::new(Vec::new());

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InteractiveZone {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize)]
struct MiniModePayload {
    active: bool,
}

#[derive(Debug, Clone, Serialize)]
struct CursorNearPayload {
    near: bool,
}

#[derive(Debug, Clone, Serialize)]
struct InteractivePayload {
    interactive: bool,
}

/// Replace the set of clickable regions for mini mode. An empty list makes
/// the window fully click-through again.
#[tauri::command]
pub fn set_mini_interactive_zones(zones: Vec<InteractiveZone>) {
    if let Ok(mut stored) = INTERACTIVE_ZONES.lock() {
        *stored = zones;
    }
}

fn point_in_zones(x: f64, y: f64, zones: &[InteractiveZone]) -> bool {
    zones
        .iter()
        .any(|z| x >= z.x && x <= z.x + z.width && y >= z.y && y <= z.y + z.height)
}

/// Whether the cursor is currently inside one of the registered interactive
/// zones of the Ondo window.
fn is_cursor_in_interactive_zone(app: &AppHandle) -> bool {
    let zones = match INTERACTIVE_ZONES.lock() {
        Ok(z) if !z.is_empty() => z.clone(),
        _ => return false,
    };
    let Some(window) = app.get_webview_window("main") else {
        return false;
    };
    let Ok(pos) = window.outer_position() else {
        return false;
    };
    let Ok(scale) = window.scale_factor() else {
        return false;
    };
    let Some((cx, cy)) = get_cursor_position() else {
        return false;
    };

    let rel_x = f64::from(cx - pos.x) / scale;
    let rel_y = f64::from(cy - pos.y) / scale;
    point_in_zones(rel_x, rel_y, &zones)
}

/// Start background monitoring for maximized/fullscreen windows on Ondo's
/// display. Emits `"minimode-changed"` whenever the answer flips.
pub fn start_monitoring(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Brief delay to let the frontend set up its event listener
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Initial check — emit even if the initial state is "maximized"
        let initial = is_any_maximized_on_app_display(&app);
        LAST_STATE.store(initial, Ordering::Relaxed);
        if initial {
            let _ = app.emit("minimode-changed", MiniModePayload { active: true });
            crate::log_info!("WindowMonitor", "Mini mode activated (startup)");
        }

        let mut tick: u32 = 0;
        loop {
            let in_mini = LAST_STATE.load(Ordering::Relaxed);
            // In mini mode: poll every 200ms. Otherwise: every 1s.
            let interval = if in_mini { 200 } else { 1000 };
            tokio::time::sleep(std::time::Duration::from_millis(interval)).await;

            // Re-check display occupancy every 1s (every 5th tick in mini mode)
            tick = tick.wrapping_add(1);
            let check_maximized = !in_mini || tick % 5 == 0;

            if check_maximized {
                let is_maximized = is_any_maximized_on_app_display(&app);
                let prev = LAST_STATE.load(Ordering::Relaxed);

                if is_maximized != prev {
                    LAST_STATE.store(is_maximized, Ordering::Relaxed);
                    // Reset cursor-near state when leaving mini mode
                    if !is_maximized {
                        LAST_CURSOR_NEAR.store(false, Ordering::Relaxed);
                    }
                    let _ = app.emit(
                        "minimode-changed",
                        MiniModePayload {
                            active: is_maximized,
                        },
                    );
                    crate::log_info!(
                        "WindowMonitor",
                        "Mini mode {}",
                        if is_maximized {
                            "activated"
                        } else {
                            "deactivated"
                        }
                    );
                }
            }

            // Check cursor proximity when in mini mode
            if LAST_STATE.load(Ordering::Relaxed) {
                let near = is_cursor_near_window(&app, 50);
                let prev_near = LAST_CURSOR_NEAR.load(Ordering::Relaxed);
                if near != prev_near {
                    LAST_CURSOR_NEAR.store(near, Ordering::Relaxed);
                    let _ = app.emit("cursor-near-minimode", CursorNearPayload { near });
                }

                // Accept clicks only while the cursor is over a registered
                // interactive zone (audio selector, update notification)
                let inside = near && is_cursor_in_interactive_zone(&app);
                let prev_inside = LAST_INTERACTIVE.load(Ordering::Relaxed);
                if inside != prev_inside {
                    LAST_INTERACTIVE.store(inside, Ordering::Relaxed);
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.set_ignore_cursor_events(!inside);
                    }
                    let _ = app.emit(
                        "minimode-interactive",
                        InteractivePayload {
                            interactive: inside,
                        },
                    );
                }
            } else if LAST_INTERACTIVE.swap(false, Ordering::Relaxed) {
                // Left mini mode while a zone was active; the frontend's exit
                // handler restores cursor events, so only reset our state
            }
        }
    });
}

// ── Resize border hover ─────────────────────────────────────────────────────

/// Width of the band (CSS px) along a vertical window edge that counts as
/// the resize border. Covers the OS sizing frame Tauri hit-tests for
/// undecorated windows (SM_CXFRAME, ~4px at 100%).
const RESIZE_BAND_CSS: f64 = 8.0;
/// Approximate OS sizing border (SM_CXFRAME at 96 DPI) in CSS px. Along the
/// top / bottom border Tauri's hit test answers HTTOP / HTBOTTOM except in
/// the corners, and the height is locked, so only the corners resize there.
const OS_BORDER_CSS: f64 = 4.0;
const EDGE_POLL_MS: u64 = 50;
/// Each wait gives up after this long; the frontend re-arms if needed
const EDGE_WAIT_LIMIT_MS: u64 = 30_000;

/// Generation per edge (West, East): a new wait supersedes the previous
/// one, so pointer jitter at the boundary can't stack pollers.
static EDGE_WAIT_GEN: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];

/// Whether a drag at the cursor would resize the window's width from the
/// given vertical edge. Coordinates are physical screen px; `band` is the
/// side band width and `os_border` the native top/bottom border height.
fn cursor_on_edge(
    cursor: (i32, i32),
    window_pos: (i32, i32),
    window_size: (u32, u32),
    east: bool,
    band: i32,
    os_border: i32,
) -> bool {
    let (cx, cy) = (i64::from(cursor.0), i64::from(cursor.1));
    let (left, top) = (i64::from(window_pos.0), i64::from(window_pos.1));
    let right = left + i64::from(window_size.0);
    let bottom = top + i64::from(window_size.1);
    let band = i64::from(band.max(1));
    let os_border = i64::from(os_border.max(1)).min(band);
    if cy < top || cy >= bottom {
        return false;
    }
    // Distance from the edge being resized
    let inset = if east { right - 1 - cx } else { cx - left };
    if inset < 0 || inset >= band {
        return false;
    }
    let in_top_or_bottom_border = cy < top + os_border || cy >= bottom - os_border;
    // There only the corner (inside the native side border) still resizes width
    !in_top_or_bottom_border || inset < os_border
}

/// Waits until the cursor leaves the width-resize band of `edge` ("West" /
/// "East"). Over the OS sizing border the webview gets no pointer events,
/// so the frontend keeps its resize hint up until this returns. Returns
/// `true` when the cursor left, `false` when the wait timed out or was
/// superseded by a newer one for the same edge.
#[tauri::command]
pub async fn wait_cursor_leaves_edge(app: AppHandle, edge: String) -> Result<bool, String> {
    let east = edge == "East";
    let gen = &EDGE_WAIT_GEN[usize::from(east)];
    let my_gen = gen.fetch_add(1, Ordering::SeqCst) + 1;
    let mut waited = 0;
    while waited < EDGE_WAIT_LIMIT_MS {
        if gen.load(Ordering::SeqCst) != my_gen {
            return Ok(false);
        }
        let Some(window) = app.get_webview_window("main") else {
            return Ok(true);
        };
        let (Ok(pos), Ok(size), Ok(scale), Some(cursor)) = (
            window.outer_position(),
            window.outer_size(),
            window.scale_factor(),
            get_cursor_position(),
        ) else {
            return Ok(true);
        };
        let band = (RESIZE_BAND_CSS * scale).ceil() as i32;
        let os_border = (OS_BORDER_CSS * scale).ceil() as i32;
        let on_edge = cursor_on_edge(
            cursor,
            (pos.x, pos.y),
            (size.width, size.height),
            east,
            band,
            os_border,
        );
        if !on_edge {
            return Ok(true);
        }
        tokio::time::sleep(std::time::Duration::from_millis(EDGE_POLL_MS)).await;
        waited += EDGE_POLL_MS;
    }
    Ok(false)
}

// ── Cursor proximity detection ──────────────────────────────────────────────

/// Check if cursor is within `margin` pixels of the Ondo window.
fn is_cursor_near_window(app: &AppHandle, margin: i32) -> bool {
    let Some(window) = app.get_webview_window("main") else {
        return false;
    };
    let Ok(pos) = window.outer_position() else {
        return false;
    };
    let Ok(size) = window.outer_size() else {
        return false;
    };
    let Some((cx, cy)) = get_cursor_position() else {
        return false;
    };

    let left = pos.x - margin;
    let top = pos.y - margin;
    let right = pos.x + size.width as i32 + margin;
    let bottom = pos.y + size.height as i32 + margin;

    cx >= left && cx <= right && cy >= top && cy <= bottom
}

#[cfg(target_os = "windows")]
fn get_cursor_position() -> Option<(i32, i32)> {
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    unsafe {
        let mut point = std::mem::zeroed();
        if GetCursorPos(&mut point).is_ok() {
            Some((point.x, point.y))
        } else {
            None
        }
    }
}

#[cfg(target_os = "macos")]
fn get_cursor_position() -> Option<(i32, i32)> {
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return None;
        }
        let loc = CGEventGetLocation(event);
        CFRelease(event as *const _);
        Some((loc.x as i32, loc.y as i32))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn get_cursor_position() -> Option<(i32, i32)> {
    None
}

// ── Windows implementation ──────────────────────────────────────────────────
//
// We don't care which window is in the foreground. As long as *any* visible
// top-level window on Ondo's monitor is maximized (or covers the full screen),
// mini mode stays on. This way opening the Start menu, clicking the desktop,
// or alt-tabbing to a small utility window doesn't toggle mini mode off while
// the user's maximized app is still sitting there.

#[cfg(target_os = "windows")]
fn is_any_maximized_on_app_display(app: &AppHandle) -> bool {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowRect, IsWindowVisible, IsZoomed,
    };

    let Some(ondo_hwnd) = app.get_webview_window("main").and_then(|w| w.hwnd().ok()) else {
        return false;
    };
    let ondo_hwnd_isize = ondo_hwnd.0 as isize;

    unsafe {
        let app_monitor = MonitorFromWindow(HWND(ondo_hwnd.0), MONITOR_DEFAULTTONEAREST);
        if app_monitor.0.is_null() {
            return false;
        }

        // Cache Ondo's monitor rect so the enum callback can compare cheaply.
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let screen_rect = if GetMonitorInfoW(app_monitor, &mut mi).as_bool() {
            Some(mi.rcMonitor)
        } else {
            None
        };

        struct State {
            ondo_hwnd: isize,
            app_monitor: HMONITOR,
            screen_rect: Option<RECT>,
            found: bool,
        }

        let mut state = State {
            ondo_hwnd: ondo_hwnd_isize,
            app_monitor,
            screen_rect,
            found: false,
        };

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let state = &mut *(lparam.0 as *mut State);

            if hwnd.0 as isize == state.ondo_hwnd {
                return BOOL(1);
            }
            if !IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }

            // Skip shell windows — they always cover the full desktop and
            // would otherwise be misread as borderless fullscreen.
            let mut class_buf = [0u16; 64];
            let len = GetClassNameW(hwnd, &mut class_buf);
            let class = if len > 0 {
                String::from_utf16_lossy(&class_buf[..len as usize])
            } else {
                String::new()
            };
            if matches!(
                class.as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            ) {
                return BOOL(1);
            }

            // Skip DWM-cloaked windows. UWP / Windows shell windows
            // (Windows.UI.Core.CoreWindow et al.) keep a screen-sized rect
            // alive in the background and would otherwise be misread as
            // borderless fullscreen.
            let mut cloaked: u32 = 0;
            if DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED,
                &mut cloaked as *mut _ as *mut core::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            )
            .is_ok()
                && cloaked != 0
            {
                return BOOL(1);
            }

            // Restrict to Ondo's monitor.
            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if mon != state.app_monitor {
                return BOOL(1);
            }

            // Native Win32 maximized state.
            if IsZoomed(hwnd).as_bool() {
                state.found = true;
                return BOOL(0); // stop enumeration
            }

            // Borderless fullscreen — window rect aligns with the monitor on
            // all sides (allow a few px slop for DPI rounding). We *don't*
            // accept windows that merely contain the monitor: Chromium-based
            // apps (Electron, VSCode, Slack, Discord) hold offscreen widget
            // windows whose rect spans multiple monitors, which would
            // otherwise be misread as borderless fullscreen.
            if let Some(scr) = state.screen_rect {
                let mut rect: RECT = std::mem::zeroed();
                if GetWindowRect(hwnd, &mut rect).is_ok() {
                    let slop = 4;
                    let scr_w = scr.right - scr.left;
                    let scr_h = scr.bottom - scr.top;
                    let win_w = rect.right - rect.left;
                    let win_h = rect.bottom - rect.top;
                    if (rect.left - scr.left).abs() <= slop
                        && (rect.top - scr.top).abs() <= slop
                        && (win_w - scr_w).abs() <= slop
                        && (win_h - scr_h).abs() <= slop
                    {
                        state.found = true;
                        return BOOL(0);
                    }
                }
            }

            BOOL(1)
        }

        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut state as *mut _ as isize));

        state.found
    }
}

// ── macOS implementation ────────────────────────────────────────────────────
// Uses CoreGraphics CGWindowListCopyWindowInfo (no accessibility permission needed).
// Returns true if any on-screen layer-0 window on Ondo's display covers the
// full screen. Foreground state is irrelevant — same intent as the Windows
// implementation: the user's maximized app keeps mini mode on even while a
// menu or another app has focus.

#[cfg(target_os = "macos")]
fn is_any_maximized_on_app_display(_app: &AppHandle) -> bool {
    use core_foundation::array::CFArray;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;

    let list_ptr = unsafe {
        CGWindowListCopyWindowInfo(
            CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS,
            CG_NULL_WINDOW_ID,
        )
    };
    if list_ptr.is_null() {
        return false;
    }

    let window_list: CFArray<CFDictionary<CFString, CFType>> =
        unsafe { CFArray::wrap_under_create_rule(list_ptr as *const _) };

    fn read_bounds(dict: &CFDictionary<CFString, CFType>) -> Option<(f64, f64, f64, f64)> {
        use core_foundation::base::TCFType;
        let bounds_key = CFString::new("kCGWindowBounds");
        let bounds_val = dict.find(&bounds_key)?;
        let bounds_dict: CFDictionary<CFString, CFType> =
            unsafe { CFDictionary::wrap_under_get_rule(bounds_val.as_CFTypeRef() as _) };
        Some((
            cf_dict_get_f64(&bounds_dict, "X").unwrap_or(0.0),
            cf_dict_get_f64(&bounds_dict, "Y").unwrap_or(0.0),
            cf_dict_get_f64(&bounds_dict, "Width").unwrap_or(0.0),
            cf_dict_get_f64(&bounds_dict, "Height").unwrap_or(0.0),
        ))
    }

    fn read_pid(dict: &CFDictionary<CFString, CFType>) -> Option<i64> {
        use core_foundation::base::TCFType;
        let key = CFString::new("kCGWindowOwnerPID");
        dict.find(&key)
            .and_then(|v| unsafe { CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as _) }.to_i64())
    }

    let our_pid = std::process::id() as i64;

    // First pass: find which display Ondo is on by reading its own CG bounds.
    // PID matching is name-independent, so dev/prod binary names both work.
    let mut app_display: u32 = 0;
    let mut app_window_area = 0.0;
    for i in 0..window_list.len() {
        let dict = unsafe { window_list.get_unchecked(i) };
        if read_pid(&dict) != Some(our_pid) {
            continue;
        }
        let Some((x, y, w, h)) = read_bounds(&dict) else {
            continue;
        };
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        let display = display_containing_point(x + w / 2.0, y + h / 2.0);
        let area = w * h;
        if display != 0 && area > app_window_area {
            app_display = display;
            app_window_area = area;
        }
    }
    if app_display == 0 {
        return false;
    }

    let app_display_bounds = unsafe { CGDisplayBounds(app_display) };
    if app_display_bounds.size.width <= 0.0 || app_display_bounds.size.height <= 0.0 {
        return false;
    }

    // Second pass: look for any maximized / fullscreen window on Ondo's display.
    for i in 0..window_list.len() {
        let dict = unsafe { window_list.get_unchecked(i) };

        // Only consider normal layer (layer == 0)
        let layer_key = CFString::new("kCGWindowLayer");
        let layer = dict
            .find(&layer_key)
            .and_then(|v| unsafe { CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as _) }.to_i32())
            .unwrap_or(-1);
        if layer != 0 {
            continue;
        }

        // Skip our own windows, regardless of dev/prod app name.
        if read_pid(&dict) == Some(our_pid) {
            continue;
        }

        let Some((x, y, w, h)) = read_bounds(&dict) else {
            continue;
        };
        let (win_center_x, win_center_y) = (x + w / 2.0, y + h / 2.0);

        // Restrict to Ondo's display.
        if display_containing_point(win_center_x, win_center_y) != app_display {
            continue;
        }

        // If the window covers >=95% of the screen → maximized / fullscreen
        if w >= app_display_bounds.size.width * 0.95 && h >= app_display_bounds.size.height * 0.90 {
            return true;
        }
    }

    false
}

#[cfg(target_os = "macos")]
fn display_containing_point(x: f64, y: f64) -> u32 {
    unsafe {
        let point = CGPoint { x, y };
        let mut display_id: u32 = 0;
        let mut count: u32 = 0;
        let err = CGGetDisplaysWithPoint(point, 1, &mut display_id, &mut count);
        if err == 0 && count > 0 {
            display_id
        } else {
            0
        }
    }
}

#[cfg(target_os = "macos")]
fn cf_dict_get_f64(
    dict: &core_foundation::dictionary::CFDictionary<
        core_foundation::string::CFString,
        core_foundation::base::CFType,
    >,
    key: &str,
) -> Option<f64> {
    use core_foundation::base::TCFType;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;

    let cf_key = CFString::new(key);
    dict.find(&cf_key)
        .and_then(|v| unsafe { CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as _) }.to_f64())
}

#[cfg(target_os = "macos")]
const CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1 << 0;
#[cfg(target_os = "macos")]
const CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
#[cfg(target_os = "macos")]
const CG_NULL_WINDOW_ID: u32 = 0;

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
struct CGSize {
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Copy, Clone)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relativeToWindow: u32) -> *const std::ffi::c_void;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGEventCreate(source: *const std::ffi::c_void) -> *const std::ffi::c_void;
    fn CGEventGetLocation(event: *const std::ffi::c_void) -> CGPoint;
    fn CGGetDisplaysWithPoint(
        point: CGPoint,
        max_displays: u32,
        displays: *mut u32,
        matching_display_count: *mut u32,
    ) -> i32;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const std::ffi::c_void);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(x: f64, y: f64, width: f64, height: f64) -> InteractiveZone {
        InteractiveZone {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn point_inside_single_zone() {
        let zones = [zone(10.0, 100.0, 160.0, 20.0)];
        assert!(point_in_zones(90.0, 110.0, &zones));
    }

    #[test]
    fn point_on_zone_edge_counts_as_inside() {
        let zones = [zone(10.0, 100.0, 160.0, 20.0)];
        assert!(point_in_zones(10.0, 100.0, &zones));
        assert!(point_in_zones(170.0, 120.0, &zones));
    }

    #[test]
    fn point_outside_all_zones() {
        let zones = [zone(10.0, 100.0, 160.0, 20.0), zone(2.0, 2.0, 30.0, 10.0)];
        assert!(!point_in_zones(90.0, 90.0, &zones));
        assert!(!point_in_zones(-5.0, 110.0, &zones));
    }

    #[test]
    fn empty_zones_never_match() {
        assert!(!point_in_zones(0.0, 0.0, &[]));
    }

    // Window at (100, 50), 300x600, band 8px, native border 4px
    const POS: (i32, i32) = (100, 50);
    const SIZE: (u32, u32) = (300, 600);

    fn on(x: i32, y: i32, east: bool) -> bool {
        cursor_on_edge((x, y), POS, SIZE, east, 8, 4)
    }

    #[test]
    fn west_band_is_left_edge_inside_window() {
        assert!(on(100, 300, false));
        assert!(on(107, 300, false));
        assert!(!on(108, 300, false)); // moved inward
        assert!(!on(99, 300, false)); // left the window
    }

    #[test]
    fn east_band_is_right_edge_inside_window() {
        assert!(on(399, 300, true));
        assert!(on(392, 300, true));
        assert!(!on(391, 300, true));
        assert!(!on(400, 300, true));
    }

    #[test]
    fn band_requires_vertical_overlap() {
        assert!(!on(102, 49, false));
        assert!(!on(102, 650, false));
        assert!(on(102, 645, false));
    }

    #[test]
    fn top_and_bottom_borders_only_count_in_the_corner() {
        // 6px in from the left but on the top border: native HTTOP, height is
        // locked, so a drag there would do nothing
        assert!(!on(106, 52, false));
        assert!(!on(106, 648, false));
        assert!(!on(393, 52, true));
        // Corner (inside the side border too) still resizes the width
        assert!(on(102, 52, false));
        assert!(on(397, 648, true));
    }
}

// ── Fallback for other platforms ────────────────────────────────────────────

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn is_any_maximized_on_app_display(_app: &AppHandle) -> bool {
    false
}
