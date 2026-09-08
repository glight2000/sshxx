export type CanvasPoint = ReadonlyArray<number>;

function canvasOrigin(
  center: CanvasPoint,
  zoom: number,
  offsetLeft: number,
  offsetTop: number,
) {
  return [
    `calc(${zoom * 50}vw - ${zoom * (offsetLeft + center[0])}px)`,
    `calc(${zoom * 50}vh - ${zoom * (offsetTop + center[1])}px)`,
  ];
}

export function canvasCameraCss(
  center: CanvasPoint,
  zoom: number,
  offsetLeft: number,
  offsetTop: number,
  gridSize: number,
) {
  const [originX, originY] = canvasOrigin(center, zoom, offsetLeft, offsetTop);
  return [
    `--canvas-world-x:${originX}`,
    `--canvas-world-y:${originY}`,
    `--canvas-world-zoom:${zoom}`,
    `--canvas-grid-dot-size:${zoom}px`,
    `--canvas-grid-step:${gridSize * zoom}px`,
  ].join(";");
}

export function canvasWorldTransform(
  center: CanvasPoint,
  zoom: number,
  offsetLeft: number,
  offsetTop: number,
) {
  const [x, y] = canvasOrigin(center, zoom, offsetLeft, offsetTop);
  return `translate3d(${x}, ${y}, 0) scale(${zoom})`;
}

// Touch previews must not change inherited camera variables on the ancestor
// of every mounted editor/terminal. Only these two visual layers need repainting.
export function previewCanvasCamera(
  world: HTMLElement,
  grid: HTMLElement,
  center: CanvasPoint,
  zoom: number,
  offsetLeft: number,
  offsetTop: number,
  gridSize: number,
) {
  world.style.transform = canvasWorldTransform(
    center,
    zoom,
    offsetLeft,
    offsetTop,
  );
  grid.style.cssText = canvasCameraCss(
    center,
    zoom,
    offsetLeft,
    offsetTop,
    gridSize,
  );
}

export function canvasViewportAnchor(
  viewportWidth: number,
  viewportHeight: number,
  offsetLeft: number,
  offsetTop: number,
): [number, number] {
  return [viewportWidth / 2 - offsetLeft, viewportHeight / 2 - offsetTop];
}

export function screenToCanvasPosition(
  screen: CanvasPoint,
  center: CanvasPoint,
  zoom: number,
  anchor: CanvasPoint,
): [number, number] {
  return [
    Math.round(center[0] + screen[0] / zoom - anchor[0]),
    Math.round(center[1] + screen[1] / zoom - anchor[1]),
  ];
}

export function canvasToScreenPosition(
  point: CanvasPoint,
  center: CanvasPoint,
  zoom: number,
  anchor: CanvasPoint,
): [number, number] {
  return [
    zoom * (anchor[0] + point[0] - center[0]),
    zoom * (anchor[1] + point[1] - center[1]),
  ];
}
