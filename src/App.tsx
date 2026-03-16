import { useState, useEffect, useRef, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
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
      className="app-container"
      style={{ opacity: settings.opacity / 100 }}
    >
      <div className="scanlines" />
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
