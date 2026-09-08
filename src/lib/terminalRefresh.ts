/** Debounce visual zoom refreshes without suspending terminal output parsing. */
export function terminalRefresh(refresh: () => void) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let refreshedZoom: number | undefined;
  let hidden = false;
  let disposed = false;

  return {
    update(zoom: number, visible: boolean, ready: boolean) {
      clearTimeout(timer);
      timer = undefined;
      if (disposed) return;
      if (!visible) hidden = true;
      // Initial xterm setup already paints at the current zoom.
      if (!ready) {
        refreshedZoom = zoom;
        return;
      }
      if (!visible || (!hidden && zoom === refreshedZoom)) return;
      timer = setTimeout(() => {
        timer = undefined;
        refreshedZoom = zoom;
        hidden = false;
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
