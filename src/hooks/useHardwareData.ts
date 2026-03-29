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
  storage: null,
  motherboard: null,
  network: null,
  display: null,
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
  // ネットワーク速度の平滑化用EMA (インターフェース名 → { dl, ul })
  const networkEmaRef = useRef<Map<string, { dl: number; ul: number }>>(new Map());

  const fetchData = useCallback(async () => {
    try {
      const data = await invoke<HardwareData>("get_hardware_data");

      // ネットワーク速度をEMAで平滑化
      if (data.network) {
        const alpha = 0.3;
        const ema = networkEmaRef.current;
        for (const iface of data.network) {
          const prev = ema.get(iface.name);
          if (prev) {
            iface.receivedPerSec = alpha * iface.receivedPerSec + (1 - alpha) * prev.dl;
            iface.sentPerSec = alpha * iface.sentPerSec + (1 - alpha) * prev.ul;
          }
          ema.set(iface.name, { dl: iface.receivedPerSec, ul: iface.sentPerSec });
        }
      }

      setHardwareData(data);
      setError(null);

      // CPU/GPUがnullの場合はエラーとしてSentryに送信（初回のみ）
      if (!errorReportedRef.current.nullData) {
        if (data.cpu === null && data.gpu === null) {
          // 詳細エラーがある場合はそれを送信
          const errorDetail = data.cpuError || data.gpuError || "Unknown error";
          captureHardwareError(`Both CPU and GPU data are null: ${errorDetail}`, "both");
          errorReportedRef.current.nullData = true;
        } else if (data.cpu === null) {
          const errorDetail = data.cpuError || "Unknown error";
          captureHardwareError(`CPU data is null: ${errorDetail}`, "cpu");
          errorReportedRef.current.nullData = true;
        } else if (data.gpu === null) {
          const errorDetail = data.gpuError || "Unknown error";
          captureHardwareError(`GPU data is null: ${errorDetail}`, "gpu");
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
        const mockData = generateMockData();
        if (mockData.network) {
          const alpha = 0.3;
          const ema = networkEmaRef.current;
          for (const iface of mockData.network) {
            const prev = ema.get(iface.name);
            if (prev) {
              iface.receivedPerSec = alpha * iface.receivedPerSec + (1 - alpha) * prev.dl;
              iface.sentPerSec = alpha * iface.sentPerSec + (1 - alpha) * prev.ul;
            }
            ema.set(iface.name, { dl: iface.receivedPerSec, ul: iface.sentPerSec });
          }
        }
        setHardwareData(mockData);
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
      frequency: 3.7 + Math.random() * 1.0,
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
      frequency: 1.7 + Math.random() * 0.5,
      memoryUsed: Math.round(4 + Math.random() * 4),
      memoryTotal: 10,
    },
    storage: [
      {
        name: "Samsung SSD 980 PRO 1TB",
        temperature: 35 + Math.random() * 10,
        usedSpace: 40 + Math.random() * 30, // percentage (0-100)
        totalSpace: 1000,
      },
    ],
    motherboard: {
      name: "ASUS ROG STRIX B550-F",
      temperature: 40 + Math.random() * 15,
      fans: [
        { name: "CPU Fan", speed: 1200 + Math.round(Math.random() * 500) },
        { name: "Chassis Fan 1", speed: 800 + Math.round(Math.random() * 300) },
      ],
    },
    network: [
      {
        name: "Ethernet",
        receivedPerSec: Math.round(Math.random() * 5_000_000),
        sentPerSec: Math.round(Math.random() * 1_000_000),
      },
    ],
    display: {
      name: "DELL U2723QE",
      refreshRate: 144,
      fps: 90 + Math.round(Math.random() * 50),
      fpsProcessName: "Game",
    },
    timestamp: Date.now(),
  };
}
