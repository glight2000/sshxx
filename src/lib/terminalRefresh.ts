/** Debounce visual zoom refreshes without suspending terminal output parsing. */
export function terminalRefresh(refresh: () => void) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let refreshedZoom: number | undefined;
  let disposed = false;

  return {
    update(zoom: number, visible: boolean, ready: boolean) {
      clearTimeout(timer);
      timer = undefined;
      if (disposed) return;
      // Initial xterm setup already paints at the current zoom.
      if (!ready) {
        refreshedZoom = zoom;
        return;
      }
      if (!visible || zoom === refreshedZoom) return;
      timer = setTimeout(() => {
        timer = undefined;
        refreshedZoom = zoom;
        refresh();
      }, 120);
    },
    dispose() {
      disposed = true;
      clearTimeout(timer);
      timer = undefined;
    },
  };
}
