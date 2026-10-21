import type { CSSProperties } from "react";

/**
 * Inline style for a bar fill at `percent` (0-100). Scaling instead of
 * animating `width` keeps the per-second value transitions on the
 * compositor: no layout or repaint of the surrounding HUD while they run.
 */
export function fillStyle(percent: number, extra?: CSSProperties): CSSProperties {
  const ratio = Number.isFinite(percent) ? Math.min(Math.max(percent, 0), 100) / 100 : 0;
  return { ...extra, transform: `scaleX(${ratio})` };
}
