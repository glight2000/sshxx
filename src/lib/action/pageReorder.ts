import {
  movePage,
  pageMoveAtX,
  pageMoveForOrder,
  pageScrollSpeed,
  type PageMove,
} from "#lib/pageOrder.ts";
import { mobilePageReorder } from "./mobilePageReorder.ts";

export type PageReorderPreview = {
  ids: number[];
  drag: {
    id: number;
    left: number;
    top: number;
    width: number;
    height: number;
    inside: boolean;
  } | null;
};
type Options = {
  pages: readonly { id: number }[];
  enabled: boolean;
  preview(value: PageReorderPreview | null): void;
  commit(move: PageMove): void;
};
type Point = { clientX: number; clientY: number; target: EventTarget | null };

/** Browser-local preview only. A drop sends one anchored move to the server. */
export function pageReorder(node: HTMLElement, options: Options) {
  let candidate: { pointerId: number; point: Point } | null = null;
  let drag: {
    id: number;
    x: number;
    y: number;
    offsetX: number;
    offsetY: number;
    width: number;
    height: number;
  } | null = null;
  let ids: number[] = [];
  let frame = 0;
  let lastFrame = 0;
  let pending: ReturnType<typeof setTimeout> | undefined;
  let suppressClick = false;
  let capturedPointer: number | null = null;
  let previousCursor = "";
  let previousSelection = "";

  const inside = () => {
    if (!drag) return false;
    const bounds = node.getBoundingClientRect();
    return (
      drag.x >= bounds.left - 12 &&
      drag.x <= bounds.right + 12 &&
      drag.y >= bounds.top - 24 &&
      drag.y <= bounds.bottom + 24
    );
  };
  const publish = () => {
    if (!drag) return;
    options.preview({
      ids,
      drag: {
        id: drag.id,
        left: drag.x - drag.offsetX,
        top: drag.y - drag.offsetY,
        width: drag.width,
        height: drag.height,
        inside: inside(),
      },
    });
  };
  const reorder = () => {
    if (!drag || !inside()) return;
    const bounds = node.getBoundingClientRect();
    const boxes = Array.from(
      node.querySelectorAll<HTMLElement>("[data-canvas-page-id]"),
    ).map((item) => ({
      id: Number(item.dataset.canvasPageId),
      center:
        bounds.left + item.offsetLeft - node.scrollLeft + item.offsetWidth / 2,
    }));
    ids = movePage(
      ids,
      pageMoveAtX(boxes, drag.id, drag.x - drag.offsetX + drag.width / 2),
    );
  };
  const scroll = (now: number) => {
    if (!drag) return;
    const bounds = node.getBoundingClientRect();
    const speed = inside()
      ? pageScrollSpeed(drag.x, bounds.left, bounds.right)
      : 0;
    const before = node.scrollLeft;
    node.scrollLeft += (speed * Math.min(32, now - (lastFrame || now))) / 1000;
    lastFrame = now;
    if (before !== node.scrollLeft) {
      reorder();
      publish();
    }
    frame = requestAnimationFrame(scroll);
  };
  const start = (point: Point) => {
    if (!options.enabled || pending || options.pages.length < 2 || drag)
      return false;
    if (
      point.target instanceof Element &&
      point.target.closest("[data-page-action]")
    )
      return false;
    const item =
      point.target instanceof Element
        ? point.target.closest<HTMLElement>("[data-canvas-page-id]")
        : null;
    if (!item || !node.contains(item)) return false;
    const id = Number(item.dataset.canvasPageId);
    if (!options.pages.some((page) => page.id === id)) return false;
    const bounds = item.getBoundingClientRect();
    ids = options.pages.map((page) => page.id);
    drag = {
      id,
      x: point.clientX,
      y: point.clientY,
      offsetX: point.clientX - bounds.left,
      offsetY: point.clientY - bounds.top,
      width: bounds.width,
      height: bounds.height,
    };
    suppressClick = true;
    previousCursor = document.documentElement.style.cursor;
    previousSelection = document.documentElement.style.userSelect;
    document.documentElement.style.cursor = "grabbing";
    document.documentElement.style.userSelect = "none";
    publish();
    frame = requestAnimationFrame(scroll);
    return true;
  };
  const move = (point: Point) => {
    if (!drag) return;
    drag.x = point.clientX;
    drag.y = point.clientY;
    reorder();
    publish();
  };
  const end = (commit: boolean) => {
    candidate = null;
    if (capturedPointer !== null) {
      const pointerId = capturedPointer;
      capturedPointer = null;
      if (node.hasPointerCapture(pointerId))
        node.releasePointerCapture(pointerId);
    }
    if (!drag) return;
    const change =
      commit && inside()
        ? pageMoveForOrder(
            options.pages.map((page) => page.id),
            ids,
            drag.id,
          )
        : null;
    drag = null;
    cancelAnimationFrame(frame);
    lastFrame = 0;
    document.documentElement.style.cursor = previousCursor;
    document.documentElement.style.userSelect = previousSelection;
    if (change) {
      options.preview({ ids, drag: null });
      // Hold the drop position until the authoritative echo. Never retry an
      // unacknowledged mutation, and never retain a speculative order forever.
      pending = setTimeout(() => {
        pending = undefined;
        options.preview(null);
      }, 2000);
      options.commit(change);
    } else options.preview(null);
  };
  const touch = mobilePageReorder(node, { start, move, end });
  const cancel = () => {
    end(false);
    touch.cancel();
    clearTimeout(pending);
    pending = undefined;
    options.preview(null);
  };
  const down = (event: PointerEvent) => {
    if (event.pointerType === "touch") return;
    suppressClick = false;
    if (event.button !== 0 || !event.isPrimary || !options.enabled) return;
    candidate = { pointerId: event.pointerId, point: event };
  };
  const pointerMove = (event: PointerEvent) => {
    if (!candidate || candidate.pointerId !== event.pointerId) return;
    if (
      !drag &&
      Math.hypot(
        event.clientX - candidate.point.clientX,
        event.clientY - candidate.point.clientY,
      ) >= 6
    ) {
      if (start(candidate.point)) {
        // Keep receiving the drop even when crossing an embedded iframe or
        // leaving the pager. Capture begins only after the drag threshold.
        node.setPointerCapture(event.pointerId);
        capturedPointer = event.pointerId;
      }
    }
    if (drag) {
      event.preventDefault();
      move(event);
    }
  };
  const up = (event: PointerEvent) => {
    if (candidate?.pointerId !== event.pointerId) return;
    if (drag) move(event);
    end(true);
  };
  const pointerCancel = (event: PointerEvent) => {
    if (candidate?.pointerId === event.pointerId) cancel();
  };
  const captureLost = (event: PointerEvent) => {
    if (capturedPointer === event.pointerId) cancel();
  };
  const click = (event: MouseEvent) => {
    if (suppressClick && event.detail !== 0) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  };
  const touchDown = () => {
    suppressClick = false;
  };
  const key = (event: KeyboardEvent) => {
    if (event.key === "Escape" && (drag || candidate)) {
      event.preventDefault();
      event.stopPropagation();
      cancel();
    }
  };
  const nativeDrag = (event: DragEvent) => event.preventDefault();
  node.addEventListener("pointerdown", down);
  node.addEventListener("touchstart", touchDown, { passive: true });
  node.addEventListener("click", click, true);
  node.addEventListener("dblclick", click, true);
  node.addEventListener("dragstart", nativeDrag);
  node.addEventListener("lostpointercapture", captureLost);
  window.addEventListener("pointermove", pointerMove, { passive: false });
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", pointerCancel);
  window.addEventListener("keydown", key, true);
  window.addEventListener("blur", cancel);
  window.addEventListener("resize", cancel);
  return {
    update(next: Options) {
      if (options.pages !== next.pages || !next.enabled) cancel();
      options = next;
    },
    destroy() {
      cancel();
      touch.destroy();
      node.removeEventListener("pointerdown", down);
      node.removeEventListener("touchstart", touchDown);
      node.removeEventListener("click", click, true);
      node.removeEventListener("dblclick", click, true);
      node.removeEventListener("dragstart", nativeDrag);
      node.removeEventListener("lostpointercapture", captureLost);
      window.removeEventListener("pointermove", pointerMove);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", pointerCancel);
      window.removeEventListener("keydown", key, true);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("resize", cancel);
    },
  };
}
