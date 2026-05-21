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

## Hardware Monitoring

### Windows

Sensor data is retrieved via [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor). Falls back to `sysinfo` when unavailable.

If the [PawnIO](https://github.com/namazso/PawnIO) driver is needed, it can be installed from the Settings screen.

### macOS

Temperature data is retrieved via system APIs.

## Debug Server

Enable the Debug Server in Settings to start an HTTP server on port **19210**, allowing you to retrieve logs and sensor data over LAN.

| Endpoint | Method | Auth | Description |
|---|---|---|---|
| `/` | GET | — | Dashboard (HTML) — tabbed view of hardware, sensors, logs, and window/DWM |
| `/help` | GET | — | API reference. Add `?format=json` for JSON output |
| `/status` | GET | — | Process state (PID, version, log count, PawnIO status) |
| `/api/hardware` | GET | — | Hardware sensor data (JSON) |
| `/api/sensors` | GET | — | Raw sensor list (text) |
| `/api/pawnio` | GET | — | PawnIO driver status (JSON, Windows only) |
| `/api/window` | GET | — | Main window state: HWND, class, styles, DWM attributes, injected DLLs (Windows only) |
| `/api/window/dwm` | POST | token | Live-set a DWM attribute. `?attr=...&value=...&token=...` |
| `/logs` | GET | — | All logs. Filters: `?since=<epoch_ms>&limit=N&level=info&tag=Hardware` |
| `/logs/tail` | GET | — | Latest N lines. `?n=100` (default) |
| `/logs/search` | GET | — | Regex search. `?q=<pattern>&limit=200` |
| `/clear` | POST | token | Clear log buffer |

Add `?format=json` to any log endpoint for JSON output. Default format is plain text.

### Auth token (POST endpoints)

`POST` endpoints require a shared-secret token because the server binds to `0.0.0.0`. On first launch the app generates a 32-hex token and writes it to:

- Windows: `%APPDATA%/Ondo/debug-token`
- macOS: `~/Library/Application Support/Ondo/debug-token`
- Linux: `$XDG_CONFIG_HOME/Ondo/debug-token`

It's also printed to stderr on startup. The token is **not** written to the in-memory log buffer that `/logs` exposes. To rotate, stop the app, delete the file, and restart.

The dashboard's *Window / DWM* tab has an input that stores the token in `localStorage`.

```bash
# Example usage (Mac to Windows over LAN)
curl -s "http://<WINDOWS_IP>:19210/status"
curl -s "http://<WINDOWS_IP>:19210/logs/tail?n=50"
curl -s "http://<WINDOWS_IP>:19210/api/hardware"
curl -s "http://<WINDOWS_IP>:19210/logs/search?q=error&format=json"

# Write endpoints — token required
TOKEN=$(ssh windows-host 'cat $APPDATA/Ondo/debug-token')
curl -sX POST "http://<WINDOWS_IP>:19210/api/window/dwm?attr=backdrop&value=none&token=$TOKEN"
```

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
