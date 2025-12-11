# Ondo - Hardware Temperature Monitor

A sleek, HUD-style hardware temperature monitoring widget.

## Features

- Real-time CPU & GPU temperature display
- Screen edge docking
- Semi-transparent, stylish HUD design
- System theme sync (auto dark/light mode)
- Boot sequence animation
- System tray integration
- Auto-start on system boot (configurable)
- Auto-update support

## Screenshots

The app displays a boot sequence animation on startup, then transitions to the main monitoring widget.

## Requirements

- Windows 10/11 or macOS
- Node.js 18+
- Rust 1.70+
- Tauri CLI

## Setup

```bash
# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Settings

The following options are available in the settings panel:

- **Position**: Widget display position (right, left, corners)
- **Opacity**: Transparency level (30-100%)
- **Always on Top**: Keep widget above other windows
- **Auto Start**: Launch on system startup
- **Show CPU Cores**: Display individual CPU core temperatures
- **Update Interval**: Refresh rate (500ms-5000ms)
- **Theme**: Theme selection (Auto/Dark/Light)

## Tech Stack

- **Frontend**: React 18 + TypeScript + Vite
- **Backend**: Tauri 2 (Rust)
- **Hardware Monitoring**: WMI (Windows Management Instrumentation)
- **Styling**: CSS with HUD-style animations

## Temperature Data

On Windows, temperature data is retrieved using native WMI. For more detailed temperature information, consider installing and running LibreHardwareMonitor.

On macOS, temperature data is retrieved using system APIs.

## License

MIT License
