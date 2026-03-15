import { useState, useRef, useCallback } from "react";
import { flushSync } from "react-dom";
import { getVersion } from "@tauri-apps/api/app";
import { useEffect } from "react";
import type { HardwareData, SectionType } from "../types";
import { TemperatureGauge } from "./TemperatureGauge";
import { CpuCoreGrid } from "./CpuCoreGrid";
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
  sectionOrder: SectionType[];
  onSectionOrderChange: (order: SectionType[]) => void;
}

export function HudWidget({
  hardwareData,
  isLoading,
  error,
  onSettingsClick,
  sectionOrder,
  onSectionOrderChange,
}: HudWidgetProps) {
  const [showCores, setShowCores] = useState(false);
  const [version, setVersion] = useState("1.0.0");
  const [isDragging, setIsDragging] = useState(false);
  const [draggedType, setDraggedType] = useState<SectionType | null>(null);
  const [visualOrder, setVisualOrder] = useState<SectionType[]>(sectionOrder);
  const dragRef = useRef<DragInfo | null>(null);
  const sectionRefs = useRef<Map<SectionType, HTMLDivElement>>(new Map());
  const { cpu, gpu } = hardwareData;

  // Keep visualOrder in sync with sectionOrder when not dragging
  useEffect(() => {
    if (!isDragging) {
      setVisualOrder(sectionOrder);
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

  const getTemperatureStatus = (temp: number, max: number) => {
    const ratio = temp / max;
    if (ratio >= 0.9) return "danger";
    if (ratio >= 0.75) return "warning";
    return "normal";
  };

  // Visible sections (those with data)
  const getVisibleSections = useCallback(() => {
    return sectionOrder.filter((type) => {
      switch (type) {
        case "cpu": return !!cpu;
        case "gpu": return !!gpu;
        case "storage": return !!hardwareData.storage && hardwareData.storage.length > 0;
        case "motherboard": return !!hardwareData.motherboard;
        default: return false;
      }
    });
  }, [sectionOrder, cpu, gpu, hardwareData.storage, hardwareData.motherboard]);

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
    setVisualOrder(newOrder);
  }, []);

  const handlePointerUp = useCallback((e: React.PointerEvent) => {
    const drag = dragRef.current;
    if (!drag) return;

    (e.target as HTMLElement).releasePointerCapture(e.pointerId);

    if (drag.isDragging) {
      const finalOrder = [...visualOrder];

      // Clear all inline styles immediately
      for (const type of drag.currentOrder) {
        const el = sectionRefs.current.get(type);
        if (el) {
          el.style.transition = "";
          el.style.transform = "";
          el.style.zIndex = "";
        }
      }

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
      });
    } else {
      setIsDragging(false);
      setDraggedType(null);
    }

    dragRef.current = null;
  }, [visualOrder, sectionOrder, onSectionOrderChange]);

  const setSectionRef = useCallback((type: SectionType, el: HTMLDivElement | null) => {
    if (el) {
      sectionRefs.current.set(type, el);
    } else {
      sectionRefs.current.delete(type);
    }
  }, []);

  const renderCpuSection = () => {
    if (!cpu) return null;
    return (
      <>
        <div
          className="hud-section-header"
          onClick={() => {
            if (!dragRef.current?.isDragging) setShowCores(!showCores);
          }}
          style={{ cursor: "pointer" }}
        >
          <div className="section-indicator cpu" />
          <span className="section-label">CPU</span>
          <span className="section-name" title={cpu.name}>
            {cpu.name}
          </span>
          <span className="expand-icon">{showCores ? "▾" : "▸"}</span>
        </div>

        <div className="hud-metrics">
          <TemperatureGauge
            value={cpu.temperature}
            max={cpu.maxTemperature}
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

        <div className="cpu-frequency">
          <span className="frequency-label">FREQ</span>
          <span className="frequency-value">
            {cpu.frequency > 0 ? `${cpu.frequency.toFixed(2)} GHz` : "N/A"}
          </span>
        </div>

        {showCores && cpu.cores && cpu.cores.length > 0 && (
          <CpuCoreGrid cores={cpu.cores} maxTemp={cpu.maxTemperature} />
        )}
      </>
    );
  };

  const renderGpuSection = () => {
    if (!gpu) return null;
    return (
      <>
        <div className="hud-section-header">
          <div className="section-indicator gpu" />
          <span className="section-label">GPU</span>
          <span className="section-name" title={gpu.name}>
            {gpu.name}
          </span>
        </div>

        <div className="hud-metrics">
          <TemperatureGauge
            value={gpu.temperature}
            max={gpu.maxTemperature}
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

        <div className="gpu-frequency">
          <span className="frequency-label">FREQ</span>
          <span className="frequency-value">
            {gpu.frequency > 0 ? `${gpu.frequency.toFixed(2)} GHz` : "N/A"}
          </span>
        </div>

        <div className="gpu-memory">
          <span className="memory-label">VRAM</span>
          <span className="memory-value">
            {gpu.memoryUsed.toFixed(1)}/{gpu.memoryTotal.toFixed(1)}GB
          </span>
          <div className="memory-bar">
            <div
              className="memory-bar-fill"
              style={{
                width: `${(gpu.memoryUsed / gpu.memoryTotal) * 100}%`,
              }}
            />
          </div>
        </div>
      </>
    );
  };

  const renderStorageSection = () => {
    if (!hardwareData.storage || hardwareData.storage.length === 0) return null;
    return (
      <>
        {hardwareData.storage.map((drive, index) => (
          <div key={index} className={index > 0 ? "storage-drive-separator" : undefined}>
            <div className="hud-section-header">
              <div className="section-indicator storage" />
              <span className="section-label">SSD</span>
              <span className="section-name" title={drive.name}>
                {drive.name}
              </span>
            </div>

            <div className="hud-metrics">
              {drive.temperature > 0 ? (
                <TemperatureGauge
                  value={drive.temperature}
                  max={70}
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
        ))}
      </>
    );
  };

  const renderMotherboardSection = () => {
    if (!hardwareData.motherboard) return null;
    return (
      <>
        <div className="hud-section-header">
          <div className="section-indicator motherboard" />
          <span className="section-label">MB</span>
          <span className="section-name" title={hardwareData.motherboard.name}>
            {hardwareData.motherboard.name}
          </span>
        </div>

        <div className="hud-metrics">
          {hardwareData.motherboard.temperature > 0 ? (
            <TemperatureGauge
              value={hardwareData.motherboard.temperature}
              max={80}
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
      </>
    );
  };

  const sectionRenderers: Record<SectionType, () => React.ReactNode> = {
    cpu: renderCpuSection,
    gpu: renderGpuSection,
    storage: renderStorageSection,
    motherboard: renderMotherboardSection,
  };

  return (
    <div className="hud-widget">
      {/* Drag region - separate from header */}
      <div className="drag-region" />

      {/* Header */}
      <div className="hud-header">
        <div className="hud-title">
          <span className="hud-title-icon">◈</span>
          <span className="hud-title-text">ONDO</span>
          <span className="hud-title-version">v{version}</span>
        </div>
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

      {/* Main content */}
      <div className="hud-content">
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
              const content = sectionRenderers[sectionType]();
              if (!content) return null;
              return (
                <div
                  key={sectionType}
                  ref={(el) => setSectionRef(sectionType, el)}
                  className={`hud-section${draggedType === sectionType ? " dragging" : ""}${isDragging && draggedType !== sectionType ? " shrink" : ""}`}
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
