import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { HardwareData } from "../types";
import { captureHardwareError, captureOnce } from "../sentry";

const EMA_ALPHA = 0.3;
// これ以上 get_hardware_data が返ってこなければ、HUDが止まっているとみなす。
// LHMデーモンの起動直後は最初のデータ待ちで1回の呼び出しが最大10秒かかるので、
// それより十分長くする
const STALL_REPORT_MS = 30_000;
const STALL_CHECK_MS = 1_000;
// チェックの間隔がこれより空いたらスリープ明けとみなし、数え直す
const STALL_CHECK_MAX_STEP_MS = 3_000;

type NetworkInterface = NonNullable<HardwareData["network"]>[number];

// ネットワーク速度をEMAで平滑化（emaマップをin-placeで更新）
function applyNetworkEma(
  interfaces: NetworkInterface[],
  ema: Map<string, { dl: number; ul: number }>,
): void {
  for (const iface of interfaces) {
    const prev = ema.get(iface.name);
    if (prev) {
      iface.receivedPerSec = EMA_ALPHA * iface.receivedPerSec + (1 - EMA_ALPHA) * prev.dl;
      iface.sentPerSec = EMA_ALPHA * iface.sentPerSec + (1 - EMA_ALPHA) * prev.ul;
    }
    ema.set(iface.name, { dl: iface.receivedPerSec, ul: iface.sentPerSec });
  }
}

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
  // ネットワーク速度の平滑化用EMA (インターフェース名 → { dl, ul })
  const networkEmaRef = useRef<Map<string, { dl: number; ul: number }>>(new Map());
  // ネットワークインターフェースの最終アクティブ時刻 (インターフェース名 → timestamp)
  const networkLastActiveRef = useRef<Map<string, number>>(new Map());
  const NETWORK_GRACE_PERIOD_MS = 3000;

  const fetchData = useCallback(async () => {
    // 返ってこない呼び出しは完了時の計測では検知できないのでタイマーで見る。
    // スリープ等でタイマー自体が止まっていた間は待ち時間に数えない
    let lastCheck = Date.now();
    let waited = 0;
    const stallTimer = setInterval(() => {
      const now = Date.now();
      const step = now - lastCheck;
      lastCheck = now;
      waited = step > STALL_CHECK_MAX_STEP_MS ? 0 : waited + step;
      if (waited >= STALL_REPORT_MS) {
        clearInterval(stallTimer);
        captureOnce("hardware-stall", () =>
          captureHardwareError(
            `get_hardware_data has not returned for ${Math.round(waited / 1000)}s`,
            "stall",
          ),
        );
      }
    }, STALL_CHECK_MS);
    try {
      const data = await invoke<HardwareData>("get_hardware_data");

      // ネットワーク: 非アクティブなインターフェースを除外し、EMAで平滑化
      if (data.network) {
        const now = Date.now();
        const lastActive = networkLastActiveRef.current;
        const ema = networkEmaRef.current;

        // rawデータでアクティブ判定を更新
        for (const iface of data.network) {
          if (iface.receivedPerSec > 0 || iface.sentPerSec > 0) {
            lastActive.set(iface.name, now);
          }
        }

        // 猶予期間内にアクティブだったインターフェースのみ残す
        const rawInterfaces = data.network;
        data.network = data.network.filter((iface) => {
          const last = lastActive.get(iface.name);
          return last !== undefined && now - last < NETWORK_GRACE_PERIOD_MS;
        });

        // すべてフィルタリングされた場合、メインアダプターを1つ保持する
        // （最後にアクティブだったもの、またはバックエンドから返された最初のもの）
        if (data.network.length === 0 && rawInterfaces.length > 0) {
          let mainAdapter = rawInterfaces[0];
          let latestTime = 0;
          for (const iface of rawInterfaces) {
            const last = lastActive.get(iface.name) ?? 0;
            if (last > latestTime) {
              latestTime = last;
              mainAdapter = iface;
            }
          }
          data.network = [mainAdapter];
        }

        // EMAで平滑化
        applyNetworkEma(data.network, ema);

        // 非アクティブなインターフェースのEMA状態をクリーンアップ
        const activeNames = new Set(data.network.map((i) => i.name));
        for (const name of ema.keys()) {
          if (!activeNames.has(name)) ema.delete(name);
        }
        for (const name of lastActive.keys()) {
          if (now - (lastActive.get(name) ?? 0) > NETWORK_GRACE_PERIOD_MS * 2) {
            lastActive.delete(name);
          }
        }
      }

      // 欠けた・おかしな値（N/A表示など）はバックエンドの health.rs が
      // ログ付きでSentryに送るので、ここでは見ない
      setHardwareData(data);
      setError(null);
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      setError(errorMessage);

      // invokeエラーをSentryに送信（初回のみ）
      captureOnce("hardware-invoke", () =>
        captureHardwareError(`invoke error: ${errorMessage}`, "invoke"),
      );

      // Use mock data in development for testing UI
      if (import.meta.env.DEV) {
        const mockData = generateMockData();
        if (mockData.network) {
          applyNetworkEma(mockData.network, networkEmaRef.current);
        }
        setHardwareData(mockData);
      }
    } finally {
      clearInterval(stallTimer);
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
