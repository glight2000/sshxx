export type CanvasResizeDirection =
  "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "nw";

export type CanvasWindowRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

type ResizeCanvasWindowOptions = {
  start: CanvasWindowRect;
  startPointer: [number, number];
  pointer: [number, number];
  direction: CanvasResizeDirection;
  minWidth: number;
  minHeight: number;
  maxWidth?: number;
  maxHeight?: number;
  snapLeading: (value: number) => number;
  snapTrailing: (value: number) => number;
};

const resizesWest = (direction: CanvasResizeDirection) =>
  direction.endsWith("w");
const resizesEast = (direction: CanvasResizeDirection) =>
  direction.endsWith("e");
const resizesNorth = (direction: CanvasResizeDirection) =>
  direction.startsWith("n");
const resizesSouth = (direction: CanvasResizeDirection) =>
  direction.startsWith("s");

function constrainEdge(
  leading: number,
  trailing: number,
  minimum: number,
  maximum: number,
  movesLeading: boolean,
) {
  const size = trailing - leading;
  if (size < minimum)
    return movesLeading
      ? [trailing - minimum, trailing]
      : [leading, leading + minimum];
  if (size > maximum)
    return movesLeading
      ? [trailing - maximum, trailing]
      : [leading, leading + maximum];
  return [leading, trailing];
}

/** Shared eight-direction resize geometry for non-terminal canvas windows. */
export function resizeCanvasWindow({
  start,
  startPointer,
  pointer,
  direction,
  minWidth,
  minHeight,
  maxWidth = Number.MAX_SAFE_INTEGER,
  maxHeight = Number.MAX_SAFE_INTEGER,
  snapLeading,
  snapTrailing,
}: ResizeCanvasWindowOptions): CanvasWindowRect {
  const dx = pointer[0] - startPointer[0];
  const dy = pointer[1] - startPointer[1];
  const startRight = start.x + start.width;
  const startBottom = start.y + start.height;
  let left = resizesWest(direction) ? snapLeading(start.x + dx) : start.x;
  let top = resizesNorth(direction) ? snapLeading(start.y + dy) : start.y;
  let right = resizesEast(direction)
    ? snapTrailing(startRight + dx)
    : startRight;
  let bottom = resizesSouth(direction)
    ? snapTrailing(startBottom + dy)
    : startBottom;

  [left, right] = constrainEdge(
    left,
    right,
    minWidth,
    maxWidth,
    resizesWest(direction),
  );
  [top, bottom] = constrainEdge(
    top,
    bottom,
    minHeight,
    maxHeight,
    resizesNorth(direction),
  );
  return {
    x: Math.round(left),
    y: Math.round(top),
    width: Math.round(right - left),
    height: Math.round(bottom - top),
  };
}
