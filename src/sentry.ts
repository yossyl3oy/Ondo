import * as Sentry from "@sentry/react";

// プラットフォーム検出
function getPlatform(): string {
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes("win")) return "windows";
  if (ua.includes("mac")) return "macos";
  if (ua.includes("linux")) return "linux";
  return "unknown";
}

// Get version from package.json (injected by Vite)
const APP_VERSION = __APP_VERSION__;

/**
 * 設定の「Send Error Reports」。initは常に行い、送信の直前で見るので
 * 切り替えは再起動なしで効く。起動ごとのセッションやclient reportは
 * beforeSendを通らないため、transportで止める
 */
let reportingEnabled = true;

export function setErrorReporting(enabled: boolean) {
  reportingEnabled = enabled;
}

export function initSentry(enabled: boolean) {
  setErrorReporting(enabled);
  const dsn = import.meta.env.VITE_SENTRY_DSN;

  if (dsn) {
    Sentry.init({
      dsn,
      environment: import.meta.env.MODE,
      release: `ondo@${APP_VERSION}`,
      tracesSampleRate: 0.1,
      transport: (options) => {
        const inner = Sentry.makeFetchTransport(options);
        return {
          ...inner,
          send: (envelope) => (reportingEnabled ? inner.send(envelope) : Promise.resolve({})),
        };
      },
      initialScope: {
        tags: {
          app: "ondo",
          platform: getPlatform(),
          version: APP_VERSION,
        },
      },
    });
  }
}

const reportedKeys = new Set<string>();

/**
 * `send` を `key` ごとにセッション中1回だけ実行する。
 * ポーリングで毎回起きる失敗を毎秒送らないため。
 * 送信オフの間は記録しないので、オンに戻せば一度は送られる
 */
export function captureOnce(key: string, send: () => void) {
  if (!reportingEnabled || reportedKeys.has(key)) return;
  reportedKeys.add(key);
  send();
}

export function testSentryError() {
  Sentry.captureException(new Error("Sentry test error from Ondo"));
  console.log("[Sentry] Test error sent");
}

/**
 * バックエンドエラーをSentryに送信
 */
export function captureBackendError(
  error: string,
  context: { source: string; [key: string]: unknown }
) {
  Sentry.captureException(new Error(`[Backend] ${error}`), {
    tags: {
      source: context.source,
      platform: getPlatform(),
    },
    extra: context,
  });
}

/**
 * ハードウェアデータ取得エラーをSentryに送信
 */
export function captureHardwareError(error: string, hardwareType?: string) {
  Sentry.captureException(new Error(`[Hardware] ${error}`), {
    tags: {
      source: "hardware",
      hardwareType: hardwareType || "unknown",
      platform: getPlatform(),
    },
  });
}

/**
 * 設定関連エラーをSentryに送信
 */
export function captureSettingsError(error: string, operation: string) {
  Sentry.captureException(new Error(`[Settings] ${error}`), {
    tags: {
      source: "settings",
      operation,
      platform: getPlatform(),
    },
  });
}

/**
 * ウィンドウ関連エラーをSentryに送信
 */
export function captureWindowError(error: string, operation: string) {
  Sentry.captureException(new Error(`[Window] ${error}`), {
    tags: {
      source: "window",
      operation,
      platform: getPlatform(),
    },
  });
}

/**
 * アップデート関連エラーをSentryに送信
 */
export function captureUpdateError(error: string) {
  Sentry.captureException(new Error(`[Update] ${error}`), {
    tags: {
      source: "update",
      platform: getPlatform(),
    },
  });
}

export { Sentry };
