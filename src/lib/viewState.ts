/** Browser-local canvas state. This data is never synchronized to sshxx-server. */

export const LOCAL_VIEW_STATE_VERSION = 1;

export type LocalCanvasView = {
  center: [number, number];
  zoom: number;
};

export type LocalViewState = {
  version: typeof LOCAL_VIEW_STATE_VERSION;
  activePageId: number;
  pages: Record<string, LocalCanvasView>;
};

const MIN_ZOOM = 0.35;
const MAX_ZOOM = 2;
const MAX_COORDINATE = 1_000_000_000;

/**
 * Scope view state to this browser profile, server, and session. Browser
 * localStorage already provides the per-user/profile isolation boundary.
 */
export function localViewStateKey(
  sessionId: string,
  pageOrigin: string,
  configuredServer: string | null,
) {
  const server = configuredServer || pageOrigin;
  return `sshxx.local-view.v${LOCAL_VIEW_STATE_VERSION}:${encodeURIComponent(server)}:${encodeURIComponent(sessionId)}`;
}

function validPageId(value: unknown): value is number {
  return Number.isSafeInteger(value) && Number(value) > 0;
}

function validView(value: unknown): value is LocalCanvasView {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<LocalCanvasView>;
  return (
    Array.isArray(candidate.center) &&
    candidate.center.length === 2 &&
    candidate.center.every(
      (coordinate) =>
        typeof coordinate === "number" &&
        Number.isFinite(coordinate) &&
        Math.abs(coordinate) <= MAX_COORDINATE,
    ) &&
    typeof candidate.zoom === "number" &&
    Number.isFinite(candidate.zoom) &&
    candidate.zoom >= MIN_ZOOM &&
    candidate.zoom <= MAX_ZOOM
  );
}

/** Parse and validate persisted state so corrupt or future data cannot break startup. */
export function parseLocalViewState(
  value: string | null,
): LocalViewState | null {
  if (!value) return null;
  try {
    const candidate = JSON.parse(value) as Partial<LocalViewState>;
    if (
      candidate.version !== LOCAL_VIEW_STATE_VERSION ||
      !validPageId(candidate.activePageId) ||
      !candidate.pages ||
      typeof candidate.pages !== "object" ||
      Array.isArray(candidate.pages)
    ) {
      return null;
    }

    const pages: Record<string, LocalCanvasView> = {};
    for (const [pageId, view] of Object.entries(candidate.pages)) {
      const numericPageId = Number(pageId);
      if (validPageId(numericPageId) && validView(view)) {
        pages[String(numericPageId)] = {
          center: [view.center[0], view.center[1]],
          zoom: view.zoom,
        };
      }
    }
    return {
      version: LOCAL_VIEW_STATE_VERSION,
      activePageId: candidate.activePageId,
      pages,
    };
  } catch {
    return null;
  }
}
