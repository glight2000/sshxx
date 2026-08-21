<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import {
    Edit2Icon,
    FilePlusIcon,
    FolderIcon,
    FolderPlusIcon,
    MoveIcon,
    TerminalIcon,
    Trash2Icon,
    UploadCloudIcon,
  } from "svelte-feather-icons";

  import type { FileTreeEntry } from "$lib/protocol";

  export let entry: FileTreeEntry;
  export let x: number;
  export let y: number;
  export let hasWriteAccess: boolean | undefined;
  export let mutationBusy: boolean;
  export let canMutate: boolean;

  type MenuEvents = {
    close: void;
    openDirectory: FileTreeEntry;
    openFile: FileTreeEntry;
    openTerminal: FileTreeEntry;
    upload: FileTreeEntry;
    create: { kind: "file" | "directory"; directory: string };
    rename: FileTreeEntry;
    move: FileTreeEntry;
    delete: FileTreeEntry;
  };
  const dispatch = createEventDispatcher<MenuEvents>();

  let menuElement: HTMLDivElement;

  function closeOnOutsideClick(event: MouseEvent) {
    if (event.target instanceof Node && !menuElement?.contains(event.target))
      dispatch("close");
  }

  function closeOnEscape(event: KeyboardEvent) {
    if (event.key === "Escape") dispatch("close");
  }

  function run<K extends Exclude<keyof MenuEvents, "close">>(
    event: K,
    detail: MenuEvents[K],
  ) {
    dispatch(event, detail);
    dispatch("close");
  }
</script>

<svelte:window
  on:mousedown|capture={closeOnOutsideClick}
  on:keydown={closeOnEscape}
/>

<div
  bind:this={menuElement}
  class="context-menu"
  role="menu"
  tabindex="-1"
  aria-label={`Actions for ${entry.name}`}
  style:left={`${x}px`}
  style:top={`${y}px`}
  on:mousedown|stopPropagation
  on:contextmenu|preventDefault|stopPropagation
>
  <div class="context-title" title={entry.path}>{entry.name}</div>
  {#if entry.kind === "directory"}
    <button
      class="context-action"
      role="menuitem"
      on:click={() => run("openDirectory", entry)}
      ><FolderIcon />Open folder</button
    >
    <button
      class="context-action"
      role="menuitem"
      on:click={() => run("openTerminal", entry)}
      ><TerminalIcon />Open terminal here</button
    >
    <div class="context-divider"></div>
    <button
      class="context-action"
      role="menuitem"
      disabled={!hasWriteAccess || mutationBusy}
      on:click={() => run("upload", entry)}
      ><UploadCloudIcon />Upload here</button
    >
    <button
      class="context-action"
      role="menuitem"
      disabled={!hasWriteAccess || mutationBusy}
      on:click={() =>
        run("create", { kind: "directory", directory: entry.path })}
      ><FolderPlusIcon />New folder</button
    >
    <button
      class="context-action"
      role="menuitem"
      disabled={!hasWriteAccess || mutationBusy}
      on:click={() => run("create", { kind: "file", directory: entry.path })}
      ><FilePlusIcon />New file</button
    >
  {:else}
    <button
      class="context-action"
      role="menuitem"
      on:click={() => run("openFile", entry)}><Edit2Icon />Open / edit</button
    >
    <button
      class="context-action"
      role="menuitem"
      on:click={() => run("openTerminal", entry)}
      ><TerminalIcon />Open terminal here</button
    >
  {/if}
  <div class="context-divider"></div>
  <button
    class="context-action"
    role="menuitem"
    disabled={!hasWriteAccess || mutationBusy || !canMutate}
    on:click={() => run("rename", entry)}><Edit2Icon />Rename</button
  >
  <button
    class="context-action"
    role="menuitem"
    disabled={!hasWriteAccess || mutationBusy || !canMutate}
    on:click={() => run("move", entry)}><MoveIcon />Move</button
  >
  <button
    class="context-action danger"
    role="menuitem"
    disabled={!hasWriteAccess || mutationBusy || !canMutate}
    on:click={() => run("delete", entry)}><Trash2Icon />Delete</button
  >
</div>

<style lang="postcss">
  @reference "../../app.css";
  .context-menu {
    @apply absolute z-50 w-[210px] rounded-lg border border-zinc-700 bg-zinc-900/98 p-1.5 text-left shadow-xl shadow-black/60 backdrop-blur-md;
  }
  .context-title {
    @apply truncate px-2 py-1.5 text-[11px] font-medium text-zinc-500;
  }
  .context-action {
    @apply flex h-8 w-full items-center gap-2 rounded-md px-2 text-xs text-zinc-300 outline-none hover:bg-zinc-700 hover:text-white focus-visible:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-35;
  }
  .context-action :global(svg) {
    @apply h-3.5 w-3.5 shrink-0;
  }
  .context-action.danger:not(:disabled) {
    @apply text-red-300;
  }
  .context-action.danger:hover:not(:disabled) {
    @apply bg-red-950/70;
  }
  .context-divider {
    @apply my-1 border-t border-zinc-700/80;
  }
</style>
