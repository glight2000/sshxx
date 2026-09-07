<script lang="ts">
  import { createEventDispatcher, onDestroy, tick } from "svelte";
  import {
    Edit3Icon,
    FileTextIcon,
    PlusIcon,
    TerminalIcon,
    XIcon,
  } from "svelte-feather-icons";
  import type { CanvasRelationItem } from "./CanvasRelations.svelte";
  import { mobileRelationPress } from "$lib/action/mobileRelationPress";

  export let items: CanvasRelationItem[];
  export let allowAdd: boolean;
  export let selecting: boolean;
  export let disabled: boolean;
  const dispatch = createEventDispatcher<{
    toggleAdd: void;
    navigate: CanvasRelationItem;
    remove: CanvasRelationItem;
  }>();
  const icons = { terminal: TerminalIcon, note: FileTextIcon, file: Edit3Icon };
  let selected: CanvasRelationItem | null = null;
  let dialog: HTMLDialogElement;
  let backdropPressed = false;
  $: if (
    selected &&
    !items.some(
      (item) => item.id === selected?.id && item.kind === selected?.kind,
    )
  )
    close();
  function close() {
    dialog?.close();
    selected = null;
  }
  async function show(item: CanvasRelationItem) {
    selected = item;
    await tick();
    if (selected === item && dialog?.isConnected && !dialog.open)
      dialog.showModal();
  }
  onDestroy(() => dialog?.close());
</script>

<div class="mobile-relations" aria-label="Associated canvas items">
  {#if allowAdd}
    <button
      class:active={selecting}
      {disabled}
      data-link-toggle
      aria-label="Add association"
      on:click={() => dispatch("toggleAdd")}><PlusIcon size="19" /></button
    >
  {/if}
  {#each [...items].reverse() as item (`${item.kind}:${item.id}`)}
    <button
      aria-label={`Open ${item.label}; hold for actions`}
      aria-haspopup="dialog"
      use:mobileRelationPress={{
        tap: () => dispatch("navigate", item),
        hold: () => show(item),
      }}
    >
      <svelte:component this={icons[item.kind]} size="19" />
    </button>
  {/each}
</div>
<dialog
  bind:this={dialog}
  class="relation-actions panel"
  aria-label="Association actions"
  on:cancel={close}
  on:close={() => (selected = null)}
  on:click={(event) => {
    if (backdropPressed && event.target === dialog) close();
    backdropPressed = false;
  }}
  on:pointerdown|stopPropagation={(event) =>
    (backdropPressed = event.target === dialog)}
  on:wheel|stopPropagation
>
  {#if selected}
    <header>
      <strong>{selected.label}</strong><button
        aria-label="Close association actions"
        on:click={close}><XIcon size="20" /></button
      >
    </header>
    <button
      class="action"
      on:click={() => {
        const item = selected!;
        close();
        dispatch("navigate", item);
      }}>Open component</button
    >
    <button
      class="action danger"
      {disabled}
      on:click={() => {
        const item = selected!;
        close();
        dispatch("remove", item);
      }}>Remove association</button
    >
  {/if}
</dialog>

<style>
  .mobile-relations {
    display: flex;
    flex-direction: row-reverse;
    gap: 4px;
    overflow-x: auto;
    scrollbar-width: none;
    overscroll-behavior: contain;
  }
  button {
    min-width: 44px;
    min-height: 44px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    touch-action: manipulation;
    -webkit-touch-callout: none;
    user-select: none;
  }
  .mobile-relations button {
    flex-shrink: 0;
    background: var(--surface-subtle);
    color: var(--surface-accent);
  }
  button:focus-visible {
    outline: 2px solid var(--surface-accent);
    outline-offset: -2px;
  }
  button:disabled {
    opacity: 0.4;
  }
  button.active {
    background: var(--surface-selection);
  }
  .relation-actions {
    position: fixed;
    inset: auto 8px max(8px, env(safe-area-inset-bottom));
    margin: 0;
    width: auto;
    max-width: none;
    max-height: 70dvh;
    padding: 12px;
    overflow: auto;
    color: var(--app-text);
  }
  .relation-actions::backdrop {
    background: rgb(0 0 0 / 25%);
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  strong {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    font-size: 14px;
  }
  .action {
    display: flex;
    width: 100%;
    justify-content: flex-start;
    padding: 0 12px;
    font-size: 14px;
  }
  .danger {
    color: var(--surface-danger);
  }
</style>
