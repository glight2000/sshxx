export type PageMove = { id: number; beforeId: number | null };

/** A reordered snapshot is not a create acknowledgement. New pages can appear
 * anywhere after the server coalesces creation with a reorder. */
export function addedPageId(
  previous: readonly { id: number }[],
  next: readonly { id: number }[],
): number | null {
  const known = new Set(previous.map((page) => page.id));
  return next.find((page) => !known.has(page.id))?.id ?? null;
}

/** Stable-ID move: unrelated pages retain their relative order. */
export function movePage(ids: readonly number[], move: PageMove): number[] {
  if (
    !ids.includes(move.id) ||
    move.beforeId === move.id ||
    (move.beforeId !== null && !ids.includes(move.beforeId))
  )
    return [...ids];
  const rest = ids.filter((id) => id !== move.id);
  rest.splice(
    move.beforeId === null ? rest.length : rest.indexOf(move.beforeId),
    0,
    move.id,
  );
  return rest;
}

export function pageMoveForOrder(
  original: readonly number[],
  ordered: readonly number[],
  id: number,
): PageMove | null {
  const index = ordered.indexOf(id);
  if (index < 0 || original.every((value, i) => value === ordered[i]))
    return null;
  return { id, beforeId: ordered[index + 1] ?? null };
}

/** Use layout centers, not animated bounding boxes, to avoid drag jitter. */
export function pageMoveAtX(
  boxes: readonly { id: number; center: number }[],
  id: number,
  center: number,
): PageMove {
  return {
    id,
    beforeId:
      boxes.find((box) => box.id !== id && center < box.center)?.id ?? null,
  };
}

export function pageScrollSpeed(
  x: number,
  left: number,
  right: number,
): number {
  const edge = Math.min(32, (right - left) / 3);
  if (x < left || x > right || edge <= 0) return 0;
  if (x < left + edge) return -480 * (1 - (x - left) / edge);
  if (x > right - edge) return 480 * (1 - (right - x) / edge);
  return 0;
}
