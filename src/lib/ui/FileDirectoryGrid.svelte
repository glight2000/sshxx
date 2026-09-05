<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { FileIcon, FolderIcon } from "svelte-feather-icons";

  import { samePath } from "$lib/filesystem";
  import type { FileTreeEntry } from "$lib/protocol";

  export let directory: FileTreeEntry;
  export let entries: FileTreeEntry[];
  export let selectedPath: string;
  export let selectedKind: "" | FileTreeEntry["kind"];
  export let loading: boolean;

  const dispatch = createEventDispatcher<{
    clearSelection: void;
    select: FileTreeEntry;
    open: FileTreeEntry;
    context: {
      entry: FileTreeEntry;
      source: "grid" | "background";
      event: MouseEvent;
    };
  }>();
</script>

<div
  class="directory-list"
  aria-label={`Contents of ${directory.path}`}
  role="presentation"
  on:mousedown={(event) => {
    if (event.target === event.currentTarget && event.button === 0)
      dispatch("clearSelection");
  }}
  on:contextmenu={(event) => {
    if (event.target === event.currentTarget)
      dispatch("context", { entry: directory, source: "background", event });
  }}
>
  {#if entries.length}
    {#each entries as entry (entry.path)}
      <button
        type="button"
        class="directory-entry"
        class:selected={selectedKind === entry.kind &&
          samePath(selectedPath, entry.path)}
        title={entry.path}
        on:click={(event) => {
          // A double click emits two click events before dblclick. The second
          // selection would otherwise race the directory-open mutation.
          if (event.detail <= 1) dispatch("select", entry);
        }}
        on:dblclick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          dispatch("open", entry);
        }}
        on:contextmenu={(event) =>
          dispatch("context", { entry, source: "grid", event })}
      >
        {#if entry.kind === "directory"}
          <FolderIcon class="text-[var(--surface-warning)]" />
        {:else}
          <FileIcon class="text-zinc-400" />
        {/if}
        <span class="directory-entry-name">{entry.name}</span>
      </button>
    {/each}
  {:else if !loading}
    <div class="empty-directory">Empty</div>
  {/if}
</div>

<style lang="postcss">
  @reference "../../app.css";
  .directory-list {
    @apply relative grid min-h-full auto-rows-min grid-cols-[repeat(auto-fill,minmax(96px,112px))] content-start gap-2 p-3;
  }
  .directory-entry {
    @apply flex h-24 w-full flex-col items-center justify-center gap-2 rounded-lg border border-transparent px-2 py-2 text-sm text-zinc-300 outline-none hover:border-zinc-700/70 hover:bg-zinc-800/80 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-indigo-500/70;
  }
  .directory-entry.selected {
    @apply ring-1 ring-inset ring-indigo-400/45;
    color: var(--surface-accent);
    background: var(--surface-selection);
  }
  .directory-entry :global(svg) {
    @apply h-9 w-9 shrink-0;
    stroke-width: 1.5;
  }
  .directory-entry-name {
    @apply max-h-8 w-full overflow-hidden break-all text-center text-xs leading-4;
  }
  .empty-directory {
    @apply pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-zinc-600;
  }
</style>
