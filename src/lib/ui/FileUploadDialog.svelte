<script lang="ts" context="module">
  export type UploadItem = { file: File; relativePath: string };
</script>

<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import {
    FileIcon,
    FolderIcon,
    PlusIcon,
    Trash2Icon,
  } from "svelte-feather-icons";

  import FileWindowDialog from "./FileWindowDialog.svelte";

  export let open = false;
  export let destination = "";
  export let busy = false;

  const dispatch = createEventDispatcher<{
    close: void;
    upload: UploadItem[];
  }>();
  let fileInput: HTMLInputElement;
  let folderInput: HTMLInputElement;
  let items: UploadItem[] = [];

  $: if (!open) items = [];

  function addFiles(files: FileList | null, fromDirectory: boolean) {
    if (!files) return;
    const next = new Map(items.map((item) => [item.relativePath, item]));
    for (const file of Array.from(files)) {
      const relativePath = fromDirectory
        ? file.webkitRelativePath || file.name
        : file.name;
      next.set(relativePath, { file, relativePath });
    }
    items = [...next.values()].sort((left, right) =>
      left.relativePath.localeCompare(right.relativePath),
    );
  }
</script>

<FileWindowDialog
  {open}
  title="Upload files and folders"
  description="Build one upload containing files, folders, or both."
  {busy}
  maxWidth={720}
  on:close={() => !busy && dispatch("close")}
>
  <div class="space-y-4">
    <div class="rounded-lg border border-zinc-800 bg-zinc-950/55 px-3 py-2">
      <p class="text-[11px] uppercase tracking-wide text-zinc-500">
        Destination
      </p>
      <p
        class="mt-1 truncate font-mono text-sm text-zinc-200"
        title={destination}
      >
        {destination}
      </p>
    </div>

    <input
      bind:this={fileInput}
      class="hidden"
      type="file"
      multiple
      on:change={(event) => {
        addFiles(event.currentTarget.files, false);
        event.currentTarget.value = "";
      }}
    />
    <input
      bind:this={folderInput}
      class="hidden"
      type="file"
      multiple
      webkitdirectory
      on:change={(event) => {
        addFiles(event.currentTarget.files, true);
        event.currentTarget.value = "";
      }}
    />

    <div class="grid gap-2 sm:grid-cols-2">
      <button
        class="picker-button"
        disabled={busy}
        on:click={() => fileInput.click()}
      >
        <FileIcon />
        <span
          ><strong>Add files</strong><small>Choose one or more files</small
          ></span
        >
        <PlusIcon />
      </button>
      <button
        class="picker-button"
        disabled={busy}
        on:click={() => folderInput.click()}
      >
        <FolderIcon />
        <span
          ><strong>Add folder</strong><small>Includes its nested files</small
          ></span
        >
        <PlusIcon />
      </button>
    </div>

    <div
      class="max-h-64 overflow-y-auto rounded-lg border border-zinc-800 bg-zinc-950/40"
    >
      {#if items.length}
        {#each items as item (item.relativePath)}
          <div
            class="flex items-center gap-2 border-b border-zinc-800/70 px-3 py-2 last:border-b-0"
          >
            <FileIcon class="h-4 w-4 shrink-0 text-zinc-500" />
            <span
              class="min-w-0 flex-1 truncate text-sm text-zinc-300"
              title={item.relativePath}
            >
              {item.relativePath}
            </span>
            <span class="text-xs text-zinc-600"
              >{Math.ceil(item.file.size / 1024)} KiB</span
            >
            <button
              class="rounded p-1 text-zinc-500 hover:bg-zinc-800 hover:text-red-300"
              aria-label="Remove {item.relativePath}"
              disabled={busy}
              on:click={() =>
                (items = items.filter((candidate) => candidate !== item))}
              ><Trash2Icon class="h-3.5 w-3.5" /></button
            >
          </div>
        {/each}
      {:else}
        <p class="px-4 py-8 text-center text-sm text-zinc-600">
          Nothing selected yet.
        </p>
      {/if}
    </div>

    <div class="flex justify-end gap-2">
      <button
        class="secondary-button"
        disabled={busy}
        on:click={() => dispatch("close")}>Cancel</button
      >
      <button
        class="primary-button"
        disabled={busy || items.length === 0}
        on:click={() => dispatch("upload", items)}
        >{busy ? "Uploading…" : `Upload ${items.length || ""}`}</button
      >
    </div>
  </div>
</FileWindowDialog>

<style lang="postcss">
  @reference "../../app.css";
  .picker-button {
    @apply flex items-center gap-3 rounded-lg border border-zinc-800 bg-zinc-900/70 px-3 py-3 text-left text-zinc-300 hover:border-indigo-500/45 hover:bg-zinc-800 disabled:opacity-40;
  }
  .picker-button :global(svg) {
    @apply h-4 w-4 shrink-0;
  }
  .picker-button span {
    @apply min-w-0 flex-1;
  }
  .picker-button strong,
  .picker-button small {
    @apply block;
  }
  .picker-button small {
    @apply mt-0.5 text-xs font-normal text-zinc-500;
  }
  .primary-button {
    @apply rounded-md bg-indigo-600 px-4 py-2 text-sm text-white hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-40;
  }
  .secondary-button {
    @apply rounded-md border border-zinc-700 px-4 py-2 text-sm text-zinc-300 hover:bg-zinc-800 disabled:opacity-40;
  }
</style>
