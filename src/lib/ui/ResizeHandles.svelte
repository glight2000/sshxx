<script lang="ts" context="module">
  export type ResizeDirection =
    "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "nw";
</script>

<script lang="ts">
  import { createEventDispatcher } from "svelte";

  export let disabled = false;

  const dispatch = createEventDispatcher<{
    start: { event: MouseEvent; direction: ResizeDirection };
  }>();

  const handles: { direction: ResizeDirection; label: string }[] = [
    { direction: "n", label: "Resize from top" },
    { direction: "ne", label: "Resize from top right" },
    { direction: "e", label: "Resize from right" },
    { direction: "se", label: "Resize from bottom right" },
    { direction: "s", label: "Resize from bottom" },
    { direction: "sw", label: "Resize from bottom left" },
    { direction: "w", label: "Resize from left" },
    { direction: "nw", label: "Resize from top left" },
  ];
</script>

{#each handles as handle (handle.direction)}
  <button
    type="button"
    aria-label={handle.label}
    class="resize-handle {handle.direction}"
    {disabled}
    on:mousedown|stopPropagation={(event) => {
      if (event.button === 0 && !disabled) {
        event.preventDefault();
        dispatch("start", { event, direction: handle.direction });
      }
    }}
    on:pointerdown|stopPropagation
  ></button>
{/each}

<style lang="postcss">
  .resize-handle {
    position: absolute;
    z-index: 30;
    padding: 0;
    border: 0;
    background: transparent;
  }

  .resize-handle:disabled {
    pointer-events: none;
  }

  .resize-handle.n,
  .resize-handle.s {
    left: 12px;
    right: 12px;
    height: 8px;
    cursor: ns-resize;
  }

  .resize-handle.n {
    top: -4px;
  }
  .resize-handle.s {
    bottom: -4px;
  }

  .resize-handle.e,
  .resize-handle.w {
    top: 12px;
    bottom: 12px;
    width: 8px;
    cursor: ew-resize;
  }

  .resize-handle.e {
    right: -4px;
  }
  .resize-handle.w {
    left: -4px;
  }

  .resize-handle.ne,
  .resize-handle.se,
  .resize-handle.sw,
  .resize-handle.nw {
    width: 16px;
    height: 16px;
  }

  .resize-handle.ne {
    top: -6px;
    right: -6px;
    cursor: nesw-resize;
  }
  .resize-handle.se {
    right: -6px;
    bottom: -6px;
    cursor: nwse-resize;
  }
  .resize-handle.sw {
    bottom: -6px;
    left: -6px;
    cursor: nesw-resize;
  }
  .resize-handle.nw {
    top: -6px;
    left: -6px;
    cursor: nwse-resize;
  }
</style>
