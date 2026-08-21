<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import {
    Edit2Icon,
    FilePlusIcon,
    FolderPlusIcon,
    SaveIcon,
    TerminalIcon,
    Trash2Icon,
    UploadCloudIcon,
  } from "svelte-feather-icons";

  import { samePath } from "$lib/filesystem";
  import type { FileTreeEntry } from "$lib/protocol";

  export let target: FileTreeEntry | null;
  export let currentPath: string;
  export let directoryCount: number;
  export let dirty: boolean;
  export let loading: boolean;
  export let mutationBusy: boolean;
  export let hasWriteAccess: boolean | undefined;
  export let canMutate: boolean;

  const dispatch = createEventDispatcher<{
    upload: FileTreeEntry;
    create: { kind: "file" | "directory"; directory: string };
    openTerminal: FileTreeEntry;
    delete: FileTreeEntry;
    openFile: FileTreeEntry;
    save: void;
  }>();
</script>

<div
  class="flex h-10 shrink-0 items-center gap-2 border-b border-zinc-800 px-3"
>
  <span class="min-w-0 flex-1 truncate text-xs text-zinc-400"
    >{target?.path ?? currentPath}</span
  >
  {#if target?.kind === "directory"}
    <span class="text-[11px] text-zinc-600"
      >{samePath(target.path, currentPath)
        ? `${directoryCount} items`
        : "Folder selected"}</span
    >
    <button
      class="content-action"
      disabled={!hasWriteAccess || mutationBusy}
      title="Upload files or folders here"
      aria-label="Upload files or folders here"
      on:click={() => dispatch("upload", target!)}><UploadCloudIcon /></button
    >
    <button
      class="content-action"
      disabled={!hasWriteAccess || mutationBusy}
      title="Create folder here"
      aria-label="Create folder here"
      on:click={() =>
        dispatch("create", { kind: "directory", directory: target!.path })}
      ><FolderPlusIcon /></button
    >
    <button
      class="content-action"
      disabled={!hasWriteAccess || mutationBusy}
      title="Create file here"
      aria-label="Create file here"
      on:click={() =>
        dispatch("create", { kind: "file", directory: target!.path })}
      ><FilePlusIcon /></button
    >
    <button
      class="content-action"
      title="Open terminal here"
      aria-label="Open terminal here"
      on:click={() => dispatch("openTerminal", target!)}
      ><TerminalIcon /></button
    >
    <button
      class="content-action danger"
      disabled={!hasWriteAccess || mutationBusy || !canMutate}
      title="Delete folder"
      aria-label="Delete folder"
      on:click={() => dispatch("delete", target!)}><Trash2Icon /></button
    >
  {:else if target}
    <button
      class="content-action"
      title="Open or edit file"
      aria-label="Open or edit file"
      on:click={() => dispatch("openFile", target!)}><Edit2Icon /></button
    >
    <button
      class="content-action"
      title="Open terminal in containing folder"
      aria-label="Open terminal in containing folder"
      on:click={() => dispatch("openTerminal", target!)}
      ><TerminalIcon /></button
    >
    <button
      class="content-action danger"
      disabled={!hasWriteAccess || mutationBusy}
      title="Delete file"
      aria-label="Delete file"
      on:click={() => dispatch("delete", target!)}><Trash2Icon /></button
    >
  {/if}
  {#if dirty}
    <span class="text-xs text-amber-300">Unsaved</span>
    <button
      class="save-button"
      disabled={!hasWriteAccess || loading}
      on:click={() => dispatch("save")}><SaveIcon />Save</button
    >
  {/if}
</div>

<style lang="postcss">
  @reference "../../app.css";
  .save-button {
    @apply inline-flex items-center gap-1.5 rounded-md bg-indigo-600 px-2.5 py-1 text-xs text-white hover:bg-indigo-500 disabled:opacity-40;
  }
  .save-button :global(svg) {
    @apply h-3.5 w-3.5;
  }
  .content-action {
    @apply inline-flex h-7 w-7 shrink-0 items-center justify-center rounded text-zinc-500 hover:bg-zinc-800 hover:text-zinc-200 disabled:cursor-not-allowed disabled:opacity-35;
  }
  .content-action :global(svg) {
    @apply h-3.5 w-3.5;
  }
  .content-action.danger:hover:not(:disabled) {
    @apply bg-red-950/70 text-red-300;
  }
</style>
