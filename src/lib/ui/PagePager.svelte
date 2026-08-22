<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import { PlusIcon } from "svelte-feather-icons";

  import type { WsPage } from "$lib/protocol";

  export let pages: WsPage[];
  export let activePageId: number;
  export let hasWriteAccess: boolean | undefined;
  export let canvasDropPageId: number | null = null;

  const dispatch = createEventDispatcher<{
    select: number;
    create: void;
    rename: { id: number; name: string };
  }>();

  let editingId: number | null = null;
  let draft = "";
  let renameInput: HTMLInputElement;

  async function beginRename(page: WsPage) {
    if (!hasWriteAccess) return;
    editingId = page.id;
    draft = page.name;
    await tick();
    renameInput?.focus();
    renameInput?.select();
  }

  function finishRename(save: boolean) {
    if (editingId === null) return;
    const name = draft.trim();
    if (save && name) dispatch("rename", { id: editingId, name });
    editingId = null;
  }
</script>

<nav
  aria-label="Canvas pages"
  class="panel fixed bottom-4 left-1/2 z-30 flex max-w-[calc(100vw-2rem)] -translate-x-1/2 items-center gap-1 p-1.5 shadow-2xl"
>
  <div class="flex max-w-[min(70vw,52rem)] items-center gap-1 overflow-x-auto">
    {#each pages as page (page.id)}
      {#if editingId === page.id}
        <input
          bind:this={renameInput}
          bind:value={draft}
          maxlength="100"
          aria-label="Page name"
          class="w-32 rounded-md border border-indigo-500 bg-zinc-950 px-2.5 py-1.5 text-sm outline-none ring-1 ring-indigo-500/40"
          on:keydown={(event) => {
            if (event.key === "Enter") finishRename(true);
            if (event.key === "Escape") finishRename(false);
          }}
          on:blur={() => finishRename(true)}
        />
      {:else}
        <div
          data-canvas-page-id={page.id}
          class="group flex shrink-0 items-center rounded-md border border-transparent {page.id ===
          activePageId
            ? 'bg-indigo-700 text-white'
            : 'text-zinc-300 hover:bg-zinc-800'}"
          class:page-drop-target={canvasDropPageId === page.id}
        >
          <button
            type="button"
            class="max-w-44 truncate px-3 py-1.5 text-sm"
            aria-current={page.id === activePageId ? "page" : undefined}
            on:click={() => dispatch("select", page.id)}
            on:dblclick={() => beginRename(page)}
          >
            {page.name}
          </button>
        </div>
      {/if}
    {/each}
  </div>

  <div class="h-5 border-l border-zinc-700"></div>
  <button
    type="button"
    aria-label="Add page"
    title="Add page"
    class="rounded-md p-1.5 text-zinc-300 hover:bg-zinc-700 hover:text-white disabled:opacity-40"
    disabled={!hasWriteAccess || pages.length >= 50}
    on:click={() => dispatch("create")}
  >
    <PlusIcon class="h-4 w-4" />
  </button>
</nav>

<style lang="postcss">
  @reference "../../app.css";

  .page-drop-target {
    @apply border-amber-100 bg-amber-300 text-zinc-950 shadow-lg shadow-amber-300/35;
    animation: page-drop-pulse 0.85s ease-in-out infinite;
  }

  @keyframes page-drop-pulse {
    50% {
      box-shadow: 0 0 18px rgb(252 211 77 / 0.65);
      transform: translateY(-2px);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .page-drop-target {
      animation: none;
    }
  }
</style>
