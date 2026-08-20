<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import { SettingsIcon, XIcon } from "svelte-feather-icons";

  import type { WsNote } from "$lib/protocol";
  import ResizeHandles, { type ResizeDirection } from "./ResizeHandles.svelte";

  export let note: WsNote;
  export let hasWriteAccess: boolean | undefined;
  export let userId: number;
  export let editingBy: number | null = null;
  export let editingName = "";

  const dispatch = createEventDispatcher<{
    close: void;
    update: WsNote;
    bringToFront: void;
    startMove: MouseEvent;
    startResize: { event: MouseEvent; direction: ResizeDirection };
    editing: boolean;
    text: string;
  }>();

  let editing = false;
  let settingsOpen = false;
  let settingsButton: HTMLButtonElement;
  let settingsPanel: HTMLDivElement;
  let textarea: HTMLTextAreaElement;
  let text = note.text;

  $: if (!editing && text !== note.text) text = note.text;
  $: if (editing && editingBy !== null && editingBy !== userId) {
    editing = false;
    textarea?.blur();
  }

  function update(values: Partial<WsNote>) {
    dispatch("update", { ...note, ...values });
  }

  async function beginEditing(event: MouseEvent) {
    if (editing) return;
    event.preventDefault();
    event.stopPropagation();
    if (!hasWriteAccess || (editingBy !== null && editingBy !== userId)) return;
    dispatch("bringToFront");
    editing = true;
    dispatch("editing", true);
    await tick();
    textarea.focus({ preventScroll: true });
    textarea.setSelectionRange(text.length, text.length);
  }

  function collapseSelection(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    const caret = textarea.selectionEnd;
    textarea.setSelectionRange(caret, caret);
  }

  function finishEditing() {
    if (!editing) return;
    editing = false;
    dispatch("editing", false);
  }

  function closeSettingsOnOutsideClick(event: MouseEvent) {
    if (!settingsOpen || !(event.target instanceof Node)) return;
    if (
      settingsButton.contains(event.target) ||
      settingsPanel?.contains(event.target)
    ) {
      return;
    }
    settingsOpen = false;
  }
</script>

<svelte:window on:mousedown|capture={closeSettingsOnOutsideClick} />

<article
  role="presentation"
  data-canvas-note
  class="note-container relative overflow-visible rounded-lg border border-white/15 shadow-xl shadow-black/40 ring-1 ring-white/10"
  style:width="{note.width}px"
  style:height="{note.height}px"
  style:background={note.background}
  on:mousedown={() => dispatch("bringToFront")}
  on:pointerdown={(event) => event.stopPropagation()}
>
  <header
    role="presentation"
    class="flex h-9 cursor-move select-none items-center justify-between rounded-t-lg border-b border-white/10 bg-black/15 px-2"
    on:mousedown={(event) => {
      if (event.button === 0 && hasWriteAccess) dispatch("startMove", event);
    }}
  >
    <button
      type="button"
      class="rounded-full bg-red-400/90 p-[2px] text-red-950/80 hover:bg-red-300 hover:text-red-950"
      aria-label="Close note"
      disabled={!hasWriteAccess}
      on:mousedown|stopPropagation={(event) => {
        if (event.button === 0) dispatch("close");
      }}
    >
      <XIcon class="h-2.5 w-2.5" strokeWidth={3} />
    </button>
    <span class="min-w-0 text-center">
      <span
        class="block text-xs font-medium uppercase tracking-widest text-zinc-200/75"
      >
        Note
      </span>
      {#if editingBy !== null && editingBy !== userId}
        <span class="block max-w-36 truncate text-[10px] text-zinc-300/60">
          {editingName || `User ${editingBy}`} is editing
        </span>
      {/if}
    </span>
    <button
      bind:this={settingsButton}
      type="button"
      class="rounded p-1 text-zinc-200/70 hover:bg-white/10 hover:text-white"
      aria-label="Note appearance"
      on:mousedown|stopPropagation={(event) => {
        if (event.button === 0) settingsOpen = !settingsOpen;
      }}
    >
      <SettingsIcon class="h-4 w-4" />
    </button>
  </header>

  {#if settingsOpen}
    <div
      bind:this={settingsPanel}
      role="presentation"
      class="panel absolute right-1 top-10 z-20 w-56 space-y-3 p-3 text-sm"
      on:mousedown|stopPropagation
    >
      <label class="flex items-center justify-between gap-3">
        Background
        <input
          type="color"
          value={note.background}
          disabled={!hasWriteAccess}
          on:input={(event) =>
            update({ background: event.currentTarget.value })}
        />
      </label>
      <label class="block">
        <span class="mb-1 flex justify-between">
          <span>Opacity</span><span>{note.opacity}%</span>
        </span>
        <input
          type="range"
          min="20"
          max="100"
          value={note.opacity}
          disabled={!hasWriteAccess}
          class="w-full accent-indigo-500"
          on:input={(event) => update({ opacity: +event.currentTarget.value })}
        />
      </label>
    </div>
  {/if}

  <textarea
    bind:this={textarea}
    bind:value={text}
    readonly={!editing}
    aria-label="Note text"
    placeholder={editing
      ? ""
      : editingBy !== null && editingBy !== userId
        ? `${editingName || `User ${editingBy}`} is editing this note`
        : "Click to edit this note"}
    class="h-[calc(100%-2.25rem)] w-full resize-none bg-transparent p-4 text-sm leading-6 text-zinc-100 outline-none placeholder:text-zinc-300/40 {editing
      ? 'cursor-text ring-2 ring-inset ring-zinc-200/70'
      : 'cursor-default'}"
    on:mousedown|stopPropagation
    on:click={beginEditing}
    on:dblclick={collapseSelection}
    on:input={() => dispatch("text", text)}
    on:blur={finishEditing}
    on:keydown={(event) => {
      if (event.key === "Escape") textarea.blur();
    }}></textarea>

  <ResizeHandles
    disabled={!hasWriteAccess}
    on:start={(event) => dispatch("startResize", event.detail)}
  />
</article>

<style lang="postcss">
  .note-container:focus-within {
    outline: 2px solid rgb(228 228 231 / 88%);
    outline-offset: -1px;
  }
</style>
