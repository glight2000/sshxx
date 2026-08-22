/**
 * @file Handles pan and zoom events to create an infinite canvas.
 *
 * This file is modified from Dispict <https://github.com/ekzhang/dispict>,
 * which itself is loosely based on tldraw.
 */

import {
  Gesture,
  type Handler,
  type WebKitGestureEvent,
} from "@use-gesture/vanilla";
import {
  addVector,
  clamp,
  divideVector,
  lerpVector,
  multiplyVector,
  subtractVector,
  vectorsEqual,
} from "$lib/vector2";

// Credits: from excalidraw
// https://github.com/excalidraw/excalidraw/blob/07ebd7c68ce6ff92ddbc22d1c3d215f2b21328d6/src/utils.ts#L542-L563
const getNearestScrollableContainer = (
  element: HTMLElement,
): HTMLElement | Document => {
  let parent = element.parentElement;
  while (parent) {
    if (parent === document.body) {
      return document;
    }
    const { overflowY } = window.getComputedStyle(parent);
    const hasScrollableContent = parent.scrollHeight > parent.clientHeight;
    if (
      hasScrollableContent &&
      (overflowY === "auto" ||
        overflowY === "scroll" ||
        overflowY === "overlay")
    ) {
      return parent;
    }
    parent = parent.parentElement;
  }
  return document;
};

/** Whether a wheel event started inside a scrollable descendant of the canvas. */
const isInsideScrollableContent = (
  target: EventTarget | null,
  boundary: HTMLElement,
) => {
  let element = target instanceof HTMLElement ? target : null;
  while (element && element !== boundary) {
    const { overflowX, overflowY } = window.getComputedStyle(element);
    const scrollsVertically =
      element.scrollHeight > element.clientHeight &&
      (overflowY === "auto" ||
        overflowY === "scroll" ||
        overflowY === "overlay");
    const scrollsHorizontally =
      element.scrollWidth > element.clientWidth &&
      (overflowX === "auto" ||
        overflowX === "scroll" ||
        overflowX === "overlay");
    if (scrollsVertically || scrollsHorizontally) return true;
    element = element.parentElement;
  }
  return false;
};

function isDarwin(): boolean {
  return /Mac|iPod|iPhone|iPad/.test(window.navigator.platform);
}

function debounce<T extends (...args: any[]) => void>(fn: T, ms = 0) {
  let timeoutId: number | any;
  return function (...args: Parameters<T>) {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn.apply(args), ms);
  };
}

const MIN_ZOOM = 0.35;
const MAX_ZOOM = 2;
const WHEEL_ZOOM_SPEED = 2.5;
export const INITIAL_ZOOM = 1.0;

export class TouchZoom {
  #node: HTMLElement;
  #shouldZoomWheel: () => boolean;
  #isEnabled: () => boolean;
  #canvasPanButton: () => number;
  #scrollingAnchor: HTMLElement | Document;
  #gesture: Gesture;
  #resizeObserver: ResizeObserver;

  #bounds = {
    minX: 0,
    maxX: 0,
    minY: 0,
    maxY: 0,
    width: 0,
    height: 0,
  };
  #originPoint: number[] | undefined = undefined;
  #delta: number[] = [0, 0];
  #lastMovement = 1;
  #wheelLastTimeStamp = 0;
  #middlePointerId: number | null = null;
  #middleLastPoint: number[] = [0, 0];
  #secondaryPointerId: number | null = null;
  #secondaryLastPoint: number[] = [0, 0];
  #secondaryStartPoint: number[] = [0, 0];
  #secondaryMoved = false;
  #suppressContextMenu = false;
  #dragPanning = false;
  #moveFrame: number | null = null;
  #pendingMoveIsManual = false;

  #callbacks = new Set<(manual: boolean) => void>();

  isPinching = false;
  center: number[] = [0, 0];
  zoom = INITIAL_ZOOM;

  #preventGesture = (event: TouchEvent) => event.preventDefault();

  constructor(
    node: HTMLElement,
    shouldZoomWheel = () => false,
    isEnabled = () => true,
    canvasPanButton = () => 2,
  ) {
    this.#node = node;
    this.#shouldZoomWheel = shouldZoomWheel;
    this.#isEnabled = isEnabled;
    this.#canvasPanButton = canvasPanButton;
    this.#scrollingAnchor = getNearestScrollableContainer(node);
    // @ts-ignore
    document.addEventListener("gesturestart", this.#preventGesture);
    // @ts-ignore
    document.addEventListener("gesturechange", this.#preventGesture);

    this.#updateBounds();
    window.addEventListener("resize", this.#updateBoundsD);
    this.#scrollingAnchor.addEventListener("scroll", this.#updateBoundsD);
    node.addEventListener("pointerdown", this.#handleMiddlePointerDown, {
      capture: true,
    });
    node.addEventListener("pointerdown", this.#handleSecondaryPointerDown, {
      capture: true,
    });
    node.addEventListener("auxclick", this.#preventMiddleAuxClick, {
      capture: true,
    });
    window.addEventListener("pointermove", this.#handleMiddlePointerMove, {
      capture: true,
    });
    window.addEventListener("pointermove", this.#handleSecondaryPointerMove, {
      capture: true,
    });
    window.addEventListener("pointerup", this.#handleMiddlePointerEnd, {
      capture: true,
    });
    window.addEventListener("pointerup", this.#handleSecondaryPointerEnd, {
      capture: true,
    });
    window.addEventListener("pointercancel", this.#handleMiddlePointerEnd, {
      capture: true,
    });
    window.addEventListener("pointercancel", this.#handleSecondaryPointerEnd, {
      capture: true,
    });
    window.addEventListener("wheel", this.#handleForcedWheel, {
      capture: true,
      passive: false,
    });

    this.#resizeObserver = new ResizeObserver((entries) => {
      if (this.isPinching) return;
      if (entries[0].contentRect) this.#updateBounds();
    });
    this.#resizeObserver.observe(node);

    this.#gesture = new Gesture(
      node,
      {
        onWheel: this.#handleWheel,
        onPinchStart: this.#handlePinchStart,
        onPinch: this.#handlePinch,
        onPinchEnd: this.#handlePinchEnd,
        onDragStart: this.#handleDragStart,
        onDrag: this.#handleDrag,
        onDragEnd: this.#handleDragEnd,
      },
      {
        target: node,
        eventOptions: { passive: false },
        pinch: {
          from: [this.zoom, 0],
          scaleBounds: () => {
            return { from: this.zoom, max: MAX_ZOOM, min: MIN_ZOOM };
          },
        },
        drag: {
          // `filterTaps` installs a capturing click listener on the whole
          // canvas. Canvas items intentionally stop pointerdown propagation,
          // so that listener cannot observe the matching pointer gesture and
          // ends up suppressing otherwise valid button clicks inside them.
          // Keep the same tap-sized activation threshold without installing
          // the global click suppressor.
          filterTaps: false,
          threshold: 3,
          pointer: { keys: false },
        },
      },
    );
  }

  #getPoint(e: PointerEvent | Touch | WheelEvent): number[] {
    return [
      +e.clientX.toFixed(2) - this.#bounds.minX,
      +e.clientY.toFixed(2) - this.#bounds.minY,
    ];
  }

  #updateBounds = () => {
    const rect = this.#node.getBoundingClientRect();
    this.#bounds = {
      minX: rect.left,
      maxX: rect.left + rect.width,
      minY: rect.top,
      maxY: rect.top + rect.height,
      width: rect.width,
      height: rect.height,
    };
  };

  #updateBoundsD = debounce(this.#updateBounds, 100);

  #handleMiddlePointerDown = (event: PointerEvent) => {
    if (
      !this.#isEnabled() ||
      event.button !== 1 ||
      this.#middlePointerId !== null
    )
      return;

    event.preventDefault();
    event.stopPropagation();
    window.getSelection()?.removeAllRanges();
    this.#middlePointerId = event.pointerId;
    this.#middleLastPoint = [event.clientX, event.clientY];
    this.#node.classList.add("canvas-middle-panning");
    this.#updatePanningSelectionGuard();
  };

  #handleMiddlePointerMove = (event: PointerEvent) => {
    if (event.pointerId !== this.#middlePointerId) return;
    if (!this.#isEnabled()) {
      this.#finishMiddlePointer();
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    const point = [event.clientX, event.clientY];
    const delta = subtractVector(point, this.#middleLastPoint);
    this.#middleLastPoint = point;
    if (vectorsEqual(delta, [0, 0])) return;

    this.center = subtractVector(this.center, divideVector(delta, this.zoom));
    this.#moved();
  };

  #handleMiddlePointerEnd = (event: PointerEvent) => {
    if (event.pointerId !== this.#middlePointerId) return;

    event.preventDefault();
    event.stopPropagation();
    this.#finishMiddlePointer();
  };

  #finishMiddlePointer() {
    this.#middlePointerId = null;
    this.#node.classList.remove("canvas-middle-panning");
    this.#updatePanningSelectionGuard();
  }

  #preventMiddleAuxClick = (event: MouseEvent) => {
    if (event.button !== 1) return;
    event.preventDefault();
    event.stopPropagation();
  };

  #handleSecondaryPointerDown = (event: PointerEvent) => {
    if (
      !this.#isEnabled() ||
      event.button !== this.#canvasPanButton() ||
      event.target !== this.#node ||
      this.#secondaryPointerId !== null
    )
      return;

    event.preventDefault();
    event.stopPropagation();
    this.#secondaryPointerId = event.pointerId;
    this.#secondaryStartPoint = [event.clientX, event.clientY];
    this.#secondaryLastPoint = [...this.#secondaryStartPoint];
    this.#secondaryMoved = false;
    this.#suppressContextMenu = false;
  };

  #handleSecondaryPointerMove = (event: PointerEvent) => {
    if (event.pointerId !== this.#secondaryPointerId) return;
    if (!this.#isEnabled()) {
      this.#finishSecondaryPointer();
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    const point = [event.clientX, event.clientY];
    const delta = subtractVector(point, this.#secondaryLastPoint);
    this.#secondaryLastPoint = point;
    if (vectorsEqual(delta, [0, 0])) return;

    if (!this.#secondaryMoved) {
      const total = subtractVector(point, this.#secondaryStartPoint);
      if (Math.hypot(total[0], total[1]) < 3) return;
      this.#secondaryMoved = true;
      this.#node.classList.add("canvas-secondary-panning");
      this.#updatePanningSelectionGuard();
      window.getSelection()?.removeAllRanges();
    }

    this.center = subtractVector(this.center, divideVector(delta, this.zoom));
    this.#moved();
  };

  #handleSecondaryPointerEnd = (event: PointerEvent) => {
    if (event.pointerId !== this.#secondaryPointerId) return;

    event.preventDefault();
    event.stopPropagation();
    this.#finishSecondaryPointer();
  };

  #finishSecondaryPointer() {
    this.#suppressContextMenu ||=
      this.#secondaryMoved && this.#canvasPanButton() === 2;
    this.#secondaryPointerId = null;
    this.#secondaryMoved = false;
    this.#node.classList.remove("canvas-secondary-panning");
    this.#updatePanningSelectionGuard();
  }

  /** Returns whether the most recent secondary click was consumed by panning. */
  consumeContextMenuSuppression() {
    const suppressed = this.#suppressContextMenu;
    this.#suppressContextMenu = false;
    return suppressed;
  }

  isSecondaryPointerActive() {
    return this.#canvasPanButton() === 2 && this.#secondaryPointerId !== null;
  }

  onMove(callback: (manual: boolean) => void): () => void {
    this.#callbacks.add(callback);
    return () => this.#callbacks.delete(callback);
  }

  async moveTo(pos: number[], zoom: number) {
    // Cubic bezier easing
    const smoothstep = (z: number) => {
      const x = Math.max(0, Math.min(1, z));
      return x * x * (3 - 2 * x);
    };

    const beginTime = Date.now();
    const totalTime = 350; // milliseconds

    const start = this.center;
    const startZ = 1 / this.zoom;
    const finishZ = 1 / zoom;
    while (true) {
      const t = Date.now() - beginTime;
      if (t > totalTime) break;
      const k = smoothstep(t / totalTime);

      this.center = lerpVector(
        start as [number, number],
        pos as [number, number],
        k,
      );
      this.zoom = 1 / (startZ * (1 - k) + finishZ * k);
      this.#moved(false);
      await new Promise((resolve) => requestAnimationFrame(resolve));
    }
    this.center = pos;
    this.zoom = zoom;
    this.#moved(false);
  }

  setView(pos: number[], zoom: number) {
    this.center = [...pos];
    this.zoom = zoom;
    this.#moved(false);
  }

  #moved(manual = true) {
    this.#pendingMoveIsManual ||= manual;
    if (this.#moveFrame !== null) return;
    this.#moveFrame = requestAnimationFrame(() => {
      this.#moveFrame = null;
      const isManual = this.#pendingMoveIsManual;
      this.#pendingMoveIsManual = false;
      for (const callback of this.#callbacks) callback(isManual);
    });
  }

  #handleForcedWheel = (event: WheelEvent) => {
    if (event.ctrlKey) this.#processWheel(event);
  };

  #handleWheel: Handler<"wheel", WheelEvent> = ({ event }) => {
    this.#processWheel(event);
  };

  #processWheel(e: WheelEvent) {
    if (!this.#isEnabled()) {
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        e.stopPropagation();
      }
      return;
    }
    // Menus and other scrollable surfaces rendered inside the canvas own the
    // wheel while hovered. This keeps their scroll state separate from canvas
    // pan/zoom state, including when the canvas currently has no focus.
    if (!e.ctrlKey && isInsideScrollableContent(e.target, this.#node)) {
      e.stopPropagation();
      return;
    }

    e.preventDefault();
    if (e.ctrlKey) e.stopPropagation();
    if (this.isPinching || e.timeStamp <= this.#wheelLastTimeStamp) return;

    this.#wheelLastTimeStamp = e.timeStamp;

    const [x, y, z] = normalizeWheel(e);

    // Modifier+scroll always zooms. Plain scrolling can also zoom when the
    // owning canvas has no active interactive item.
    if (
      (e.altKey || e.ctrlKey || e.metaKey || this.#shouldZoomWheel()) &&
      e.buttons === 0
    ) {
      const point =
        e.clientX && e.clientY
          ? this.#getPoint(e)
          : [this.#bounds.width / 2, this.#bounds.height / 2];
      const delta = z * 0.618 * WHEEL_ZOOM_SPEED;

      let newZoom = (1 - delta / 320) * this.zoom;
      newZoom = clamp(newZoom, MIN_ZOOM, MAX_ZOOM);

      const movement = multiplyVector(point, 1 / this.zoom - 1 / newZoom);
      this.center = addVector(this.center, movement);
      this.zoom = newZoom;

      this.#moved();
      return;
    }

    // otherwise pan
    const delta = multiplyVector(
      e.shiftKey && !isDarwin()
        ? // shift+scroll = pan horizontally
          [y, 0]
        : // scroll = pan vertically (or in any direction on a trackpad)
          [x, y],
      0.5,
    );

    if (vectorsEqual(delta, [0, 0])) return;

    this.center = addVector(this.center, divideVector(delta, this.zoom));
    this.#moved();
  }

  #handlePinchStart: Handler<
    "pinch",
    WheelEvent | PointerEvent | TouchEvent | WebKitGestureEvent
  > = ({ origin, event }) => {
    if (!this.#isEnabled() || event instanceof WheelEvent) return;

    this.isPinching = true;
    this.#originPoint = origin;
    this.#delta = [0, 0];
    this.#lastMovement = 1;
    this.#moved();
  };

  #handlePinch: Handler<
    "pinch",
    WheelEvent | PointerEvent | TouchEvent | WebKitGestureEvent
  > = ({ origin, movement, event }) => {
    if (!this.#isEnabled() || event instanceof WheelEvent) return;

    if (!this.#originPoint) return;
    const delta = subtractVector(
      this.#originPoint as [number, number],
      origin as [number, number],
    );
    const trueDelta = subtractVector(delta, this.#delta as [number, number]);
    this.#delta = delta;

    const zoomLevel = movement[0] / this.#lastMovement;
    this.#lastMovement = movement[0];

    this.center = addVector(
      this.center,
      divideVector(trueDelta, this.zoom * 2),
    );
    this.zoom = clamp(this.zoom * zoomLevel, MIN_ZOOM, MAX_ZOOM);
    this.#moved();
  };

  #handlePinchEnd: Handler<
    "pinch",
    WheelEvent | PointerEvent | TouchEvent | WebKitGestureEvent
  > = () => {
    this.isPinching = false;
    this.#originPoint = undefined;
    this.#delta = [0, 0];
    this.#lastMovement = 1;
    this.#moved();
  };

  #handleDrag: Handler<
    "drag",
    MouseEvent | PointerEvent | TouchEvent | KeyboardEvent
  > = ({ delta, elapsedTime }) => {
    if (!this.#isEnabled() || !this.#dragPanning) return;
    if (delta[0] === 0 && delta[1] === 0 && elapsedTime < 200) return;
    this.center = subtractVector(
      this.center,
      divideVector(delta as [number, number], this.zoom),
    );
    this.#moved();
  };

  #handleDragStart: Handler<
    "drag",
    MouseEvent | PointerEvent | TouchEvent | KeyboardEvent
  > = ({ event }) => {
    if (!this.#isEnabled()) return;
    // Mouse blank-canvas selection and panning are handled explicitly above so
    // their configurable buttons and the stationary context menu stay
    // independent. Preserve the original one-finger touch/pen pan gesture.
    if (
      event instanceof MouseEvent &&
      (!(event instanceof PointerEvent) || event.pointerType === "mouse")
    )
      return;
    event.preventDefault();
    window.getSelection()?.removeAllRanges();
    this.#dragPanning = true;
    this.#updatePanningSelectionGuard();
  };

  #handleDragEnd: Handler<
    "drag",
    MouseEvent | PointerEvent | TouchEvent | KeyboardEvent
  > = () => {
    this.#dragPanning = false;
    this.#updatePanningSelectionGuard();
  };

  #updatePanningSelectionGuard() {
    this.#node.classList.toggle(
      "canvas-panning",
      this.#dragPanning ||
        this.#middlePointerId !== null ||
        this.#secondaryMoved,
    );
  }

  destroy() {
    if (this.#node) {
      // @ts-ignore
      document.removeEventListener("gesturestart", this.#preventGesture);
      // @ts-ignore
      document.removeEventListener("gesturechange", this.#preventGesture);

      window.removeEventListener("resize", this.#updateBoundsD);
      this.#scrollingAnchor.removeEventListener("scroll", this.#updateBoundsD);
      this.#node.removeEventListener(
        "pointerdown",
        this.#handleMiddlePointerDown,
        { capture: true },
      );
      this.#node.removeEventListener(
        "pointerdown",
        this.#handleSecondaryPointerDown,
        { capture: true },
      );
      this.#node.removeEventListener("auxclick", this.#preventMiddleAuxClick, {
        capture: true,
      });
      window.removeEventListener("pointermove", this.#handleMiddlePointerMove, {
        capture: true,
      });
      window.removeEventListener(
        "pointermove",
        this.#handleSecondaryPointerMove,
        { capture: true },
      );
      window.removeEventListener("pointerup", this.#handleMiddlePointerEnd, {
        capture: true,
      });
      window.removeEventListener("pointerup", this.#handleSecondaryPointerEnd, {
        capture: true,
      });
      window.removeEventListener(
        "pointercancel",
        this.#handleMiddlePointerEnd,
        { capture: true },
      );
      window.removeEventListener(
        "pointercancel",
        this.#handleSecondaryPointerEnd,
        { capture: true },
      );
      window.removeEventListener("wheel", this.#handleForcedWheel, {
        capture: true,
      });
      this.#node.classList.remove("canvas-middle-panning");
      this.#node.classList.remove("canvas-secondary-panning");
      this.#node.classList.remove("canvas-panning");

      this.#resizeObserver.disconnect();

      if (this.#moveFrame !== null) cancelAnimationFrame(this.#moveFrame);
      this.#moveFrame = null;
      this.#pendingMoveIsManual = false;

      this.#gesture.destroy();
      this.#node = null as any;
    }
  }
}

// Reasonable defaults
const MAX_ZOOM_STEP = 10;

// Adapted from https://stackoverflow.com/a/13650579
function normalizeWheel(event: WheelEvent) {
  const { deltaY, deltaX } = event;
  const signY = Math.sign(deltaY);
  const deltaZ =
    Math.abs(deltaY) > MAX_ZOOM_STEP ? MAX_ZOOM_STEP * signY : deltaY;

  return [deltaX, deltaY, deltaZ];
}
