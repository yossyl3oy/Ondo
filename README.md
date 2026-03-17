# Ondo - Hardware Temperature Monitor

[![Release](https://img.shields.io/github/v/release/yossyl3oy/Ondo?style=flat-square)](https://github.com/yossyl3oy/Ondo/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/yossyl3oy/Ondo/release.yml?style=flat-square)](https://github.com/yossyl3oy/Ondo/actions)
[![License](https://img.shields.io/github/license/yossyl3oy/Ondo?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-blue?style=flat-square)]()

An Iron Man HUD-style hardware temperature monitoring widget built with Tauri 2.

## Features

- Real-time CPU & GPU temperature, load, and frequency monitoring
- Per-core CPU temperature and load display
- GPU VRAM usage monitoring
- Storage (SSD/HDD) temperature and usage
- Motherboard temperature and fan speeds
- Network speed monitoring (download/upload) with live graph
- Audio device switching
- Drag-and-drop section reordering with hide/restore
- Three display modes: Normal, Compact, and Mini
- Screen edge docking with drag support
- Semi-transparent, frameless HUD design
- System theme sync (Auto/Dark/Light)
- Boot sequence animation
- System tray integration
- Auto-start on system boot (configurable)
- Auto-update support
- Remote debug server for LAN diagnostics

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

Hardware data is retrieved via [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) running as a daemon process (`ondo-hwmon.exe`), communicating over stdout in JSON. Monitored sensors:

- **CPU**: Temperature, load, frequency (per-core)
- **GPU**: Temperature, load, frequency, VRAM usage (NVIDIA / AMD / Intel)
- **Storage**: Temperature, used space, total capacity
- **Motherboard**: Temperature, fan speeds (RPM)
- **Network**: Download/upload speed per adapter

Falls back to `sysinfo` for basic data when LibreHardwareMonitor is unavailable.

#### PawnIO Driver

[PawnIO](https://github.com/namazso/PawnIO) is a modern, signed driver that replaces the legacy WinRing0 driver (blocked by Windows Defender since March 2025 due to CVE-2020-14979). The installer is bundled with Ondo.

If sensors are not showing:
1. Open Settings (gear icon)
2. Click "Install PawnIO Driver"
3. Accept the UAC prompt
4. Restart Ondo

PawnIO is licensed under GPL-2.0 and developed by [namazso](https://github.com/namazso/PawnIO).

### macOS

Temperature data is retrieved using system APIs. Some sensors may not be available depending on hardware.

## Debug Server

Enable the Debug Server in Settings to start an HTTP server on port **19210**, allowing you to retrieve logs and sensor data over LAN.

| Endpoint | Method | Description |
|---|---|---|
| `/` | GET | Dashboard (HTML) — tabbed view of hardware, sensors, and logs |
| `/help` | GET | API reference. Add `?format=json` for JSON output |
| `/status` | GET | Process state (PID, version, log count, PawnIO status) |
| `/api/hardware` | GET | Hardware sensor data (JSON) |
| `/api/sensors` | GET | Raw sensor list (text) |
| `/api/pawnio` | GET | PawnIO driver status (JSON, Windows only) |
| `/logs` | GET | All logs. Filters: `?since=<epoch_ms>&limit=N&level=info&tag=Hardware` |
| `/logs/tail` | GET | Latest N lines. `?n=100` (default) |
| `/logs/search` | GET | Regex search. `?q=<pattern>&limit=200` |
| `/clear` | POST | Clear log buffer |

Add `?format=json` to any log endpoint for JSON output. Default format is plain text.

```bash
# Example usage (Mac to Windows over LAN)
curl -s "http://<WINDOWS_IP>:19210/status"
curl -s "http://<WINDOWS_IP>:19210/logs/tail?n=50"
curl -s "http://<WINDOWS_IP>:19210/api/hardware"
curl -s "http://<WINDOWS_IP>:19210/logs/search?q=error&format=json"
```

## Tech Stack

- **Frontend**: React 19 + TypeScript + Vite
- **Backend**: Tauri 2 (Rust)
- **Hardware Monitoring**:
  - Windows: LibreHardwareMonitor (.NET 8, daemon mode) + sysinfo fallback
  - macOS: System APIs
- **Package Manager**: pnpm (managed via [mise](https://mise.jdx.dev/))
- **Error Tracking**: Sentry

## Development

### Requirements

- [mise](https://mise.jdx.dev/) (manages Node.js and pnpm)
- Rust 1.70+
- Tauri CLI 2.x
- .NET 8 SDK (Windows only, for building `ondo-hwmon`)

### Setup

```bash
git clone https://github.com/yossyl3oy/Ondo.git
cd Ondo
pnpm install
pnpm tauri dev
```

### Build

```bash
pnpm tauri build
```

## License

[MIT](LICENSE)

### Third-Party Licenses

- [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) — MPL-2.0
- [PawnIO](https://github.com/namazso/PawnIO) — GPL-2.0
