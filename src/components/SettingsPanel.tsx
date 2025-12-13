import { useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import type { AppSettings } from "../types";
import { testSentryError } from "../sentry";
import "./SettingsPanel.css";

interface UpdateInfo {
  available: boolean;
  version?: string;
  notes?: string;
}

interface SettingsPanelProps {
  settings: AppSettings;
  onSettingsChange: (settings: Partial<AppSettings>) => void;
  onClose: () => void;
  onCheckUpdate?: () => void;
  checkingUpdate?: boolean;
  updateMessage?: string | null;
  updateInfo?: UpdateInfo | null;
  onInstallUpdate?: () => void;
  downloading?: boolean;
  downloadProgress?: number;
}

export function SettingsPanel({
  settings,
  onSettingsChange,
  onClose,
  onCheckUpdate,
  checkingUpdate,
  updateMessage,
  updateInfo,
  onInstallUpdate,
  downloading,
  downloadProgress,
}: SettingsPanelProps) {
  const [version, setVersion] = useState("1.0.0");

  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  return (
    <div className="settings-overlay" onClick={onClose}>
      <div className="settings-panel" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <span className="settings-title">SETTINGS</span>
          <button className="settings-close" onClick={onClose}>
            ×
          </button>
        </div>

        <div className="settings-content">
          {/* Position */}
          <div className="setting-group">
            <label className="setting-label">Position</label>
            <select
              className="setting-select"
              value={settings.position}
              onChange={(e) =>
                onSettingsChange({
                  position: e.target.value as AppSettings["position"],
                })
              }
            >
              <option value="right">Right</option>
              <option value="left">Left</option>
              <option value="top-right">Top Right</option>
              <option value="top-left">Top Left</option>
              <option value="bottom-right">Bottom Right</option>
              <option value="bottom-left">Bottom Left</option>
            </select>
          </div>

          {/* Opacity */}
          <div className="setting-group">
            <label className="setting-label">
              Opacity: {settings.opacity}%
            </label>
            <input
              type="range"
              className="setting-slider"
              min="30"
              max="100"
              value={settings.opacity}
              onChange={(e) =>
                onSettingsChange({ opacity: parseInt(e.target.value) })
              }
            />
          </div>

          {/* Update Interval */}
          <div className="setting-group">
            <label className="setting-label">
              Update Interval: {settings.updateInterval}ms
            </label>
            <input
              type="range"
              className="setting-slider"
              min="500"
              max="5000"
              step="500"
              value={settings.updateInterval}
              onChange={(e) =>
                onSettingsChange({ updateInterval: parseInt(e.target.value) })
              }
            />
          </div>

          {/* Theme */}
          <div className="setting-group">
            <label className="setting-label">Theme</label>
            <select
              className="setting-select"
              value={settings.theme}
              onChange={(e) =>
                onSettingsChange({
                  theme: e.target.value as AppSettings["theme"],
                })
              }
            >
              <option value="auto">Auto (System)</option>
              <option value="dark">Dark</option>
              <option value="light">Light</option>
            </select>
          </div>

          {/* Toggle switches */}
          <div className="setting-group toggle-group">
            <label className="setting-toggle">
              <span>Always on Top</span>
              <input
                type="checkbox"
                checked={settings.alwaysOnTop}
                onChange={(e) =>
                  onSettingsChange({
                    alwaysOnTop: e.target.checked,
                    alwaysOnBack: e.target.checked ? false : settings.alwaysOnBack,
                  })
                }
              />
              <span className="toggle-slider" />
            </label>
          </div>

          <div className="setting-group toggle-group">
            <label className="setting-toggle">
              <span>Always on Back</span>
              <input
                type="checkbox"
                checked={settings.alwaysOnBack}
                onChange={(e) =>
                  onSettingsChange({
                    alwaysOnBack: e.target.checked,
                    alwaysOnTop: e.target.checked ? false : settings.alwaysOnTop,
                  })
                }
              />
              <span className="toggle-slider" />
            </label>
          </div>

          <div className="setting-group toggle-group">
            <label className="setting-toggle">
              <span>Auto Start with Windows</span>
              <input
                type="checkbox"
                checked={settings.autoStart}
                onChange={(e) =>
                  onSettingsChange({ autoStart: e.target.checked })
                }
              />
              <span className="toggle-slider" />
            </label>
          </div>

          <div className="setting-group toggle-group">
            <label className="setting-toggle">
              <span>Show CPU Cores</span>
              <input
                type="checkbox"
                checked={settings.showCpuCores}
                onChange={(e) =>
                  onSettingsChange({ showCpuCores: e.target.checked })
                }
              />
              <span className="toggle-slider" />
            </label>
          </div>

          <div className="setting-group toggle-group">
            <label className="setting-toggle">
              <span>Compact Mode</span>
              <input
                type="checkbox"
                checked={settings.compactMode}
                onChange={(e) =>
                  onSettingsChange({ compactMode: e.target.checked })
                }
              />
              <span className="toggle-slider" />
            </label>
          </div>

          {/* Update Check Button */}
          {onCheckUpdate && (
            <div className="setting-group">
              <button
                className="setting-button"
                onClick={onCheckUpdate}
                disabled={checkingUpdate || downloading}
              >
                {checkingUpdate ? "Checking..." : "Check for Updates"}
              </button>
              {updateMessage && (
                <div className="update-message">{updateMessage}</div>
              )}
              {updateInfo?.available && onInstallUpdate && (
                <div className="update-install-section">
                  {downloading ? (
                    <div className="update-progress">
                      <div className="update-progress-bar">
                        <div
                          className="update-progress-fill"
                          style={{ width: `${downloadProgress || 0}%` }}
                        />
                      </div>
                      <span className="update-progress-text">
                        Downloading... {Math.round(downloadProgress || 0)}%
                      </span>
                    </div>
                  ) : (
                    <button
                      className="setting-button setting-button-update"
                      onClick={onInstallUpdate}
                    >
                      Install Update v{updateInfo.version}
                    </button>
                  )}
                </div>
              )}
            </div>
          )}

          {/* Debug: Test Sentry */}
          {import.meta.env.DEV && (
            <div className="setting-group">
              <button
                className="setting-button setting-button-debug"
                onClick={testSentryError}
              >
                Test Sentry Error
              </button>
            </div>
          )}
        </div>

        <div className="settings-footer">
          <span className="settings-version">Ondo v{version}</span>
        </div>
      </div>
    </div>
  );
}
