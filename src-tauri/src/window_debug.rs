//! Window debugging helpers — exposes DWM attributes and injected-module
//! information so we can diagnose third-party desktop modifications (e.g.
//! Windhawk glass-effect mods) that bleed visual styling into the HUD.

use serde::Serialize;

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicIsize, Ordering};

#[cfg(target_os = "windows")]
static MAIN_HWND_RAW: AtomicIsize = AtomicIsize::new(0);

#[cfg(target_os = "windows")]
pub fn set_main_hwnd_raw(value: isize) {
    MAIN_HWND_RAW.store(value, Ordering::SeqCst);
}

#[cfg(target_os = "windows")]
fn current_hwnd_raw() -> isize {
    MAIN_HWND_RAW.load(Ordering::SeqCst)
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowInfo {
    pub hwnd: String,
    pub class_name: String,
    pub title: String,
    pub style_hex: String,
    pub style_flags: Vec<&'static str>,
    pub ex_style_hex: String,
    pub ex_style_flags: Vec<&'static str>,
    pub dwm: DwmAttributes,
    pub suspicious_modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DwmAttributes {
    pub system_backdrop_type: AttributeValue<&'static str>,
    pub nc_rendering_enabled: AttributeValue<bool>,
    pub nc_rendering_policy: AttributeValue<&'static str>,
    pub transitions_force_disabled: AttributeValue<bool>,
    pub window_corner_preference: AttributeValue<&'static str>,
    pub use_host_backdrop_brush: AttributeValue<bool>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct AttributeValue<T: Serialize> {
    pub value: Option<T>,
    pub raw: Option<i32>,
    pub error: Option<String>,
}

// ── Windows implementation ───────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub fn get_window_info() -> Result<WindowInfo, String> {
    let raw = current_hwnd_raw();
    if raw == 0 {
        return Err("Main window HWND not registered yet".into());
    }
    let hwnd = windows::Win32::Foundation::HWND(raw as *mut std::ffi::c_void);

    let class_name = read_class_name(hwnd);
    let title = read_window_text(hwnd);
    let (style, ex_style) = read_styles(hwnd);
    let dwm = read_dwm_attributes(hwnd);
    let suspicious_modules = list_suspicious_modules();

    Ok(WindowInfo {
        hwnd: format!("0x{:X}", raw),
        class_name,
        title,
        style_hex: format!("0x{:08X}", style),
        style_flags: decode_style(style),
        ex_style_hex: format!("0x{:08X}", ex_style),
        ex_style_flags: decode_ex_style(ex_style),
        dwm,
        suspicious_modules,
    })
}

#[cfg(target_os = "windows")]
fn read_class_name(hwnd: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetClassNameW;
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    if len == 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..len])
    }
}

#[cfg(target_os = "windows")]
fn read_window_text(hwnd: windows::Win32::Foundation::HWND) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let mut buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, &mut buf) } as usize;
    if len == 0 {
        String::new()
    } else {
        String::from_utf16_lossy(&buf[..len])
    }
}

#[cfg(target_os = "windows")]
fn read_styles(hwnd: windows::Win32::Foundation::HWND) -> (u32, u32) {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongW, GWL_EXSTYLE, GWL_STYLE};
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        (style, ex_style)
    }
}

#[cfg(target_os = "windows")]
fn decode_style(style: u32) -> Vec<&'static str> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let mut out = Vec::new();
    let pairs: &[(u32, &'static str)] = &[
        (WS_POPUP.0, "WS_POPUP"),
        (WS_CHILD.0, "WS_CHILD"),
        (WS_VISIBLE.0, "WS_VISIBLE"),
        (WS_DISABLED.0, "WS_DISABLED"),
        (WS_CLIPSIBLINGS.0, "WS_CLIPSIBLINGS"),
        (WS_CLIPCHILDREN.0, "WS_CLIPCHILDREN"),
        (WS_MAXIMIZE.0, "WS_MAXIMIZE"),
        (WS_CAPTION.0, "WS_CAPTION"),
        (WS_BORDER.0, "WS_BORDER"),
        (WS_DLGFRAME.0, "WS_DLGFRAME"),
        (WS_VSCROLL.0, "WS_VSCROLL"),
        (WS_HSCROLL.0, "WS_HSCROLL"),
        (WS_SYSMENU.0, "WS_SYSMENU"),
        (WS_THICKFRAME.0, "WS_THICKFRAME"),
        (WS_MINIMIZEBOX.0, "WS_MINIMIZEBOX"),
        (WS_MAXIMIZEBOX.0, "WS_MAXIMIZEBOX"),
    ];
    for (bit, name) in pairs {
        if style & bit == *bit && *bit != 0 {
            out.push(*name);
        }
    }
    out
}

#[cfg(target_os = "windows")]
fn decode_ex_style(ex_style: u32) -> Vec<&'static str> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    let mut out = Vec::new();
    let pairs: &[(u32, &'static str)] = &[
        (WS_EX_DLGMODALFRAME.0, "WS_EX_DLGMODALFRAME"),
        (WS_EX_NOPARENTNOTIFY.0, "WS_EX_NOPARENTNOTIFY"),
        (WS_EX_TOPMOST.0, "WS_EX_TOPMOST"),
        (WS_EX_ACCEPTFILES.0, "WS_EX_ACCEPTFILES"),
        (WS_EX_TRANSPARENT.0, "WS_EX_TRANSPARENT"),
        (WS_EX_MDICHILD.0, "WS_EX_MDICHILD"),
        (WS_EX_TOOLWINDOW.0, "WS_EX_TOOLWINDOW"),
        (WS_EX_WINDOWEDGE.0, "WS_EX_WINDOWEDGE"),
        (WS_EX_CLIENTEDGE.0, "WS_EX_CLIENTEDGE"),
        (WS_EX_CONTEXTHELP.0, "WS_EX_CONTEXTHELP"),
        (WS_EX_RIGHT.0, "WS_EX_RIGHT"),
        (WS_EX_RTLREADING.0, "WS_EX_RTLREADING"),
        (WS_EX_LEFTSCROLLBAR.0, "WS_EX_LEFTSCROLLBAR"),
        (WS_EX_CONTROLPARENT.0, "WS_EX_CONTROLPARENT"),
        (WS_EX_STATICEDGE.0, "WS_EX_STATICEDGE"),
        (WS_EX_APPWINDOW.0, "WS_EX_APPWINDOW"),
        (WS_EX_LAYERED.0, "WS_EX_LAYERED"),
        (WS_EX_NOINHERITLAYOUT.0, "WS_EX_NOINHERITLAYOUT"),
        (WS_EX_NOREDIRECTIONBITMAP.0, "WS_EX_NOREDIRECTIONBITMAP"),
        (WS_EX_LAYOUTRTL.0, "WS_EX_LAYOUTRTL"),
        (WS_EX_COMPOSITED.0, "WS_EX_COMPOSITED"),
        (WS_EX_NOACTIVATE.0, "WS_EX_NOACTIVATE"),
    ];
    for (bit, name) in pairs {
        if ex_style & bit == *bit && *bit != 0 {
            out.push(*name);
        }
    }
    out
}

#[cfg(target_os = "windows")]
fn read_dwm_attributes(hwnd: windows::Win32::Foundation::HWND) -> DwmAttributes {
    use windows::Win32::Graphics::Dwm::*;

    let mut out = DwmAttributes::default();

    out.system_backdrop_type = read_i32_attr(hwnd, DWMWA_SYSTEMBACKDROP_TYPE).map(|v| match v {
        0 => "Auto",
        1 => "None",
        2 => "MainWindow (Mica)",
        3 => "TransientWindow (Acrylic)",
        4 => "TabbedWindow (Mica Alt)",
        _ => "Unknown",
    });
    out.nc_rendering_enabled = read_bool_attr(hwnd, DWMWA_NCRENDERING_ENABLED);
    out.nc_rendering_policy = read_i32_attr(hwnd, DWMWA_NCRENDERING_POLICY).map(|v| match v {
        0 => "UseWindowStyle",
        1 => "Disabled",
        2 => "Enabled",
        _ => "Unknown",
    });
    out.transitions_force_disabled = read_bool_attr(hwnd, DWMWA_TRANSITIONS_FORCEDISABLED);
    out.window_corner_preference =
        read_i32_attr(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE).map(|v| match v {
            0 => "Default",
            1 => "DoNotRound",
            2 => "Round",
            3 => "RoundSmall",
            _ => "Unknown",
        });
    out.use_host_backdrop_brush = read_bool_attr(hwnd, DWMWA_USE_HOSTBACKDROPBRUSH);
    out
}

#[cfg(target_os = "windows")]
fn read_i32_attr(
    hwnd: windows::Win32::Foundation::HWND,
    attr: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE,
) -> AttributeValue<i32> {
    use windows::Win32::Graphics::Dwm::DwmGetWindowAttribute;
    let mut value: i32 = 0;
    let res = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            attr,
            &mut value as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };
    match res {
        Ok(_) => AttributeValue {
            value: Some(value),
            raw: Some(value),
            error: None,
        },
        Err(e) => AttributeValue {
            value: None,
            raw: None,
            error: Some(e.message()),
        },
    }
}

#[cfg(target_os = "windows")]
fn read_bool_attr(
    hwnd: windows::Win32::Foundation::HWND,
    attr: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE,
) -> AttributeValue<bool> {
    let i = read_i32_attr(hwnd, attr);
    AttributeValue {
        value: i.value.map(|v| v != 0),
        raw: i.raw,
        error: i.error,
    }
}

// Implements the generic mapping for read_i32_attr -> AttributeValue<&str>
#[cfg(target_os = "windows")]
impl AttributeValue<i32> {
    fn map<F: FnOnce(i32) -> &'static str>(self, f: F) -> AttributeValue<&'static str> {
        AttributeValue {
            value: self.value.map(f),
            raw: self.raw,
            error: self.error,
        }
    }
}

#[cfg(target_os = "windows")]
fn list_suspicious_modules() -> Vec<String> {
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::ProcessStatus::{EnumProcessModules, GetModuleFileNameExW};
    use windows::Win32::System::Threading::GetCurrentProcess;

    let proc = unsafe { GetCurrentProcess() };
    let mut modules: Vec<HMODULE> = vec![HMODULE::default(); 1024];
    let mut needed: u32 = 0;
    let res = unsafe {
        EnumProcessModules(
            proc,
            modules.as_mut_ptr(),
            (modules.len() * std::mem::size_of::<HMODULE>()) as u32,
            &mut needed,
        )
    };
    if res.is_err() {
        return vec![format!("EnumProcessModules failed: {}", res.err().unwrap())];
    }
    let count = (needed as usize) / std::mem::size_of::<HMODULE>();
    let mut out: Vec<String> = Vec::new();
    for i in 0..count.min(modules.len()) {
        let mut name_buf = [0u16; 512];
        let n = unsafe { GetModuleFileNameExW(Some(proc), Some(modules[i]), &mut name_buf) }
            as usize;
        if n == 0 {
            continue;
        }
        let name = String::from_utf16_lossy(&name_buf[..n]);
        let lower = name.to_lowercase();
        // Common injected DLLs from desktop customization tools.
        if lower.contains("windhawk")
            || lower.contains("dwmblurglass")
            || lower.contains("micaforeveryone")
            || lower.contains("translucent")
            || lower.contains("rainmeter")
            || lower.contains("startallback")
            || lower.contains("explorerpatcher")
            || lower.contains("nilesoft")
        {
            out.push(name);
        }
    }
    out
}

// ── DWM attribute writers ────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub fn set_dwm_attribute(attr_name: &str, value: &str) -> Result<String, String> {
    use windows::Win32::Graphics::Dwm::*;

    let raw = current_hwnd_raw();
    if raw == 0 {
        return Err("Main window HWND not registered yet".into());
    }
    let hwnd = windows::Win32::Foundation::HWND(raw as *mut std::ffi::c_void);

    let (attr, encoded) = match attr_name {
        "backdrop" => {
            let v: i32 = match value.to_lowercase().as_str() {
                "auto" => 0,
                "none" => 1,
                "main" | "mica" | "mainwindow" => 2,
                "transient" | "acrylic" | "transientwindow" => 3,
                "tabbed" | "mica-alt" | "tabbedwindow" => 4,
                other => return Err(format!("Unknown backdrop value: {}", other)),
            };
            (DWMWA_SYSTEMBACKDROP_TYPE, v)
        }
        "ncrendering" => {
            let v: i32 = match value.to_lowercase().as_str() {
                "usewindowstyle" => 0,
                "disabled" => 1,
                "enabled" => 2,
                other => return Err(format!("Unknown ncrendering value: {}", other)),
            };
            (DWMWA_NCRENDERING_POLICY, v)
        }
        "hostbackdrop" => {
            let v: i32 = parse_bool(value)? as i32;
            (DWMWA_USE_HOSTBACKDROPBRUSH, v)
        }
        "corner" => {
            let v: i32 = match value.to_lowercase().as_str() {
                "default" => 0,
                "donotround" | "square" => 1,
                "round" => 2,
                "roundsmall" => 3,
                other => return Err(format!("Unknown corner value: {}", other)),
            };
            (DWMWA_WINDOW_CORNER_PREFERENCE, v)
        }
        "transitions" => {
            let v: i32 = parse_bool(value)? as i32;
            (DWMWA_TRANSITIONS_FORCEDISABLED, v)
        }
        other => return Err(format!("Unknown attribute: {}", other)),
    };

    let res = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            attr,
            &encoded as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<i32>() as u32,
        )
    };
    match res {
        Ok(_) => Ok(format!(
            "Set {} = {} (raw {}) on HWND 0x{:X}",
            attr_name, value, encoded, raw
        )),
        Err(e) => Err(format!("DwmSetWindowAttribute failed: {}", e)),
    }
}

#[cfg(target_os = "windows")]
fn parse_bool(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(format!("Expected boolean, got: {}", other)),
    }
}

// ── Non-Windows stubs ────────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
pub fn get_window_info() -> Result<WindowInfo, String> {
    Err("Window debug is only available on Windows".into())
}

#[cfg(not(target_os = "windows"))]
pub fn set_dwm_attribute(_attr_name: &str, _value: &str) -> Result<String, String> {
    Err("DWM attributes are only available on Windows".into())
}
