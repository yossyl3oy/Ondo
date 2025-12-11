import { useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useEffect } from "react";
import type { HardwareData } from "../types";
import { TemperatureGauge } from "./TemperatureGauge";
import { CpuCoreGrid } from "./CpuCoreGrid";
import "./HudWidget.css";

interface HudWidgetProps {
  hardwareData: HardwareData;
  isLoading: boolean;
  error: string | null;
  onSettingsClick: () => void;
}

export function HudWidget({
  hardwareData,
  isLoading,
  error,
  onSettingsClick,
}: HudWidgetProps) {
  const [showCores, setShowCores] = useState(false);
  const [version, setVersion] = useState("1.0.0");
  const { cpu, gpu } = hardwareData;

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  const getTemperatureStatus = (temp: number, max: number) => {
    const ratio = temp / max;
    if (ratio >= 0.9) return "danger";
    if (ratio >= 0.75) return "warning";
    return "normal";
  };

  return (
    <div className="hud-widget">
      {/* Drag region - separate from header */}
      <div className="drag-region" />

      {/* Header */}
      <div className="hud-header">
        <div className="hud-title">
          <span className="hud-title-icon">◈</span>
          <span className="hud-title-text">ONDO</span>
          <span className="hud-title-version">v{version}</span>
        </div>
        <button
          className="hud-settings-btn"
          onClick={onSettingsClick}
          title="Settings"
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
            <path d="M19.14 12.94c.04-.31.06-.63.06-.94 0-.31-.02-.63-.06-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.04.31-.06.63-.06.94s.02.63.06.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z" />
          </svg>
        </button>
      </div>

      {/* Main content */}
      <div className="hud-content">
        {isLoading && !cpu && !gpu ? (
          <div className="hud-loading">
            <div className="loading-spinner" />
            <span>Connecting...</span>
          </div>
        ) : error && !cpu && !gpu ? (
          <div className="hud-error">
            <span className="error-icon">⚠</span>
            <span className="error-text">Sensor unavailable</span>
          </div>
        ) : (
          <>
            {/* CPU Section */}
            {cpu && (
              <div className="hud-section">
                <div
                  className="hud-section-header"
                  onClick={() => setShowCores(!showCores)}
                  style={{ cursor: "pointer" }}
                >
                  <div className="section-indicator cpu" />
                  <span className="section-label">CPU</span>
                  <span className="section-name" title={cpu.name}>
                    {shortenName(cpu.name)}
                  </span>
                  <span className="expand-icon">{showCores ? "▾" : "▸"}</span>
                </div>

                <div className="hud-metrics">
                  <TemperatureGauge
                    value={cpu.temperature}
                    max={cpu.maxTemperature}
                    status={getTemperatureStatus(
                      cpu.temperature,
                      cpu.maxTemperature
                    )}
                    label="TEMP"
                  />
                  <div className="metric-divider" />
                  <div className="metric-item">
                    <span className="metric-label">LOAD</span>
                    <span className="metric-value">{cpu.load}%</span>
                    <div className="metric-bar">
                      <div
                        className="metric-bar-fill"
                        style={{ width: `${cpu.load}%` }}
                      />
                    </div>
                  </div>
                </div>

                {showCores && cpu.cores && cpu.cores.length > 0 && (
                  <CpuCoreGrid cores={cpu.cores} maxTemp={cpu.maxTemperature} />
                )}
              </div>
            )}

            {/* GPU Section */}
            {gpu && (
              <div className="hud-section">
                <div className="hud-section-header">
                  <div className="section-indicator gpu" />
                  <span className="section-label">GPU</span>
                  <span className="section-name" title={gpu.name}>
                    {shortenName(gpu.name)}
                  </span>
                </div>

                <div className="hud-metrics">
                  <TemperatureGauge
                    value={gpu.temperature}
                    max={gpu.maxTemperature}
                    status={getTemperatureStatus(
                      gpu.temperature,
                      gpu.maxTemperature
                    )}
                    label="TEMP"
                  />
                  <div className="metric-divider" />
                  <div className="metric-item">
                    <span className="metric-label">LOAD</span>
                    <span className="metric-value">{gpu.load}%</span>
                    <div className="metric-bar">
                      <div
                        className="metric-bar-fill gpu"
                        style={{ width: `${gpu.load}%` }}
                      />
                    </div>
                  </div>
                </div>

                <div className="gpu-memory">
                  <span className="memory-label">VRAM</span>
                  <span className="memory-value">
                    {gpu.memoryUsed.toFixed(1)}/{gpu.memoryTotal}GB
                  </span>
                  <div className="memory-bar">
                    <div
                      className="memory-bar-fill"
                      style={{
                        width: `${(gpu.memoryUsed / gpu.memoryTotal) * 100}%`,
                      }}
                    />
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {/* Footer status */}
      <div className="hud-footer">
        <div className="status-indicator online" />
        <span className="status-text">MONITORING</span>
        <span className="timestamp">
          {new Date().toLocaleTimeString("ja-JP", {
            hour: "2-digit",
            minute: "2-digit",
            second: "2-digit",
          })}
        </span>
      </div>
    </div>
  );
}

function shortenName(name: string): string {
  // Shorten common hardware names
  return name
    .replace("AMD Ryzen ", "R")
    .replace("Intel Core ", "")
    .replace("NVIDIA GeForce ", "")
    .replace("AMD Radeon ", "")
    .replace(" Processor", "")
    .replace("-Core", "C")
    .substring(0, 18);
}
