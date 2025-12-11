import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { HardwareData } from "../types";
import { captureHardwareError } from "../sentry";

interface UseHardwareDataResult {
  hardwareData: HardwareData;
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
}

const INITIAL_DATA: HardwareData = {
  cpu: null,
  gpu: null,
  timestamp: Date.now(),
};

export function useHardwareData(intervalMs: number = 1000): UseHardwareDataResult {
  const [hardwareData, setHardwareData] = useState<HardwareData>(INITIAL_DATA);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  // エラーを一度だけ送信するためのフラグ
  const errorReportedRef = useRef<{ invoke: boolean; nullData: boolean }>({
    invoke: false,
    nullData: false,
  });

  const fetchData = useCallback(async () => {
    try {
      const data = await invoke<HardwareData>("get_hardware_data");
      setHardwareData(data);
      setError(null);

      // CPU/GPUがnullの場合はエラーとしてSentryに送信（初回のみ）
      if (!errorReportedRef.current.nullData) {
        if (data.cpu === null && data.gpu === null) {
          captureHardwareError("Both CPU and GPU data are null", "both");
          errorReportedRef.current.nullData = true;
        } else if (data.cpu === null) {
          captureHardwareError("CPU data is null", "cpu");
          errorReportedRef.current.nullData = true;
        } else if (data.gpu === null) {
          captureHardwareError("GPU data is null", "gpu");
          errorReportedRef.current.nullData = true;
        }
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setError(errorMessage);

      // invokeエラーをSentryに送信（初回のみ）
      if (!errorReportedRef.current.invoke) {
        captureHardwareError(`invoke error: ${errorMessage}`, "invoke");
        errorReportedRef.current.invoke = true;
      }

      // Use mock data in development for testing UI
      if (import.meta.env.DEV) {
        setHardwareData(generateMockData());
      }
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, intervalMs);
    return () => clearInterval(interval);
  }, [fetchData, intervalMs]);

  return {
    hardwareData,
    isLoading,
    error,
    refresh: fetchData,
  };
}

// Mock data generator for development/testing
function generateMockData(): HardwareData {
  const baseTemp = 45 + Math.random() * 20;
  const gpuTemp = 50 + Math.random() * 25;

  return {
    cpu: {
      name: "AMD Ryzen 9 5900X",
      temperature: Math.round(baseTemp),
      maxTemperature: 95,
      load: Math.round(20 + Math.random() * 40),
      cores: Array.from({ length: 12 }, (_, i) => ({
        index: i,
        temperature: Math.round(baseTemp + (Math.random() - 0.5) * 10),
        load: Math.round(Math.random() * 100),
      })),
    },
    gpu: {
      name: "NVIDIA GeForce RTX 3080",
      temperature: Math.round(gpuTemp),
      maxTemperature: 93,
      load: Math.round(15 + Math.random() * 50),
      memoryUsed: Math.round(4 + Math.random() * 4),
      memoryTotal: 10,
    },
    timestamp: Date.now(),
  };
}
