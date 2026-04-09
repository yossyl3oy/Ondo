import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "../types";
import { DEFAULT_SETTINGS } from "../types";
import { captureSettingsError } from "../sentry";

interface UseSettingsResult {
  settings: AppSettings;
  updateSettings: (newSettings: Partial<AppSettings>) => Promise<void>;
  resetSettings: () => Promise<void>;
  isLoading: boolean;
}

const STORAGE_KEY = "ondo_settings";

export function useSettings(): UseSettingsResult {
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [isLoading, setIsLoading] = useState(true);
  const settingsRef = useRef<AppSettings>(DEFAULT_SETTINGS);
  const saveChainRef = useRef<Promise<void>>(Promise.resolve());

  // Load settings on mount
  useEffect(() => {
    loadSettings();
  }, []);

  const migrateSettings = (s: AppSettings): AppSettings => {
    // Ensure new section types are added to existing sectionOrder
    for (const section of DEFAULT_SETTINGS.sectionOrder) {
      if (!s.sectionOrder.includes(section) && !s.hiddenSections.includes(section)) {
        s.sectionOrder.push(section);
      }
    }
    return s;
  };

  const loadSettings = async () => {
    try {
      // Try to load from Tauri store first
      const stored = await invoke<AppSettings | null>("get_settings");
      if (stored) {
        const loaded = migrateSettings({ ...DEFAULT_SETTINGS, ...stored });
        settingsRef.current = loaded;
        setSettings(loaded);
      } else {
        // Fallback to localStorage for development
        const localStored = localStorage.getItem(STORAGE_KEY);
        if (localStored) {
          const loaded = migrateSettings({ ...DEFAULT_SETTINGS, ...JSON.parse(localStored) });
          settingsRef.current = loaded;
          setSettings(loaded);
        }
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : String(err);
      captureSettingsError(errorMessage, "load");

      // Fallback to localStorage
      const localStored = localStorage.getItem(STORAGE_KEY);
      if (localStored) {
        try {
          const loaded = migrateSettings({ ...DEFAULT_SETTINGS, ...JSON.parse(localStored) });
          settingsRef.current = loaded;
          setSettings(loaded);
        } catch {
          // Use defaults
        }
      }
    } finally {
      setIsLoading(false);
    }
  };

  const persistSettings = useCallback((updated: AppSettings, newSettings: Partial<AppSettings>) => {
    saveChainRef.current = saveChainRef.current
      .catch(() => {})
      .then(async () => {
        try {
          // Try to save via Tauri
          await invoke("save_settings", { settings: updated });

          // Apply window settings
          if (newSettings.alwaysOnTop !== undefined) {
            // If enabling always on top, disable always on back first
            if (newSettings.alwaysOnTop) {
              await invoke("set_always_on_back", { enabled: false });
            }
            await invoke("set_always_on_top", { enabled: newSettings.alwaysOnTop });
          }
          if (newSettings.alwaysOnBack !== undefined) {
            // If enabling always on back, disable always on top first
            if (newSettings.alwaysOnBack) {
              await invoke("set_always_on_top", { enabled: false });
            }
            await invoke("set_always_on_back", { enabled: newSettings.alwaysOnBack });
          }
          if (newSettings.position !== undefined) {
            await invoke("set_window_position", { position: newSettings.position });
          }
          if (newSettings.autoStart !== undefined) {
            await invoke("set_auto_start", { enabled: newSettings.autoStart });
          }
          if (newSettings.debugServer !== undefined) {
            await invoke("toggle_debug_server", { enabled: newSettings.debugServer });
          }
        } catch (err) {
          const errorMessage = err instanceof Error ? err.message : String(err);
          captureSettingsError(errorMessage, "save");
        }
      });

    return saveChainRef.current;
  }, []);

  const updateSettings = useCallback(async (newSettings: Partial<AppSettings>) => {
    const updated = { ...settingsRef.current, ...newSettings };
    settingsRef.current = updated;
    setSettings(updated);

    // Save to localStorage immediately
    localStorage.setItem(STORAGE_KEY, JSON.stringify(updated));
    await persistSettings(updated, newSettings);
  }, [persistSettings]);

  const resetSettings = useCallback(async () => {
    settingsRef.current = DEFAULT_SETTINGS;
    setSettings(DEFAULT_SETTINGS);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(DEFAULT_SETTINGS));

    saveChainRef.current = saveChainRef.current
      .catch(() => {})
      .then(async () => {
        try {
          await invoke("save_settings", { settings: DEFAULT_SETTINGS });
        } catch (err) {
          const errorMessage = err instanceof Error ? err.message : String(err);
          captureSettingsError(errorMessage, "reset");
        }
      });

    await saveChainRef.current;
  }, []);

  return {
    settings,
    updateSettings,
    resetSettings,
    isLoading,
  };
}
