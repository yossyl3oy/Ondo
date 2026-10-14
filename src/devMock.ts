// Browser-only preview harness. Loaded from main.tsx exclusively when the app
// runs under `pnpm dev` with `?mock` in the URL — never bundled into builds.
// Emulates the Tauri IPC surface just enough to render and drive the UI:
//   window.__emit("minimode-changed", { active: true })  → toggle mini mode
//   window.__emit("cursor-near-minimode", { near: true }) → cursor proximity
//   window.__emit("minimode-interactive", { interactive: true }) → zone hover
//   window.__zones → interactive zones the app registered (px, CSS logical)
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { HardwareData } from "./types";

type Listener = { event: string; handler: number };
const listeners: Listener[] = [];

function fakeHardware(): HardwareData {
  const jitter = (base: number, spread: number) =>
    base + (Math.random() - 0.5) * spread;
  return {
    cpu: {
      name: "Intel Core i9-14900K",
      temperature: Math.round(jitter(62, 6)),
      maxTemperature: 100,
      load: Math.round(jitter(12, 8)),
      frequency: 5.2,
      cores: Array.from({ length: 8 }, (_, index) => ({
        index,
        temperature: Math.round(jitter(58, 10)),
        load: Math.round(jitter(15, 20)),
      })),
    },
    gpu: {
      name: "NVIDIA GeForce RTX 4090",
      temperature: Math.round(jitter(58, 6)),
      maxTemperature: 90,
      load: Math.round(jitter(35, 20)),
      frequency: 2.5,
      memoryUsed: 8.2,
      memoryTotal: 24,
    },
    storage: [
      { name: "Samsung 990 PRO", temperature: 42, usedSpace: 1200, totalSpace: 2000 },
    ],
    motherboard: {
      name: "ASUS ROG STRIX",
      temperature: 38,
      fans: [{ name: "CPU Fan", speed: 1200 }],
    },
    network: [
      {
        name: "Ethernet",
        receivedPerSec: jitter(674_000, 200_000),
        sentPerSec: jitter(29_600, 10_000),
      },
    ],
    display: {
      name: "ROG Swift",
      refreshRate: 240,
      fps: Math.round(jitter(401, 30)),
      fpsProcessName: "game.exe",
    },
    timestamp: Date.now(),
  };
}

const audioDevices = [
  { id: "spk-1", name: "スピーカー (RME Babyface Pro)", is_default: true, device_type: "playback", format: { sample_rate: 48000, bits_per_sample: 24, channels: 2 }, connection: "USB" },
  { id: "spk-2", name: "ヘッドホン (Sony WH-1000XM5)", is_default: false, device_type: "playback", format: { sample_rate: 44100, bits_per_sample: 16, channels: 2 }, connection: "Bluetooth" },
  { id: "mic-1", name: "マイク (RME Babyface Pro)", is_default: true, device_type: "recording", format: { sample_rate: 48000, bits_per_sample: 24, channels: 2 }, connection: "USB" },
];

mockWindows("main");

mockIPC((cmd, payload) => {
  switch (cmd) {
    case "plugin:event|listen": {
      const { event, handler } = payload as { event: string; handler: number };
      listeners.push({ event, handler });
      return listeners.length;
    }
    case "plugin:event|unlisten":
      return undefined;
    case "get_hardware_data":
      return fakeHardware();
    case "get_settings":
      return null; // fall back to DEFAULT_SETTINGS
    case "get_audio_devices":
      return audioDevices;
    case "set_default_audio_device": {
      const { deviceId } = payload as { deviceId: string };
      for (const d of audioDevices) {
        if (d.device_type === "playback") d.is_default = d.id === deviceId;
      }
      return undefined;
    }
    case "get_window_state":
      return { x: 40, y: 40, width: 380, height: 600 };
    case "set_mini_interactive_zones": {
      (window as unknown as { __zones: unknown }).__zones = (
        payload as { zones: unknown }
      ).zones;
      console.log("[devMock] interactive zones:", (payload as { zones: unknown }).zones);
      return undefined;
    }
    case "plugin:updater|check":
      return { rid: 1, available: true, currentVersion: "0.1.26", version: "0.1.27" };
    default:
      return undefined;
  }
});

(window as unknown as { __emit: unknown }).__emit = (
  event: string,
  payload: unknown,
) => {
  const internals = (
    window as unknown as {
      __TAURI_INTERNALS__: {
        runCallback: (id: number, arg: unknown) => void;
      };
    }
  ).__TAURI_INTERNALS__;
  for (const l of listeners) {
    if (l.event !== event) continue;
    internals.runCallback(l.handler, { event, id: l.handler, payload });
  }
};

console.log("[devMock] Tauri IPC mocked — use window.__emit(event, payload)");
