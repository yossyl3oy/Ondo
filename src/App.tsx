import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { HudWidget } from "./components/HudWidget";
import { BootSequence } from "./components/BootSequence";
import { SettingsPanel } from "./components/SettingsPanel";
import { useHardwareData } from "./hooks/useHardwareData";
import { useSettings } from "./hooks/useSettings";
import "./styles/App.css";

function App() {
  const [isBooting, setIsBooting] = useState(true);
  const [showSettings, setShowSettings] = useState(false);
  const { settings, updateSettings } = useSettings();
  const { hardwareData, isLoading, error } = useHardwareData(settings.updateInterval);

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

  const handleSettingsToggle = () => {
    setShowSettings(!showSettings);
  };

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
      />
      {showSettings && (
        <SettingsPanel
          settings={settings}
          onSettingsChange={updateSettings}
          onClose={() => setShowSettings(false)}
        />
      )}
    </div>
  );
}

export default App;
