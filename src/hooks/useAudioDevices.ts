import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AudioDevice } from "../types";
import { accrueUsage, loadUsage, saveUsage, sortByUsage } from "../audioUsage";
import { captureBackendError, captureOnce } from "../sentry";

interface UseAudioDevicesReturn {
  devices: AudioDevice[];
  switching: boolean;
  error: string | null;
  switchDevice: (deviceId: string, deviceType: "playback" | "recording") => Promise<void>;
}

export function useAudioDevices(): UseAudioDevicesReturn {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [switching, setSwitching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  // Time each device has spent as the default drives the list order
  const usageRef = useRef(loadUsage());
  const lastPollRef = useRef<number | null>(null);

  const fetchDevices = useCallback(async () => {
    try {
      const result = await invoke<AudioDevice[]>("get_audio_devices");
      const now = Date.now();
      if (lastPollRef.current !== null) {
        usageRef.current = accrueUsage(usageRef.current, result, (now - lastPollRef.current) / 1000);
        saveUsage(usageRef.current);
      }
      lastPollRef.current = now;
      setDevices(sortByUsage(result, usageRef.current));
      setError(null);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      // 5秒ごとのポーリングなので初回のみ送信
      captureOnce("get_audio_devices", () =>
        captureBackendError(message, { source: "audio", command: "get_audio_devices" }),
      );
    }
  }, []);

  // Initial fetch and polling
  useEffect(() => {
    fetchDevices();
    intervalRef.current = setInterval(fetchDevices, 5000);

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
      }
    };
  }, [fetchDevices]);

  const switchDevice = useCallback(
    async (deviceId: string, deviceType: "playback" | "recording") => {
      setSwitching(true);
      try {
        await invoke("set_default_audio_device", { deviceId, deviceType });
        // Refresh device list immediately after switching
        await fetchDevices();
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        captureOnce(`set_default_audio_device:${message}`, () =>
          captureBackendError(message, {
            source: "audio",
            command: "set_default_audio_device",
            deviceType,
          }),
        );
      } finally {
        setSwitching(false);
      }
    },
    [fetchDevices]
  );

  return { devices, switching, error, switchDevice };
}
