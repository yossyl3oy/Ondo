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

export function initSentry() {
  const dsn = import.meta.env.VITE_SENTRY_DSN;

  if (dsn) {
    Sentry.init({
      dsn,
      environment: import.meta.env.MODE,
      release: `ondo@${APP_VERSION}`,
      tracesSampleRate: 0.1,
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
