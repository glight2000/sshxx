<script lang="ts">
  import type { FilePreviewKind } from "$lib/filesystem";
  import type { FileTreeEntry } from "$lib/protocol";
  import type {
    TextInsertPosition,
    TextInsertResult,
  } from "./CodeEditor.svelte";

  export let selected: FileTreeEntry | null;
  export let encoding: "utf8" | "base64" | null;
  export let content: string;
  export let previewUrl: string;
  export let previewKind: FilePreviewKind;
  export let readOnly: boolean;
  export let loading: boolean;
  export let paragraphDropBlocked: boolean;
  export let onChange: (value: string) => void;
  export let insertText: (
    text: string,
    position?: TextInsertPosition,
  ) => TextInsertResult;
  export let previewTextDrop: (position: TextInsertPosition) => boolean;
  export let cancelTextDropPreview: () => void;

  let editorModulePromise: Promise<
    typeof import("./CodeEditor.svelte")
  > | null = null;

  function loadEditor() {
    return (editorModulePromise ??= import("./CodeEditor.svelte"));
  }
</script>

{#if selected?.kind === "directory"}
  <slot name="directory"></slot>
{:else if selected && encoding === "utf8"}
  {#key selected.path}
    {#await loadEditor()}
      <div
        class="flex h-full items-center justify-center text-sm text-zinc-500"
      >
        Loading editor…
      </div>
    {:then editorModule}
      <svelte:component
        this={editorModule.default}
        value={content}
        filename={selected.name}
        {readOnly}
        {onChange}
        bind:insertText
        bind:previewTextDrop
        bind:cancelTextDropPreview
      />
    {:catch error}
      <div
        class="flex h-full items-center justify-center p-8 text-center text-sm text-red-300"
        role="alert"
      >
        Could not load the text editor: {error instanceof Error
          ? error.message
          : String(error)}
      </div>
    {/await}
  {/key}
{:else if previewUrl && previewKind === "image"}
  <div class="flex min-h-full items-center justify-center p-6">
    <img
      class="max-h-full max-w-full object-contain"
      src={previewUrl}
      alt={selected?.name}
    />
  </div>
{:else if previewUrl && previewKind === "audio"}
  <div class="flex min-h-full items-center justify-center p-6">
    <audio controls src={previewUrl}></audio>
  </div>
{:else if previewUrl && previewKind === "video"}
  <!-- svelte-ignore a11y_media_has_caption -->
  <div class="flex min-h-full items-center justify-center p-6">
    <video class="max-h-full max-w-full" controls src={previewUrl}></video>
  </div>
{:else if previewUrl && previewKind === "pdf"}
  <iframe class="h-full w-full border-0" src={previewUrl} title={selected?.name}
  ></iframe>
{:else if selected && encoding === "base64"}
  <div
    class="flex h-full items-center justify-center p-8 text-center text-sm text-zinc-500"
  >
    Binary preview is not available for this file type.
  </div>
{:else}
  <div
    class="flex h-full items-center justify-center p-8 text-center text-sm text-zinc-600"
  >
    Select a folder on the left, then double-click an item to open it.
  </div>
{/if}

{#if loading}<div
    class="absolute inset-0 flex items-center justify-center bg-zinc-950/65 text-sm text-zinc-300 backdrop-blur-[1px]"
  >
    Loading…
  </div>{/if}
{#if paragraphDropBlocked}<div
    class="pointer-events-none absolute inset-0 z-20 flex items-center justify-center border-2 border-dashed border-amber-300/65 bg-amber-950/35 p-8 text-center text-sm font-medium text-amber-100 backdrop-blur-[1px]"
  >
    Open an editable text file before dropping this paragraph.
  </div>{/if}
