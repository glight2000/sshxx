<script lang="ts">
  import { createEventDispatcher, onDestroy } from "svelte";
  import {
    PaperclipIcon,
    DownloadIcon,
    XIcon,
    EyeIcon,
  } from "svelte-feather-icons";
  import type { WorkspaceAttachment } from "$lib/protocol";
  import type { WorkspaceMedia } from "$lib/workspaceMedia";
  import { safeDownloadName } from "$lib/fileDownload";

  export let media: WorkspaceMedia | null;
  export let attachments: WorkspaceAttachment[] = [];
  export let editable = false;
  export let limit = 8;
  const dispatch = createEventDispatcher<{ change: WorkspaceAttachment[] }>();
  let input: HTMLInputElement;
  export let busy = false;
  let progress = 0;
  let error = "";
  let controller: AbortController | null = null;
  let preview: { id: string; url: string; release: () => void } | null = null;

  function closePreview() {
    preview?.release();
    preview = null;
  }
  $: if (preview && !attachments.some((item) => item.id === preview?.id))
    closePreview();
  export async function addFiles(files: File[]) {
    if (!editable || !media || busy) return;
    if (files.length + attachments.length > limit) {
      error = `Up to ${limit} attachments are allowed.`;
      return;
    }
    busy = true;
    error = "";
    controller = new AbortController();
    let next = attachments;
    try {
      for (const file of files) {
        progress = 0;
        const item = await media.upload(
          file,
          controller.signal,
          (value) => (progress = value),
        );
        if (attachments.length >= limit)
          throw new Error(
            `The attachment limit of ${limit} was reached while uploading.`,
          );
        next = [...attachments, item];
        attachments = next;
        dispatch("change", next);
      }
    } catch (cause) {
      if (!controller.signal.aborted)
        error = String(cause instanceof Error ? cause.message : cause);
    } finally {
      busy = false;
    }
  }
  async function open(item: WorkspaceAttachment, download: boolean) {
    if (!media || busy) return;
    busy = true;
    error = "";
    progress = 0;
    closePreview();
    controller = new AbortController();
    try {
      const result = await media.open(item, controller.signal);
      preview = { id: item.id, ...result };
      if (download) {
        const anchor = document.createElement("a");
        anchor.href = result.url;
        anchor.download = safeDownloadName(item.name);
        anchor.hidden = true;
        document.body.append(anchor);
        anchor.click();
        anchor.remove();
        // Keep the bounded URL alive until close/destroy, including slower browsers.
      }
    } catch (cause) {
      if (!controller.signal.aborted)
        error = String(cause instanceof Error ? cause.message : cause);
    } finally {
      busy = false;
    }
  }
  onDestroy(() => {
    controller?.abort();
    closePreview();
  });
</script>

<!-- This container only isolates child controls from canvas gestures. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="attachments"
  role="group"
  aria-label="Attachments"
  on:mousedown|stopPropagation
  on:pointerdown|stopPropagation
  on:keydown|stopPropagation
>
  {#each attachments as item (item.id)}
    <div class="attachment">
      <div class="attachment-row">
        <PaperclipIcon size="14" />
        <span class="filename" title={item.name}
          >{item.name}<small>{(item.size / 1024).toFixed(1)} KiB</small></span
        >
        {#if item.mediaType.startsWith("image/") || item.mediaType.startsWith("video/")}
          <button
            type="button"
            aria-label={`Preview ${item.name}`}
            disabled={!media || busy}
            on:click={() => open(item, false)}><EyeIcon size="15" /></button
          >
        {/if}
        <button
          type="button"
          aria-label={`Download ${item.name}`}
          disabled={!media || busy}
          on:click={() => open(item, true)}><DownloadIcon size="15" /></button
        >
        {#if editable}<button
            type="button"
            aria-label={`Remove ${item.name}`}
            disabled={busy}
            on:click={() =>
              dispatch(
                "change",
                attachments.filter((a) => a.id !== item.id),
              )}><XIcon size="15" /></button
          >{/if}
      </div>
      {#if preview?.id === item.id}
        <div class="preview">
          <button
            type="button"
            aria-label="Close attachment preview"
            on:click={closePreview}><XIcon size="14" /></button
          >
          {#if item.mediaType.startsWith("image/")}
            <img
              src={preview.url}
              alt={item.name}
              on:error={() =>
                (error =
                  "This browser cannot decode this image. You can still download it.")}
            />
          {:else if item.mediaType.startsWith("video/")}
            <!-- svelte-ignore a11y_media_has_caption -->
            <video
              controls
              preload="metadata"
              src={preview.url}
              on:error={() =>
                (error =
                  "This browser cannot play this video. You can still download it.")}
            ></video>
          {:else}<small>Download ready</small>{/if}
        </div>
      {/if}
    </div>
  {/each}
  {#if editable}
    <input
      hidden
      multiple
      type="file"
      bind:this={input}
      on:change={() => {
        void addFiles(Array.from(input.files ?? []));
        input.value = "";
      }}
    />
    <button
      class="attach"
      type="button"
      disabled={!media || busy || attachments.length >= limit}
      on:click={() => input.click()}
      ><PaperclipIcon size="15" /> Attach files</button
    >
  {/if}
  {#if busy}<small role="status"
      >{progress ? `Uploading ${progress}%` : "Loading attachment…"}<button
        type="button"
        on:click={() => controller?.abort()}>Cancel</button
      ></small
    >{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</div>

<style>
  .attachments {
    display: grid;
    gap: 0.4rem;
    font-size: 0.8rem;
    min-width: 0;
  }
  .attachment {
    border: 1px solid var(--surface-border);
    border-radius: 0.6rem;
    background: var(--surface-subtle);
    overflow: hidden;
  }
  .attachment-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.45rem;
  }
  .filename {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  small {
    display: block;
    color: var(--surface-muted);
    font-size: 0.7rem;
  }
  button {
    display: inline-flex;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    padding: 0.3rem;
    border-radius: 0.35rem;
  }
  button:hover {
    background: var(--surface-hover);
  }
  button:disabled {
    opacity: 0.45;
  }
  .attach {
    justify-self: start;
    color: var(--surface-accent);
  }
  .preview {
    position: relative;
    padding: 0.4rem;
  }
  .preview > button {
    position: absolute;
    right: 0.5rem;
    top: 0.5rem;
    z-index: 1;
    background: var(--surface-bg);
  }
  img,
  video {
    display: block;
    max-width: 100%;
    max-height: 20rem;
    margin: auto;
    object-fit: contain;
  }
  .error {
    color: var(--surface-danger);
    overflow-wrap: anywhere;
  }
</style>
