import { cubicOut } from "svelte/easing";
import type { Action } from "svelte/action";

export type SlideParams = {
  x: number;
  y: number;
  immediate?: boolean;
};

type Position = { x: number; y: number };

const TRANSITION_DURATION = 150;

function samePosition(a: Position, b: Position) {
  return a.x === b.x && a.y === b.y;
}

/**
 * Position one item in world coordinates. The shared canvas-world element owns
 * camera pan and zoom, so camera updates never fan out to every item action.
 * Only remote/programmatic item movement is eased.
 */
export const slide: Action<HTMLElement, SlideParams> = (node, params) => {
  let current = { x: params.x ?? 0, y: params.y ?? 0 };
  let target = current;
  let animationFrame: number | null = null;
  const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

  const render = () => {
    node.style.transform = `translate3d(${current.x}px, ${current.y}px, 0)`;
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
      const next = { x: nextParams.x ?? 0, y: nextParams.y ?? 0 };

      if (nextParams.immediate || reducedMotion.matches) {
        stopAnimation();
        target = next;
        if (!samePosition(next, current)) {
          current = next;
          render();
        }
      } else if (!samePosition(next, target)) {
        animateTo(next);
      }
    },

    destroy() {
      stopAnimation();
      node.style.transform = "";
    },
  };
};
