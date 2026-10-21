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

// Idle time before WebView2 is asked to trim memory again (ms)
const MEMORY_LOW_DELAY_MS = 3000;

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

  // Latest settings the transition queue needs, without re-subscribing the
  // listener (and losing queued work) whenever they change
  const positionRef = useRef(settings.position);
  positionRef.current = settings.position;
  // Mini mode transitions run strictly one after another: entering awaits
  // several IPC round-trips, and a quick exit used to run in the middle of
  // them — it found no saved size, and the rest of the entry then shrank
  // the window to mini size while the HUD was back in standard mode.
  const miniTransitionRef = useRef<Promise<void>>(Promise.resolve());

  // Listen for minimode-changed events from window monitor
  useEffect(() => {
    // Checked before every window-changing step: if mini mode was left while
    // this entry was in flight, stop before touching the window at all
    const stillMini = () => miniModeRef.current;

    const enterMini = async () => {
      // A saved state means a previous entry already captured the standard
      // size (an enter/exit/enter burst whose exit was skipped). The window
      // may be mini-sized by now, so re-capturing would record the mini size
      // as the one to restore; reuse it and re-apply the steps below, which
      // are idempotent (standard mode may have re-locked the height since).
      let currentState = savedWindowStateRef.current;
      if (!currentState) {
        currentState = await invoke<WindowState>("get_window_state");
        if (!stillMini()) return;
        savedWindowStateRef.current = currentState;
        // Persist the pre-mini size: if the process is killed while in
        // mini mode, the next launch must restore the standard-mode size
        await updateSettings({ windowState: currentState });
      }

      // Drop the standard-mode height lock so the window can shrink
      if (!stillMini()) return;
      await queueFit(null);
      // Make window click-through so users can interact with apps below
      if (!stillMini()) return;
      await getCurrentWindow().setIgnoreCursorEvents(true);

      // Wait a frame for React to render mini content, then fit window to it
      await new Promise((resolve) => requestAnimationFrame(resolve));
      if (!stillMini()) return;
      const miniEl = document.querySelector(".mini-content");
      const cssHeight = miniEl ? miniEl.getBoundingClientRect().height + 4 : 120;
      const dpr = window.devicePixelRatio || 1;
      await invoke("restore_window_state", {
        state: {
          x: currentState.x,
          y: currentState.y,
          width: Math.round(180 * dpr),
          height: Math.round(cssHeight * dpr),
        },
      });
      // Reposition to settings position
      await invoke("set_window_position", { position: positionRef.current });
    };

    const exitMini = async () => {
      // Always give clicks back, even if the entry stopped part-way
      await getCurrentWindow().setIgnoreCursorEvents(false);
      // Nothing saved means the entry never got as far as shrinking
      const saved = savedWindowStateRef.current;
      if (!saved) return;
      // Restore saved window size, then reposition to avoid overflow
      await invoke("restore_window_state", { state: saved });
      await invoke("set_window_position", { position: positionRef.current });
      // Cleared only now: until the restore lands the window is still
      // mini-sized, and saveWindowState must not persist that
      savedWindowStateRef.current = null;
    };

    const unlisten = listen<{ active: boolean }>("minimode-changed", (event) => {
      const active = event.payload.active;
      miniModeRef.current = active;
      setMiniMode(active);
      if (!active) setCursorNear(false);

      miniTransitionRef.current = miniTransitionRef.current
        .then(() => {
          // A later event already superseded this one; it will run itself
          if (miniModeRef.current !== active) return;
          return active ? enterMini() : exitMini();
        })
        .catch(() => {
          // Ignore errors during window resize
        });
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [updateSettings, queueFit]);

  // WebView2 runs with a "low" memory target while the HUD is just being
  // looked at, and "normal" while it's used: low can page memory out and
  // make clicks / dropdowns sluggish. Going back to low waits a moment so
  // brushing past the window doesn't flip it back and forth.
  const [pointerInside, setPointerInside] = useState(false);
  useEffect(() => {
    const root = document.documentElement;
    const enter = () => setPointerInside(true);
    const leave = () => setPointerInside(false);
    root.addEventListener("mouseenter", enter);
    root.addEventListener("mouseleave", leave);
    return () => {
      root.removeEventListener("mouseenter", enter);
      root.removeEventListener("mouseleave", leave);
    };
  }, []);
  const interacting = miniMode
    ? miniInteractive
    : pointerInside || showSettings || showRestorePanel;
  useEffect(() => {
    if (interacting) {
      invoke("set_webview_memory_low", { low: false }).catch(() => {});
      return;
    }
    const timer = setTimeout(() => {
      invoke("set_webview_memory_low", { low: true }).catch(() => {});
    }, MEMORY_LOW_DELAY_MS);
    return () => clearTimeout(timer);
  }, [interacting]);

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
    // make the next launch restore a tiny window in standard mode. A saved
    // pre-mini state means an exit is still pending its restore.
    if (miniModeRef.current || savedWindowStateRef.current) return;
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
