/** Spacing between dotted canvas grid points, in canvas coordinates. */
export const GRID_SIZE = 40;

/** Shared collapsed height for every canvas component. */
export const MINIMIZED_WINDOW_HEIGHT = GRID_SIZE;

/** Small visual inset from a grid point, shared by leading and trailing edges. */
export const GRID_EDGE_GAP = GRID_SIZE / 10;

const nearestGridPoint = (value: number) =>
  Math.round(value / GRID_SIZE) * GRID_SIZE;

/** Snap a top or left edge just inside its nearest background grid point. */
export function gridLeadingEdge(value: number) {
  return nearestGridPoint(value) + GRID_EDGE_GAP;
}

/** Snap a bottom or right edge just inside its nearest background grid point. */
export function gridTrailingEdge(value: number) {
  return nearestGridPoint(value) - GRID_EDGE_GAP;
}

export type CanvasRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

/** Outer window size spanning whole grid cells with both visual insets. */
export function gridSpanSize(cells: number) {
  return Math.max(1, Math.round(cells) * GRID_SIZE - 2 * GRID_EDGE_GAP);
}

/** Align all four edges of a new item to the same inset grid convention. */
export function gridAlignedRect(candidate: CanvasRect): CanvasRect {
  const x = gridLeadingEdge(candidate.x);
  const y = gridLeadingEdge(candidate.y);
  const right = gridTrailingEdge(x + candidate.width);
  const bottom = gridTrailingEdge(y + candidate.height);
  return {
    x,
    y,
    width: Math.max(1, right - x),
    height: Math.max(1, bottom - y),
  };
}
