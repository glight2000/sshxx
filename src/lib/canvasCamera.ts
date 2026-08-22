export type CanvasPoint = ReadonlyArray<number>;

export function canvasCameraCss(
  center: CanvasPoint,
  zoom: number,
  offsetLeft: number,
  offsetTop: number,
  gridSize: number,
) {
  const originX = `calc(${zoom * 50}vw - ${zoom * (offsetLeft + center[0])}px)`;
  const originY = `calc(${zoom * 50}vh - ${zoom * (offsetTop + center[1])}px)`;
  return [
    `--canvas-world-x:${originX}`,
    `--canvas-world-y:${originY}`,
    `--canvas-world-zoom:${zoom}`,
    `--canvas-grid-dot-size:${zoom}px`,
    `--canvas-grid-step:${gridSize * zoom}px`,
  ].join(";");
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
