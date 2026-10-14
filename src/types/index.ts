export interface SensorData {
  name: string;
  value: number;
  unit: string;
  min?: number;
  max?: number;
}

export interface HardwareInfo {
  name: string;
  type: "CPU" | "GPU" | "RAM" | "Storage" | "Motherboard";
  sensors: SensorData[];
}

export interface HardwareData {
  cpu: {
    name: string;
    temperature: number;
    maxTemperature: number;
    load: number;
    frequency: number; // Current frequency in GHz
    cores: Array<{
      index: number;
      temperature: number;
      load: number;
    }>;
  } | null;
  gpu: {
    name: string;
    temperature: number;
    maxTemperature: number;
    load: number;
    frequency: number; // Current frequency in GHz
    memoryUsed: number;
    memoryTotal: number;
  } | null;
  storage: Array<{
    name: string;
    temperature: number;
    usedSpace: number; // in GB
    totalSpace: number; // in GB
  }> | null;
  motherboard: {
    name: string;
    temperature: number;
    fans: Array<{
      name: string;
      speed: number; // RPM
    }>;
  } | null;
  network: Array<{
    name: string;
    receivedPerSec: number; // bytes/sec
    sentPerSec: number; // bytes/sec
  }> | null;
  display: {
    name: string | null; // Monitor model name
    refreshRate: number; // Hz
    fps: number | null;
    fpsProcessName: string | null;
  } | null;
  timestamp: number;
  cpuError?: string;
  gpuError?: string;
}

export interface WindowState {
  x: number;
  y: number;
  width: number;
  height: number;
}

// Native window backdrop (Windows only; ignored elsewhere)
export type BackgroundEffect = "none" | "acrylic" | "mica";

export interface AppSettings {
  position: "right" | "left" | "top-right" | "top-left" | "bottom-right" | "bottom-left";
  /** Opacity of the section cards' backing, 0-100 (panel itself is transparent) */
  sectionOpacity: number;
  backgroundEffect: BackgroundEffect;
  alwaysOnTop: boolean;
  alwaysOnBack: boolean;
  autoStart: boolean;
  updateInterval: number;
  theme: "auto" | "dark" | "light";
  temperatureUnit: "celsius" | "fahrenheit";
  compactMode: boolean;
  debugServer: boolean;
  sectionOrder: SectionType[];
  hiddenSections: SectionType[];
  windowState?: WindowState;
}

export const DEFAULT_SETTINGS: AppSettings = {
  position: "right",
  sectionOpacity: 80,
  backgroundEffect: "none",
  alwaysOnTop: false,
  alwaysOnBack: false,
  autoStart: false,
  updateInterval: 1000,
  theme: "auto",
  temperatureUnit: "celsius",
  compactMode: false,
  debugServer: false,
  sectionOrder: ["cpu", "gpu", "storage", "motherboard", "network", "audio", "display"],
  hiddenSections: [],
};

// Runtime source of truth for valid section types. Used by migrateSettings to
// drop stale/unknown sections from persisted user settings (e.g. removed
// features) so the renderer never receives a key it can't dispatch.
export const SECTION_TYPES = ["cpu", "gpu", "storage", "motherboard", "audio", "network", "display"] as const;

export type SectionType = (typeof SECTION_TYPES)[number];

export interface AudioDevice {
  id: string;
  name: string;
  is_default: boolean;
  device_type: "playback" | "recording";
}

export interface PawnIOStatus {
  installed: boolean;
  checking: boolean;
}
