type Point = { clientX: number; clientY: number; target: EventTarget | null };

/** A hold begins sorting; moving first leaves native horizontal scrolling intact. */
export function mobilePageReorder(
  node: HTMLElement,
  callbacks: {
    start(point: Point): boolean;
    move(point: Point): void;
    end(commit: boolean): void;
  },
) {
  let touch: { id: number; point: Point } | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let dragging = false;

  const reset = () => {
    clearTimeout(timer);
    timer = undefined;
    touch = null;
    dragging = false;
  };
  const cancel = () => {
    if (dragging) callbacks.end(false);
    reset();
  };
  const additionalTouch = (event: TouchEvent) => {
    // A second finger can land outside the pager, so its touchstart never
    // reaches the node. Cancel before canvas capture handlers consume it.
    if (touch && event.touches.length !== 1) cancel();
  };
  const start = (event: TouchEvent) => {
    cancel();
    if (event.touches.length !== 1) return;
    const first = event.touches[0];
    touch = {
      id: first.identifier,
      point: {
        clientX: first.clientX,
        clientY: first.clientY,
        target: event.target,
      },
    };
    timer = setTimeout(() => {
      timer = undefined;
      if (touch) dragging = callbacks.start(touch.point);
    }, 450);
  };
  const move = (event: TouchEvent) => {
    if (!touch) return;
    if (event.touches.length !== 1) return cancel();
    const current = Array.from(event.touches).find(
      (item) => item.identifier === touch?.id,
    );
    if (!current) return cancel();
    if (!dragging) {
      if (
        Math.hypot(
          current.clientX - touch.point.clientX,
          current.clientY - touch.point.clientY,
        ) > 8
      )
        reset();
      return;
    }
    // Touch events allow native scrolling until the hold actually starts.
    if (!event.cancelable) return cancel();
    event.preventDefault();
    callbacks.move({
      clientX: current.clientX,
      clientY: current.clientY,
      target: touch.point.target,
    });
  };
  const end = (event: TouchEvent) => {
    if (
      !touch ||
      !Array.from(event.changedTouches).some(
        (item) => item.identifier === touch?.id,
      )
    )
      return;
    if (dragging) {
      event.preventDefault();
      const last = Array.from(event.changedTouches).find(
        (item) => item.identifier === touch?.id,
      )!;
      callbacks.move({
        clientX: last.clientX,
        clientY: last.clientY,
        target: touch.point.target,
      });
      callbacks.end(true);
    }
    reset();
  };
  const context = (event: Event) => {
    if (touch) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  };
  node.addEventListener("touchstart", start, { passive: true });
  window.addEventListener("touchstart", additionalTouch, {
    capture: true,
    passive: true,
  });
  window.addEventListener("touchmove", move, { passive: false });
  window.addEventListener("touchend", end, { passive: false });
  window.addEventListener("touchcancel", cancel);
  node.addEventListener("contextmenu", context, true);
  return {
    cancel,
    destroy() {
      cancel();
      node.removeEventListener("touchstart", start);
      window.removeEventListener("touchstart", additionalTouch, true);
      window.removeEventListener("touchmove", move);
      window.removeEventListener("touchend", end);
      window.removeEventListener("touchcancel", cancel);
      node.removeEventListener("contextmenu", context, true);
    },
  };
}
