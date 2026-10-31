# Ondo - Hardware Temperature Monitor

[![Release](https://img.shields.io/github/v/release/yossyl3oy/Ondo?style=flat-square)](https://github.com/yossyl3oy/Ondo/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/yossyl3oy/Ondo/release.yml?style=flat-square)](https://github.com/yossyl3oy/Ondo/actions)
[![License](https://img.shields.io/github/license/yossyl3oy/Ondo?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-blue?style=flat-square)]()

A sleek, HUD-style hardware temperature monitoring widget.

## Why I made this

- Windows keeps switching my audio devices and resetting the refresh rate to 60Hz on its own, which is annoying — so I wanted a widget to keep an eye on things at all times
- My CPU is an i9-14900KF that runs insanely hot, so I needed a way to monitor temperatures constantly

This app is built around features I personally wanted. If there's something you'd like to see added, feel free to open an [Issue](https://github.com/yossyl3oy/Ondo/issues) and I'll look into it.

## Download

Download the latest release from the [Releases](https://github.com/yossyl3oy/Ondo/releases) page.

- **Windows**: `.msi` or `.exe` installer
- **macOS**: `.dmg` disk image

## Display Modes

- **Normal**: Full HUD widget with expandable/collapsible sections
- **Compact**: All sections collapsed, showing only summary bars
- **Mini**: Automatically activated when the window is resized narrow — single-line display per component with click-through support

## Settings

- **Position**: Right, Left, Top-Right, Top-Left, Bottom-Right, Bottom-Left
- **Opacity**: 30–100%
- **Always on Top / Always on Back**: Window layering control
- **Auto Start**: Launch on system startup
- **Show CPU Cores**: Individual core temperatures in a grid
- **Update Interval**: 500ms–5000ms
- **Theme**: Auto (system) / Dark / Light
- **Compact Mode**: Reduced widget size
- **Debug Server**: Enable HTTP debug server on port 19210
- **Send Error Reports**: On by default; see [Privacy](#privacy)

## Hardware Monitoring

### Windows

Sensor data is retrieved via [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor). Falls back to `sysinfo` when unavailable.

If the [PawnIO](https://github.com/namazso/PawnIO) driver is needed, it can be installed from the Settings screen.

### macOS

Temperature data is retrieved via system APIs.

## Debug Server

Enable the Debug Server in Settings to start an HTTP server on port **19210**, allowing you to retrieve logs and sensor data over LAN.

| Endpoint | Method | Description |
|---|---|---|
| `/` | GET | Dashboard (HTML) — tabbed view of hardware, sensors, logs, and window state |
| `/help` | GET | API reference. Add `?format=json` for JSON output |
| `/status` | GET | Process state (PID, version, log count, PawnIO status) |
| `/api/hardware` | GET | Hardware sensor data (JSON) |
| `/api/sensors` | GET | Raw sensor list (text) |
| `/api/pawnio` | GET | PawnIO driver status (JSON, Windows only) |
| `/api/window` | GET | Main window state: HWND, class, styles, DWM attributes, injected DLLs (Windows only) |
| `/logs` | GET | All logs. Filters: `?since=<epoch_ms>&limit=N&level=info&tag=Hardware` |
| `/logs/tail` | GET | Latest N lines. `?n=100` (default) |
| `/logs/search` | GET | Regex search. `?q=<pattern>&limit=200` |

All endpoints are read-only — no mutating actions are exposed. Add `?format=json` to any log endpoint for JSON output. Default format is plain text.

```bash
# Example usage (Mac to Windows over LAN)
curl -s "http://<WINDOWS_IP>:19210/status"
curl -s "http://<WINDOWS_IP>:19210/logs/tail?n=50"
curl -s "http://<WINDOWS_IP>:19210/api/hardware"
curl -s "http://<WINDOWS_IP>:19210/api/window"
curl -s "http://<WINDOWS_IP>:19210/logs/search?q=error&format=json"
```

## Privacy

Release builds send error reports to the developer through [Sentry](https://sentry.io), so missing or wrong sensor readings can be fixed without asking you for logs. Turn **Send Error Reports** off in Settings to stop this. It applies immediately, and while it's off nothing is sent at all, including Sentry's ping at app start. Builds from source send nothing unless the Sentry DSNs are set at build time (see `.env.example`).

A report can contain:

- Error messages, with a stack trace for crashes
- Hardware model names (CPU, GPU, motherboard, display) and their readings: temperatures, loads, clocks, fan speeds, memory/disk usage, refresh rate and current FPS
- The state of the LibreHardwareMonitor sensor process and the PawnIO driver service (Windows)
- The last ~300 lines of Ondo's log, limited to the `Hardware`, `PawnIO`, `Health` and `macOS` tags
- Drives only as "drive N of M (size)"
- What Sentry adds by default: app version, OS name and version, CPU architecture and device model; for errors in the UI, the webview's user agent and the recent console messages and clicked UI elements leading up to the error; and one session ping per app start, used to count crash-free sessions

Reports never include network interface names, the names of processes Ondo measures FPS for, drive or volume names, or your computer's hostname.

## Tech Stack

- **Frontend**: React 19 + TypeScript + Vite
- **Backend**: Tauri 2 (Rust)
- **Hardware**: LibreHardwareMonitor (.NET 8) / System APIs
- **Package Manager**: pnpm (managed via [mise](https://mise.jdx.dev/))

## Development

```bash
git clone https://github.com/yossyl3oy/Ondo.git
cd Ondo
pnpm install
pnpm tauri dev
```

## License

[MIT](LICENSE)

### Third-Party Licenses

- [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) — MPL-2.0
- [PawnIO](https://github.com/namazso/PawnIO) — GPL-2.0
