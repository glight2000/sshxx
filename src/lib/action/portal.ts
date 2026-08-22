import type { Action } from "svelte/action";

export type PortalParams = {
  active: boolean;
  target: HTMLElement | null;
};

/**
 * Reparent an existing component without recreating it. This keeps stateful
 * surfaces such as xterm and editors alive when a canvas item enters the
 * screen-space fullscreen layer.
 */
export const portal: Action<HTMLElement, PortalParams> = (node, params) => {
  const parent = node.parentNode;

  const move = ({ active, target }: PortalParams) => {
    if (active && target) {
      if (node.parentNode !== target) target.appendChild(node);
      return;
    }
    if (parent && node.parentNode !== parent) {
      parent.appendChild(node);
    }
  };

  move(params);

  return {
    update: move,
    destroy() {
      // A portaled node is no longer a physical child of its keyed-each slot,
      // so removing that slot cannot remove the node. Never restore it here:
      // doing so would resurrect ordinary items that Svelte already detached.
      node.remove();
    },
  };
};
