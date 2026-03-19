import { useState, useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { HudWidget } from "./components/HudWidget";
import { BootSequence } from "./components/BootSequence";
import { SettingsPanel } from "./components/SettingsPanel";
import { UpdateNotification } from "./components/UpdateNotification";
import { RestorePanel } from "./components/RestorePanel";
import { useHardwareData } from "./hooks/useHardwareData";
import { useSettings } from "./hooks/useSettings";
import { useUpdater } from "./hooks/useUpdater";
import { useAudioDevices } from "./hooks/useAudioDevices";
import type { WindowState, SectionType } from "./types";
import "./styles/App.css";

function App() {
  const [isBooting, setIsBooting] = useState(true);
  const [showSettings, setShowSettings] = useState(false);
  const [showRestorePanel, setShowRestorePanel] = useState(false);
  const [showUpdateNotification, setShowUpdateNotification] = useState(true);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [miniMode, setMiniMode] = useState(false);
  const [cursorNear, setCursorNear] = useState(false);
  const savedWindowStateRef = useRef<WindowState | null>(null);
  const { settings, updateSettings } = useSettings();
  const { hardwareData, isLoading, error } = useHardwareData(settings.updateInterval);
  const { updateInfo, checking, downloading, progress, error: updateError, downloadAndInstall, checkForUpdate } = useUpdater();
  const { devices: audioDevices, switching: audioSwitching, switchDevice: switchAudioDevice } = useAudioDevices();

  // Update message based on update check result
  useEffect(() => {
    if (checking) {
      setUpdateMessage(null);
    } else if (updateError) {
      setUpdateMessage(`Error: ${updateError}`);
    } else if (updateInfo?.available) {
      setUpdateMessage(`Update available: v${updateInfo.version}`);
    } else if (updateInfo && !updateInfo.available) {
      setUpdateMessage("You're on the latest version");
    }
  }, [checking, updateInfo, updateError]);

  useEffect(() => {
    const bootTimer = setTimeout(() => {
      setIsBooting(false);
    }, 3000);

    return () => clearTimeout(bootTimer);
  }, []);

  // Listen for tray menu events
  useEffect(() => {
    const unlisten = listen("open-settings", () => {
      setShowSettings(true);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Listen for cursor proximity in mini mode
  useEffect(() => {
    const unlisten = listen<{ near: boolean }>("cursor-near-minimode", (event) => {
      setCursorNear(event.payload.near);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Listen for minimode-changed events from window monitor
  useEffect(() => {
    const unlisten = listen<{ active: boolean }>("minimode-changed", async (event) => {
      const active = event.payload.active;
      setMiniMode(active);
      if (!active) setCursorNear(false);

      try {
        if (active) {
          // Save current window state before shrinking
          const currentState = await invoke<WindowState>("get_window_state");
          savedWindowStateRef.current = currentState;

          // Remove min size constraint so window can shrink below 350px
          await invoke("set_window_min_size", { width: null, height: null });
          // Disable window shadow to remove transparent border in mini mode
          await invoke("set_window_shadow", { enable: false });
          // Make window click-through so users can interact with apps below
          await getCurrentWindow().setIgnoreCursorEvents(true);

          // Wait a frame for React to render mini content, then fit window to it
          requestAnimationFrame(async () => {
            const miniEl = document.querySelector(".mini-content");
            const cssHeight = miniEl
              ? miniEl.getBoundingClientRect().height + 4
              : 120;
            const dpr = window.devicePixelRatio || 1;
            const physicalHeight = Math.round(cssHeight * dpr);
            const miniWidth = Math.round(180 * dpr);
            await invoke("restore_window_state", {
              state: {
                x: currentState.x,
                y: currentState.y,
                width: miniWidth,
                height: physicalHeight,
              },
            });
            // Reposition to settings position
            await invoke("set_window_position", { position: settings.position });
          });
        } else if (savedWindowStateRef.current) {
          // Restore min size constraint
          const dpr = window.devicePixelRatio || 1;
          await invoke("set_window_min_size", {
            width: Math.round(180 * dpr),
            height: Math.round(350 * dpr),
          });
          // Re-enable window shadow
          await invoke("set_window_shadow", { enable: true });
          // Restore click interaction
          await getCurrentWindow().setIgnoreCursorEvents(false);
          // Restore saved window size, then reposition to avoid overflow
          await invoke("restore_window_state", { state: savedWindowStateRef.current });
          savedWindowStateRef.current = null;
          await invoke("set_window_position", { position: settings.position });
        }
      } catch {
        // Ignore errors during window resize
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [settings.position]);

  // Apply theme to document root
  useEffect(() => {
    const root = document.documentElement;
    if (settings.theme === "auto") {
      root.removeAttribute("data-theme");
    } else {
      root.setAttribute("data-theme", settings.theme);
    }
  }, [settings.theme]);

  // Save window state periodically and before close
  const saveWindowStateRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const saveWindowState = useCallback(async () => {
    try {
      const state = await invoke<WindowState>("get_window_state");
      if (state) {
        await updateSettings({ windowState: state });
      }
    } catch {
      // Ignore errors
    }
  }, [updateSettings]);

  // Save window state when app is about to close
  useEffect(() => {
    const handleBeforeUnload = () => {
      saveWindowState();
    };

    window.addEventListener("beforeunload", handleBeforeUnload);
    return () => {
      window.removeEventListener("beforeunload", handleBeforeUnload);
    };
  }, [saveWindowState]);

  // Periodically save window state (debounced)
  useEffect(() => {
    const handleResize = () => {
      if (saveWindowStateRef.current) {
        clearTimeout(saveWindowStateRef.current);
      }
      saveWindowStateRef.current = setTimeout(() => {
        saveWindowState();
      }, 1000);
    };

    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
      if (saveWindowStateRef.current) {
        clearTimeout(saveWindowStateRef.current);
      }
    };
  }, [saveWindowState]);

  const handleSettingsToggle = () => {
    setShowSettings(!showSettings);
  };

  const handleSectionOrderChange = useCallback((order: SectionType[]) => {
    updateSettings({ sectionOrder: order });
  }, [updateSettings]);

  const handleHiddenSectionsChange = useCallback((hidden: SectionType[]) => {
    updateSettings({ hiddenSections: hidden });
  }, [updateSettings]);

  const handleRestoreSection = useCallback((type: SectionType) => {
    const newHidden = settings.hiddenSections.filter((t) => t !== type);
    updateSettings({ hiddenSections: newHidden });
    if (newHidden.length === 0) {
      setShowRestorePanel(false);
    }
  }, [settings.hiddenSections, updateSettings]);

  if (isBooting) {
    return <BootSequence />;
  }

  return (
    <div
      className={`app-container${miniMode ? " mini" : ""}`}
      style={{ opacity: miniMode ? (cursorNear ? (settings.opacity / 100) * 0.1 : (settings.opacity / 100) * 0.5) : settings.opacity / 100 }}
    >
      {!miniMode && <div className="scanlines" />}
      <HudWidget
        hardwareData={hardwareData}
        isLoading={isLoading}
        error={error}
        onSettingsClick={handleSettingsToggle}
        onRestoreClick={() => setShowRestorePanel(true)}
        sectionOrder={settings.sectionOrder}
        onSectionOrderChange={handleSectionOrderChange}
        hiddenSections={settings.hiddenSections}
        onHiddenSectionsChange={handleHiddenSectionsChange}
        audioDevices={audioDevices}
        onSwitchAudioDevice={switchAudioDevice}
        audioSwitching={audioSwitching}
        miniMode={miniMode}
        compactMode={settings.compactMode}
        temperatureUnit={settings.temperatureUnit}
      />
      {showSettings && (
        <SettingsPanel
          settings={settings}
          onSettingsChange={updateSettings}
          onClose={() => setShowSettings(false)}
          onCheckUpdate={checkForUpdate}
          checkingUpdate={checking}
          updateMessage={updateMessage}
          updateInfo={updateInfo}
          onInstallUpdate={downloadAndInstall}
          downloading={downloading}
          downloadProgress={progress}
        />
      )}
      {showRestorePanel && settings.hiddenSections.length > 0 && (
        <RestorePanel
          hiddenSections={settings.hiddenSections}
          onRestore={handleRestoreSection}
          onClose={() => setShowRestorePanel(false)}
        />
      )}
      {showUpdateNotification && updateInfo?.available && !showSettings && (
        <UpdateNotification
          updateInfo={updateInfo}
          downloading={downloading}
          progress={progress}
          error={updateError}
          onUpdate={downloadAndInstall}
          onDismiss={() => setShowUpdateNotification(false)}
        />
      )}
    </div>
  );
}

export default App;
