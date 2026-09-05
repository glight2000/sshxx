<script lang="ts" context="module">
  export type CanvasRelationItem = {
    id: number;
    label: string;
    kind: "terminal" | "note" | "file";
  };
</script>

<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import {
    Edit3Icon,
    FileTextIcon,
    PlusIcon,
    TerminalIcon,
  } from "svelte-feather-icons";

  export let items: CanvasRelationItem[] = [];
  export let allowAdd = false;
  export let selecting = false;
  export let disabled = false;

  const dispatch = createEventDispatcher<{
    toggleAdd: void;
    navigate: CanvasRelationItem;
    remove: CanvasRelationItem;
  }>();
</script>

{#if allowAdd || items.length}
  <div
    class="relation-strip"
    class:selecting
    aria-label="Associated canvas items"
  >
    {#if allowAdd}
      <button
        type="button"
        class="relation-button add-button"
        class:active={selecting}
        data-link-toggle
        {disabled}
        aria-label={selecting
          ? "Cancel target selection"
          : "Associate a terminal, note, or file editor"}
        title={selecting
          ? "Cancel target selection"
          : "Associate a terminal, note, or file editor"}
        on:mousedown|stopPropagation={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          dispatch("toggleAdd");
        }}><PlusIcon /></button
      >
    {/if}
    {#each [...items].reverse() as item (`${item.kind}:${item.id}`)}
      <button
        type="button"
        class="relation-button item-button"
        aria-label={`Go to ${item.label}`}
        title={`${item.label} · Left-click to locate · Right-click to unlink`}
        on:mousedown|stopPropagation={(event) => {
          if (event.button !== 0) return;
          event.preventDefault();
          dispatch("navigate", item);
        }}
        on:contextmenu|stopPropagation|preventDefault={() =>
          !disabled && dispatch("remove", item)}
      >
        {#if item.kind === "terminal"}<TerminalIcon
          />{:else if item.kind === "file"}<Edit3Icon />{:else}<FileTextIcon
          />{/if}
      </button>
    {/each}
  </div>
{/if}

<style lang="postcss">
  @reference "../../app.css";

  .relation-strip {
    @apply flex min-w-0 flex-row-reverse items-center gap-1 overflow-x-auto rounded-md border border-zinc-700/50 bg-zinc-900/25 px-1 py-0.5;
    scrollbar-width: none;
  }
  .relation-strip::-webkit-scrollbar {
    display: none;
  }
  .relation-button {
    @apply inline-flex h-6 w-6 shrink-0 items-center justify-center rounded text-zinc-400 outline-none hover:bg-zinc-700/40 hover:text-zinc-100 focus-visible:ring-2 focus-visible:ring-indigo-400 disabled:opacity-35;
  }
  .relation-button :global(svg) {
    @apply h-3.5 w-3.5;
  }
  .add-button.active {
    @apply bg-indigo-400/20 text-zinc-100 ring-1 ring-indigo-400/60;
    animation: relation-selecting 1.25s ease-in-out infinite;
  }
  .item-button {
    @apply bg-white/[0.045];
  }
  @keyframes relation-selecting {
    50% {
      box-shadow: 0 0 8px rgb(165 180 252 / 45%);
    }
  }
</style>
