type TerminalScreenGeometry = {
  left: number;
  top: number;
  visualWidth: number;
  visualHeight: number;
  layoutWidth: number;
  layoutHeight: number;
};

const correctedEvents = new WeakSet<Event>();
const scaleTolerance = 0.001;

export function remapTerminalClientPoint(
  clientX: number,
  clientY: number,
  geometry: TerminalScreenGeometry,
): [number, number] {
  const scaleX = geometry.visualWidth / geometry.layoutWidth;
  const scaleY = geometry.visualHeight / geometry.layoutHeight;
  return [
    Number.isFinite(scaleX) && Math.abs(scaleX - 1) > scaleTolerance
      ? geometry.left + (clientX - geometry.left) / scaleX
      : clientX,
    Number.isFinite(scaleY) && Math.abs(scaleY - 1) > scaleTolerance
      ? geometry.top + (clientY - geometry.top) / scaleY
      : clientY,
  ];
}

function correctMouseEvent(event: MouseEvent, screen: HTMLElement) {
  if (correctedEvents.has(event)) return;
  const bounds = screen.getBoundingClientRect();
  if (
    screen.offsetWidth <= 0 ||
    screen.offsetHeight <= 0 ||
    bounds.width <= 0 ||
    bounds.height <= 0
  )
    return;
  const [clientX, clientY] = remapTerminalClientPoint(
    event.clientX,
    event.clientY,
    {
      left: bounds.left,
      top: bounds.top,
      visualWidth: bounds.width,
      visualHeight: bounds.height,
      layoutWidth: screen.offsetWidth,
      layoutHeight: screen.offsetHeight,
    },
  );
  if (clientX === event.clientX && clientY === event.clientY) return;
  try {
    Object.defineProperties(event, {
      clientX: { configurable: true, value: clientX },
      clientY: { configurable: true, value: clientY },
    });
    correctedEvents.add(event);
  } catch (error) {
    console.warn("Could not correct scaled terminal mouse coordinates.", error);
  }
}

/**
 * Keep xterm's public DOM input surface in the same coordinate system as a
 * CSS-transformed canvas. xterm intentionally has no public mouse-coordinate
 * hook, so correction happens during DOM capture before xterm reads the event.
 */
export function installXtermMouseCoordinateAdapter(
  terminalElement: HTMLElement,
) {
  const screen = terminalElement.querySelector<HTMLElement>(".xterm-screen");
  if (!screen) return { dispose() {} };

  let dragging = false;
  const correct = (event: MouseEvent) => correctMouseEvent(event, screen);
  const beginDrag = (event: MouseEvent) => {
    correct(event);
    dragging = true;
  };
  const correctDrag = (event: MouseEvent) => {
    if (dragging) correct(event);
  };
  const finishDrag = (event: MouseEvent) => {
    if (!dragging) return;
    correct(event);
    dragging = false;
  };
  const correctedLocalEvents = [
    "mousemove",
    "mouseup",
    "contextmenu",
    "wheel",
  ] as const;

  terminalElement.addEventListener("mousedown", beginDrag, true);
  for (const type of correctedLocalEvents)
    terminalElement.addEventListener(type, correct, true);
  document.addEventListener("mousemove", correctDrag, true);
  document.addEventListener("mouseup", finishDrag, true);

  return {
    dispose() {
      terminalElement.removeEventListener("mousedown", beginDrag, true);
      for (const type of correctedLocalEvents)
        terminalElement.removeEventListener(type, correct, true);
      document.removeEventListener("mousemove", correctDrag, true);
      document.removeEventListener("mouseup", finishDrag, true);
    },
  };
}
