export type CanvasItemKind = "terminal" | "note" | "file";
export type CanvasItemKey = `${CanvasItemKind}:${number}`;

export type ClientRect = {
  left: number;
  top: number;
  right: number;
  bottom: number;
};

export function canvasItemKey(kind: CanvasItemKind, id: number): CanvasItemKey {
  return `${kind}:${id}`;
}

export function parseCanvasItemKey(key: CanvasItemKey): {
  kind: CanvasItemKind;
  id: number;
} {
  const [kind, id] = key.split(":") as [CanvasItemKind, string];
  return { kind, id: Number(id) };
}

export function marqueeRect(
  startX: number,
  startY: number,
  endX: number,
  endY: number,
): ClientRect {
  return {
    left: Math.min(startX, endX),
    top: Math.min(startY, endY),
    right: Math.max(startX, endX),
    bottom: Math.max(startY, endY),
  };
}

/** A component is selected as soon as any visible part enters the marquee. */
export function rectsIntersect(a: ClientRect, b: ClientRect): boolean {
  return !(
    a.right < b.left ||
    a.left > b.right ||
    a.bottom < b.top ||
    a.top > b.bottom
  );
}
