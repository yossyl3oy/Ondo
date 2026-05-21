import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, SectionType } from "../types";
import { DEFAULT_SETTINGS, SECTION_TYPES } from "../types";
import { captureSettingsError } from "../sentry";

const VALID_SECTIONS = new Set<string>(SECTION_TYPES);

// Keep only known section types and remove duplicates. Stale entries from
// older builds (e.g. a removed "bluetooth" section) would otherwise survive
// in persisted settings and crash the renderer when it tries to dispatch on
// the unknown key.
function sanitizeSectionList(raw: unknown): SectionType[] {
  if (!Array.isArray(raw)) return [];
  const seen = new Set<string>();
  const out: SectionType[] = [];
  for (const item of raw) {
    if (typeof item !== "string") continue;
    if (!VALID_SECTIONS.has(item)) continue;
    if (seen.has(item)) continue;
    seen.add(item);
    out.push(item as SectionType);
  }
  return out;
}

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
    // First drop anything that isn't a current SectionType (and dedupe).
    s.sectionOrder = sanitizeSectionList(s.sectionOrder);
    s.hiddenSections = sanitizeSectionList(s.hiddenSections);

    // Then ensure newly-added default sections are present.
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
