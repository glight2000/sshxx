export const HISTORY_SCROLL_INTERVAL_MS = 300;

/** Viewer-local top edges, with one active load and at most one trailing edge.
 * The owner reports both scroll events and the new position after prepending
 * rows, since xterm's buffer insertion does not emit a public scroll event.
 */
export function terminalHistoryScroll(
  load: () => Promise<unknown>,
  now: () => number = () => performance.now(),
) {
  let atTop: boolean | null = null;
  let pending = false;
  let loading = false;
  let disposed = false;
  let generation = 0;
  let lastStarted = -Infinity;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function cancelTimer() {
    clearTimeout(timer);
    timer = undefined;
  }

  async function drain() {
    if (disposed || loading || !pending || !atTop || timer !== undefined)
      return;
    const delay = HISTORY_SCROLL_INTERVAL_MS - (now() - lastStarted);
    if (delay > 0) {
      timer = setTimeout(() => {
        timer = undefined;
        void drain();
      }, delay);
      return;
    }
    pending = false;
    loading = true;
    lastStarted = now();
    const current = generation;
    try {
      await load();
    } catch {
      // The owner reports load failures. Never create an automatic retry loop.
      if (current === generation) pending = false;
    } finally {
      if (current === generation) {
        loading = false;
        void drain();
      }
    }
  }

  function reset(position: number | null = null) {
    generation++;
    cancelTimer();
    pending = loading = false;
    atTop = position === null ? null : position <= 0;
    lastStarted = -Infinity;
  }

  return {
    update(position: number) {
      if (disposed || !Number.isFinite(position)) return;
      const previous = atTop;
      atTop = position <= 0;
      if (!atTop) {
        pending = false;
        cancelTimer();
      } else if (previous === false) {
        pending = true;
        void drain();
      }
    },
    reset,
    dispose() {
      disposed = true;
      reset();
    },
  };
}
