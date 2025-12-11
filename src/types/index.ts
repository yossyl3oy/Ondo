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
    memoryUsed: number;
    memoryTotal: number;
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

export interface AppSettings {
  position: "right" | "left" | "top-right" | "top-left" | "bottom-right" | "bottom-left";
  opacity: number;
  alwaysOnTop: boolean;
  alwaysOnBack: boolean;
  autoStart: boolean;
  showCpuCores: boolean;
  updateInterval: number;
  theme: "auto" | "dark" | "light";
  compactMode: boolean;
  windowState?: WindowState;
}

export const DEFAULT_SETTINGS: AppSettings = {
  position: "right",
  opacity: 95,
  alwaysOnTop: false,
  alwaysOnBack: false,
  autoStart: false,
  showCpuCores: false,
  updateInterval: 1000,
  theme: "auto",
  compactMode: false,
};
