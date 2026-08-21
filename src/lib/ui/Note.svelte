<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import {
    Maximize2Icon,
    Minimize2Icon,
    Edit3Icon,
    FileTextIcon,
    PlayIcon,
    PlusIcon,
    SendIcon,
    SettingsIcon,
    TerminalIcon,
    Trash2Icon,
    XIcon,
  } from "svelte-feather-icons";
  import type { WsNote } from "$lib/protocol";
  import CanvasRelations, {
    type CanvasRelationItem,
  } from "./CanvasRelations.svelte";
  import ResizeHandles, { type ResizeDirection } from "./ResizeHandles.svelte";

  export let note: WsNote;
  export let noteId: number;
  export let hasWriteAccess: boolean | undefined;
  export let userId: number;
  export let editingBy: number | null = null;
  export let editingName = "";
  export let fullscreen = false;
  export let linkedItems: CanvasRelationItem[] = [];
  export let linkSelecting = false;
  export let linkedHighlight = false;
  export let paragraphDropIndex: number | null = null;

  const dispatch = createEventDispatcher<{
    close: void;
    update: WsNote;
    bringToFront: void;
    startMove: MouseEvent;
    startResize: { event: MouseEvent; direction: ResizeDirection };
    editing: boolean;
    paragraphs: string[];
    focus: void;
    blur: void;
    toggleLink: void;
    navigateRelation: CanvasRelationItem;
    unlinkRelation: CanvasRelationItem;
    sendParagraph: {
      text: string;
      target: "all" | "notes" | "terminals" | "terminals-execute" | "files";
    };
    paragraphDragStart: { text: string; sourceNoteId: number };
    paragraphDragEnd: void;
    toggleFullscreen: void;
  }>();

  let root: HTMLElement;
  let editing = false;
  let focused = false;
  let settingsOpen = false;
  let settingsButton: HTMLButtonElement;
  let settingsPanel: HTMLDivElement;
  let paragraphMenu: number | null = null;
  let paragraphs = noteParagraphs(note);
  const editors: Record<number, HTMLTextAreaElement> = {};
  type HistoryEntry = {
    paragraphs: string[];
    paragraphIndex: number;
    selectionStart: number;
    selectionEnd: number;
  };
  const undoStack: HistoryEntry[] = [];
  const redoStack: HistoryEntry[] = [];
  let lastHistoryGroup = "";
  let lastHistoryTime = 0;

  $: if (!editing && !sameParagraphs(paragraphs, noteParagraphs(note)))
    applyExternalParagraphs(noteParagraphs(note));
  $: if (editing && editingBy !== null && editingBy !== userId) finishEditing();

  function splitParagraphs(text: string) {
    const values = text.split("\n");
    return values.length ? values : [""];
  }
  function noteParagraphs(value: WsNote) {
    return value.paragraphs?.length
      ? [...value.paragraphs]
      : splitParagraphs(value.text);
  }
  function sameParagraphs(left: string[], right: string[]) {
    return (
      left.length === right.length &&
      left.every((value, index) => value === right[index])
    );
  }
  function applyExternalParagraphs(next: string[]) {
    paragraphs = next;
    undoStack.length = 0;
    redoStack.length = 0;
    resetHistoryGroup();
  }
  function resetHistoryGroup() {
    lastHistoryGroup = "";
    lastHistoryTime = 0;
  }
  function historyEntry(
    paragraphIndex: number,
    editor: HTMLTextAreaElement | undefined,
  ): HistoryEntry {
    return {
      paragraphs: [...paragraphs],
      paragraphIndex,
      selectionStart: editor?.selectionStart ?? 0,
      selectionEnd: editor?.selectionEnd ?? editor?.selectionStart ?? 0,
    };
  }
  function recordHistory(
    paragraphIndex: number,
    editor: HTMLTextAreaElement | undefined,
    group = "",
  ) {
    const now = Date.now();
    const coalesced =
      group && group === lastHistoryGroup && now - lastHistoryTime < 750;
    if (!coalesced) {
      undoStack.push(historyEntry(paragraphIndex, editor));
      if (undoStack.length > 100) undoStack.shift();
    }
    redoStack.length = 0;
    lastHistoryGroup = group;
    lastHistoryTime = now;
  }
  async function restoreHistory(entry: HistoryEntry) {
    paragraphs = [...entry.paragraphs];
    emitText();
    await tick();
    const index = Math.min(
      Math.max(0, entry.paragraphIndex),
      paragraphs.length - 1,
    );
    const editor = editors[index];
    if (!editor) return;
    const length = paragraphs[index].length;
    editor.focus({ preventScroll: true });
    editor.setSelectionRange(
      Math.min(entry.selectionStart, length),
      Math.min(entry.selectionEnd, length),
    );
  }
  async function undoNote(index: number, editor: HTMLTextAreaElement) {
    const entry = undoStack.pop();
    if (!entry) return;
    redoStack.push(historyEntry(index, editor));
    resetHistoryGroup();
    await restoreHistory(entry);
  }
  async function redoNote(index: number, editor: HTMLTextAreaElement) {
    const entry = redoStack.pop();
    if (!entry) return;
    undoStack.push(historyEntry(index, editor));
    resetHistoryGroup();
    await restoreHistory(entry);
  }
  function update(values: Partial<WsNote>) {
    dispatch("update", { ...note, ...values });
  }
  function emitText() {
    paragraphs = [...paragraphs];
    dispatch("paragraphs", paragraphs);
  }
  async function ensureEditing() {
    if (!hasWriteAccess || (editingBy !== null && editingBy !== userId))
      return false;
    if (!editing) {
      editing = true;
      dispatch("editing", true);
      await tick();
    }
    return true;
  }
  async function beginEditing(index: number, event: MouseEvent) {
    event.stopPropagation();
    if (!(await ensureEditing())) return;
    dispatch("bringToFront");
    editors[index]?.focus({ preventScroll: true });
  }
  function finishEditing(blurActiveElement = true) {
    if (!editing) return;
    editing = false;
    paragraphMenu = null;
    resetHistoryGroup();
    dispatch("editing", false);
    if (blurActiveElement)
      (document.activeElement as HTMLElement | null)?.blur?.();
  }
  async function handleParagraphBlur() {
    await tick();
    const active = document.activeElement;
    if (active instanceof HTMLTextAreaElement && root.contains(active)) return;
    if (root.contains(active)) finishEditing(false);
  }
  async function handleParagraphKey(event: KeyboardEvent, index: number) {
    const editor = event.currentTarget as HTMLTextAreaElement;
    const commandModifier = (event.ctrlKey || event.metaKey) && !event.altKey;
    const key = event.key.toLowerCase();
    if (commandModifier && key === "z") {
      event.preventDefault();
      if (event.shiftKey) await redoNote(index, editor);
      else await undoNote(index, editor);
    } else if (commandModifier && key === "y" && !event.shiftKey) {
      event.preventDefault();
      await redoNote(index, editor);
    } else if (event.key === "Escape") {
      event.preventDefault();
      finishEditing();
    } else if (
      event.key === "Enter" &&
      (event.ctrlKey || event.metaKey) &&
      !event.shiftKey
    ) {
      event.preventDefault();
      recordHistory(index, editor);
      const start = editor.selectionStart;
      const end = editor.selectionEnd;
      const current = paragraphs[index];
      paragraphs.splice(index, 1, current.slice(0, start), current.slice(end));
      emitText();
      await tick();
      editors[index + 1]?.focus({ preventScroll: true });
      editors[index + 1]?.setSelectionRange(0, 0);
    } else if (
      event.key === "Backspace" &&
      editor.selectionStart === 0 &&
      editor.selectionEnd === 0 &&
      index > 0
    ) {
      event.preventDefault();
      recordHistory(index, editor);
      const caret = paragraphs[index - 1].length;
      paragraphs.splice(
        index - 1,
        2,
        paragraphs[index - 1] + paragraphs[index],
      );
      emitText();
      await tick();
      editors[index - 1]?.focus({ preventScroll: true });
      editors[index - 1]?.setSelectionRange(caret, caret);
    }
  }
  async function insertParagraph(index: number) {
    if (!(await ensureEditing())) return;
    recordHistory(index, editors[index]);
    paragraphs.splice(index + 1, 0, "");
    paragraphMenu = null;
    emitText();
    await tick();
    editors[index + 1]?.focus({ preventScroll: true });
    editors[index + 1]?.setSelectionRange(0, 0);
  }
  async function deleteParagraph(index: number) {
    if (!(await ensureEditing())) return;
    recordHistory(index, editors[index]);
    paragraphs.splice(index, 1);
    if (!paragraphs.length) paragraphs = [""];
    paragraphMenu = null;
    emitText();
    await tick();
    editors[Math.min(index, paragraphs.length - 1)]?.focus({
      preventScroll: true,
    });
  }
  function handleParagraphBeforeInput(event: Event, index: number) {
    const editor = event.currentTarget as HTMLTextAreaElement;
    const inputType = (event as InputEvent).inputType || "input";
    const group = [
      "insertText",
      "deleteContentBackward",
      "deleteContentForward",
    ].includes(inputType)
      ? `${index}:${inputType}`
      : "";
    recordHistory(index, editor, group);
  }
  function handleParagraphInput(event: Event, index: number) {
    const editor = event.currentTarget as HTMLTextAreaElement;
    paragraphs[index] = editor.value;
    resizeEditor(editor);
    emitText();
  }
  function handleWindowMouseDown(event: MouseEvent) {
    if (!(event.target instanceof Node)) return;
    if (
      settingsOpen &&
      !settingsButton.contains(event.target) &&
      !settingsPanel?.contains(event.target)
    )
      settingsOpen = false;
    if (root && !root.contains(event.target)) {
      paragraphMenu = null;
      finishEditing();
    }
  }
  function resizeEditor(editor: HTMLTextAreaElement) {
    editor.style.height = "0";
    editor.style.height = `${Math.max(28, editor.scrollHeight)}px`;
  }
  function autoResizeParagraph(
    editor: HTMLTextAreaElement,
    _paragraph: string,
  ) {
    void tick().then(() => resizeEditor(editor));
    return {
      update() {
        void tick().then(() => resizeEditor(editor));
      },
    };
  }
  function startParagraphDrag(event: DragEvent, text: string) {
    if (!hasWriteAccess || !text || !event.dataTransfer) {
      event.preventDefault();
      return;
    }
    paragraphMenu = null;
    event.dataTransfer.effectAllowed = "copy";
    event.dataTransfer.setData("text/plain", text);
    event.dataTransfer.setData("application/x-sshxx-note-paragraph", text);
    const preview = document.createElement("div");
    preview.textContent = text.replace(/\s+/g, " ").trim() || "Empty paragraph";
    Object.assign(preview.style, {
      position: "fixed",
      left: "-10000px",
      top: "-10000px",
      maxWidth: "360px",
      padding: "10px 14px",
      border: "1px solid rgb(165 180 252 / 70%)",
      borderRadius: "8px",
      background: "rgb(24 24 27 / 88%)",
      color: "#f4f4f5",
      font: "13px Inter, sans-serif",
      boxShadow: "0 12px 32px rgb(0 0 0 / 45%)",
      opacity: "0.82",
      overflow: "hidden",
      textOverflow: "ellipsis",
      whiteSpace: "nowrap",
      pointerEvents: "none",
    });
    document.body.append(preview);
    event.dataTransfer.setDragImage(preview, 18, 18);
    requestAnimationFrame(() => preview.remove());
    dispatch("paragraphDragStart", { text, sourceNoteId: noteId });
  }
  function handleFocusIn() {
    focused = true;
    dispatch("focus");
  }
  function handleFocusOut(event: FocusEvent) {
    if (
      event.relatedTarget instanceof Node &&
      root.contains(event.relatedTarget)
    )
      return;
    focused = false;
    dispatch("blur");
  }
</script>

<svelte:window on:mousedown|capture={handleWindowMouseDown} />

<article
  bind:this={root}
  role="presentation"
  tabindex="-1"
  data-canvas-note
  data-canvas-note-id={noteId}
  class="note-container relative overflow-visible rounded-lg border border-white/15 shadow-xl shadow-black/40 ring-1 ring-white/10"
  class:fullscreen
  class:focused
  class:editing-active={focused && editing}
  class:linked-highlight={linkedHighlight}
  style:width={fullscreen ? "100%" : `${note.width}px`}
  style:height={fullscreen ? "100%" : `${note.height}px`}
  style:background={note.background}
  on:mousedown={() => {
    root.focus({ preventScroll: true });
    dispatch("bringToFront");
  }}
  on:focusin={handleFocusIn}
  on:focusout={handleFocusOut}
  on:pointerdown={(event) => event.stopPropagation()}
  on:wheel={(event) => {
    if (!event.ctrlKey) event.stopPropagation();
  }}
>
  <header
    role="presentation"
    class="flex h-9 cursor-move select-none items-center justify-between rounded-t-lg border-b border-white/10 bg-black/15 px-2"
    class:cursor-default={fullscreen}
    on:mousedown={(event) => {
      if (event.button === 0 && hasWriteAccess && !fullscreen)
        dispatch("startMove", event);
    }}
  >
    <div class="inline-flex h-5 items-center gap-0.5">
      <button
        type="button"
        class="circle-action red"
        aria-label="Close note"
        disabled={!hasWriteAccess}
        on:mousedown|stopPropagation={(event) =>
          event.button === 0 && dispatch("close")}><XIcon /></button
      >
      <button
        type="button"
        class="circle-action purple"
        aria-label={fullscreen ? "Exit full screen" : "Full screen"}
        on:mousedown|stopPropagation={(event) =>
          event.button === 0 && dispatch("toggleFullscreen")}
        >{#if fullscreen}<Minimize2Icon />{:else}<Maximize2Icon />{/if}</button
      >
    </div>
    <span class="min-w-0 text-center">
      <span
        class="block text-xs font-medium uppercase tracking-widest text-zinc-200/75"
        >Note</span
      >
      {#if focused}<span
          class="block max-w-40 truncate text-[10px] text-zinc-300/65"
          >{editing ? "Editing paragraph" : "Selected"}</span
        >
      {:else if editingBy !== null && editingBy !== userId}<span
          class="block max-w-36 truncate text-[10px] text-zinc-300/60"
          >{editingName || `User ${editingBy}`} is editing</span
        >{/if}
    </span>
    <button
      bind:this={settingsButton}
      type="button"
      class="rounded p-1 text-zinc-200/70 hover:bg-white/10 hover:text-white"
      aria-label="Note appearance"
      on:mousedown|stopPropagation={(event) =>
        event.button === 0 && (settingsOpen = !settingsOpen)}
      ><SettingsIcon class="h-4 w-4" /></button
    >
  </header>

  {#if settingsOpen}
    <div
      bind:this={settingsPanel}
      role="presentation"
      class="panel absolute right-1 top-10 z-20 w-56 space-y-3 p-3 text-sm"
      on:mousedown|stopPropagation
    >
      <label class="flex items-center justify-between gap-3"
        >Background<input
          type="color"
          value={note.background}
          disabled={!hasWriteAccess}
          on:input={(event) =>
            update({ background: event.currentTarget.value })}
        /></label
      >
      <label class="block"
        ><span class="mb-1 flex justify-between"
          ><span>Opacity</span><span>{note.opacity}%</span></span
        ><input
          type="range"
          min="20"
          max="100"
          value={note.opacity}
          disabled={!hasWriteAccess}
          class="w-full accent-indigo-500"
          on:input={(event) => update({ opacity: +event.currentTarget.value })}
        /></label
      >
    </div>
  {/if}

  <div
    role="presentation"
    class="note-editor h-[calc(100%-2.25rem)] overflow-y-auto py-3 pr-3"
    class:editing
    on:mousedown|stopPropagation
  >
    {#each paragraphs as paragraph, index (index)}
      <div
        class="paragraph-row group relative flex min-h-8 items-start pl-6"
        data-note-paragraph-index={index}
      >
        {#if paragraphDropIndex === index}<div
            class="paragraph-drop-line"
            aria-hidden="true"
          ></div>{/if}
        <button
          type="button"
          class="paragraph-marker"
          class:active={paragraphMenu === index}
          aria-label={`Paragraph ${index + 1} actions`}
          title="Paragraph actions · Drag to copy"
          draggable={hasWriteAccess && Boolean(paragraph)}
          disabled={!hasWriteAccess}
          on:mousedown|stopPropagation
          on:click|stopPropagation={() =>
            (paragraphMenu = paragraphMenu === index ? null : index)}
          on:dragstart|stopPropagation={(event) =>
            startParagraphDrag(event, paragraph)}
          on:dragend={() => dispatch("paragraphDragEnd")}
          ><span></span><span></span><span></span><span></span></button
        >
        {#if paragraphMenu === index}
          <div
            role="presentation"
            class="paragraph-menu"
            on:mousedown|stopPropagation
          >
            <button
              type="button"
              disabled={editingBy !== null && editingBy !== userId}
              on:click={() => deleteParagraph(index)}
              ><Trash2Icon />Delete</button
            >
            <button
              type="button"
              disabled={editingBy !== null && editingBy !== userId}
              on:click={() => insertParagraph(index)}
              ><PlusIcon />New paragraph below</button
            >
            <button
              type="button"
              on:click={() => {
                paragraphMenu = null;
                dispatch("sendParagraph", { text: paragraph, target: "all" });
              }}><SendIcon />Send to all linked</button
            >
            <button
              type="button"
              on:click={() => {
                paragraphMenu = null;
                dispatch("sendParagraph", { text: paragraph, target: "notes" });
              }}><FileTextIcon />Send to linked notes</button
            >
            <button
              type="button"
              on:click={() => {
                paragraphMenu = null;
                dispatch("sendParagraph", {
                  text: paragraph,
                  target: "terminals",
                });
              }}><TerminalIcon />Send to linked terminals</button
            >
            <button
              type="button"
              on:click={() => {
                paragraphMenu = null;
                dispatch("sendParagraph", {
                  text: paragraph,
                  target: "terminals-execute",
                });
              }}><PlayIcon />Send to terminals &amp; run</button
            >
            <button
              type="button"
              on:click={() => {
                paragraphMenu = null;
                dispatch("sendParagraph", { text: paragraph, target: "files" });
              }}><Edit3Icon />Send to file editors</button
            >
          </div>
        {/if}
        <textarea
          bind:this={editors[index]}
          value={paragraph}
          rows="1"
          readonly={!editing}
          aria-label={`Note paragraph ${index + 1}`}
          placeholder={!editing && paragraphs.length === 1 && !paragraph
            ? editingBy !== null && editingBy !== userId
              ? `${editingName || `User ${editingBy}`} is editing this note`
              : "Click to edit this note"
            : ""}
          class="paragraph-input"
          use:autoResizeParagraph={paragraph}
          on:click={(event) => beginEditing(index, event)}
          on:focus={(event) => resizeEditor(event.currentTarget)}
          on:blur={handleParagraphBlur}
          on:beforeinput={(event) => handleParagraphBeforeInput(event, index)}
          on:input={(event) => handleParagraphInput(event, index)}
          on:keydown={(event) => handleParagraphKey(event, index)}></textarea>
      </div>
    {/each}
    {#if paragraphDropIndex === paragraphs.length}<div
        class="paragraph-drop-end"
        aria-hidden="true"
      >
        <span></span>
      </div>{/if}
  </div>
  <footer class="note-relations">
    <span class="relation-hint"
      >Drag a handle · Ctrl/Cmd+Enter adds a paragraph</span
    >
    <CanvasRelations
      items={linkedItems}
      allowAdd
      selecting={linkSelecting}
      disabled={!hasWriteAccess}
      on:toggleAdd={() => dispatch("toggleLink")}
      on:navigate={(event) => dispatch("navigateRelation", event.detail)}
      on:remove={(event) => dispatch("unlinkRelation", event.detail)}
    />
  </footer>
  <ResizeHandles
    disabled={!hasWriteAccess || fullscreen}
    on:start={(event) => dispatch("startResize", event.detail)}
  />
</article>

<style lang="postcss">
  @reference "../../app.css";
  .note-container.focused {
    outline: 2px solid rgb(228 228 231 / 88%);
    outline-offset: -1px;
  }
  .note-container.editing-active {
    outline-color: rgb(186 230 253 / 92%);
    box-shadow:
      0 0 0 1px rgb(125 211 252 / 20%),
      0 8px 24px rgb(0 0 0 / 35%);
  }
  .note-container.editing-active > header {
    background: rgb(125 211 252 / 10%);
  }
  .note-container {
    @apply flex flex-col;
  }
  .note-container.linked-highlight {
    outline: 2px solid rgb(125 211 252 / 80%);
    outline-offset: 1px;
    animation: linked-note-pulse 1.8s ease-in-out infinite;
  }
  .note-container.fullscreen {
    display: flex;
    flex-direction: column;
  }
  .circle-action {
    @apply inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md p-0 disabled:opacity-40;
  }
  .circle-action :global(svg) {
    @apply h-3.5 w-3.5 rounded-full border p-[2px];
    stroke-width: 2.5;
  }
  .circle-action.red :global(svg) {
    @apply border-rose-300/50 bg-rose-400 text-rose-950;
  }
  .circle-action.purple :global(svg) {
    @apply border-violet-300/50 bg-violet-400 text-violet-950;
  }
  .paragraph-input {
    @apply block min-h-7 w-full resize-none overflow-hidden bg-transparent px-2 py-1 text-sm leading-6 text-zinc-100 outline-none placeholder:text-zinc-300/40;
  }
  .note-editor.editing .paragraph-input:focus {
    @apply rounded bg-white/[0.035];
  }
  .paragraph-marker {
    @apply absolute left-1.5 top-2 grid h-4 w-4 cursor-grab grid-cols-[repeat(2,2px)] grid-rows-[repeat(2,2px)] place-content-center gap-x-[1.5px] gap-y-[3px] rounded text-zinc-300 opacity-35 transition-opacity hover:bg-white/10 hover:text-zinc-100 hover:opacity-100 active:cursor-grabbing disabled:pointer-events-none;
  }
  .paragraph-row {
    @apply mx-1 rounded-md border border-white/[0.055] bg-black/[0.06] transition-colors hover:border-white/10 hover:bg-white/[0.035];
  }
  .paragraph-row + .paragraph-row {
    @apply mt-1.5;
  }
  .paragraph-row:hover .paragraph-marker,
  .paragraph-marker.active {
    @apply opacity-100;
  }
  .paragraph-marker span {
    @apply h-0.5 w-0.5 rounded-full bg-current;
  }
  .paragraph-menu {
    @apply absolute left-1.5 top-7 z-30 w-60 overflow-hidden rounded-md border border-zinc-700 bg-zinc-900 p-1 shadow-xl;
  }
  .paragraph-menu button {
    @apply flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs text-zinc-300 hover:bg-zinc-800 hover:text-white;
  }
  .paragraph-menu button:disabled {
    @apply cursor-not-allowed opacity-35;
  }
  .paragraph-menu :global(svg) {
    @apply h-3.5 w-3.5;
  }
  .note-editor {
    @apply relative min-h-0 flex-1;
    height: auto;
  }
  .paragraph-drop-line {
    @apply pointer-events-none absolute -top-1 left-5 right-2 z-20 h-0.5 rounded-full bg-sky-300 shadow-[0_0_7px_rgb(125_211_252/0.8)];
  }
  .paragraph-drop-line::before {
    content: "";
    @apply absolute -left-1 -top-[3px] h-2 w-2 rounded-full bg-sky-200;
  }
  .paragraph-drop-end {
    @apply relative mx-3 h-3;
  }
  .paragraph-drop-end span {
    @apply pointer-events-none absolute left-3 right-0 top-1 h-0.5 rounded-full bg-sky-300 shadow-[0_0_7px_rgb(125_211_252/0.8)];
  }
  .paragraph-drop-end span::before {
    content: "";
    @apply absolute -left-1 -top-[3px] h-2 w-2 rounded-full bg-sky-200;
  }
  .note-relations {
    @apply flex h-9 shrink-0 items-center justify-between gap-2 rounded-b-lg border-t border-white/10 bg-black/15 px-2;
  }
  .relation-hint {
    @apply min-w-0 truncate text-[10px] text-zinc-300/35;
  }
  @keyframes linked-note-pulse {
    0%,
    100% {
      box-shadow: 0 0 4px rgb(125 211 252 / 18%);
    }
    50% {
      box-shadow: 0 0 11px rgb(125 211 252 / 48%);
    }
  }
</style>
