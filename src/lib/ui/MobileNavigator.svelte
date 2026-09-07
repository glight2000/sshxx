<script lang="ts">
  import { createEventDispatcher, onMount, tick } from "svelte";
  import {
    ArrowLeftIcon,
    ChevronDownIcon,
    CodeIcon,
    FileTextIcon,
    FolderIcon,
    GridIcon,
    LayersIcon,
    MoreHorizontalIcon,
    TerminalIcon,
    XIcon,
  } from "svelte-feather-icons";
  import type { WsPage } from "$lib/protocol";
  import {
    groupNavigationItems,
    MOBILE_NAVIGATION_QUERY,
    navigationItemKey,
  } from "$lib/mobileNavigation";
  import type { CanvasSearchItem } from "./TerminalSearch.svelte";

  export let pages: WsPage[];
  export let items: CanvasSearchItem[];
  export let activePageId: number;
  export let currentKey: string | null;
  export let enabled = true;
  export let associationTargets: CanvasSearchItem[] | null = null;

  const dispatch = createEventDispatcher<{
    mode: boolean;
    available: boolean;
    select: CanvasSearchItem;
    page: number;
    back: void;
    actions: { x: number; y: number };
    associate: CanvasSearchItem;
    cancelAssociation: void;
  }>();
  const icons = {
    terminal: TerminalIcon,
    note: FileTextIcon,
    file: FolderIcon,
    custom: CodeIcon,
  };
  let available = false;
  let open = true;
  let expanded: Record<number, boolean> = {};
  let dialog: HTMLDialogElement;
  let previousKey: string | null = null;
  $: groups = groupNavigationItems(pages, associationTargets ?? items).filter(
    (group) => associationTargets === null || group.items.length > 0,
  );
  $: current = items.find((item) => navigationItemKey(item) === currentKey);
  $: if (!Object.hasOwn(expanded, activePageId))
    expanded = { ...expanded, [activePageId]: true };
  $: if (previousKey !== currentKey) {
    if (previousKey !== null && currentKey === null) open = true;
    if (currentKey !== null) open = false;
    previousKey = currentKey;
  }
  $: dispatch("mode", available && enabled);
  $: dispatch("available", available);
  $: syncDialog(available && enabled && (open || associationTargets !== null));

  async function syncDialog(show: boolean) {
    await tick();
    // A newer selection/mode change may have happened while waiting for DOM.
    if (
      show !==
        (available && enabled && (open || associationTargets !== null)) ||
      !dialog?.isConnected
    )
      return;
    if (show && !dialog.open) dialog.showModal();
    else if (!show && dialog.open) dialog.close();
  }

  function choose(item: CanvasSearchItem) {
    open = false;
    dialog.close();
    if (associationTargets !== null) dispatch("associate", item);
    else dispatch("select", item);
  }

  function closeList() {
    open = false;
    dispatch("cancelAssociation");
  }

  function showCanvas() {
    open = false;
    enabled = false;
  }

  onMount(() => {
    const media = window.matchMedia(MOBILE_NAVIGATION_QUERY);
    const update = () => {
      available = media.matches;
    };
    update();
    media.addEventListener("change", update);
    return () => {
      media.removeEventListener("change", update);
      dialog?.close();
    };
  });
</script>

{#if available}
  {#if enabled}
    <nav
      class="mobile-navigation"
      class:detail={currentKey !== null}
      data-mobile-navigation
      aria-label="Mobile workspace"
    >
      {#if currentKey !== null}
        <button
          class="back-button"
          aria-label="Back to workspace"
          on:click={() => dispatch("back")}
          ><ArrowLeftIcon size="20" /> Back</button
        >
      {/if}
      <button
        class="workspace-toggle"
        aria-label="Pages and components"
        aria-haspopup="dialog"
        aria-expanded={open}
        on:click={() => (open = !open)}
      >
        <LayersIcon size="19" />
        <span>{current?.title ?? "Pages and components"}</span>
        <ChevronDownIcon size="16" />
      </button>
      {#if currentKey === null}
        <button
          class="icon-button"
          aria-label="Show canvas"
          title="Show canvas"
          on:click={showCanvas}><GridIcon size="19" /></button
        >
      {/if}
      <button
        class="icon-button"
        aria-label="Workspace actions"
        aria-haspopup="menu"
        on:click={(event) => {
          const rect = event.currentTarget.getBoundingClientRect();
          dispatch("actions", { x: rect.left, y: rect.bottom + 4 });
        }}><MoreHorizontalIcon size="19" /></button
      >
    </nav>
  {:else}
    <button
      class="mobile-launcher"
      data-mobile-navigation
      on:click={() => {
        enabled = true;
        open = true;
      }}><LayersIcon size="18" /> Pages</button
    >
  {/if}

  <dialog
    bind:this={dialog}
    class="mobile-dialog"
    data-mobile-navigation
    aria-label={associationTargets !== null
      ? "Choose an association"
      : "Pages and components"}
    on:cancel={closeList}
    on:click={(event) => {
      if (event.target === dialog) closeList();
    }}
    on:wheel|stopPropagation
    on:pointerdown|stopPropagation
  >
    <header>
      <div>
        <strong
          >{associationTargets !== null
            ? "Choose an association"
            : "Workspace"}</strong
        ><span
          >{associationTargets !== null
            ? "Available components on this note’s page"
            : `${pages.length} pages · ${items.length} components`}</span
        >
      </div>
      <button
        class="icon-button"
        aria-label="Close component list"
        on:click={closeList}><XIcon size="20" /></button
      >
    </header>
    <div class="page-list">
      {#each groups as group (group.page.id)}
        <details bind:open={expanded[group.page.id]}>
          <summary class:current-page={group.page.id === activePageId}>
            <ChevronDownIcon size="16" /><LayersIcon size="17" />
            <span>{group.page.name}</span><small>{group.items.length}</small>
          </summary>
          <div class="component-list">
            {#each group.items as item (navigationItemKey(item))}
              <button
                class="component-row"
                aria-current={navigationItemKey(item) === currentKey
                  ? "true"
                  : undefined}
                on:click={() => choose(item)}
              >
                <svelte:component this={icons[item.kind]} size="18" />
                <span>{item.title}</span><small>{item.kind}</small>
              </button>
            {:else}
              <button
                class="empty-page"
                on:click={() => {
                  open = false;
                  dialog.close();
                  dispatch("page", group.page.id);
                }}>Empty page · Open canvas</button
              >
            {/each}
          </div>
        </details>
      {/each}
      {#if associationTargets?.length === 0}<p class="empty-page">
          No available components on this page.
        </p>{/if}
    </div>
    <footer>
      {#if associationTargets !== null}<button on:click={closeList}
          >Cancel</button
        >
      {:else}<button on:click={showCanvas}
          ><GridIcon size="16" /> Switch to canvas</button
        >{/if}
    </footer>
  </dialog>
{/if}

<style>
  .mobile-navigation,
  .mobile-launcher,
  .mobile-dialog {
    background: var(--surface-bg);
    color: var(--app-text);
    border: 1px solid var(--surface-border);
    border-radius: 12px;
  }
  .mobile-navigation {
    position: fixed;
    z-index: 48;
    top: max(6px, env(safe-area-inset-top));
    left: max(8px, env(safe-area-inset-left));
    right: max(8px, env(safe-area-inset-right));
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
  }
  .mobile-navigation.detail {
    top: var(--mobile-viewport-top, 0px);
    left: var(--mobile-viewport-left, 0px);
    right: auto;
    width: var(--mobile-viewport-width, 100%);
    height: calc(52px + env(safe-area-inset-top));
    padding: env(safe-area-inset-top) env(safe-area-inset-right) 0
      env(safe-area-inset-left);
    border: 0;
    border-bottom: 1px solid var(--surface-border);
    border-radius: 0;
  }
  .back-button,
  button {
    cursor: pointer;
  }
  button,
  summary {
    -webkit-tap-highlight-color: transparent;
    touch-action: manipulation;
  }
  button:focus-visible,
  summary:focus-visible {
    outline: 2px solid var(--surface-accent);
    outline-offset: -2px;
  }
  .icon-button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    flex-shrink: 0;
    border-radius: 9px;
  }
  .workspace-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    flex: 1;
    min-height: 44px;
    padding: 0 10px;
    text-align: left;
    font-size: 13px;
  }
  .workspace-toggle span {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .workspace-toggle :global(svg) {
    flex-shrink: 0;
  }
  .mobile-launcher {
    position: fixed;
    left: 12px;
    bottom: 80px;
    z-index: 48;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    padding: 0 12px;
    font-size: 13px;
  }
  .mobile-dialog {
    position: fixed;
    inset: calc(
        var(--mobile-viewport-top, 0px) + env(safe-area-inset-top) + 58px
      )
      max(8px, env(safe-area-inset-right)) auto
      max(8px, env(safe-area-inset-left));
    margin: 0;
    padding: 0;
    width: auto;
    max-width: none;
    max-height: calc(
      var(--mobile-viewport-height, 100dvh) - 76px - env(safe-area-inset-top) -
        env(safe-area-inset-bottom)
    );
    overscroll-behavior: contain;
    box-shadow: 0 12px 36px rgb(0 0 0 / 20%);
  }
  .mobile-dialog[open] {
    display: flex;
    flex-direction: column;
  }
  .mobile-dialog::backdrop {
    background: rgb(20 24 32 / 24%);
  }
  .mobile-dialog header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-bottom: 1px solid var(--surface-border);
    flex-shrink: 0;
  }
  .mobile-dialog header strong {
    font-size: 14px;
  }
  .mobile-dialog header span {
    display: block;
    font-size: 11px;
    opacity: 0.65;
    margin-top: 3px;
  }
  .page-list {
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 8px;
  }
  summary {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 46px;
    padding: 0 10px;
    border-radius: 8px;
    cursor: pointer;
    list-style: none;
    font-size: 13px;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  summary span,
  .component-row span {
    min-width: 0;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  summary :global(svg:first-child) {
    transform: rotate(-90deg);
    transition: transform 140ms;
  }
  details[open] > summary :global(svg:first-child) {
    transform: none;
  }
  .current-page {
    color: var(--surface-accent);
    background: var(--surface-subtle);
  }
  small {
    font-size: 10px;
    opacity: 0.6;
    flex-shrink: 0;
  }
  .component-list {
    margin: 2px 0 6px 18px;
    padding-left: 8px;
    border-left: 1px solid var(--surface-border);
  }
  .component-row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 46px;
    padding: 8px;
    border-radius: 8px;
    text-align: left;
    font-size: 13px;
  }
  .component-row :global(svg) {
    flex-shrink: 0;
    color: var(--surface-accent);
  }
  .component-row[aria-current="true"] {
    background: var(--surface-selection);
  }
  .empty-page {
    min-height: 44px;
    padding: 8px;
    opacity: 0.65;
    font-size: 12px;
  }
  .mobile-dialog footer {
    flex-shrink: 0;
    padding: 4px 12px;
    border-top: 1px solid var(--surface-border);
  }
  footer button {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    font-size: 12px;
  }
  :global(main.mobile-list-mode .desktop-toolbar),
  :global(main.mobile-list-mode .desktop-page-pager) {
    display: none;
  }
  :global(main.mobile-list-mode [data-canvas-root]) {
    touch-action: auto;
  }
  :global(main.mobile-overview [data-canvas-root]) {
    touch-action: none;
  }
  /* Overview is a camera surface, not an editor. This also covers iframe content. */
  :global(main.mobile-overview .canvas-world-item *) {
    pointer-events: none !important;
  }
  :global(main.mobile-list-mode .canvas-fullscreen) {
    left: max(8px, env(safe-area-inset-left)) !important;
    right: max(8px, env(safe-area-inset-right)) !important;
    top: calc(env(safe-area-inset-top) + 62px) !important;
    bottom: max(8px, env(safe-area-inset-bottom)) !important;
  }
  /* A phone detail page, not a translucent canvas window. Instances stay mounted. */
  :global(main.mobile-detail)::before {
    content: "";
    position: fixed;
    inset: 0;
    background: var(--app-bg);
    z-index: 34;
  }
  :global(main.mobile-detail .canvas-fullscreen) {
    left: var(--mobile-viewport-left, 0px) !important;
    right: auto !important;
    width: var(--mobile-viewport-width, 100%) !important;
    top: calc(
      var(--mobile-viewport-top, 0px) + env(safe-area-inset-top) + 52px
    ) !important;
    bottom: auto !important;
    height: max(
      0px,
      calc(
        var(--mobile-viewport-height, 100dvh) - env(safe-area-inset-top) - 52px
      )
    ) !important;
    padding-bottom: env(safe-area-inset-bottom);
    background: var(--surface-bg);
  }
  :global(main.mobile-detail .canvas-fullscreen > :first-child) {
    border: 0 !important;
    border-radius: 0 !important;
    box-shadow: none !important;
    opacity: 1 !important;
  }
  :global(main.mobile-detail .custom-window-border) {
    display: none;
  }
  /* The detail page already has its own title and Back navigation. */
  :global(main.mobile-detail .canvas-fullscreen [data-canvas-titlebar]) {
    display: none !important;
  }
  @media (prefers-reduced-motion: reduce) {
    summary :global(svg:first-child) {
      transition: none;
    }
  }
</style>
