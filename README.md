# Ondo - Hardware Temperature Monitor

[![Release](https://img.shields.io/github/v/release/yossyl3oy/Ondo?style=flat-square)](https://github.com/yossyl3oy/Ondo/releases)
[![Build](https://img.shields.io/github/actions/workflow/status/yossyl3oy/Ondo/release.yml?style=flat-square)](https://github.com/yossyl3oy/Ondo/actions)
[![License](https://img.shields.io/github/license/yossyl3oy/Ondo?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-blue?style=flat-square)]()

A sleek, HUD-style hardware temperature monitoring widget built with Tauri 2.

## Features

- Real-time CPU & GPU temperature, load, and frequency monitoring
- Per-core CPU temperature and load display
- GPU VRAM usage monitoring
- Storage (SSD/HDD) temperature and usage
- Motherboard temperature and fan speeds
- Screen edge docking with drag support
- Semi-transparent, stylish HUD design
- System theme sync (auto dark/light mode)
- Boot sequence animation
- System tray integration
- Auto-start on system boot (configurable)
- Auto-update support

## Screenshots

The app displays a boot sequence animation on startup, then transitions to the main monitoring widget.

## Download

Download the latest release from the [Releases](https://github.com/yossyl3oy/Ondo/releases) page.

- **Windows**: `.msi` or `.exe` installer
- **macOS**: `.dmg` disk image

## Requirements (Development)

- Node.js 18+
- Rust 1.70+
- Tauri CLI 2.x
- .NET 8 SDK (for LibreHardwareMonitor CLI on Windows)

## Settings

The following options are available in the settings panel:

- **Position**: Widget display position (right, left, corners)
- **Opacity**: Transparency level (30-100%)
- **Always on Top**: Keep widget above other windows
- **Always on Back**: Keep widget below other windows (desktop widget mode)
- **Auto Start**: Launch on system startup
- **Show CPU Cores**: Display individual CPU core temperatures
- **Update Interval**: Refresh rate (500ms-5000ms)
- **Theme**: Theme selection (Auto/Dark/Light)
- **Compact Mode**: Reduced widget size

## Tech Stack

- **Frontend**: React 18 + TypeScript + Vite
- **Backend**: Tauri 2 (Rust)
- **Hardware Monitoring**:
  - Windows: LibreHardwareMonitor (daemon mode) + WMI fallback
  - macOS: System APIs
- **Error Tracking**: Sentry
- **Styling**: CSS with HUD-style animations

## Hardware Monitoring Details

### Windows

Temperature and hardware data is retrieved using [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) with [PawnIO](https://pawnio.eu/) driver support. This provides accurate sensor data for:
- CPU: Temperature, load, frequency (per-core)
- GPU: Temperature, load, frequency, VRAM usage
- Storage: Temperature, used space
- Motherboard: Temperature, fan speeds

#### PawnIO Driver (Recommended)

Starting from March 2025, Windows Defender blocks the legacy WinRing0 driver used by many hardware monitoring tools. Ondo uses PawnIO as a modern, signed replacement driver.

**To enable full hardware monitoring:**
1. Open Ondo Settings
2. Check the "Hardware Driver (PawnIO)" section
3. If not installed, click "Install PawnIO Driver" to download from [pawnio.eu](https://pawnio.eu/)
4. Install PawnIO and restart Ondo

Without PawnIO, some sensors (CPU temperature, motherboard temperature, fan speeds) may not be available. GPU and storage monitoring typically work without the driver.

WMI (Windows Management Instrumentation) is used as a fallback when PawnIO/LHM data is unavailable.

### macOS
Temperature data is retrieved using system APIs. Some sensors may not be available depending on hardware.

## Development Setup

```bash
# Clone with submodules (required for PawnIO-enabled LibreHardwareMonitor)
git clone --recursive https://github.com/yossyl3oy/Ondo.git

# Or if already cloned, initialize submodules
git submodule update --init --recursive

# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## License

MIT License
