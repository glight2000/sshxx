<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import { PlusIcon, Trash2Icon } from "svelte-feather-icons";

  import type { WsPage } from "$lib/protocol";

  export let pages: WsPage[];
  export let activePageId: number;
  export let hasWriteAccess: boolean | undefined;
  export let canvasDropPageId: number | null = null;

  const dispatch = createEventDispatcher<{
    select: number;
    create: void;
    rename: { id: number; name: string };
    delete: number;
  }>();

  let editingId: number | null = null;
  let draft = "";
  let renameInput: HTMLInputElement;
  let contextPage: WsPage | null = null;
  let menu: HTMLDivElement;
  let menuX = 0;
  let menuY = 0;

  async function openContextMenu(page: WsPage, event: MouseEvent) {
    contextPage = page;
    menuX = event.clientX;
    menuY = event.clientY;
    await tick();
    if (!menu) return;
    const bounds = menu.getBoundingClientRect();
    menuX = Math.max(8, Math.min(menuX, window.innerWidth - bounds.width - 8));
    menuY = Math.max(
      8,
      Math.min(menuY, window.innerHeight - bounds.height - 8),
    );
    menu.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }

  $: if (contextPage && !pages.some((page) => page.id === contextPage?.id))
    contextPage = null;

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

<svelte:window
  on:pointerdown|capture={(event) => {
    if (event.target instanceof Node && !menu?.contains(event.target))
      contextPage = null;
  }}
  on:keydown={(event) => {
    if (event.key === "Escape") contextPage = null;
  }}
  on:resize={() => (contextPage = null)}
/>

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
            on:contextmenu|preventDefault|stopPropagation={(event) =>
              openContextMenu(page, event)}
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

{#if contextPage}
  <div
    bind:this={menu}
    class="panel fixed z-50 w-48 p-1.5"
    role="menu"
    tabindex="-1"
    aria-label="Page actions"
    style:left={`${menuX}px`}
    style:top={`${menuY}px`}
    on:contextmenu|preventDefault|stopPropagation
    on:pointerdown|stopPropagation
    on:mousedown|stopPropagation
    on:wheel|stopPropagation
  >
    <div class="truncate px-2 py-1 text-xs text-zinc-400">
      {contextPage.name}
    </div>
    <button
      type="button"
      role="menuitem"
      class="flex w-full items-center gap-2 rounded-md px-2 py-2 text-sm text-[var(--surface-danger)] hover:bg-zinc-800 disabled:opacity-40"
      disabled={!hasWriteAccess || pages.length <= 1}
      title={pages.length <= 1
        ? "At least one page must remain"
        : "Delete page"}
      on:click={() => {
        if (contextPage) dispatch("delete", contextPage.id);
        contextPage = null;
      }}><Trash2Icon class="h-4 w-4" />Delete page</button
    >
  </div>
{/if}

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
