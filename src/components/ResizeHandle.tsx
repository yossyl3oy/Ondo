import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppSettings } from "../types";
import "./ResizeHandle.css";

export type ResizeEdge = "West" | "East";

// The edge pinned to the screen side is rarely dragged, so only offer the
// one facing the desktop: left-anchored windows resize from the right.
export function resizeEdgeFor(position: AppSettings["position"]): ResizeEdge {
  return position.endsWith("left") ? "East" : "West";
}

interface ResizeHandleProps {
  position: AppSettings["position"];
}

/**
 * Hover-revealed width handle. The panel is transparent and frameless, so
 * nothing shows where the window can be resized; pressing the handle hands
 * off to the OS resize loop (the height is locked to the content).
 */
export function ResizeHandle({ position }: ResizeHandleProps) {
  const edge = resizeEdgeFor(position);

  const handlePointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    getCurrentWindow().startResizeDragging(edge).catch(() => {});
  };

  return (
    <div
      className={`resize-handle ${edge === "West" ? "west" : "east"}`}
      onPointerDown={handlePointerDown}
      aria-label="Resize width"
    >
      <div className="resize-handle-bar" />
      <span className="resize-handle-label">Resize</span>
    </div>
  );
}
