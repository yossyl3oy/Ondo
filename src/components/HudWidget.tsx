import { useState, useRef, useCallback, useEffect } from "react";
import { flushSync } from "react-dom";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { HardwareData, SectionType, AudioDevice } from "../types";

const COLLAPSED_KEY = "ondo_collapsed_sections";
const SHOW_CORES_KEY = "ondo_show_cpu_cores";
import { TemperatureGauge } from "./TemperatureGauge";
import { CpuCoreGrid } from "./CpuCoreGrid";
import { NetworkGraph } from "./NetworkGraph";
import "./HudWidget.css";

const DRAG_THRESHOLD = 5;

interface DragInfo {
  sectionType: SectionType;
  startY: number;
  currentY: number;
  isDragging: boolean;
  itemRects: Map<SectionType, { top: number; height: number }>;
  currentOrder: SectionType[];
  originalIndex: number;
}

interface HudWidgetProps {
  hardwareData: HardwareData;
  isLoading: boolean;
  error: string | null;
  onSettingsClick: () => void;
  onRestoreClick: () => void;
  sectionOrder: SectionType[];
  onSectionOrderChange: (order: SectionType[]) => void;
  hiddenSections: SectionType[];
  onHiddenSectionsChange: (hidden: SectionType[]) => void;
  audioDevices: AudioDevice[];
  onSwitchAudioDevice: (deviceId: string, deviceType: "playback" | "recording") => void;
  audioSwitching?: boolean;
  miniMode?: boolean;
  compactMode?: boolean;
  temperatureUnit?: "celsius" | "fahrenheit";
}

export function HudWidget({
  hardwareData,
  isLoading,
  error,
  onSettingsClick,
  onRestoreClick,
  sectionOrder,
  onSectionOrderChange,
  hiddenSections,
  onHiddenSectionsChange,
  audioDevices,
  onSwitchAudioDevice,
  audioSwitching,
  miniMode,
  compactMode,
  temperatureUnit = "celsius",
}: HudWidgetProps) {
  const [showCpuCores, setShowCpuCores] = useState(() => {
    try {
      return localStorage.getItem(SHOW_CORES_KEY) === "true";
    } catch { return false; }
  });
  const [version, setVersion] = useState("1.0.0");
  const [isDragging, setIsDragging] = useState(false);
  const [draggedType, setDraggedType] = useState<SectionType | null>(null);
  const [isOverTrash, setIsOverTrash] = useState(false);
  const visualOrderRef = useRef<SectionType[]>(sectionOrder);
  const [collapsedSections, setCollapsedSections] = useState<Set<SectionType>>(() => {
    try {
      const stored = localStorage.getItem(COLLAPSED_KEY);
      if (stored) {
        return new Set(JSON.parse(stored) as SectionType[]);
      }
    } catch { /* use default */ }
    return new Set();
  });
  const dragRef = useRef<DragInfo | null>(null);
  const wasDraggingRef = useRef(false);
  const sectionRefs = useRef<Map<SectionType, HTMLDivElement>>(new Map());
  const trashZoneRef = useRef<HTMLDivElement | null>(null);
  const { cpu, gpu } = hardwareData;
  const isFahrenheit = temperatureUnit === "fahrenheit";
  const toUnit = (c: number) => isFahrenheit ? Math.round(c * 9 / 5 + 32) : Math.round(c);
  const tempUnit = isFahrenheit ? "℉" : "℃";
  const toMax = (c: number) => isFahrenheit ? c * 9 / 5 + 32 : c;

  const displayFps = hardwareData.display?.fps ?? null;
  const fpsProcessName = hardwareData.display?.fpsProcessName ?? null;

  // Keep visualOrderRef in sync with sectionOrder when not dragging
  useEffect(() => {
    if (!isDragging) {
      visualOrderRef.current = sectionOrder;
    }
  }, [sectionOrder, isDragging]);

  useEffect(() => {
    getVersion()
      .then((v) => {
        console.log("[HudWidget] Version:", v);
        setVersion(v);
      })
      .catch((e) => {
        console.error("[HudWidget] Version error:", e);
      });
  }, []);

  // Persist collapsed sections to localStorage (skip when compact mode forces all collapsed)
  useEffect(() => {
    if (!compactMode) {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify([...collapsedSections]));
    }
  }, [collapsedSections, compactMode]);

  // Compact mode: collapse all sections; restore saved state when exiting
  useEffect(() => {
    if (compactMode) {
      setCollapsedSections(new Set(sectionOrder));
    } else {
      try {
        const stored = localStorage.getItem(COLLAPSED_KEY);
        if (stored) {
          setCollapsedSections(new Set(JSON.parse(stored) as SectionType[]));
        } else {
          setCollapsedSections(new Set());
        }
      } catch {
        setCollapsedSections(new Set());
      }
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [compactMode]);

  const toggleCollapse = useCallback((type: SectionType) => {
    if (dragRef.current?.isDragging || wasDraggingRef.current) return;
    setCollapsedSections((prev) => {
      const next = new Set(prev);
      if (next.has(type)) {
        next.delete(type);
      } else {
        next.add(type);
      }
      return next;
    });
  }, []);

  const getTemperatureStatus = (temp: number, max: number) => {
    const ratio = temp / max;
    if (ratio >= 0.9) return "danger";
    if (ratio >= 0.75) return "warning";
    return "normal";
  };

  // Visible sections (those with data and not hidden)
  const getVisibleSections = useCallback(() => {
    return sectionOrder.filter((type) => {
      if (hiddenSections.includes(type)) return false;
      switch (type) {
        case "cpu": return !!cpu;
        case "gpu": return !!gpu;
        case "storage": return !!hardwareData.storage && hardwareData.storage.length > 0;
        case "motherboard": return !!hardwareData.motherboard;
        case "network": return !!hardwareData.network && hardwareData.network.length > 0;
        case "audio": return true;
        case "display": return true;
        default: return false;
      }
    });
  }, [sectionOrder, hiddenSections, cpu, gpu, hardwareData.storage, hardwareData.motherboard, hardwareData.network]);

  const handlePointerDown = useCallback((e: React.PointerEvent, sectionType: SectionType) => {
    // Don't start drag on interactive elements
    const target = e.target as HTMLElement;
    if (target.closest("button") || target.closest("a") || target.closest("input")) return;

    const visibleSections = getVisibleSections();
    const itemRects = new Map<SectionType, { top: number; height: number }>();

    for (const type of visibleSections) {
      const el = sectionRefs.current.get(type);
      if (el) {
        const rect = el.getBoundingClientRect();
        itemRects.set(type, { top: rect.top, height: rect.height });
      }
    }

    dragRef.current = {
      sectionType,
      startY: e.clientY,
      currentY: e.clientY,
      isDragging: false,
      itemRects,
      currentOrder: [...visibleSections],
      originalIndex: visibleSections.indexOf(sectionType),
    };

    (e.target as HTMLElement).setPointerCapture(e.pointerId);
  }, [getVisibleSections]);

  const handlePointerMove = useCallback((e: React.PointerEvent) => {
    const drag = dragRef.current;
    if (!drag) return;

    const deltaY = e.clientY - drag.startY;

    // Check threshold before starting drag
    if (!drag.isDragging) {
      if (Math.abs(deltaY) < DRAG_THRESHOLD) return;
      drag.isDragging = true;
      setIsDragging(true);
      setDraggedType(drag.sectionType);
    }

    drag.currentY = e.clientY;

    // Move the dragged element
    const draggedEl = sectionRefs.current.get(drag.sectionType);
    if (draggedEl) {
      draggedEl.style.transform = `translateY(${deltaY}px)`;
      draggedEl.style.zIndex = "10";
    }

    // Calculate new position based on center of dragged item
    const draggedRect = drag.itemRects.get(drag.sectionType);
    if (!draggedRect) return;
    const draggedCenterY = draggedRect.top + draggedRect.height / 2 + deltaY;

    // Build new order based on where the dragged item's center is
    const otherSections = drag.currentOrder.filter((t) => t !== drag.sectionType);
    let newIndex = otherSections.length; // default: end

    for (let i = 0; i < otherSections.length; i++) {
      const rect = drag.itemRects.get(otherSections[i]);
      if (rect) {
        const midY = rect.top + rect.height / 2;
        if (draggedCenterY < midY) {
          newIndex = i;
          break;
        }
      }
    }

    const newOrder = [...otherSections];
    newOrder.splice(newIndex, 0, drag.sectionType);

    // Apply shift transforms to other sections
    const draggedHeight = draggedRect.height + 8; // include margin-bottom
    for (const type of otherSections) {
      const el = sectionRefs.current.get(type);
      if (!el) continue;

      const originalIdx = drag.currentOrder.indexOf(type);
      const targetIdx = newOrder.indexOf(type);

      let translateY = 0;
      if (originalIdx !== targetIdx) {
        translateY = targetIdx > originalIdx ? draggedHeight : -draggedHeight;
      }

      el.style.transition = "transform 0.25s cubic-bezier(0.2, 0, 0, 1)";
      el.style.transform = translateY !== 0 ? `translateY(${translateY}px)` : "";
    }

    // Update visual order for the drop
    visualOrderRef.current = newOrder;

    // Check if over trash zone
    const trashEl = trashZoneRef.current;
    if (trashEl) {
      const trashRect = trashEl.getBoundingClientRect();
      setIsOverTrash(e.clientY >= trashRect.top && e.clientY <= trashRect.bottom);
    }
  }, []);

  const handlePointerUp = useCallback((e: React.PointerEvent) => {
    const drag = dragRef.current;
    if (!drag) return;

    (e.target as HTMLElement).releasePointerCapture(e.pointerId);

    if (drag.isDragging) {
      // Prevent the subsequent click event from toggling collapse
      wasDraggingRef.current = true;
      requestAnimationFrame(() => { wasDraggingRef.current = false; });

      // Clear all inline styles immediately
      for (const type of drag.currentOrder) {
        const el = sectionRefs.current.get(type);
        if (el) {
          el.style.transition = "";
          el.style.transform = "";
          el.style.zIndex = "";
        }
      }

      // Check if dropped on trash zone
      if (isOverTrash) {
        const newHidden = [...hiddenSections, drag.sectionType];
        flushSync(() => {
          onHiddenSectionsChange(newHidden);
          setIsDragging(false);
          setDraggedType(null);
          setIsOverTrash(false);
        });
      } else {
        const finalOrder = [...visualOrderRef.current];

        // Build the full order (including invisible sections)
        const result: SectionType[] = [];
        let visibleIdx = 0;
        for (const type of sectionOrder) {
          if (finalOrder.includes(type)) {
            result.push(finalOrder[visibleIdx++]);
          } else {
            result.push(type);
          }
        }

        // Use flushSync so React re-renders in the same frame as style clearing
        flushSync(() => {
          onSectionOrderChange(result);
          setIsDragging(false);
          setDraggedType(null);
          setIsOverTrash(false);
        });
      }
    } else {
      setIsDragging(false);
      setDraggedType(null);
      setIsOverTrash(false);
    }

    dragRef.current = null;
  }, [sectionOrder, hiddenSections, isOverTrash, onSectionOrderChange, onHiddenSectionsChange]);

  const setSectionRef = useCallback((type: SectionType, el: HTMLDivElement | null) => {
    if (el) {
      sectionRefs.current.set(type, el);
    } else {
      sectionRefs.current.delete(type);
    }
  }, []);

  const renderCpuSection = () => {
    if (!cpu) return null;
    const collapsed = collapsedSections.has("cpu");
    return (
      <>
        <div
          className={`hud-section-header${collapsed ? " collapsed" : ""}`}
          onClick={() => toggleCollapse("cpu")}
          style={{ cursor: "pointer" }}
        >
          <div className="section-indicator cpu" />
          <span className="section-label">CPU</span>
          <span className="section-name" title={cpu.name}>
            {cpu.name}
          </span>
          {collapsed && (
            <div className="collapsed-values">
              <span className="collapsed-val">{toUnit(cpu.temperature)}<span className="collapsed-val-unit">{tempUnit}</span></span>
              <span className="collapsed-val">{Math.round(cpu.load)}<span className="collapsed-val-unit">%</span></span>
            </div>
          )}
          <span className="expand-icon">{collapsed ? "▸" : "▾"}</span>
        </div>

        {collapsed ? (
          <div className="collapsed-bar">
            <div className="collapsed-bar-fill" style={{ width: `${Math.round(cpu.load)}%` }} />
          </div>
        ) : (
          <>
            <div className="hud-metrics">
              <TemperatureGauge
                value={toUnit(cpu.temperature)}
                max={toMax(cpu.maxTemperature)}
                unit={tempUnit}
                status={getTemperatureStatus(cpu.temperature, cpu.maxTemperature)}
                label="TEMP"
              />
              <div className="metric-divider" />
              <div className="metric-item">
                <span className="metric-label">LOAD</span>
                <span className="metric-value">{Math.round(cpu.load)}%</span>
                <div className="metric-bar">
                  <div
                    className="metric-bar-fill"
                    style={{ width: `${Math.round(cpu.load)}%` }}
                  />
                </div>
              </div>
            </div>

            {cpu.frequency > 0 && (
              <div className="cpu-frequency">
                <span className="frequency-label">FREQ</span>
                <span className="frequency-value">
                  {cpu.frequency.toFixed(2)} GHz
                </span>
              </div>
            )}

            {showCpuCores && cpu.cores && cpu.cores.length > 0 && (
              <CpuCoreGrid cores={cpu.cores} maxTemp={cpu.maxTemperature} temperatureUnit={temperatureUnit} />
            )}
            {cpu.cores && cpu.cores.length > 0 && (
              <div
                className="cores-toggle"
                onClick={(e) => { e.stopPropagation(); setShowCpuCores((v) => { localStorage.setItem(SHOW_CORES_KEY, String(!v)); return !v; }); }}
              >
                <span className="cores-toggle-label">{showCpuCores ? "Hide Cores" : "Show Cores"}</span>
                <span className="expand-icon">{showCpuCores ? "▾" : "▸"}</span>
              </div>
            )}
          </>
        )}
      </>
    );
  };

  const renderGpuSection = () => {
    if (!gpu) return null;
    const collapsed = collapsedSections.has("gpu");
    return (
      <>
        <div
          className={`hud-section-header${collapsed ? " collapsed" : ""}`}
          onClick={() => toggleCollapse("gpu")}
          style={{ cursor: "pointer" }}
        >
          <div className="section-indicator gpu" />
          <span className="section-label">GPU</span>
          <span className="section-name" title={gpu.name}>
            {gpu.name}
          </span>
          {collapsed && (
            <div className="collapsed-values">
              <span className="collapsed-val">{toUnit(gpu.temperature)}<span className="collapsed-val-unit">{tempUnit}</span></span>
              <span className="collapsed-val">{Math.round(gpu.load)}<span className="collapsed-val-unit">%</span></span>
            </div>
          )}
          <span className="expand-icon">{collapsed ? "▸" : "▾"}</span>
        </div>

        {collapsed ? (
          <div className="collapsed-bar">
            <div className="collapsed-bar-fill gpu" style={{ width: `${Math.round(gpu.load)}%` }} />
          </div>
        ) : (
          <>
            <div className="hud-metrics">
              <TemperatureGauge
                value={toUnit(gpu.temperature)}
                max={toMax(gpu.maxTemperature)}
                unit={tempUnit}
                status={getTemperatureStatus(gpu.temperature, gpu.maxTemperature)}
                label="TEMP"
              />
              <div className="metric-divider" />
              <div className="metric-item">
                <span className="metric-label">LOAD</span>
                <span className="metric-value">{Math.round(gpu.load)}%</span>
                <div className="metric-bar">
                  <div
                    className="metric-bar-fill gpu"
                    style={{ width: `${Math.round(gpu.load)}%` }}
                  />
                </div>
              </div>
            </div>

            {gpu.frequency > 0 && (
              <div className="gpu-frequency">
                <span className="frequency-label">FREQ</span>
                <span className="frequency-value">
                  {gpu.frequency.toFixed(2)} GHz
                </span>
              </div>
            )}

            {gpu.memoryTotal > 0 && (
              <div className="gpu-memory">
                <span className="memory-label">{gpu.memoryUsed > 0 ? "VRAM" : "MEM"}</span>
                <span className="memory-value">
                  {gpu.memoryUsed > 0
                    ? `${gpu.memoryUsed.toFixed(1)}/${gpu.memoryTotal.toFixed(1)}GB`
                    : `${gpu.memoryTotal.toFixed(0)}GB Unified`}
                </span>
                {gpu.memoryUsed > 0 && (
                  <div className="memory-bar">
                    <div
                      className="memory-bar-fill"
                      style={{
                        width: `${(gpu.memoryUsed / gpu.memoryTotal) * 100}%`,
                      }}
                    />
                  </div>
                )}
              </div>
            )}
          </>
        )}
      </>
    );
  };

  const renderStorageSection = () => {
    if (!hardwareData.storage || hardwareData.storage.length === 0) return null;
    const collapsed = collapsedSections.has("storage");
    return (
      <>
        <div
          className={`hud-section-header${collapsed ? " collapsed" : ""}`}
          onClick={() => toggleCollapse("storage")}
          style={{ cursor: "pointer" }}
        >
          <div className="section-indicator storage" />
          <span className="section-label">SSD</span>
          <span className="section-name">
            {hardwareData.storage.length > 1
              ? `${hardwareData.storage.length} drives`
              : hardwareData.storage[0].name}
          </span>
          {collapsed && (
            <div className="collapsed-values">
              {hardwareData.storage.map((drive, i) => (
                <span key={i} className="collapsed-val">
                  {drive.temperature > 0 && <>{toUnit(drive.temperature)}<span className="collapsed-val-unit">{tempUnit}</span>{" "}</>}
                  {Math.round(drive.usedSpace)}<span className="collapsed-val-unit">%</span>
                </span>
              ))}
            </div>
          )}
          <span className="expand-icon">{collapsed ? "▸" : "▾"}</span>
        </div>

        {collapsed ? (
          <div className="collapsed-bar-group">
            {hardwareData.storage.map((drive, i) => (
              <div key={i} className="collapsed-bar">
                <div className="collapsed-bar-fill storage" style={{ width: `${Math.min(drive.usedSpace, 100)}%` }} />
              </div>
            ))}
          </div>
        ) : (
          hardwareData.storage.map((drive, index) => (
            <div key={index} className={index > 0 ? "storage-drive-separator" : undefined}>
              {hardwareData.storage!.length > 1 && (
                <div className="storage-drive-name" title={drive.name}>
                  {drive.name}
                </div>
              )}

              <div className="hud-metrics">
                {drive.temperature > 0 ? (
                  <TemperatureGauge
                    value={toUnit(drive.temperature)}
                    max={toMax(70)}
                    unit={tempUnit}
                    status={getTemperatureStatus(drive.temperature, 70)}
                    label="TEMP"
                  />
                ) : (
                  <div className="temp-gauge">
                    <div className="gauge-content">
                      <span className="gauge-value unavailable">N/A</span>
                      <span className="gauge-label">TEMP</span>
                    </div>
                  </div>
                )}
                <div className="metric-divider" />
                <div className="metric-item">
                  <span className="metric-label">USED</span>
                  <span className="metric-value">
                    {drive.usedSpace > 0 ? `${Math.round(drive.usedSpace)}%` : "0%"}
                  </span>
                  <div className="metric-bar">
                    <div
                      className="metric-bar-fill storage"
                      style={{ width: `${Math.min(drive.usedSpace, 100)}%` }}
                    />
                  </div>
                </div>
              </div>

              <div className="storage-capacity-info">
                <span className="capacity-label">CAPACITY</span>
                <span className="capacity-value">
                  {drive.totalSpace > 0 ? `${Math.round(drive.totalSpace)}GB` : "N/A"}
                </span>
              </div>
            </div>
          ))
        )}
      </>
    );
  };

  const renderMotherboardSection = () => {
    if (!hardwareData.motherboard) return null;
    const collapsed = collapsedSections.has("motherboard");
    return (
      <>
        <div
          className={`hud-section-header${collapsed ? " collapsed" : ""}`}
          onClick={() => toggleCollapse("motherboard")}
          style={{ cursor: "pointer" }}
        >
          <div className="section-indicator motherboard" />
          <span className="section-label">MB</span>
          <span className="section-name" title={hardwareData.motherboard.name}>
            {hardwareData.motherboard.name}
          </span>
          {collapsed && (
            <div className="collapsed-values">
              {hardwareData.motherboard.temperature > 0 && (
                <span className="collapsed-val">{toUnit(hardwareData.motherboard.temperature)}<span className="collapsed-val-unit">{tempUnit}</span></span>
              )}
              {hardwareData.motherboard.fans.length > 0 && (
                <span className="collapsed-val">{hardwareData.motherboard.fans[0].speed}<span className="collapsed-val-unit">RPM</span></span>
              )}
            </div>
          )}
          <span className="expand-icon">{collapsed ? "▸" : "▾"}</span>
        </div>

        {!collapsed && (
          <div className="hud-metrics">
            {hardwareData.motherboard.temperature > 0 ? (
              <TemperatureGauge
                value={toUnit(hardwareData.motherboard.temperature)}
                max={toMax(80)}
                unit={tempUnit}
                status={getTemperatureStatus(hardwareData.motherboard.temperature, 80)}
                label="TEMP"
              />
            ) : (
              <div className="temp-gauge">
                <div className="gauge-content">
                  <span className="gauge-value unavailable">N/A</span>
                  <span className="gauge-label">TEMP</span>
                </div>
              </div>
            )}
            <div className="metric-divider" />
            <div className="metric-item fan-metrics">
              <span className="metric-label">FAN</span>
              {hardwareData.motherboard.fans.length > 0 ? (
                <div className="fan-speeds">
                  {hardwareData.motherboard.fans.slice(0, 3).map((fan, idx) => (
                    <div key={idx} className="fan-speed-item" title={fan.name}>
                      <span className="fan-speed-value">{fan.speed}</span>
                      <span className="fan-speed-unit">RPM</span>
                    </div>
                  ))}
                </div>
              ) : (
                <span className="metric-value unavailable">N/A</span>
              )}
            </div>
          </div>
        )}
      </>
    );
  };

  const renderAudioDeviceSelect = (
    devices: AudioDevice[],
    deviceType: "playback" | "recording",
    label: string,
  ) => {
    const defaultDevice = devices.find((d) => d.is_default);
    return (
      <div className="audio-subsection">
        <span className="audio-subsection-label">{label}</span>
        <div className="audio-select-wrapper">
          <select
            className="audio-device-select"
            value={defaultDevice?.id ?? ""}
            onChange={(e) => onSwitchAudioDevice(e.target.value, deviceType)}
            onPointerDown={(e) => e.stopPropagation()}
            disabled={audioSwitching || devices.length === 0}
          >
            {devices.map((device) => (
              <option key={device.id} value={device.id}>
                {device.name}
              </option>
            ))}
            {devices.length === 0 && (
              <option value="">No devices found</option>
            )}
          </select>
          {audioSwitching && (
            <span className="audio-switching-indicator">⟳</span>
          )}
        </div>
      </div>
    );
  };

  const renderAudioSection = () => {
    const collapsed = collapsedSections.has("audio");
    const playbackDevices = audioDevices.filter((d) => d.device_type === "playback");
    const recordingDevices = audioDevices.filter((d) => d.device_type === "recording");
    const defaultPlayback = playbackDevices.find((d) => d.is_default);
    return (
      <>
        <div
          className={`hud-section-header${collapsed ? " collapsed" : ""}`}
          onClick={() => toggleCollapse("audio")}
          style={{ cursor: "pointer" }}
        >
          <div className="section-indicator audio" />
          <span className="section-label">AUDIO</span>
          <span className="section-name" title={defaultPlayback?.name ?? "No device"}>
            {defaultPlayback?.name ?? "No device"}
          </span>
          <span className="expand-icon">{collapsed ? "▸" : "▾"}</span>
        </div>

        {!collapsed && (
          <>
            {renderAudioDeviceSelect(playbackDevices, "playback", "PLAYBACK")}
            {renderAudioDeviceSelect(recordingDevices, "recording", "RECORDING")}
          </>
        )}
      </>
    );
  };

  const formatSpeed = (bytesPerSec: number, short = false): string => {
    const suffix = short ? "" : "/s";
    if (bytesPerSec >= 1_073_741_824) return `${(bytesPerSec / 1_073_741_824).toFixed(1)} GB${suffix}`;
    if (bytesPerSec >= 1_048_576) return `${(bytesPerSec / 1_048_576).toFixed(1)} MB${suffix}`;
    if (bytesPerSec >= 1_024) return `${(bytesPerSec / 1_024).toFixed(1)} KB${suffix}`;
    return `${Math.round(bytesPerSec)} B${suffix}`;
  };

  const renderNetworkSection = () => {
    if (!hardwareData.network || hardwareData.network.length === 0) return null;
    const collapsed = collapsedSections.has("network");
    return (
      <>
        <div
          className={`hud-section-header${collapsed ? " collapsed" : ""}`}
          onClick={() => toggleCollapse("network")}
          style={{ cursor: "pointer" }}
        >
          <div className="section-indicator network" />
          <span className="section-label">NETWORK</span>
          <span className="section-name">
            {hardwareData.network.length > 1
              ? `${hardwareData.network.length} adapters`
              : hardwareData.network[0].name}
          </span>
          {collapsed && (
            <div className="collapsed-values">
              <span className="collapsed-val">
                <span className="collapsed-val-unit">▼</span>{formatSpeed(hardwareData.network.reduce((sum, n) => sum + n.receivedPerSec, 0))}
              </span>
              <span className="collapsed-val">
                <span className="collapsed-val-unit">▲</span>{formatSpeed(hardwareData.network.reduce((sum, n) => sum + n.sentPerSec, 0))}
              </span>
            </div>
          )}
          <span className="expand-icon">{collapsed ? "▸" : "▾"}</span>
        </div>

        {collapsed ? (
          <div className="collapsed-bar">
            <div className="collapsed-bar-fill network" style={{ width: "0%" }} />
          </div>
        ) : (
          hardwareData.network.map((iface, index) => (
            <div key={index} className={index > 0 ? "network-iface-separator" : undefined}>
              {hardwareData.network!.length > 1 && (
                <div className="network-iface-name" title={iface.name}>
                  {iface.name}
                </div>
              )}

              <div className="network-speeds">
                <div className="network-speed-item">
                  <span className="network-speed-icon dl">▼</span>
                  <span className="network-speed-label">DL</span>
                  <span className="network-speed-value">{formatSpeed(iface.receivedPerSec)}</span>
                </div>
                <div className="network-speed-item">
                  <span className="network-speed-icon ul">▲</span>
                  <span className="network-speed-label">UL</span>
                  <span className="network-speed-value">{formatSpeed(iface.sentPerSec)}</span>
                </div>
              </div>

              <NetworkGraph
                receivedPerSec={iface.receivedPerSec}
                sentPerSec={iface.sentPerSec}
                formatSpeed={formatSpeed}
              />
            </div>
          ))
        )}
      </>
    );
  };

  const renderDisplaySection = () => {
    const collapsed = collapsedSections.has("display");
    const refreshRate = hardwareData.display?.refreshRate ?? null;
    const fpsBarPercent = displayFps !== null && refreshRate
      ? Math.min(Math.round((displayFps / refreshRate) * 100), 100)
      : 0;
    return (
      <>
        <div
          className={`hud-section-header${collapsed ? " collapsed" : ""}`}
          onClick={() => toggleCollapse("display")}
          style={{ cursor: "pointer" }}
        >
          <div className="section-indicator display" />
          <span className="section-label">DISPLAY</span>
          <span className="section-name" title={hardwareData.display?.name ?? undefined}>
            {hardwareData.display?.name ?? ""}
          </span>
          {collapsed && (
            <div className="collapsed-values">
              {displayFps !== null ? (
                <span className="collapsed-val">{displayFps}<span className="collapsed-val-unit">FPS</span></span>
              ) : (
                <span className="collapsed-val">—<span className="collapsed-val-unit">FPS</span></span>
              )}
              {refreshRate !== null && (
                <span className="collapsed-val">{refreshRate}<span className="collapsed-val-unit">Hz</span></span>
              )}
            </div>
          )}
          <span className="expand-icon">{collapsed ? "▸" : "▾"}</span>
        </div>

        {collapsed ? (
          <div className="collapsed-bar">
            <div className="collapsed-bar-fill display" style={{ width: `${fpsBarPercent}%` }} />
          </div>
        ) : (
          <div className="display-metrics">
            <div className="display-metric-item">
              <span className="display-metric-label">{fpsProcessName ? `FPS · ${fpsProcessName}` : "FPS"}</span>
              {displayFps !== null ? (
                <span className="display-metric-value">{displayFps}</span>
              ) : (
                <span className="display-metric-value unavailable">N/A</span>
              )}
            </div>
            {refreshRate !== null && (
              <>
                <div className="metric-divider" />
                <div className="display-metric-item">
                  <span className="display-metric-label">REFRESH RATE</span>
                  <span className="display-metric-value">{refreshRate}<span className="display-metric-unit"> Hz</span></span>
                </div>
              </>
            )}
          </div>
        )}
      </>
    );
  };

  const sectionRenderers: Record<SectionType, () => React.ReactNode> = {
    cpu: renderCpuSection,
    gpu: renderGpuSection,
    storage: renderStorageSection,
    motherboard: renderMotherboardSection,
    audio: renderAudioSection,
    network: renderNetworkSection,
    display: renderDisplaySection,
  };

  // ── Mini mode: compact 1-line per section ──────────────────────────────
  const renderMiniRow = (
    rowKey: string,
    type: SectionType,
    label: string,
    temp: number | null,
    usage: number,
    barClass?: string,
  ) => (
    <div key={rowKey} className={`mini-row ${type}`}>
      <div className={`section-indicator ${type}`} />
      <span className="mini-label">{label}</span>
      <span className="mini-temp">
        {temp !== null && temp > 0 ? <>{toUnit(temp)}<span className="mini-unit">{tempUnit}</span></> : "—"}
      </span>
      <span className="mini-divider">|</span>
      <div className="mini-bar">
        <div
          className={`mini-bar-fill${barClass ? ` ${barClass}` : ""}`}
          style={{ width: `${Math.min(Math.round(usage), 100)}%` }}
        />
      </div>
      <span className="mini-usage">{Math.round(usage)}<span className="mini-unit">%</span></span>
    </div>
  );

  const renderMiniContent = () => {
    const rows: React.ReactNode[] = [];

    for (const type of sectionOrder) {
      if (hiddenSections.includes(type)) continue;

      switch (type) {
        case "cpu":
          if (cpu) rows.push(renderMiniRow("cpu", "cpu", "CPU", cpu.temperature, cpu.load));
          break;
        case "gpu":
          if (gpu) rows.push(renderMiniRow("gpu", "gpu", "GPU", gpu.temperature, gpu.load, "gpu"));
          break;
        case "storage":
          if (hardwareData.storage && hardwareData.storage.length > 0) {
            for (const [index, drive] of hardwareData.storage.entries()) {
              rows.push(renderMiniRow(`storage-${drive.name}-${index}`, "storage", "SSD", drive.temperature, drive.usedSpace, "storage"));
            }
          }
          break;
        case "motherboard":
          if (hardwareData.motherboard) {
            rows.push(renderMiniRow("motherboard", "motherboard", "MB", hardwareData.motherboard.temperature, 0));
          }
          break;
        case "network":
          if (hardwareData.network && hardwareData.network.length > 0) {
            const totalDl = hardwareData.network.reduce((s, n) => s + n.receivedPerSec, 0);
            const totalUl = hardwareData.network.reduce((s, n) => s + n.sentPerSec, 0);
            rows.push(
              <div key="network" className="mini-row network">
                <div className="section-indicator network" />
                <span className="mini-label">NET</span>
                <span className="mini-net-speed"><span className="mini-unit">▼</span>{formatSpeed(totalDl, true)}</span>
                <span className="mini-divider">|</span>
                <span className="mini-net-speed"><span className="mini-unit">▲</span>{formatSpeed(totalUl, true)}</span>
              </div>
            );
          }
          break;
        case "display":
          rows.push(
            <div key="display" className="mini-row display">
              <div className="section-indicator display" />
              <span className="mini-label">FPS</span>
              <span className="mini-temp">{displayFps !== null ? displayFps : "—"}</span>
              {hardwareData.display && (
                <>
                  <span className="mini-divider">|</span>
                  <span className="mini-temp">{hardwareData.display.refreshRate}<span className="mini-unit">Hz</span></span>
                </>
              )}
            </div>
          );
          break;
        // audio is omitted in mini mode
      }
    }

    return rows;
  };

  // ── Mini mode render ───────────────────────────────────────────────────
  if (miniMode) {
    return (
      <div className="hud-widget mini" onMouseDown={() => getCurrentWindow().startDragging()}>
        <div className="mini-content">
          {renderMiniContent()}
        </div>
      </div>
    );
  }

  // ── Normal mode render ─────────────────────────────────────────────────
  return (
    <div className="hud-widget">
      {/* Drag region - separate from header */}
      <div className="drag-region" onMouseDown={() => getCurrentWindow().startDragging()} />

      {/* Header */}
      <div className="hud-header">
        <div className="hud-title">
          <span className="hud-title-icon">◈</span>
          <span className="hud-title-text">ONDO</span>
          <span className="hud-title-version">v{version}</span>
        </div>
        <div className="hud-header-buttons">
          {hiddenSections.length > 0 && (
            <button
              className="hud-restore-btn"
              onClick={onRestoreClick}
              title="Restore hidden sections"
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z" />
              </svg>
            </button>
          )}
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
      </div>

      {/* Main content */}
      <div className={`hud-content${isDragging ? " dragging" : ""}`}>
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
            {sectionOrder.map((sectionType) => {
              if (hiddenSections.includes(sectionType)) return null;
              const content = sectionRenderers[sectionType]();
              if (!content) return null;
              return (
                <div
                  key={sectionType}
                  ref={(el) => setSectionRef(sectionType, el)}
                  className={`hud-section${draggedType === sectionType ? ` dragging${isOverTrash ? " near-trash" : ""}` : ""}${isDragging && draggedType !== sectionType ? " shrink" : ""}`}
                  onPointerDown={(e) => handlePointerDown(e, sectionType)}
                  onPointerMove={handlePointerMove}
                  onPointerUp={handlePointerUp}
                >
                  {content}
                </div>
              );
            })}
          </>
        )}
      </div>

      {/* Trash zone - fixed at bottom during drag */}
      {isDragging && (
        <div
          ref={trashZoneRef}
          className={`trash-zone${isOverTrash ? " active" : ""}`}
        >
          <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor">
            <path d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z" />
          </svg>
        </div>
      )}

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
