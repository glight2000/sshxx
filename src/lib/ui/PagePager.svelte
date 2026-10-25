<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import { flip } from "svelte/animate";
  import { MoreVerticalIcon, PlusIcon, Trash2Icon } from "svelte-feather-icons";

  import type { WsPage } from "#lib/protocol.ts";
  import {
    pageReorder,
    type PageReorderPreview,
  } from "#lib/action/pageReorder.ts";
  import { movePage, type PageMove } from "#lib/pageOrder.ts";

  export let pages: WsPage[];
  export let activePageId: number;
  export let hasWriteAccess: boolean | undefined;
  export let connected = true;
  export let canvasDropPageId: number | null = null;

  const dispatch = createEventDispatcher<{
    select: number;
    create: void;
    rename: { id: number; name: string };
    delete: number;
    reorder: PageMove;
  }>();

  let editingId: number | null = null;
  let draft = "";
  let renameInput: HTMLInputElement;
  let contextPage: WsPage | null = null;
  let menu: HTMLDivElement;
  let menuX = 0;
  let menuY = 0;
  let preview: PageReorderPreview | null = null;
  let announcement = "";
  $: canReorder =
    !!hasWriteAccess && connected && editingId === null && contextPage === null;
  $: orderedPages = preview
    ? preview.ids.flatMap((id) => pages.filter((page) => page.id === id))
    : pages;
  $: draggedPage = pages.find((page) => page.id === preview?.drag?.id);

  function requestReorder(move: PageMove) {
    const order = movePage(
      pages.map((page) => page.id),
      move,
    );
    announcement = `Moving ${pages.find((page) => page.id === move.id)?.name} to position ${order.indexOf(move.id) + 1} of ${pages.length}`;
    dispatch("reorder", move);
  }

  function reorderWithKeyboard(event: KeyboardEvent, page: WsPage) {
    if (
      !canReorder ||
      !event.altKey ||
      !event.shiftKey ||
      !["ArrowLeft", "ArrowRight"].includes(event.key)
    )
      return;
    event.preventDefault();
    event.stopPropagation();
    const index = pages.findIndex((item) => item.id === page.id);
    if (event.key === "ArrowLeft" && index > 0)
      requestReorder({ id: page.id, beforeId: pages[index - 1].id });
    if (event.key === "ArrowRight" && index < pages.length - 1)
      requestReorder({ id: page.id, beforeId: pages[index + 2]?.id ?? null });
  }

  function reorderDuration() {
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches
      ? 0
      : 160;
  }

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
  class="page-pager panel fixed bottom-4 left-1/2 z-30 flex max-w-[calc(100vw-2rem)] -translate-x-1/2 items-center gap-1 p-1.5 shadow-2xl"
>
  <div
    class="page-list relative flex max-w-[min(70vw,52rem)] items-center gap-1 overflow-x-auto"
    class:can-reorder={canReorder && pages.length > 1}
    class:reordering={!!preview?.drag}
    use:pageReorder={{
      pages,
      enabled: canReorder,
      preview: (value) => (preview = value),
      commit: requestReorder,
    }}
  >
    {#each orderedPages as page (page.id)}
      <div
        animate:flip={{ duration: reorderDuration }}
        data-canvas-page-id={page.id}
        class="page-item group flex shrink-0 items-center rounded-md border border-transparent {page.id ===
        activePageId
          ? 'bg-indigo-700 text-white'
          : 'text-zinc-300 hover:bg-zinc-800'}"
        class:page-drop-target={canvasDropPageId === page.id}
        class:page-placeholder={preview?.drag?.id === page.id}
      >
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
          <button
            type="button"
            class="max-w-44 truncate px-3 py-1.5 text-sm"
            aria-current={page.id === activePageId ? "page" : undefined}
            aria-describedby={canReorder ? "page-order-help" : undefined}
            title={canReorder
              ? "Drag to reorder · Touch: hold and drag · Alt+Shift+←/→"
              : page.name}
            on:click={() => dispatch("select", page.id)}
            on:keydown={(event) => reorderWithKeyboard(event, page)}
            on:dblclick={() => beginRename(page)}
            on:contextmenu|preventDefault|stopPropagation={(event) =>
              openContextMenu(page, event)}
          >
            {page.name}
          </button>
          {#if page.id === activePageId}
            <button
              type="button"
              data-page-action
              class="page-actions shrink-0 rounded-md px-1.5 py-1.5 hover:bg-white/10"
              aria-label={`Actions for ${page.name}`}
              title="Page actions"
              on:click={(event) => openContextMenu(page, event)}
              ><MoreVerticalIcon class="h-4 w-4" /></button
            >
          {/if}
        {/if}
      </div>
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

<p id="page-order-help" class="sr-only">
  Drag to reorder pages, or hold and drag on touch screens. With a page button
  focused, use Alt+Shift+Left or Right. Escape cancels dragging.
</p>
<span class="sr-only" aria-live="polite">{announcement}</span>

{#if preview?.drag && draggedPage}
  <div
    class="page-drag-ghost panel fixed z-50 flex items-center rounded-md border border-indigo-400 bg-indigo-700 px-3 text-sm text-white shadow-xl"
    class:outside={!preview.drag.inside}
    aria-hidden="true"
    style:left={`${preview.drag.left}px`}
    style:top={`${preview.drag.top}px`}
    style:width={`${preview.drag.width}px`}
    style:height={`${preview.drag.height}px`}
  >
    <span class="truncate">{draggedPage.name}</span>
  </div>
{/if}

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

  .can-reorder .page-item button {
    cursor: grab;
  }
  .reordering .page-item button {
    cursor: grabbing;
  }
  .page-item .page-actions {
    display: none;
    cursor: pointer;
  }
  @media (any-pointer: coarse) {
    .page-item .page-actions {
      display: block;
    }
  }
  .page-list {
    overscroll-behavior-x: contain;
  }
  .can-reorder .page-item {
    user-select: none;
    -webkit-touch-callout: none;
  }
  .page-placeholder {
    background: rgb(99 102 241 / 0.15);
    border: 1px dashed #818cf8;
  }
  .page-placeholder button {
    opacity: 0;
  }
  .page-drag-ghost {
    pointer-events: none;
    transform: translateY(-4px) scale(1.04);
  }
  .page-drag-ghost.outside {
    opacity: 0.45;
  }

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
