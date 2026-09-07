const surfaces = new WeakMap<HTMLElement, (event: WheelEvent) => void>();
const forwardedWheels = new WeakSet<Event>();

function scrollPane(target: EventTarget | null, boundary: HTMLElement) {
  let element =
    target instanceof HTMLElement
      ? target
      : target instanceof Element
        ? target.parentElement
        : null;
  while (element && element !== boundary && boundary.contains(element)) {
    const { overflowX, overflowY } = getComputedStyle(element);
    if (/auto|scroll|overlay/.test(`${overflowX} ${overflowY}`)) return element;
    element = element.parentElement;
  }
  return null;
}

/** Register a window's scroll surface and contain native menu scrolling. */
export function containWheel(
  node: HTMLElement,
  fallback: (event: WheelEvent) => void = () => {},
) {
  node.classList.add("canvas-scroll-boundary");
  let lastPane: HTMLElement | null = null;
  const rememberPane = (event: PointerEvent) => {
    const pane = scrollPane(event.target, node);
    if (pane && !pane.closest(".panel, [role=menu]")) lastPane = pane;
  };
  surfaces.set(node, (event) => {
    const active = scrollPane(document.activeElement, node);
    const activePane = active?.closest(".panel, [role=menu]") ? null : active;
    const pane =
      activePane ?? (lastPane && node.contains(lastPane) ? lastPane : null);
    if (pane) scrollWheel(pane, event);
    else fallback(event);
  });
  const capture = (event: WheelEvent) => {
    if (event.ctrlKey || forwardedWheels.has(event)) return;
    // Leave native scrolling (including xterm / CodeMirror wheel handlers)
    // intact. CSS contains scroll chaining when a pane reaches either end.
    let element = event.target instanceof Element ? event.target : null;
    while (element && node.contains(element)) {
      const { overflowX, overflowY } = getComputedStyle(element);
      if (
        (/auto|scroll|overlay/.test(overflowY) &&
          element.scrollHeight > element.clientHeight) ||
        (/auto|scroll|overlay/.test(overflowX) &&
          element.scrollWidth > element.clientWidth)
      ) {
        const dx = event.deltaX || (event.shiftKey ? event.deltaY : 0);
        const dy = event.shiftKey ? 0 : event.deltaY;
        const canScrollX =
          /auto|scroll|overlay/.test(overflowX) &&
          (dx < 0
            ? element.scrollLeft > 0
            : dx > 0 &&
              element.scrollLeft + element.clientWidth <
                element.scrollWidth - 1);
        const canScrollY =
          /auto|scroll|overlay/.test(overflowY) &&
          (dy < 0
            ? element.scrollTop > 0
            : dy > 0 &&
              element.scrollTop + element.clientHeight <
                element.scrollHeight - 1);
        // Explicitly consume an edge event as well: native scroll chaining is
        // not consistently suppressed by overscroll-behavior in every engine.
        if (!canScrollX && !canScrollY) event.preventDefault();
        return;
      }
      if (element === node) break;
      element = element.parentElement;
    }
    // Headers, padding, and panes without overflow must not scroll the page.
    // Do not stop propagation here: terminal mouse-wheel reporting still runs.
    event.preventDefault();
    if (
      event.target instanceof Element &&
      event.target.closest(".panel, [role=menu]")
    )
      return;
    fallback(event);
  };
  const bubble = (event: WheelEvent) => {
    if (!event.ctrlKey) event.stopPropagation();
  };
  node.addEventListener("wheel", capture, { capture: true, passive: false });
  node.addEventListener("wheel", bubble);
  node.addEventListener("pointerdown", rememberPane, true);
  return {
    update(next: typeof fallback) {
      fallback = next;
    },
    destroy() {
      node.removeEventListener("wheel", capture, { capture: true });
      node.removeEventListener("wheel", bubble);
      node.removeEventListener("pointerdown", rememberPane, { capture: true });
      surfaces.delete(node);
      node.classList.remove("canvas-scroll-boundary");
    },
  };
}

/** Canvas routing happens before hovered children can consume the event. */
export function installCanvasWheel(
  node: HTMLElement,
  focusedWindow: () => HTMLElement | null,
  navigate: (
    event: WheelEvent,
    focused: boolean,
  ) => "component" | "pan" | "zoom",
  moveCamera: (event: WheelEvent, mode: "pan" | "zoom") => void,
) {
  const wheel = (event: WheelEvent) => {
    // Ctrl+wheel/pinch is handled by the existing camera capture listener.
    if (event.ctrlKey || forwardedWheels.has(event)) return;
    const target = event.target instanceof Element ? event.target : null;
    if (target?.closest(".panel, [role=menu], [role=dialog]")) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    const focused = focusedWindow();
    const destination = navigate(event, focused !== null);
    if (destination !== "component") {
      moveCamera(event, destination);
      return;
    }
    const surface = focused?.querySelector<HTMLElement>(
      ".canvas-scroll-boundary",
    );
    if (surface) surfaces.get(surface)?.(event);
  };
  node.addEventListener("wheel", wheel, { capture: true, passive: false });
  return () => node.removeEventListener("wheel", wheel, { capture: true });
}

/** Reuse xterm's DOM wheel path, including alternate-screen mouse reporting. */
export function forwardTerminalWheel(
  element: HTMLElement | undefined,
  event: WheelEvent,
) {
  const screen = element?.querySelector<HTMLElement>(".xterm-screen");
  if (!screen) return;
  const bounds = screen.getBoundingClientRect();
  const forwarded = new WheelEvent("wheel", {
    bubbles: true,
    cancelable: true,
    deltaX: event.deltaX,
    deltaY: event.deltaY,
    deltaZ: event.deltaZ,
    deltaMode: event.deltaMode,
    shiftKey: event.shiftKey,
    altKey: event.altKey,
    metaKey: event.metaKey,
    // The pointer may be over a different window. Report the focused screen's
    // center so terminal mouse coordinates cannot refer to another component.
    clientX: bounds.left + bounds.width / 2,
    clientY: bounds.top + bounds.height / 2,
  });
  forwardedWheels.add(forwarded);
  screen.dispatchEvent(forwarded);
}

export function scrollWheel(element: HTMLElement | null, event: WheelEvent) {
  if (!element) return;
  const unit =
    event.deltaMode === 1
      ? 16
      : event.deltaMode === 2
        ? element.clientHeight
        : 1;
  element.scrollBy({
    left: (event.deltaX || (event.shiftKey ? event.deltaY : 0)) * unit,
    top: (event.shiftKey ? 0 : event.deltaY) * unit,
  });
}
