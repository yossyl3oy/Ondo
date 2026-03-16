import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AudioDevice } from "../types";

interface UseAudioDevicesReturn {
  devices: AudioDevice[];
  switching: boolean;
  error: string | null;
  switchDevice: (deviceId: string) => Promise<void>;
}

export function useAudioDevices(): UseAudioDevicesReturn {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [switching, setSwitching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchDevices = useCallback(async () => {
    try {
      const result = await invoke<AudioDevice[]>("get_audio_devices");
      setDevices(result);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
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
    async (deviceId: string) => {
      setSwitching(true);
      try {
        await invoke("set_default_audio_device", { deviceId });
        // Refresh device list immediately after switching
        await fetchDevices();
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setSwitching(false);
      }
    },
    [fetchDevices]
  );

  return { devices, switching, error, switchDevice };
}
