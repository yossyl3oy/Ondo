import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./ResizeHandle.css";

export type ResizeEdge = "West" | "East";

// Matches the hit zone width in ResizeHandle.css
const ZONE_WIDTH = 8;

/**
 * Whether a pointer that left the handle zone went outward, into the OS
 * sizing border (or past the window edge), rather than back into the HUD.
 */
export function leftTowardEdge(edge: ResizeEdge, clientX: number, viewportWidth: number): boolean {
  return edge === "West" ? clientX < ZONE_WIDTH : clientX > viewportWidth - ZONE_WIDTH;
}

/**
 * Hover-revealed width handle for one vertical edge. The panel is
 * transparent and frameless, so nothing shows where the window can be
 * resized. The outermost few px are the OS sizing border, where the webview
 * receives no pointer events; when the pointer slips out there the hint is
 * held until the backend reports the cursor has left that band, so the
 * visible hint matches exactly where a drag will resize.
 */
function EdgeHandle({ edge }: { edge: ResizeEdge }) {
  const [hovered, setHovered] = useState(false);
  const [held, setHeld] = useState(false);
  // Ignores a stale wait that resolves after the pointer came back
  const holdToken = useRef(0);

  useEffect(() => () => {
    holdToken.current++;
  }, []);

  const handlePointerEnter = () => {
    holdToken.current++;
    setHeld(false);
    setHovered(true);
  };

  const handlePointerLeave = (e: React.PointerEvent) => {
    setHovered(false);
    if (!leftTowardEdge(edge, e.clientX, window.innerWidth)) return;
    const token = ++holdToken.current;
    setHeld(true);
    // The backend answers true once the cursor left the band; false means it
    // timed out (cursor resting on the border) — wait again in that case
    const hold = () => {
      invoke<boolean>("wait_cursor_leaves_edge", { edge })
        .then((left) => {
          if (holdToken.current !== token) return;
          if (left) setHeld(false);
          else hold();
        })
        .catch(() => {
          if (holdToken.current === token) setHeld(false);
        });
    };
    hold();
  };

  const handlePointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    getCurrentWindow().startResizeDragging(edge).catch(() => {});
  };

  const visible = hovered || held;
  return (
    <div
      className={`resize-handle ${edge === "West" ? "west" : "east"}${visible ? " visible" : ""}`}
      onPointerEnter={handlePointerEnter}
      onPointerLeave={handlePointerLeave}
      onPointerDown={handlePointerDown}
      aria-label="Resize width"
    >
      <div className="resize-handle-bar" />
      <span className="resize-handle-label">Resize</span>
    </div>
  );
}

/** Both side edges resize (the OS allows either), so both get a hint. */
export function ResizeHandle() {
  return (
    <>
      <EdgeHandle edge="West" />
      <EdgeHandle edge="East" />
    </>
  );
}
