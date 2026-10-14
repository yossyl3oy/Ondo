import type { AudioDevice } from "./types";

/** Seconds each device has spent as the default, keyed by usageKey() */
export type AudioUsage = Record<string, number>;

const STORAGE_KEY = "ondo_audio_usage";

// Polls are ~5s apart; a larger gap means the machine slept or the app was
// suspended, and that time shouldn't count as use.
const MAX_ACCRUAL_SECONDS = 10;

// Playback and recording ids can coincide on some platforms
export function usageKey(device: AudioDevice): string {
  return `${device.device_type}:${device.id}`;
}

/** Credits `elapsedSeconds` to every current default device. */
export function accrueUsage(
  usage: AudioUsage,
  devices: AudioDevice[],
  elapsedSeconds: number,
): AudioUsage {
  const seconds = Math.min(Math.max(elapsedSeconds, 0), MAX_ACCRUAL_SECONDS);
  if (seconds === 0) return usage;
  const next = { ...usage };
  for (const device of devices) {
    if (!device.is_default) continue;
    const key = usageKey(device);
    next[key] = (next[key] ?? 0) + seconds;
  }
  return next;
}

/** Most-used first; ties keep the order the OS reported. */
export function sortByUsage(devices: AudioDevice[], usage: AudioUsage): AudioDevice[] {
  return devices
    .map((device, index) => ({ device, index, used: usage[usageKey(device)] ?? 0 }))
    .sort((a, b) => b.used - a.used || a.index - b.index)
    .map(({ device }) => device);
}

export function loadUsage(): AudioUsage {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    if (!parsed || typeof parsed !== "object") return {};
    return Object.fromEntries(
      Object.entries(parsed).filter(([, v]) => typeof v === "number" && Number.isFinite(v)),
    );
  } catch {
    return {};
  }
}

export function saveUsage(usage: AudioUsage): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(usage));
  } catch {
    // Ordering is a convenience; losing it is harmless
  }
}
