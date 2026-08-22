import { cubicOut } from "svelte/easing";
import type { Action } from "svelte/action";

export type SlideParams = {
  x: number;
  y: number;
  center: number[];
  zoom: number;
  immediate?: boolean;
};

type Position = { x: number; y: number };

const TRANSITION_DURATION = 150;

function samePosition(a: Position, b: Position) {
  return a.x === b.x && a.y === b.y;
}

function snapTranslation(value: number, zoom: number) {
  const devicePixelRatio = window.devicePixelRatio || 1;
  const scale = zoom * devicePixelRatio;
  return Number.isFinite(scale) && scale > 0
    ? Math.round(value * scale) / scale
    : value;
}

/**
 * Position a canvas item while keeping camera updates in the same animation
 * frame as the background grid. Only remote/programmatic item movement is
 * eased; direct manipulation and camera pan/zoom render immediately.
 */
export const slide: Action<HTMLElement, SlideParams> = (node, params) => {
  let center = params.center ?? [0, 0];
  let zoom = params.zoom ?? 1;
  let current = { x: params.x ?? 0, y: params.y ?? 0 };
  let target = current;
  let animationFrame: number | null = null;
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

  const render = () => {
    const x = snapTranslation(current.x - center[0], zoom);
    const y = snapTranslation(current.y - center[1], zoom);
    node.style.transform = `scale(${zoom}) translate(${x}px, ${y}px)`;
  };

  const stopAnimation = () => {
    if (animationFrame !== null) cancelAnimationFrame(animationFrame);
    animationFrame = null;
  };

  const animateTo = (next: Position) => {
    stopAnimation();
    const start = current;
    const startedAt = performance.now();
    target = next;

    const tick = (now: number) => {
      const progress = Math.min(1, (now - startedAt) / TRANSITION_DURATION);
      const eased = cubicOut(progress);
      current = {
        x: start.x + (target.x - start.x) * eased,
        y: start.y + (target.y - start.y) * eased,
      };
      render();
      if (progress < 1) animationFrame = requestAnimationFrame(tick);
      else animationFrame = null;
    };
    animationFrame = requestAnimationFrame(tick);
  };

  render();

  return {
    update(nextParams) {
      center = nextParams.center ?? [0, 0];
      zoom = nextParams.zoom ?? 1;
      const next = { x: nextParams.x ?? 0, y: nextParams.y ?? 0 };

      if (nextParams.immediate || reducedMotion.matches) {
        stopAnimation();
        current = next;
        target = next;
        render();
      } else if (!samePosition(next, target)) {
        animateTo(next);
      } else {
        // Camera changes must never wait for the per-item position tween.
        render();
      }
    },

    destroy() {
      stopAnimation();
      node.style.transform = "";
    },
  };
};
