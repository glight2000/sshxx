import type { CanvasItemRect } from "./arrange";

export type CanvasPageMoveView = {
  center: [number, number];
  zoom: number;
};

/** Center and, only when necessary, zoom out to reveal a moved item group. */
export function canvasPageMoveView(
  items: readonly CanvasItemRect[],
  viewportWidth: number,
  viewportHeight: number,
  preferredZoom: number,
): CanvasPageMoveView | null {
  if (items.length === 0) return null;
  const left = Math.min(...items.map((item) => item.x));
  const top = Math.min(...items.map((item) => item.y));
  const right = Math.max(...items.map((item) => item.x + item.width));
  const bottom = Math.max(...items.map((item) => item.y + item.height));
  const width = Math.max(1, right - left);
  const height = Math.max(1, bottom - top);
  const availableWidth = Math.max(1, viewportWidth - 192);
  const availableHeight = Math.max(1, viewportHeight - 240);
  const fittedZoom = Math.min(availableWidth / width, availableHeight / height);
  return {
    center: [Math.round((left + right) / 2), Math.round((top + bottom) / 2)],
    zoom: Math.max(0.35, Math.min(2, preferredZoom, fittedZoom)),
  };
}
