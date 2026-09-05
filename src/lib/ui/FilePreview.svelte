<script lang="ts">
  import type { FilePreviewKind } from "$lib/filesystem";
  import type { FileTreeEntry } from "$lib/protocol";
  import type {
    TextInsertPosition,
    TextInsertResult,
  } from "./CodeEditor.svelte";

  export let selected: FileTreeEntry | null;
  export let encoding: "utf8" | "utf16le" | "utf16be" | "base64" | null;
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
  let editorLoadAttempt = 0;
  let mediaPath = "";
  let mediaError = "";

  $: if ((selected?.path ?? "") !== mediaPath) {
    mediaPath = selected?.path ?? "";
    mediaError = "";
  }

  function loadEditor() {
    return (editorModulePromise ??= import("./CodeEditor.svelte"));
  }

  function retryEditor() {
    editorModulePromise = null;
    editorLoadAttempt += 1;
  }
</script>

{#if selected?.kind === "directory"}
  <slot name="directory"></slot>
{:else if selected && encoding && encoding !== "base64"}
  {#key `${selected.path}:${editorLoadAttempt}`}
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
        class="flex h-full flex-col items-center justify-center gap-3 p-8 text-center text-sm text-red-300"
        role="alert"
      >
        <span
          >Could not load the text editor: {error instanceof Error
            ? error.message
            : String(error)}</span
        >
        <div class="flex gap-2">
          <button type="button" class="recovery-button" on:click={retryEditor}
            >Retry</button
          >
          <button
            type="button"
            class="recovery-button"
            on:click={() => window.location.reload()}>Reload application</button
          >
        </div>
      </div>
    {/await}
  {/key}
{:else if previewUrl && previewKind === "image"}
  {#if mediaError}
    <div class="media-error">{mediaError}</div>
  {:else}
    <div class="flex min-h-full items-center justify-center p-6">
      <img
        class="max-h-full max-w-full object-contain"
        src={previewUrl}
        alt={selected?.name}
        on:error={() =>
          (mediaError = "This browser could not decode the selected image.")}
      />
    </div>
  {/if}
{:else if previewUrl && previewKind === "audio"}
  {#if mediaError}
    <div class="media-error">{mediaError}</div>
  {:else}
    <div class="flex min-h-full items-center justify-center p-6">
      <audio
        controls
        src={previewUrl}
        on:error={() =>
          (mediaError = "This browser could not decode the selected audio.")}
      ></audio>
    </div>
  {/if}
{:else if previewUrl && previewKind === "video"}
  <!-- svelte-ignore a11y_media_has_caption -->
  {#if mediaError}
    <div class="media-error">{mediaError}</div>
  {:else}
    <div class="flex min-h-full items-center justify-center p-6">
      <video
        class="max-h-full max-w-full"
        controls
        src={previewUrl}
        on:error={() =>
          (mediaError = "This browser could not decode the selected video.")}
      ></video>
    </div>
  {/if}
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
    class="pointer-events-none absolute inset-0 z-20 flex items-center justify-center border-2 border-dashed border-[var(--surface-warning)] bg-[var(--surface-warning-bg)]/90 p-8 text-center text-sm font-medium text-[var(--surface-warning)] backdrop-blur-[1px]"
  >
    Open an editable text file before dropping this paragraph.
  </div>{/if}

<style lang="postcss">
  @reference "../../app.css";
  .recovery-button {
    @apply rounded-md border border-red-300/35 bg-red-950/40 px-3 py-1.5 text-xs text-red-100 hover:bg-red-900/55;
  }
  .media-error {
    @apply flex h-full items-center justify-center p-8 text-center text-sm;
    color: var(--surface-warning);
  }
</style>
