// Phone touch routing is separate from desktop pointer/selection handling.
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
  fullscreen: () => boolean = () => false,
  nativeContent: (target: EventTarget | null) => boolean = () => false,
) {
  let points: Point[] = [];
  let target: EventTarget | null = null;
  let start: Point = { x: 0, y: 0 };
  let moved = false;
  let owned = false;
  let pinched = false;
  let changed = false;
  let view: View;
  let frame: number | null = null;
  const paint = () => {
    frame = null;
    write(enabled() && !fullscreen() ? view : read(), false);
  };
  const cancelFrame = () => {
    if (frame !== null) window.cancelAnimationFrame(frame);
    frame = null;
  };
  const consume = (event: TouchEvent) => {
    event.preventDefault();
    event.stopImmediatePropagation();
  };
  const positions = (event: TouchEvent) =>
    Array.from(event.touches)
      .slice(0, 2)
      .map((touch) => ({
        x: touch.clientX,
        y: touch.clientY,
      }));
  const down = (event: TouchEvent) => {
    if (!enabled()) return;
    if (!points.length) {
      target = event.target;
      start = positions(event)[0];
      moved = false;
      pinched = false;
      changed = false;
      owned = !fullscreen() && !nativeContent(target);
      view = read();
    }
    points = positions(event);
    if (points.length > 1) pinched = moved = true;
    if (owned || pinched) consume(event);
  };
  const move = (event: TouchEvent) => {
    if (!points.length) return;
    const before = points;
    points = positions(event);
    if (points.length > 1) pinched = moved = true;
    if (owned || pinched) consume(event);
    if (!points.length) return;
    moved ||= Math.hypot(points[0].x - start.x, points[0].y - start.y) > 6;
    if (
      enabled() &&
      !fullscreen() &&
      moved &&
      before.length === points.length &&
      (points.length === 2 || (owned && !pinched))
    ) {
      view = touchCamera(view, before, points);
      changed = true;
      // Preview only the camera, at most once per frame. The session and all
      // mounted editors/terminals need the final view, not every pointer sample.
      if (frame === null) frame = window.requestAnimationFrame(paint);
    }
  };
  const end = (event: TouchEvent) => {
    if (!points.length) return;
    if (owned || pinched) consume(event);
    points = event.type === "touchcancel" ? [] : positions(event);
    if (!points.length) {
      cancelFrame();
      if (changed) write(enabled() && !fullscreen() ? view : read(), true);
      else if (
        owned &&
        !moved &&
        enabled() &&
        !fullscreen() &&
        event.type === "touchend"
      )
        tap(target);
    }
  };
  const options = { capture: true, passive: false };
  node.addEventListener("touchstart", down, options);
  window.addEventListener("touchmove", move, options);
  window.addEventListener("touchend", end, options);
  window.addEventListener("touchcancel", end, options);
  return () => {
    cancelFrame();
    node.removeEventListener("touchstart", down, true);
    window.removeEventListener("touchmove", move, true);
    window.removeEventListener("touchend", end, true);
    window.removeEventListener("touchcancel", end, true);
    points = [];
  };
}
