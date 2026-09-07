<script lang="ts">
  import { createEventDispatcher, tick, onDestroy } from "svelte";
  import { SendIcon, XIcon, MessageCircleIcon } from "svelte-feather-icons";
  import type { ChatRecord, WorkspaceAttachment } from "$lib/protocol";
  import type { WorkspaceMedia } from "$lib/workspaceMedia";
  import { containWheel, scrollWheel } from "$lib/action/containWheel";
  import MediaAttachments from "./MediaAttachments.svelte";
  export let messages: ChatRecord[];
  export let media: WorkspaceMedia | null;
  export let canUpload: boolean;
  export let connected: boolean;
  const dispatch = createEventDispatcher<{
    chat: { text: string; attachments: WorkspaceAttachment[] };
    close: void;
  }>();
  let scroller: HTMLElement;
  let input: HTMLTextAreaElement;
  let uploader: MediaAttachments;
  let uploading = false;
  let text = "";
  let attachments: WorkspaceAttachment[] = [];
  let follow = true;
  let alive = true;
  $: if (messages.length && scroller && follow)
    tick().then(() => {
      if (alive && follow) scroller.scrollTo({ top: scroller.scrollHeight });
    });
  onDestroy(() => {
    alive = false;
  });
  function submit() {
    if (!connected || uploading || (!text.trim() && !attachments.length))
      return;
    dispatch("chat", { text, attachments });
    text = "";
    attachments = [];
    follow = true;
    input.focus();
  }
  function paste(event: ClipboardEvent) {
    const files = Array.from(event.clipboardData?.files ?? []);
    if (files.length && canUpload) {
      event.preventDefault();
      void uploader.addFiles(files);
    }
  }
  function keydown(event: KeyboardEvent) {
    if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
      event.preventDefault();
      submit();
    }
  }
  function drop(event: DragEvent) {
    if (!event.dataTransfer?.files.length) return;
    event.preventDefault();
    event.stopPropagation();
    if (canUpload) void uploader.addFiles(Array.from(event.dataTransfer.files));
  }
</script>

<!-- The sidebar isolates its interactive children from canvas gestures. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside
  class="chat-sidebar panel"
  aria-label="Workspace chat"
  use:containWheel={(event) => scrollWheel(scroller, event)}
  on:mousedown|stopPropagation
  on:pointerdown|stopPropagation
  on:keydown|stopPropagation
  on:dragover={(event) => {
    if (event.dataTransfer?.types.includes("Files")) event.preventDefault();
  }}
  on:drop={drop}
>
  <header>
    <MessageCircleIcon size="19" />
    <div>
      <h2>Chat</h2>
      <small>Workspace history · latest 500 messages</small>
    </div>
    <button
      type="button"
      aria-label="Close chat"
      on:click={() => dispatch("close")}><XIcon size="19" /></button
    >
  </header>
  <div
    class="messages"
    bind:this={scroller}
    on:scroll={() =>
      (follow =
        scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight <
        64)}
  >
    {#if messages.length === 0}<p class="empty">
        Start a conversation with your workspace.
      </p>{/if}
    {#each messages as message (message.id)}
      <article class="message">
        <div class="sender">
          <strong>{message.name || "Guest"}</strong><time
            datetime={new Date(message.sentAt).toISOString()}
            >{new Date(message.sentAt).toLocaleTimeString([], {
              hour: "2-digit",
              minute: "2-digit",
            })}</time
          >
        </div>
        {#if message.text}<p>{message.text}</p>{/if}
        {#if message.attachments.length}<MediaAttachments
            {media}
            attachments={message.attachments}
          />{/if}
      </article>
    {/each}
  </div>
  <form on:submit|preventDefault={submit} on:paste={paste}>
    <MediaAttachments
      bind:this={uploader}
      bind:busy={uploading}
      {media}
      {attachments}
      editable={canUpload}
      on:change={(event) => (attachments = event.detail)}
    />
    <div class="compose">
      <textarea
        bind:this={input}
        bind:value={text}
        rows="2"
        maxlength="2000"
        placeholder="Message the workspace…"
        aria-label="Chat message"
        on:keydown={keydown}></textarea><button
        type="submit"
        aria-label="Send chat message"
        disabled={!connected ||
          uploading ||
          (!text.trim() && !attachments.length)}><SendIcon size="19" /></button
      >
    </div>
    <small>Enter to send · Shift+Enter for a new line</small>
  </form>
</aside>

<style>
  .chat-sidebar {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    width: 100%;
    border-radius: 0.9rem 0 0 0.9rem;
    background: var(--surface-bg);
    color: var(--app-text);
    pointer-events: auto;
    box-shadow: -5px 0 24px rgb(0 0 0 / 12%);
  }
  header {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    padding: 1rem;
    border-bottom: 1px solid var(--surface-border);
  }
  header > div {
    flex: 1;
  }
  h2 {
    font-size: 0.95rem;
    font-weight: 600;
  }
  small,
  time,
  .empty {
    color: var(--surface-muted);
    font-size: 0.72rem;
  }
  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 2rem;
    height: 2rem;
    border-radius: 0.5rem;
  }
  button:hover {
    background: var(--surface-hover);
  }
  button:disabled {
    opacity: 0.4;
  }
  .messages {
    flex: 1;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
    padding: 1rem;
  }
  .empty {
    text-align: center;
    margin-top: 2rem;
  }
  .message {
    margin-bottom: 1rem;
    overflow-wrap: anywhere;
  }
  .sender {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    font-size: 0.75rem;
    margin-bottom: 0.35rem;
  }
  .message p {
    white-space: pre-wrap;
    font-size: 0.85rem;
    line-height: 1.6;
    background: var(--surface-subtle);
    border-radius: 0.65rem;
    padding: 0.6rem 0.75rem;
    margin-bottom: 0.4rem;
  }
  form {
    padding: 0.8rem;
    border-top: 1px solid var(--surface-border);
    display: grid;
    gap: 0.45rem;
  }
  .compose {
    display: flex;
    align-items: flex-end;
    gap: 0.3rem;
    border: 1px solid var(--surface-border);
    border-radius: 0.7rem;
    padding: 0.4rem;
  }
  .compose:focus-within {
    border-color: var(--surface-accent);
  }
  textarea {
    flex: 1;
    min-width: 0;
    resize: vertical;
    max-height: 9rem;
    background: transparent;
    outline: none;
    padding: 0.2rem;
    font-size: 0.85rem;
  }
  .compose button {
    color: var(--surface-accent);
  }
  :global(.mobile-mode) .chat-sidebar {
    border-radius: 0;
    padding-top: env(safe-area-inset-top);
    padding-bottom: env(safe-area-inset-bottom);
  }
</style>
