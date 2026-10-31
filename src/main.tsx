import React from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { Sentry, initSentry } from "./sentry";
import { readStoredSettings } from "./hooks/useSettings";
import type { AppSettings } from "./types";
import App from "./App";
import "./styles/global.css";

async function bootstrap() {
  // Browser preview: `pnpm dev` + `?mock` swaps the Tauri IPC for a mock
  if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("mock")) {
    await import("./devMock");
  }

  // Initialize Sentry error tracking. 起動時のセッション送信より前に、
  // useSettingsと同じ順 (Tauri → localStorage) で読んだ送信オフを反映する
  const stored =
    (await invoke<AppSettings | null>("get_settings").catch(() => null)) ?? readStoredSettings();
  initSentry(stored?.errorReporting ?? true);

  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <Sentry.ErrorBoundary fallback={<ErrorFallback />}>
        <App />
      </Sentry.ErrorBoundary>
    </React.StrictMode>
  );
}

bootstrap();

function ErrorFallback() {
  return (
    <div style={{
      padding: "20px",
      background: "var(--hud-bg, #1a1a2e)",
      color: "var(--hud-danger, #ff4757)",
      fontFamily: "monospace",
      fontSize: "12px",
    }}>
      <p>エラーが発生しました</p>
      <button
        onClick={() => window.location.reload()}
        style={{
          marginTop: "10px",
          padding: "5px 10px",
          background: "var(--hud-primary, #00d4ff)",
          border: "none",
          color: "#000",
          cursor: "pointer",
        }}
      >
        再読み込み
      </button>
    </div>
  );
}
