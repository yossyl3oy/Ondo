import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "../types";
import { DEFAULT_SETTINGS } from "../types";

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

  // Load settings on mount
  useEffect(() => {
    loadSettings();
  }, []);

  const loadSettings = async () => {
    try {
      // Try to load from Tauri store first
      const stored = await invoke<AppSettings | null>("get_settings");
      if (stored) {
        setSettings({ ...DEFAULT_SETTINGS, ...stored });
      } else {
        // Fallback to localStorage for development
        const localStored = localStorage.getItem(STORAGE_KEY);
        if (localStored) {
          setSettings({ ...DEFAULT_SETTINGS, ...JSON.parse(localStored) });
        }
      }
    } catch {
      // Fallback to localStorage
      const localStored = localStorage.getItem(STORAGE_KEY);
      if (localStored) {
        try {
          setSettings({ ...DEFAULT_SETTINGS, ...JSON.parse(localStored) });
        } catch {
          // Use defaults
        }
      }
    } finally {
      setIsLoading(false);
    }
  };

  const updateSettings = useCallback(async (newSettings: Partial<AppSettings>) => {
    const updated = { ...settings, ...newSettings };
    setSettings(updated);

    // Save to localStorage immediately
    localStorage.setItem(STORAGE_KEY, JSON.stringify(updated));

    try {
      // Try to save via Tauri
      await invoke("save_settings", { settings: updated });

      // Apply window settings
      if (newSettings.alwaysOnTop !== undefined) {
        await invoke("set_always_on_top", { enabled: newSettings.alwaysOnTop });
      }
      if (newSettings.position !== undefined) {
        await invoke("set_window_position", { position: newSettings.position });
      }
      if (newSettings.autoStart !== undefined) {
        await invoke("set_auto_start", { enabled: newSettings.autoStart });
      }
    } catch {
      // Settings saved to localStorage as fallback
    }
  }, [settings]);

  const resetSettings = useCallback(async () => {
    setSettings(DEFAULT_SETTINGS);
    localStorage.setItem(STORAGE_KEY, JSON.stringify(DEFAULT_SETTINGS));

    try {
      await invoke("save_settings", { settings: DEFAULT_SETTINGS });
    } catch {
      // Reset completed locally
    }
  }, []);

  return {
    settings,
    updateSettings,
    resetSettings,
    isLoading,
  };
}
