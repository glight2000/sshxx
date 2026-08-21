export const TERMINAL_MIN_ROWS = 8;
export const TERMINAL_MAX_ROWS = 500;
export const TERMINAL_MIN_COLS = 32;
export const TERMINAL_MAX_COLS = 500;
export const TERMINAL_MIN_WINDOW_WIDTH = 240;
export const TERMINAL_MIN_WINDOW_HEIGHT = 160;
export const TERMINAL_MAX_WINDOW_SIZE = 4_000;

export type TerminalResizeDirection =
  "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "nw";

type TerminalResizeInput = {
  direction: TerminalResizeDirection;
  left: number;
  top: number;
  right: number;
  bottom: number;
  startWidth: number;
  startHeight: number;
  startRows: number;
  startCols: number;
  cellWidth: number;
  cellHeight: number;
};

const resizesWest = (direction: TerminalResizeDirection) =>
  direction.endsWith("w");
const resizesEast = (direction: TerminalResizeDirection) =>
  direction.endsWith("e");
const resizesNorth = (direction: TerminalResizeDirection) =>
  direction.startsWith("n");
const resizesSouth = (direction: TerminalResizeDirection) =>
  direction.startsWith("s");
const clamp = (value: number, minimum: number, maximum: number) =>
  Math.min(maximum, Math.max(minimum, value));

function resizeBounds(
  startPixels: number,
  startCells: number,
  cellPixels: number,
  minimumCells: number,
  maximumCells: number,
  minimumWindowPixels: number,
) {
  const safeCellPixels =
    Number.isFinite(cellPixels) && cellPixels > 0
      ? cellPixels
      : startPixels / Math.max(1, startCells);
  const nonCellPixels = Math.max(0, startPixels - startCells * safeCellPixels);
  const minimum = Math.min(
    TERMINAL_MAX_WINDOW_SIZE,
    Math.max(
      minimumWindowPixels,
      Math.ceil(nonCellPixels + minimumCells * safeCellPixels),
    ),
  );
  const maximum = Math.max(
    minimum,
    Math.min(
      TERMINAL_MAX_WINDOW_SIZE,
      Math.floor(nonCellPixels + maximumCells * safeCellPixels),
    ),
  );
  return { minimum, maximum, cellPixels: safeCellPixels };
}

/**
 * Constrain all eight terminal resize directions to the same pixel and PTY
 * limits while preserving the fixed edge for north/west resizing.
 */
export function constrainTerminalResize(input: TerminalResizeInput) {
  const horizontal = resizeBounds(
    input.startWidth,
    input.startCols,
    input.cellWidth,
    TERMINAL_MIN_COLS,
    TERMINAL_MAX_COLS,
    TERMINAL_MIN_WINDOW_WIDTH,
  );
  const vertical = resizeBounds(
    input.startHeight,
    input.startRows,
    input.cellHeight,
    TERMINAL_MIN_ROWS,
    TERMINAL_MAX_ROWS,
    TERMINAL_MIN_WINDOW_HEIGHT,
  );
  let { left, top, right, bottom } = input;
  const changesWidth =
    resizesWest(input.direction) || resizesEast(input.direction);
  const changesHeight =
    resizesNorth(input.direction) || resizesSouth(input.direction);

  if (changesWidth) {
    const width = clamp(right - left, horizontal.minimum, horizontal.maximum);
    if (resizesWest(input.direction)) left = right - width;
    else right = left + width;
  }
  if (changesHeight) {
    const height = clamp(bottom - top, vertical.minimum, vertical.maximum);
    if (resizesNorth(input.direction)) top = bottom - height;
    else bottom = top + height;
  }

  const width = Math.round(right - left);
  const height = Math.round(bottom - top);
  const cols = changesWidth
    ? clamp(
        input.startCols +
          Math.floor((width - input.startWidth) / horizontal.cellPixels),
        TERMINAL_MIN_COLS,
        TERMINAL_MAX_COLS,
      )
    : input.startCols;
  const rows = changesHeight
    ? clamp(
        input.startRows +
          Math.floor((height - input.startHeight) / vertical.cellPixels),
        TERMINAL_MIN_ROWS,
        TERMINAL_MAX_ROWS,
      )
    : input.startRows;

  return {
    x: Math.round(left),
    y: Math.round(top),
    width,
    height,
    rows,
    cols,
    minimumWidth: horizontal.minimum,
    minimumHeight: vertical.minimum,
  };
}
