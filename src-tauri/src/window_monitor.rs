use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager};

static LAST_STATE: AtomicBool = AtomicBool::new(false);
static LAST_CURSOR_NEAR: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
struct MiniModePayload {
    active: bool,
}

#[derive(Debug, Clone, Serialize)]
struct CursorNearPayload {
    near: bool,
}

/// Start background monitoring for maximized/fullscreen foreground windows.
/// Emits `"minimode-changed"` event when the state toggles.
pub fn start_monitoring(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Brief delay to let the frontend set up its event listener
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Initial check — emit even if the initial state is "maximized"
        let initial = is_foreground_maximized(&app);
        LAST_STATE.store(initial, Ordering::Relaxed);
        if initial {
            let _ = app.emit(
                "minimode-changed",
                MiniModePayload { active: true },
            );
            crate::log_info!("WindowMonitor", "Mini mode activated (startup)");
        }

        let mut tick: u32 = 0;
        loop {
            let in_mini = LAST_STATE.load(Ordering::Relaxed);
            // In mini mode: poll every 200ms. Otherwise: every 1s.
            let interval = if in_mini { 200 } else { 1000 };
            tokio::time::sleep(std::time::Duration::from_millis(interval)).await;

            // Check foreground maximized state every 1s (every 5th tick in mini mode)
            tick = tick.wrapping_add(1);
            let check_maximized = !in_mini || tick % 5 == 0;

            if check_maximized {
                let is_maximized = is_foreground_maximized(&app);
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
            }
        }
    });
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

#[cfg(target_os = "windows")]
fn is_foreground_maximized(app: &AppHandle) -> bool {
    use windows::Win32::Graphics::Gdi::MonitorFromWindow;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowRect, IsZoomed,
    };

    unsafe {
        let fg = GetForegroundWindow();
        if fg.0 == std::ptr::null_mut() {
            return false;
        }

        // Skip if the foreground window is Ondo itself
        if let Some(window) = app.get_webview_window("main") {
            if let Ok(hwnd) = window.hwnd() {
                if fg.0 as isize == hwnd.0 as isize {
                    return false;
                }
            }
        }

        // Skip the desktop window (wallpaper click makes Progman/WorkerW foreground)
        let mut class_buf = [0u16; 64];
        let len = GetClassNameW(fg, &mut class_buf) as usize;
        if len > 0 {
            let class_name = String::from_utf16_lossy(&class_buf[..len]);
            if class_name == "Progman" || class_name == "WorkerW" {
                return false;
            }
        }

        // Check Win32 maximized state first
        if IsZoomed(fg).as_bool() {
            return true;
        }

        // Also detect fullscreen windows (e.g. video players) by checking if
        // the window covers the entire monitor
        let mut win_rect = std::mem::zeroed();
        if GetWindowRect(fg, &mut win_rect).is_err() {
            return false;
        }

        let monitor = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if !GetMonitorInfoW(monitor, &mut mi).as_bool() {
            return false;
        }

        let scr = mi.rcMonitor;
        win_rect.left <= scr.left
            && win_rect.top <= scr.top
            && win_rect.right >= scr.right
            && win_rect.bottom >= scr.bottom
    }
}

// ── macOS implementation ────────────────────────────────────────────────────
// Uses CoreGraphics CGWindowListCopyWindowInfo (no accessibility permission needed)
// to check if the frontmost window covers the full screen.

#[cfg(target_os = "macos")]
fn is_foreground_maximized(_app: &AppHandle) -> bool {
    use core_foundation::array::CFArray;
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::number::CFNumber;
    use core_foundation::string::CFString;

    // Get main display size
    let (screen_w, screen_h) = unsafe {
        let display = CGMainDisplayID();
        (
            CGDisplayPixelsWide(display) as f64,
            CGDisplayPixelsHigh(display) as f64,
        )
    };

    // Get on-screen window list (excludes desktop elements)
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

    // Iterate on-screen windows (ordered front-to-back)
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

        // Skip Ondo windows
        let owner_key = CFString::new("kCGWindowOwnerName");
        if let Some(v) = dict.find(&owner_key) {
            let owner: CFString =
                unsafe { CFString::wrap_under_get_rule(v.as_CFTypeRef() as _) };
            if owner.to_string() == "Ondo" {
                continue;
            }
        }

        // Read window bounds
        let bounds_key = CFString::new("kCGWindowBounds");
        if let Some(bounds_val) = dict.find(&bounds_key) {
            let bounds_dict: CFDictionary<CFString, CFType> =
                unsafe { CFDictionary::wrap_under_get_rule(bounds_val.as_CFTypeRef() as _) };

            let w = cf_dict_get_f64(&bounds_dict, "Width").unwrap_or(0.0);
            let h = cf_dict_get_f64(&bounds_dict, "Height").unwrap_or(0.0);

            // If the frontmost normal-layer window covers >=95% of the screen → maximized
            if w >= screen_w * 0.95 && h >= screen_h * 0.90 {
                return true;
            }
        }

        // Only inspect the topmost normal-layer window
        break;
    }

    false
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
    dict.find(&cf_key).and_then(|v| {
        unsafe { CFNumber::wrap_under_get_rule(v.as_CFTypeRef() as _) }.to_f64()
    })
}

#[cfg(target_os = "macos")]
const CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1 << 0;
#[cfg(target_os = "macos")]
const CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
#[cfg(target_os = "macos")]
const CG_NULL_WINDOW_ID: u32 = 0;

#[cfg(target_os = "macos")]
#[repr(C)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowListCopyWindowInfo(
        option: u32,
        relativeToWindow: u32,
    ) -> *const std::ffi::c_void;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayPixelsWide(display: u32) -> usize;
    fn CGDisplayPixelsHigh(display: u32) -> usize;
    fn CGEventCreate(source: *const std::ffi::c_void) -> *const std::ffi::c_void;
    fn CGEventGetLocation(event: *const std::ffi::c_void) -> CGPoint;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const std::ffi::c_void);
}

// ── Fallback for other platforms ────────────────────────────────────────────

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn is_foreground_maximized(_app: &AppHandle) -> bool {
    false
}
