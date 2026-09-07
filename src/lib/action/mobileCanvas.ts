// Phone overview owns touch input before editors, terminals and iframes see it.
// Desktop input and focused component content keep their existing handlers.
type Point = { x: number; y: number };
type View = { center: number[]; zoom: number };

export function touchCamera(view: View, before: Point[], after: Point[]): View {
  const midpoint = (points: Point[]) =>
    points.reduce(
      (p, q) => ({
        x: p.x + q.x / points.length,
        y: p.y + q.y / points.length,
      }),
      { x: 0, y: 0 },
    );
  const a = midpoint(before),
    b = midpoint(after);
  const distance = (points: Point[]) =>
    Math.hypot(points[0].x - points[1].x, points[0].y - points[1].y);
  const zoom =
    before.length === 2 && distance(before) > 0
      ? Math.max(
          0.35,
          Math.min(2, (view.zoom * distance(after)) / distance(before)),
        )
      : view.zoom;
  return {
    zoom,
    center: view.center.map(
      (value, i) => value + [a.x, a.y][i] / view.zoom - [b.x, b.y][i] / zoom,
    ),
  };
}

export function installMobileCanvas(
  node: HTMLElement,
  enabled: () => boolean,
  read: () => View,
  write: (view: View, settled: boolean) => void,
  tap: (target: EventTarget | null) => void,
) {
  const points = new Map<number, Point>();
  let target: EventTarget | null = null;
  let start: Point = { x: 0, y: 0 };
  let moved = false;
  let view: View;
  let frame: number | null = null;
  const paint = () => {
    frame = null;
    write(enabled() ? view : read(), false);
  };
  const cancelFrame = () => {
    if (frame !== null) window.cancelAnimationFrame(frame);
    frame = null;
  };
  const consume = (event: PointerEvent) => {
    event.preventDefault();
    event.stopImmediatePropagation();
  };
  const down = (event: PointerEvent) => {
    if (event.pointerType !== "touch" || !enabled()) return;
    consume(event);
    if (!points.size) {
      target = event.target;
      start = { x: event.clientX, y: event.clientY };
      moved = false;
      view = read();
    } else moved = true;
    points.set(event.pointerId, { x: event.clientX, y: event.clientY });
    node.setPointerCapture(event.pointerId);
  };
  const move = (event: PointerEvent) => {
    if (!points.has(event.pointerId)) return;
    consume(event);
    const before = [...points.values()].slice(0, 2);
    points.set(event.pointerId, { x: event.clientX, y: event.clientY });
    moved ||= Math.hypot(event.clientX - start.x, event.clientY - start.y) > 6;
    if (enabled() && moved) {
      view = touchCamera(view, before, [...points.values()].slice(0, 2));
      // Preview only the camera, at most once per frame. The session and all
      // mounted editors/terminals need the final view, not every pointer sample.
      if (frame === null) frame = window.requestAnimationFrame(paint);
    }
  };
  const end = (event: PointerEvent) => {
    if (!points.has(event.pointerId)) return;
    consume(event);
    points.delete(event.pointerId);
    if (node.hasPointerCapture(event.pointerId))
      node.releasePointerCapture(event.pointerId);
    if (event.type !== "pointerup") moved = true;
    if (!points.size) {
      cancelFrame();
      if (moved) write(enabled() ? view : read(), true);
      else if (enabled()) tap(target);
    }
  };
  node.addEventListener("pointerdown", down, true);
  window.addEventListener("pointermove", move, true);
  window.addEventListener("pointerup", end, true);
  window.addEventListener("pointercancel", end, true);
  node.addEventListener("lostpointercapture", end, true);
  return () => {
    cancelFrame();
    node.removeEventListener("pointerdown", down, true);
    window.removeEventListener("pointermove", move, true);
    window.removeEventListener("pointerup", end, true);
    window.removeEventListener("pointercancel", end, true);
    node.removeEventListener("lostpointercapture", end, true);
    for (const id of points.keys())
      if (node.hasPointerCapture(id)) node.releasePointerCapture(id);
    points.clear();
  };
}
