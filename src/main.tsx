import React from "react";
import ReactDOM from "react-dom/client";
import { Sentry, initSentry } from "./sentry";
import App from "./App";
import "./styles/global.css";

// Initialize Sentry error tracking
initSentry();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Sentry.ErrorBoundary fallback={<ErrorFallback />}>
      <App />
    </Sentry.ErrorBoundary>
  </React.StrictMode>
);

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
