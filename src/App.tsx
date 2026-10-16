import { useState, useEffect, useRef, useCallback } from "react";
import type { CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { HudWidget } from "./components/HudWidget";
import { BootSequence } from "./components/BootSequence";
import { SettingsPanel } from "./components/SettingsPanel";
import { UpdateNotification } from "./components/UpdateNotification";
import { RestorePanel } from "./components/RestorePanel";
import { ResizeHandle } from "./components/ResizeHandle";
import { useHardwareData } from "./hooks/useHardwareData";
import { useSettings } from "./hooks/useSettings";
import { useUpdater } from "./hooks/useUpdater";
import { useAudioDevices } from "./hooks/useAudioDevices";
import type { WindowState, SectionType } from "./types";
import "./styles/App.css";

// Window height floor while the settings / restore overlay is open (CSS px)
const OVERLAY_MIN_HEIGHT = 560;
// The update notice floats 60px above the bottom; leave room for it plus the header
const NOTICE_CLEARANCE = 110;

function App() {
  const [isBooting, setIsBooting] = useState(true);
  const [showSettings, setShowSettings] = useState(false);
  const [showRestorePanel, setShowRestorePanel] = useState(false);
  const [showUpdateNotification, setShowUpdateNotification] = useState(true);
  const [dismissedUpdateVersion, setDismissedUpdateVersion] = useState<string | null>(null);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [miniMode, setMiniMode] = useState(false);
  const [cursorNear, setCursorNear] = useState(false);
  const [miniInteractive, setMiniInteractive] = useState(false);
  const savedWindowStateRef = useRef<WindowState | null>(null);
  const miniModeRef = useRef(false);
  // Window fits run strictly in order, so a standard-mode fit still in
  // flight can't re-apply the height lock after mini mode has dropped it
  const fitChainRef = useRef<Promise<unknown>>(Promise.resolve());
  const queueFit = useCallback((height: number | null) => {
    fitChainRef.current = fitChainRef.current
      .catch(() => {})
      .then(() => invoke("fit_window_height", { height }))
      .catch(() => {});
    return fitChainRef.current;
  }, []);
  const { settings, updateSettings } = useSettings();
  const { hardwareData, isLoading, error } = useHardwareData(settings.updateInterval);
  const { updateInfo, checking, downloading, progress, error: updateError, downloadAndInstall, checkForUpdate } = useUpdater();
  const { devices: audioDevices, switching: audioSwitching, switchDevice: switchAudioDevice } = useAudioDevices();
  const activeUpdateVersion = updateInfo?.available ? (updateInfo.version ?? "__unknown__") : null;

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
    if (activeUpdateVersion) {
      if (activeUpdateVersion !== dismissedUpdateVersion) {
        setShowUpdateNotification(true);
      }
      return;
    }

    setDismissedUpdateVersion(null);
    setShowUpdateNotification(true);
  }, [activeUpdateVersion, dismissedUpdateVersion]);

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
      miniModeRef.current = active;
      setMiniMode(active);
      if (!active) setCursorNear(false);

      try {
        if (active) {
          // Save current window state before shrinking
          const currentState = await invoke<WindowState>("get_window_state");
          savedWindowStateRef.current = currentState;
          // Persist the pre-mini size: if the process is killed while in
          // mini mode, the next launch must restore the standard-mode size
          await updateSettings({ windowState: currentState });

          // Drop the standard-mode height lock so the window can shrink
          await queueFit(null);
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
  }, [settings.position, updateSettings]);

  // Apply theme to document root
  useEffect(() => {
    const root = document.documentElement;
    if (settings.theme === "auto") {
      root.removeAttribute("data-theme");
    } else {
      root.setAttribute("data-theme", settings.theme);
    }
  }, [settings.theme]);

  // Native backdrop blur. Mini mode floats chips over the desktop, so the
  // effect is dropped there or the whole mini window would turn frosted.
  useEffect(() => {
    invoke("set_background_effect", {
      effect: miniMode ? "none" : settings.backgroundEffect,
    }).catch(() => {});
  }, [miniMode, settings.backgroundEffect]);

  // Save window state periodically and before close
  const saveWindowStateRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const saveWindowState = useCallback(async () => {
    // In mini mode the window holds the mini size — persisting it would
    // make the next launch restore a tiny window in standard mode
    if (miniModeRef.current) return;
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

  const updateNotificationVisible =
    showUpdateNotification && !!updateInfo?.available && !showSettings;

  // Standard mode: the panel is transparent, so size the window to the HUD
  // content and keep only the width user-resizable. Any extra height would
  // be an invisible strip that still swallows clicks meant for the desktop.
  useEffect(() => {
    if (isBooting || miniMode) return;
    const widget = document.querySelector<HTMLElement>(".hud-widget");
    const header = widget?.querySelector<HTMLElement>(".hud-header");
    const content = widget?.querySelector<HTMLElement>(".hud-content");
    const sections = content?.querySelector<HTMLElement>(".hud-sections");
    if (!widget || !header || !content || !sections) return;
    const notice = document.querySelector<HTMLElement>(".update-notification");

    let frame = 0;
    let lastKey = "";
    const measure = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (miniModeRef.current) return;
        // Resizing mid-drag would shift the pointer's frame of reference
        if (content.classList.contains("dragging")) return;
        // Built only from parts whose size doesn't depend on the window's
        // height, so the result can't feed back into itself (fractional DPI
        // rounding would otherwise make it oscillate by a pixel)
        const widgetStyle = getComputedStyle(widget);
        let height =
          parseFloat(widgetStyle.paddingTop) +
          header.getBoundingClientRect().height +
          parseFloat(getComputedStyle(header).marginBottom) +
          sections.getBoundingClientRect().height +
          parseFloat(widgetStyle.paddingBottom);
        if (showSettings || showRestorePanel) {
          height = Math.max(height, OVERLAY_MIN_HEIGHT);
        }
        if (notice) {
          height = Math.max(height, notice.offsetHeight + NOTICE_CLEARANCE);
        }
        height = Math.ceil(height);
        // Physical size depends on the scale too (monitor moves)
        const key = `${height}@${window.devicePixelRatio}`;
        if (key === lastKey) return;
        lastKey = key;
        queueFit(height);
      });
    };

    const observer = new ResizeObserver(measure);
    for (const el of [header, sections, notice]) if (el) observer.observe(el);
    const unlistenScale = getCurrentWindow().onScaleChanged(measure);
    measure();
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
      unlistenScale.then((fn) => fn()).catch(() => {});
    };
  }, [isBooting, miniMode, showSettings, showRestorePanel, updateNotificationVisible, queueFit]);

  // Mini mode makes the window click-through, which also blocks the audio
  // selector and the update notification's INSTALL button. Register their
  // rects as interactive zones; the Rust cursor monitor flips cursor events
  // back on only while the cursor is inside one of them.
  useEffect(() => {
    if (!miniMode) {
      setMiniInteractive(false);
      invoke("set_mini_interactive_zones", { zones: [] }).catch(() => {});
      return;
    }
    const measure = () => {
      const zones = [
        ...document.querySelectorAll(
          ".mini-row.audio .audio-device-select, .update-notification",
        ),
      ].map((el) => {
        const rect = el.getBoundingClientRect();
        return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
      });
      invoke("set_mini_interactive_zones", { zones }).catch(() => {});
    };
    // Wait for the mini layout to settle (the window is resized a frame later)
    const timer = setTimeout(measure, 150);
    window.addEventListener("resize", measure);
    return () => {
      clearTimeout(timer);
      window.removeEventListener("resize", measure);
    };
  }, [miniMode, updateNotificationVisible, settings.sectionOrder, settings.hiddenSections]);

  // Track whether the cursor is currently inside an interactive zone
  useEffect(() => {
    const unlisten = listen<{ interactive: boolean }>(
      "minimode-interactive",
      (event) => {
        setMiniInteractive(event.payload.interactive);
      },
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

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
      style={{
        "--hud-section-opacity": settings.sectionOpacity / 100,
        opacity: miniMode
          ? miniInteractive || (cursorNear && updateNotificationVisible)
            ? 1
            : cursorNear
              ? 0.1
              : 0.5
          : 1,
      } as CSSProperties}
    >
      {!miniMode && <ResizeHandle />}
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
          onDismiss={() => {
            setDismissedUpdateVersion(activeUpdateVersion);
            setShowUpdateNotification(false);
          }}
        />
      )}
    </div>
  );
}

export default App;
