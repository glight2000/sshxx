const ISECT_PAD = 16;
const NEARBY_GAP = 40;
const NEARBY_STEP = 40;
const NEARBY_SEARCH_RINGS = 6;

export type CanvasItemRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

/** Choose a position for a new terminal that does not intersect existing ones. */
export function arrangeNewCanvasItem(
  existing: CanvasItemRect[],
  width: number,
  height: number,
) {
  if (existing.length === 0) {
    return { x: 0, y: 0 };
  }

  const startX = 100 * (Math.random() - 0.5);
  const startY = 60 * (Math.random() - 0.5);

  for (let i = 0; ; i++) {
    const t = 1.94161103872 * i;
    const x = Math.round(startX + 8 * i * Math.cos(t));
    const y = Math.round(startY + 8 * i * Math.sin(t));
    let ok = true;
    for (const box of existing) {
      if (
        isect(x, x + width, box.x, box.x + box.width) &&
        isect(y, y + height, box.y, box.y + box.height)
      ) {
        ok = false;
        break;
      }
    }
    if (ok) {
      return { x, y };
    }
  }
}

/**
 * Place a source-derived window near its source without allowing a crowded
 * canvas to push it arbitrarily far away. Adjacent sides are tried first, then
 * a small bounded grid around each side. If every nearby position is occupied,
 * use a small cascaded overlap instead of surprising the user with a distant
 * window.
 */
export function arrangeNewCanvasItemNear(
  existing: CanvasItemRect[],
  width: number,
  height: number,
  source: CanvasItemRect,
) {
  const origins = [
    {
      x: source.x,
      y: source.y + source.height + NEARBY_GAP,
      distance: source.height / 2 + height / 2 + NEARBY_GAP,
    },
    {
      x: source.x + source.width + NEARBY_GAP,
      y: source.y,
      distance: source.width / 2 + width / 2 + NEARBY_GAP,
    },
    {
      x: source.x,
      y: source.y - height - NEARBY_GAP,
      distance: source.height / 2 + height / 2 + NEARBY_GAP,
    },
    {
      x: source.x - width - NEARBY_GAP,
      y: source.y,
      distance: source.width / 2 + width / 2 + NEARBY_GAP,
    },
  ].sort((left, right) => left.distance - right.distance);

  for (let ring = 0; ring <= NEARBY_SEARCH_RINGS; ring++) {
    const offsets = squareRingOffsets(ring);
    for (const origin of origins) {
      for (const [dx, dy] of offsets) {
        const candidate = {
          x: origin.x + dx * NEARBY_STEP,
          y: origin.y + dy * NEARBY_STEP,
        };
        if (canPlace(existing, candidate.x, candidate.y, width, height))
          return candidate;
      }
    }
  }

  return {
    x: source.x + NEARBY_GAP,
    y: source.y + NEARBY_GAP,
  };
}

function squareRingOffsets(ring: number): [number, number][] {
  if (ring === 0) return [[0, 0]];
  const offsets: [number, number][] = [];
  for (let x = -ring; x <= ring; x++) {
    offsets.push([x, -ring], [x, ring]);
  }
  for (let y = -ring + 1; y < ring; y++) {
    offsets.push([-ring, y], [ring, y]);
  }
  return offsets;
}

function canPlace(
  existing: CanvasItemRect[],
  x: number,
  y: number,
  width: number,
  height: number,
) {
  return existing.every(
    (box) =>
      !isect(x, x + width, box.x, box.x + box.width) ||
      !isect(y, y + height, box.y, box.y + box.height),
  );
}

function isect(s1: number, e1: number, s2: number, e2: number): boolean {
  return s1 - ISECT_PAD < e2 && e1 + ISECT_PAD > s2;
}
