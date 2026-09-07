<script lang="ts">
  import { createEventDispatcher, onMount, tick } from "svelte";
  import {
    Maximize2Icon,
    ArrowLeftIcon,
    XIcon,
    TerminalIcon,
    FileTextIcon,
    FolderIcon,
    CodeIcon,
  } from "svelte-feather-icons";
  import { MOBILE_NAVIGATION_QUERY } from "$lib/mobileNavigation";
  import type { CanvasSearchItem } from "./TerminalSearch.svelte";
  import MobileTerminalKeypad from "./MobileTerminalKeypad.svelte";

  export let current: CanvasSearchItem | undefined;
  export let fullscreen: boolean;
  export let writable: boolean;
  export let sendKey: (key: string) => void;
  export let associationTargets: CanvasSearchItem[] | null = null;
  const dispatch = createEventDispatcher<{
    available: boolean;
    toggle: void;
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
  let dialog: HTMLDialogElement;
  $: dispatch("available", available);
  $: syncDialog(available && associationTargets !== null);
  async function syncDialog(show: boolean) {
    await tick();
    if (
      show !== (available && associationTargets !== null) ||
      !dialog?.isConnected
    )
      return;
    if (show && !dialog.open) dialog.showModal();
    else if (!show && dialog.open) dialog.close();
  }
  onMount(() => {
    const media = window.matchMedia(MOBILE_NAVIGATION_QUERY);
    const update = () => (available = media.matches);
    update();
    media.addEventListener("change", update);
    return () => {
      media.removeEventListener("change", update);
      dialog?.close();
    };
  });
</script>

{#if available}
  {#if current}
    <div
      class="mobile-focus-controls panel"
      class:fullscreen
      data-mobile-navigation
    >
      {#if current.kind === "terminal" && !fullscreen}
        <MobileTerminalKeypad disabled={!writable} send={sendKey} />
      {/if}
      <button
        class="view-toggle"
        class:ui-icon-button={fullscreen}
        aria-label={fullscreen
          ? "Restore canvas view"
          : "Fullscreen focused component"}
        on:mousedown|preventDefault
        on:click={() => dispatch("toggle")}
      >
        <svelte:component
          this={fullscreen ? ArrowLeftIcon : Maximize2Icon}
          size="18"
        />
        <span>{fullscreen ? "Restore" : "Fullscreen"}</span>
      </button>
    </div>
  {/if}
  <dialog
    bind:this={dialog}
    class="association-picker"
    data-mobile-navigation
    aria-label="Choose an association"
    on:cancel={() => dispatch("cancelAssociation")}
    on:click={(event) => {
      if (event.target === dialog) dispatch("cancelAssociation");
    }}
    on:wheel|stopPropagation
    on:pointerdown|stopPropagation
  >
    <header>
      Choose an association<button
        aria-label="Close component list"
        on:click={() => dispatch("cancelAssociation")}
        ><XIcon size="20" /></button
      >
    </header>
    <div class="targets">
      {#each associationTargets ?? [] as item (`${item.kind}:${item.id}`)}
        <button on:click={() => dispatch("associate", item)}>
          <svelte:component this={icons[item.kind]} size="18" /><span
            >{item.title}</span
          ><small>{item.kind}</small>
        </button>
      {:else}<p>No available components on this page.</p>{/each}
    </div>
  </dialog>
{/if}

<style>
  .mobile-focus-controls {
    position: fixed;
    z-index: 48;
    right: 8px;
    bottom: calc(
      100dvh - var(--mobile-viewport-top, 0px) -
        var(--mobile-viewport-height, 100dvh) + 66px +
        env(safe-area-inset-bottom)
    );
    display: flex;
    align-items: flex-end;
    gap: 6px;
    padding: 4px;
    max-width: calc(100% - 16px);
  }
  .view-toggle {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    min-width: 58px;
    min-height: 40px;
    font-size: 10px;
    border-radius: 6px;
  }
  .mobile-focus-controls.fullscreen {
    top: calc(var(--mobile-viewport-top, 0px) + env(safe-area-inset-top) + 8px);
    left: calc(var(--mobile-viewport-left, 0px) + 8px);
    right: auto;
    bottom: auto;
  }
  .fullscreen .view-toggle {
    flex-direction: row;
    gap: 8px;
    width: auto;
    height: 44px;
    padding: 0 12px;
    font-size: 13px;
    font-weight: 500;
    color: var(--app-text);
  }
  button {
    touch-action: manipulation;
  }
  button:focus-visible {
    outline: 2px solid var(--surface-accent);
  }
  .association-picker {
    padding: 0;
    width: min(400px, calc(100vw - 24px));
    max-height: 70dvh;
    border-radius: 12px;
    background: var(--surface-bg);
    color: var(--app-text);
    border: 1px solid var(--surface-border);
  }
  .association-picker::backdrop {
    background: rgb(20 24 32 / 24%);
  }
  header,
  .targets button {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
  }
  header {
    justify-content: space-between;
    border-bottom: 1px solid var(--surface-border);
  }
  header button {
    padding: 8px;
  }
  .targets {
    overflow-y: auto;
    max-height: 55dvh;
    overscroll-behavior: contain;
  }
  .targets button {
    width: 100%;
    min-height: 44px;
    text-align: left;
  }
  .targets span {
    flex: 1;
    overflow-wrap: anywhere;
  }
  small,
  p {
    opacity: 0.65;
    font-size: 12px;
  }
  p {
    padding: 12px;
  }
  :global(main.mobile-mode [data-canvas-root]) {
    touch-action: pan-x pan-y;
  }
  /* A cross-origin frame cannot bubble touches to its parent. Enter fullscreen to interact with it. */
  :global(main.mobile-overview .canvas-world-item iframe),
  :global(main.mobile-mode .resize-handle) {
    pointer-events: none !important;
  }
  :global(main.mobile-mode .workspace-chrome) {
    position: fixed;
    z-index: 40;
    top: calc(var(--mobile-viewport-top, 0px) + env(safe-area-inset-top) + 6px);
  }
  :global(main.mobile-mode .workspace-toolbar) {
    max-width: calc(100vw - 16px);
    padding: 4px 6px;
  }
  :global(main.mobile-mode .workspace-toolbar > div) {
    flex-wrap: wrap;
    justify-content: center;
    row-gap: 4px;
  }
  :global(main.mobile-mode .page-pager) {
    z-index: 40;
    bottom: calc(
      100dvh - var(--mobile-viewport-top, 0px) -
        var(--mobile-viewport-height, 100dvh) + 12px +
        env(safe-area-inset-bottom)
    );
  }
  :global(main.mobile-detail .desktop-toolbar),
  :global(main.mobile-detail .desktop-page-pager) {
    display: none;
  }
  :global(main.mobile-detail)::before {
    content: "";
    position: fixed;
    inset: 0;
    background: var(--app-bg);
    z-index: 34;
  }
  :global(main.mobile-detail .canvas-fullscreen) {
    left: calc(var(--mobile-viewport-left, 0px) + 8px) !important;
    right: auto !important;
    width: calc(var(--mobile-viewport-width, 100%) - 16px) !important;
    top: calc(
      var(--mobile-viewport-top, 0px) + env(safe-area-inset-top) + 66px
    ) !important;
    bottom: auto !important;
    height: max(
      0px,
      calc(
        var(--mobile-viewport-height, 100dvh) - env(safe-area-inset-top) -
          env(safe-area-inset-bottom) - 74px
      )
    ) !important;
    background: var(--surface-bg);
    touch-action: pan-x pan-y;
  }
  :global(main.mobile-detail .canvas-fullscreen > :first-child) {
    opacity: 1 !important;
  }
  :global(main.mobile-detail .canvas-fullscreen [data-canvas-titlebar]) {
    display: none !important;
  }
</style>
