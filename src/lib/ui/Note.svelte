<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import {
    CopyIcon,
    Edit3Icon,
    FileTextIcon,
    PlayIcon,
    PlusIcon,
    SendIcon,
    SettingsIcon,
    TerminalIcon,
    Trash2Icon,
  } from "svelte-feather-icons";
  import { MINIMIZED_WINDOW_HEIGHT } from "$lib/grid";
  import type { WsNote } from "$lib/protocol";
  import { makeToast } from "$lib/toast";
  import {
    deleteParagraphs,
    PARAGRAPH_CLIPBOARD_TYPE,
    paragraphPlainText,
    reorderParagraphs,
    selectedParagraphs,
    serializeParagraphs,
  } from "$lib/paragraphs";
  import {
    copyParagraphs,
    readParagraphClipboard,
    writeParagraphClipboard,
  } from "$lib/paragraphClipboard";
  import CanvasRelations, {
    type CanvasRelationItem,
  } from "./CanvasRelations.svelte";
  import BackgroundPicker from "./BackgroundPicker.svelte";
  import CircleButton from "./CircleButton.svelte";
  import CircleButtons from "./CircleButtons.svelte";
  import InlineTitle from "./InlineTitle.svelte";
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
  export let linkedHighlightSource: "terminal" | "note" | "file" | null = null;
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
      paragraphs: string[];
      text: string;
      target: "all" | "notes" | "terminals" | "terminals-execute" | "files";
    };
    paragraphDragStart: {
      paragraphs: string[];
      text: string;
      sourceNoteId: number;
      paragraphIndexes: number[];
    };
    paragraphDragEnd: void;
    toggleFullscreen: void;
    floatingChange: boolean;
  }>();

  let root: HTMLElement;
  let editing = false;
  let activeParagraphIndex: number | null = null;
  let focused = false;
  let settingsOpen = false;
  let settingsButton: HTMLButtonElement;
  let settingsPanel: HTMLDivElement;
  let paragraphMenu: number | null = null;
  let paragraphMenuKind: "actions" | "send" = "actions";
  let paragraphMenuAnchor: HTMLButtonElement | null = null;
  let paragraphMenuPanel: HTMLDivElement | null = null;
  let paragraphMenuLeft = 0;
  let paragraphMenuTop = 0;
  let paragraphMenuPositioned = false;
  let observedMinimized = note.minimized;
  let paragraphs = noteParagraphs(note);
  let selectedParagraphIndexes: number[] = [];
  let selectionAnchor: number | null = null;
  let rangeSelection: {
    anchor: number;
    startX: number;
    startY: number;
    active: boolean;
  } | null = null;
  let suppressParagraphClick = false;
  let draggingParagraphIndexes: number[] = [];
  let movedParagraphIndexes: number[] = [];
  let movedParagraphTimer: number | null = null;
  let editorViewport: HTMLElement;
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
    activeParagraphIndex = null;
    selectedParagraphIndexes = [];
    selectionAnchor = null;
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
    const index = Math.min(
      Math.max(0, entry.paragraphIndex),
      paragraphs.length - 1,
    );
    activeParagraphIndex = index;
    await tick();
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
    if (suppressParagraphClick) {
      event.preventDefault();
      suppressParagraphClick = false;
      return;
    }
    const editor = event.currentTarget as HTMLTextAreaElement;
    const selectionStart = editor.selectionStart;
    const selectionEnd = editor.selectionEnd;
    rangeSelection = null;
    closeParagraphMenu();
    activeParagraphIndex = index;
    selectedParagraphIndexes = [index];
    selectionAnchor = index;
    if (!(await ensureEditing())) {
      activeParagraphIndex = null;
      return;
    }
    dispatch("bringToFront");
    window.getSelection()?.removeAllRanges();
    await tick();
    const activeEditor = editors[index];
    if (!activeEditor) return;
    activeEditor.focus({ preventScroll: true });
    activeEditor.setSelectionRange(selectionStart, selectionEnd);
  }

  function portal(node: HTMLElement) {
    document.body.append(node);
    return {
      destroy() {
        node.remove();
      },
    };
  }
  function closeParagraphMenu(restoreFocus = false) {
    const anchor = paragraphMenuAnchor;
    paragraphMenu = null;
    paragraphMenuKind = "actions";
    paragraphMenuAnchor = null;
    paragraphMenuPanel = null;
    paragraphMenuPositioned = false;
    if (restoreFocus)
      requestAnimationFrame(() => anchor?.focus({ preventScroll: true }));
  }
  function positionParagraphMenu() {
    if (
      paragraphMenu === null ||
      !paragraphMenuAnchor?.isConnected ||
      !paragraphMenuPanel?.isConnected
    )
      return;
    const anchor = paragraphMenuAnchor.getBoundingClientRect();
    const menu = paragraphMenuPanel.getBoundingClientRect();
    const edge = 8;
    const gap = 6;
    paragraphMenuLeft = Math.min(
      Math.max(edge, anchor.left),
      Math.max(edge, window.innerWidth - menu.width - edge),
    );
    paragraphMenuTop =
      anchor.bottom + gap + menu.height <= window.innerHeight - edge
        ? anchor.bottom + gap
        : Math.max(edge, anchor.top - menu.height - gap);
    paragraphMenuPositioned = true;
  }
  async function selectParagraph(index: number, event: MouseEvent) {
    finishEditing(false);
    if (event.shiftKey && selectionAnchor !== null) {
      const start = Math.min(selectionAnchor, index);
      const end = Math.max(selectionAnchor, index);
      selectedParagraphIndexes = Array.from(
        { length: end - start + 1 },
        (_, offset) => start + offset,
      );
      closeParagraphMenu();
    } else if (event.ctrlKey || event.metaKey) {
      selectedParagraphIndexes = selectedParagraphIndexes.includes(index)
        ? selectedParagraphIndexes.filter((value) => value !== index)
        : [...selectedParagraphIndexes, index].sort(
            (left, right) => left - right,
          );
      selectionAnchor = index;
      closeParagraphMenu();
    } else {
      const preserveSelection =
        selectedParagraphIndexes.length > 1 &&
        selectedParagraphIndexes.includes(index);
      if (!preserveSelection) selectedParagraphIndexes = [index];
      selectionAnchor = index;
      if (paragraphMenu === index) {
        closeParagraphMenu();
        return;
      }
      paragraphMenu = index;
      paragraphMenuKind = "actions";
      paragraphMenuAnchor = event.currentTarget as HTMLButtonElement;
      paragraphMenuPositioned = false;
      await tick();
      positionParagraphMenu();
    }
  }

  async function openParagraphSendMenu(index: number, event: MouseEvent) {
    if (paragraphMenu === index && paragraphMenuKind === "send") {
      closeParagraphMenu();
      return;
    }
    paragraphMenu = index;
    paragraphMenuKind = "send";
    paragraphMenuAnchor = event.currentTarget as HTMLButtonElement;
    paragraphMenuPositioned = false;
    await tick();
    positionParagraphMenu();
  }

  function sendParagraph(
    index: number,
    target: "all" | "notes" | "terminals" | "terminals-execute" | "files",
  ) {
    closeParagraphMenu();
    dispatch("sendParagraph", { ...paragraphTransfer(index), target });
  }

  function prepareParagraphDrag(index: number, event: MouseEvent) {
    if (event.button !== 0) return;
    finishEditing(false);
    if (
      !selectedParagraphIndexes.includes(index) &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.shiftKey
    ) {
      selectedParagraphIndexes = [index];
      selectionAnchor = index;
    }
  }
  function finishEditing(blurActiveElement = true) {
    if (!editing) return;
    editing = false;
    activeParagraphIndex = null;
    closeParagraphMenu();
    resetHistoryGroup();
    dispatch("editing", false);
    if (blurActiveElement)
      (document.activeElement as HTMLElement | null)?.blur?.();
  }

  function acquireStructuralEdit() {
    if (!hasWriteAccess || (editingBy !== null && editingBy !== userId)) {
      makeToast({
        kind: "info",
        message:
          editingBy !== null && editingBy !== userId
            ? `${editingName || `User ${editingBy}`} is editing this note.`
            : "You cannot modify this note in read-only mode.",
      });
      return null;
    }
    const temporary = !editing;
    if (temporary) {
      // Component events are synchronous, so the ownership message is queued
      // before the paragraph update and the release message.
      dispatch("editing", true);
    }
    return () => {
      if (temporary) dispatch("editing", false);
    };
  }

  function startRangeSelection(index: number, event: MouseEvent) {
    if (event.button !== 0) return;
    if (
      editing &&
      activeParagraphIndex === index &&
      event.target === editors[index]
    )
      return;
    finishEditing(false);
    closeParagraphMenu();
    rangeSelection = {
      anchor: index,
      startX: event.clientX,
      startY: event.clientY,
      active: false,
    };
  }

  function paragraphIndexAt(clientY: number) {
    const rows = Array.from(
      editorViewport.querySelectorAll<HTMLElement>(
        "[data-note-paragraph-index]",
      ),
    );
    if (!rows.length) return 0;
    for (const row of rows) {
      const bounds = row.getBoundingClientRect();
      if (clientY < bounds.bottom)
        return Number(row.dataset.noteParagraphIndex);
    }
    return rows.length - 1;
  }

  function updateRangeSelection(event: MouseEvent) {
    if (!rangeSelection) return;
    if ((event.buttons & 1) === 0) {
      finishRangeSelection();
      return;
    }
    if (
      !rangeSelection.active &&
      Math.hypot(
        event.clientX - rangeSelection.startX,
        event.clientY - rangeSelection.startY,
      ) < 5
    )
      return;
    event.preventDefault();
    rangeSelection.active = true;
    window.getSelection()?.removeAllRanges();
    const bounds = editorViewport.getBoundingClientRect();
    if (event.clientY < bounds.top + 24) editorViewport.scrollTop -= 12;
    else if (event.clientY > bounds.bottom - 24) editorViewport.scrollTop += 12;
    const current = paragraphIndexAt(event.clientY);
    const start = Math.min(rangeSelection.anchor, current);
    const end = Math.max(rangeSelection.anchor, current);
    selectedParagraphIndexes = Array.from(
      { length: end - start + 1 },
      (_, offset) => start + offset,
    );
    selectionAnchor = rangeSelection.anchor;
  }

  function finishRangeSelection(event?: MouseEvent) {
    if (!rangeSelection) return;
    if (rangeSelection.active) {
      event?.preventDefault();
      suppressParagraphClick = true;
      window.setTimeout(() => (suppressParagraphClick = false), 0);
      root.focus({ preventScroll: true });
    }
    rangeSelection = null;
  }
  async function handleParagraphBlur() {
    await tick();
    const active = document.activeElement;
    if (active instanceof HTMLTextAreaElement && root.contains(active)) return;
    if (root.contains(active)) finishEditing(false);
  }
  function caretOffsetTop(editor: HTMLTextAreaElement) {
    const style = getComputedStyle(editor);
    const mirror = document.createElement("div");
    Object.assign(mirror.style, {
      position: "fixed",
      left: "-10000px",
      top: "0",
      visibility: "hidden",
      boxSizing: "border-box",
      width: `${editor.clientWidth}px`,
      whiteSpace: "pre-wrap",
      overflowWrap: "break-word",
      wordBreak: style.wordBreak,
      font: style.font,
      letterSpacing: style.letterSpacing,
      lineHeight: style.lineHeight,
      padding: style.padding,
      border: "0",
    });
    mirror.textContent = editor.value.slice(0, editor.selectionStart);
    const marker = document.createElement("span");
    marker.textContent = "\u200b";
    mirror.append(marker);
    document.body.append(mirror);
    const top = marker.offsetTop;
    mirror.remove();
    return top;
  }
  function revealCaret(editor: HTMLTextAreaElement) {
    if (!editorViewport) return;
    const viewportBounds = editorViewport.getBoundingClientRect();
    const editorBounds = editor.getBoundingClientRect();
    const lineHeight =
      Number.parseFloat(getComputedStyle(editor).lineHeight) || 24;
    const caretTop = editorBounds.top + caretOffsetTop(editor);
    const margin = 10;
    if (caretTop + lineHeight > viewportBounds.bottom - margin) {
      editorViewport.scrollTop +=
        caretTop + lineHeight - viewportBounds.bottom + margin;
    } else if (caretTop < viewportBounds.top + margin) {
      editorViewport.scrollTop -= viewportBounds.top + margin - caretTop;
    }
  }
  async function focusParagraph(
    index: number,
    selectionStart: number,
    selectionEnd = selectionStart,
  ) {
    await tick();
    const editor = editors[index];
    if (!editor) return;
    activeParagraphIndex = index;
    resizeEditor(editor);
    editor.focus({ preventScroll: true });
    editor.setSelectionRange(selectionStart, selectionEnd);
    revealCaret(editor);
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
      selectedParagraphIndexes = [index + 1];
      selectionAnchor = index + 1;
      await focusParagraph(index + 1, 0);
    } else if (
      event.key === "Backspace" &&
      paragraphs[index] === "" &&
      editor.selectionStart === editor.selectionEnd &&
      paragraphs.length > 1
    ) {
      event.preventDefault();
      recordHistory(index, editor);
      paragraphs.splice(index, 1);
      emitText();
      const target = index > 0 ? index - 1 : 0;
      const caret = index > 0 ? paragraphs[target].length : 0;
      selectedParagraphIndexes = [target];
      selectionAnchor = target;
      await focusParagraph(target, caret);
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
      selectedParagraphIndexes = [index - 1];
      selectionAnchor = index - 1;
      await focusParagraph(index - 1, caret);
    } else if (
      event.key === "Delete" &&
      editor.selectionStart === editor.selectionEnd &&
      editor.selectionStart === paragraphs[index].length &&
      index < paragraphs.length - 1
    ) {
      event.preventDefault();
      recordHistory(index, editor);
      const caret = editor.selectionStart;
      paragraphs.splice(index, 2, paragraphs[index] + paragraphs[index + 1]);
      emitText();
      selectedParagraphIndexes = [index];
      selectionAnchor = index;
      await focusParagraph(index, caret);
    } else if (event.key === "Enter") {
      requestAnimationFrame(() => {
        resizeEditor(editor);
        revealCaret(editor);
      });
    }
  }
  async function insertParagraph(index: number) {
    if (!(await ensureEditing())) return;
    recordHistory(index, editors[index]);
    paragraphs.splice(index + 1, 0, "");
    closeParagraphMenu();
    emitText();
    selectedParagraphIndexes = [index + 1];
    selectionAnchor = index + 1;
    await focusParagraph(index + 1, 0);
  }
  async function deleteParagraph(index: number) {
    if (!(await ensureEditing())) return;
    recordHistory(index, editors[index]);
    const removed = selectedParagraphIndexes.includes(index)
      ? selectedParagraphIndexes
      : [index];
    const result = deleteParagraphs(paragraphs, removed);
    paragraphs = result.paragraphs;
    closeParagraphMenu();
    emitText();
    const target = result.selectedIndex;
    selectedParagraphIndexes = [target];
    selectionAnchor = target;
    await focusParagraph(target, 0);
  }

  function paragraphSelection(index: number) {
    const indexes = selectedParagraphIndexes.includes(index)
      ? selectedParagraphIndexes
      : [index];
    return selectedParagraphs(paragraphs, indexes);
  }

  function paragraphTransfer(index: number) {
    const values = paragraphSelection(index);
    return { paragraphs: values, text: paragraphPlainText(values) };
  }

  async function copyParagraphSelection(index: number) {
    const values = paragraphSelection(index);
    closeParagraphMenu();
    try {
      await copyParagraphs(values);
    } catch (cause) {
      makeToast({
        kind: "error",
        message:
          cause instanceof Error ? cause.message : "Could not copy paragraphs.",
      });
    }
  }

  function handleCopy(event: ClipboardEvent) {
    if (event.target instanceof HTMLInputElement) return;
    if (!event.clipboardData || !selectedParagraphIndexes.length) return;
    const active = document.activeElement;
    if (
      active instanceof HTMLTextAreaElement &&
      active === editors[activeParagraphIndex ?? -1]
    )
      return;
    event.preventDefault();
    writeParagraphClipboard(
      event.clipboardData,
      selectedParagraphs(paragraphs, selectedParagraphIndexes),
    );
  }

  async function pasteStructuredParagraphs(
    values: string[],
    editor: HTMLTextAreaElement | null,
  ) {
    const release = acquireStructuralEdit();
    if (!release) return;
    dispatch("bringToFront");
    try {
      if (
        editor &&
        editing &&
        activeParagraphIndex !== null &&
        editor === editors[activeParagraphIndex]
      ) {
        const index = activeParagraphIndex;
        const before = paragraphs[index].slice(0, editor.selectionStart);
        const after = paragraphs[index].slice(editor.selectionEnd);
        const inserted = [...values];
        inserted[0] = before + inserted[0];
        const last = inserted.length - 1;
        const caret = inserted[last].length;
        inserted[last] += after;
        const next = [...paragraphs];
        next.splice(index, 1, ...inserted);
        if (next.length > 500 || paragraphPlainText(next).length > 10_000) {
          makeToast({
            kind: "error",
            message: "The note is too large to paste.",
          });
          return;
        }
        recordHistory(index, editor);
        paragraphs = next;
        emitText();
        selectedParagraphIndexes = inserted.map((_, offset) => index + offset);
        selectionAnchor = index;
        await focusParagraph(index + last, caret);
        return;
      }

      const anchor = selectedParagraphIndexes.length
        ? Math.max(...selectedParagraphIndexes) + 1
        : paragraphs.length;
      const next = [...paragraphs];
      next.splice(anchor, 0, ...values);
      if (next.length > 500 || paragraphPlainText(next).length > 10_000) {
        makeToast({
          kind: "error",
          message: "The note is too large to paste.",
        });
        return;
      }
      recordHistory(Math.max(0, anchor - 1), editors[anchor - 1]);
      paragraphs = next;
      emitText();
      selectedParagraphIndexes = values.map((_, offset) => anchor + offset);
      selectionAnchor = anchor;
    } finally {
      release();
    }
  }

  function handlePaste(event: ClipboardEvent) {
    if (event.target instanceof HTMLInputElement) return;
    if (!event.clipboardData) return;
    const values = readParagraphClipboard(event.clipboardData);
    if (!values) return;
    event.preventDefault();
    const active = document.activeElement;
    void pasteStructuredParagraphs(
      values,
      active instanceof HTMLTextAreaElement ? active : null,
    );
  }

  function handleSelectStart(event: Event) {
    const target = event.target;
    if (target instanceof HTMLInputElement) return;
    if (
      target instanceof HTMLTextAreaElement &&
      editing &&
      activeParagraphIndex !== null &&
      target === editors[activeParagraphIndex]
    )
      return;
    event.preventDefault();
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
    if ((event as InputEvent).inputType === "insertLineBreak") {
      void tick().then(() => {
        resizeEditor(editor);
        revealCaret(editor);
      });
    }
  }
  function handleWindowMouseDown(event: MouseEvent) {
    if (!(event.target instanceof Node)) return;
    const insideParagraphMenu =
      paragraphMenuPanel?.contains(event.target) ?? false;
    const onParagraphMenuAnchor =
      paragraphMenuAnchor?.contains(event.target) ?? false;
    if (
      settingsOpen &&
      !settingsButton.contains(event.target) &&
      !settingsPanel?.contains(event.target)
    )
      setSettingsOpen(false);
    if (
      paragraphMenu !== null &&
      !insideParagraphMenu &&
      !onParagraphMenuAnchor
    )
      closeParagraphMenu();
    if (root && !root.contains(event.target) && !insideParagraphMenu) {
      finishEditing();
    }
  }
  function setSettingsOpen(open: boolean) {
    if (settingsOpen === open) return;
    settingsOpen = open;
    dispatch("floatingChange", open);
  }

  function handleMinimizedChange(minimized: boolean) {
    if (observedMinimized === minimized) return;
    observedMinimized = minimized;
    if (!minimized) return;
    closeParagraphMenu();
    setSettingsOpen(false);
    if (editing) finishEditing();
  }

  $: handleMinimizedChange(note.minimized);
  function handleWindowKeyDown(event: KeyboardEvent) {
    if (event.key !== "Escape" || paragraphMenu === null) return;
    event.preventDefault();
    event.stopPropagation();
    closeParagraphMenu(true);
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
  function startParagraphDrag(event: DragEvent, index: number) {
    if (!hasWriteAccess || !event.dataTransfer) {
      event.preventDefault();
      return;
    }
    const indexes = selectedParagraphIndexes.includes(index)
      ? [...selectedParagraphIndexes].sort((left, right) => left - right)
      : [index];
    selectedParagraphIndexes = indexes;
    selectionAnchor = index;
    draggingParagraphIndexes = indexes;
    const values = selectedParagraphs(paragraphs, indexes);
    const text = paragraphPlainText(values);
    closeParagraphMenu();
    event.dataTransfer.effectAllowed = "copyMove";
    event.dataTransfer.setData("text/plain", text);
    event.dataTransfer.setData(
      PARAGRAPH_CLIPBOARD_TYPE,
      serializeParagraphs(values),
    );
    const preview = document.createElement("div");
    preview.textContent =
      indexes.length > 1
        ? `${indexes.length} paragraphs · ${text.replace(/\s+/g, " ").trim()}`
        : text.replace(/\s+/g, " ").trim() || "Empty paragraph";
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
    dispatch("paragraphDragStart", {
      paragraphs: values,
      text,
      sourceNoteId: noteId,
      paragraphIndexes: indexes,
    });
  }
  function finishParagraphDrag() {
    draggingParagraphIndexes = [];
    dispatch("paragraphDragEnd");
  }
  function internalParagraphDropIndex(clientY: number) {
    const rows = Array.from(
      editorViewport.querySelectorAll<HTMLElement>(
        "[data-note-paragraph-index]",
      ),
    );
    for (const row of rows) {
      const bounds = row.getBoundingClientRect();
      if (clientY < bounds.top + bounds.height / 2) {
        return Number(row.dataset.noteParagraphIndex);
      }
    }
    return rows.length;
  }
  function handleInternalParagraphDrop(event: DragEvent) {
    if (!draggingParagraphIndexes.length) return;
    event.preventDefault();
    event.stopPropagation();
    const release = acquireStructuralEdit();
    if (!release) {
      finishParagraphDrag();
      return;
    }
    const targetIndex = internalParagraphDropIndex(event.clientY);
    const indexes = [...draggingParagraphIndexes].sort(
      (left, right) => left - right,
    );
    const reordered = reorderParagraphs(paragraphs, indexes, targetIndex);
    try {
      if (!sameParagraphs(reordered.paragraphs, paragraphs)) {
        recordHistory(indexes[0], editors[indexes[0]]);
        paragraphs = reordered.paragraphs;
        emitText();
        selectedParagraphIndexes = reordered.selectedIndexes;
        selectionAnchor = selectedParagraphIndexes[0] ?? null;
        movedParagraphIndexes = [...selectedParagraphIndexes];
        if (movedParagraphTimer !== null)
          window.clearTimeout(movedParagraphTimer);
        movedParagraphTimer = window.setTimeout(() => {
          movedParagraphIndexes = [];
          movedParagraphTimer = null;
        }, 220);
      }
    } finally {
      release();
      finishParagraphDrag();
    }
  }
  function handleFocusIn() {
    focused = true;
    dispatch("focus");
  }
  function handleFocusOut(event: FocusEvent) {
    if (
      event.relatedTarget instanceof Node &&
      (root.contains(event.relatedTarget) ||
        paragraphMenuPanel?.contains(event.relatedTarget))
    )
      return;
    focused = false;
    dispatch("blur");
  }

  function handleNoteKeyDown(event: KeyboardEvent) {
    const active = document.activeElement;
    if (active instanceof HTMLInputElement) return;
    if (
      active instanceof HTMLTextAreaElement &&
      editing &&
      activeParagraphIndex !== null &&
      active === editors[activeParagraphIndex]
    )
      return;
    const commandModifier = (event.ctrlKey || event.metaKey) && !event.altKey;
    if (commandModifier && event.key.toLowerCase() === "a") {
      event.preventDefault();
      selectedParagraphIndexes = paragraphs.map((_, index) => index);
      selectionAnchor = 0;
    } else if (
      (event.key === "Delete" || event.key === "Backspace") &&
      selectedParagraphIndexes.length
    ) {
      event.preventDefault();
      void deleteParagraph(selectedParagraphIndexes[0]);
    }
  }
</script>

<svelte:window
  on:mousedown|capture={handleWindowMouseDown}
  on:mousemove={updateRangeSelection}
  on:mouseup={finishRangeSelection}
  on:keydown|capture={handleWindowKeyDown}
  on:resize={positionParagraphMenu}
/>

<article
  bind:this={root}
  role="presentation"
  tabindex="-1"
  data-canvas-note
  data-canvas-note-id={noteId}
  class="note-container relative overflow-visible rounded-lg border border-white/15 shadow-xl shadow-black/40"
  class:fullscreen
  class:minimized={note.minimized}
  class:focused
  class:editing-active={focused && editing}
  class:linked-highlight={linkedHighlight}
  class:linked-from-terminal={linkedHighlight &&
    linkedHighlightSource === "terminal"}
  style:width={fullscreen ? "100%" : `${note.width}px`}
  style:height={fullscreen
    ? "100%"
    : note.minimized
      ? `${MINIMIZED_WINDOW_HEIGHT}px`
      : `${note.height}px`}
  style:background={note.background}
  on:mousedown={() => {
    root.focus({ preventScroll: true });
    dispatch("bringToFront");
  }}
  on:focusin={handleFocusIn}
  on:focusout={handleFocusOut}
  on:pointerdown={(event) => event.stopPropagation()}
  on:copy={handleCopy}
  on:paste={handlePaste}
  on:selectstart={handleSelectStart}
  on:keydown={handleNoteKeyDown}
  on:wheel={(event) => {
    if (!event.ctrlKey) event.stopPropagation();
  }}
>
  <header
    role="presentation"
    data-canvas-titlebar
    class="note-titlebar flex h-9 cursor-default select-none items-center justify-between rounded-t-lg border-b border-white/10 bg-black/15 px-2"
    class:cursor-default={fullscreen}
    on:mousedown|stopPropagation={(event) => {
      dispatch("bringToFront");
      if (event.button === 0 && hasWriteAccess && !fullscreen)
        dispatch("startMove", event);
    }}
  >
    <CircleButtons>
      <CircleButton
        kind="red"
        disabled={!hasWriteAccess}
        ariaLabel="Close note"
        on:mousedown={(event) => event.button === 0 && dispatch("close")}
      />
      <CircleButton
        kind="yellow"
        active={note.minimized}
        disabled={!hasWriteAccess}
        ariaLabel={note.minimized ? "Restore note" : "Minimize note"}
        on:mousedown={(event) =>
          event.button === 0 && update({ minimized: !note.minimized })}
      />
      <CircleButton
        kind="purple"
        active={fullscreen}
        disabled={note.minimized}
        ariaLabel={fullscreen ? "Exit full screen" : "Full screen"}
        on:mousedown={(event) =>
          event.button === 0 && dispatch("toggleFullscreen")}
      />
    </CircleButtons>
    <div class="min-w-0 flex-[4] text-center">
      <div class="mx-auto max-w-48 text-xs font-medium text-zinc-200/75">
        <InlineTitle
          value={note.title}
          fallback={`Note #${noteId}`}
          disabled={!hasWriteAccess}
          ariaLabel="Note title"
          on:change={(event) => update({ title: event.detail })}
        />
      </div>
      {#if focused}<span
          class="note-title-status block max-w-40 truncate text-[10px] text-zinc-300/65"
          >{editing
            ? "Editing paragraph"
            : selectedParagraphIndexes.length > 1
              ? `${selectedParagraphIndexes.length} paragraphs selected`
              : "Selected"}</span
        >
      {:else if editingBy !== null && editingBy !== userId}<span
          class="note-title-status block max-w-36 truncate text-[10px] text-zinc-300/60"
          >{editingName || `User ${editingBy}`} is editing</span
        >{/if}
    </div>
    <button
      bind:this={settingsButton}
      type="button"
      class="rounded p-1 text-zinc-200/70 hover:bg-white/10 hover:text-white"
      aria-label="Note appearance"
      on:mousedown|stopPropagation={(event) =>
        event.button === 0 && setSettingsOpen(!settingsOpen)}
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
      <BackgroundPicker
        value={note.background}
        disabled={!hasWriteAccess}
        on:change={(event) => update({ background: event.detail })}
      />
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
    bind:this={editorViewport}
    role="presentation"
    class="note-editor h-[calc(100%-2.25rem)] overflow-y-auto py-3 pr-3"
    class:editing
    class:block-selecting={rangeSelection?.active}
    on:mousedown|stopPropagation
    on:scroll={positionParagraphMenu}
    on:drop={handleInternalParagraphDrop}
  >
    {#each paragraphs as paragraph, index (index)}
      <div
        class="paragraph-row group relative flex min-h-8 items-start pl-6 pr-7"
        class:selected={selectedParagraphIndexes.includes(index)}
        class:dragging={draggingParagraphIndexes.includes(index)}
        class:moved={movedParagraphIndexes.includes(index)}
        data-note-paragraph-index={index}
      >
        {#if paragraphDropIndex === index}<div
            class="paragraph-drop-line"
            aria-hidden="true"
          ></div>{/if}
        <button
          type="button"
          class="paragraph-marker"
          class:active={paragraphMenu === index &&
            paragraphMenuKind === "actions"}
          class:selected={selectedParagraphIndexes.includes(index)}
          aria-label={`Paragraph ${index + 1} actions`}
          aria-pressed={selectedParagraphIndexes.includes(index)}
          title="Select paragraph · Ctrl/Cmd-click to toggle · Shift-click for a range · Drag to move or copy"
          draggable={hasWriteAccess}
          disabled={!hasWriteAccess}
          on:mousedown|stopPropagation={(event) =>
            prepareParagraphDrag(index, event)}
          on:click|stopPropagation={(event) => selectParagraph(index, event)}
          on:dragstart|stopPropagation={(event) =>
            startParagraphDrag(event, index)}
          on:dragend={finishParagraphDrag}
          ><span></span><span></span><span></span><span></span></button
        >
        <button
          type="button"
          class="paragraph-send"
          class:active={paragraphMenu === index && paragraphMenuKind === "send"}
          aria-label={`Send paragraph ${index + 1}`}
          title={selectedParagraphIndexes.length > 1 &&
          selectedParagraphIndexes.includes(index)
            ? `Send ${selectedParagraphIndexes.length} selected paragraphs`
            : "Send paragraph"}
          disabled={!hasWriteAccess}
          on:mousedown|stopPropagation
          on:click|stopPropagation={(event) =>
            openParagraphSendMenu(index, event)}><SendIcon /></button
        >
        {#if paragraphMenu === index}
          <div
            bind:this={paragraphMenuPanel}
            role="presentation"
            class="paragraph-menu"
            class:positioned={paragraphMenuPositioned}
            style:left={`${paragraphMenuLeft}px`}
            style:top={`${paragraphMenuTop}px`}
            use:portal
            on:mousedown|stopPropagation
          >
            {#if paragraphMenuKind === "actions"}
              <button
                type="button"
                disabled={editingBy !== null && editingBy !== userId}
                on:click={() => deleteParagraph(index)}
                ><Trash2Icon />Delete{selectedParagraphIndexes.length > 1
                  ? ` ${selectedParagraphIndexes.length} paragraphs`
                  : ""}</button
              >
              <button
                type="button"
                on:click={() => copyParagraphSelection(index)}
                ><CopyIcon />Copy{selectedParagraphIndexes.length > 1
                  ? ` ${selectedParagraphIndexes.length} paragraphs`
                  : ""}</button
              >
              <button
                type="button"
                disabled={editingBy !== null && editingBy !== userId}
                on:click={() => insertParagraph(index)}
                ><PlusIcon />New paragraph below</button
              >
            {:else}
              <button type="button" on:click={() => sendParagraph(index, "all")}
                ><SendIcon />Send to all linked</button
              >
              <button
                type="button"
                on:click={() => sendParagraph(index, "notes")}
                ><FileTextIcon />Send to linked notes</button
              >
              <button
                type="button"
                on:click={() => sendParagraph(index, "terminals")}
                ><TerminalIcon />Send to linked terminals</button
              >
              <button
                type="button"
                on:click={() => sendParagraph(index, "terminals-execute")}
                ><PlayIcon />Send to terminals &amp; run</button
              >
              <button
                type="button"
                on:click={() => sendParagraph(index, "files")}
                ><Edit3Icon />Send to file editors</button
              >
            {/if}
          </div>
        {/if}
        <textarea
          bind:this={editors[index]}
          value={paragraph}
          rows="1"
          readonly={!editing || activeParagraphIndex !== index}
          aria-label={`Note paragraph ${index + 1}`}
          placeholder={!editing && paragraphs.length === 1 && !paragraph
            ? editingBy !== null && editingBy !== userId
              ? `${editingName || `User ${editingBy}`} is editing this note`
              : "Click to edit this note"
            : ""}
          class="paragraph-input"
          class:text-editing={editing && activeParagraphIndex === index}
          use:autoResizeParagraph={paragraph}
          on:mousedown={(event) => startRangeSelection(index, event)}
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
    <div class="paragraph-hint" aria-hidden="true">
      Drag across paragraphs to select · Drag a selected handle to move ·
      Ctrl/Cmd+Enter adds a paragraph
    </div>
  </div>
  <footer class="note-relations">
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
    disabled={!hasWriteAccess || note.minimized || fullscreen}
    on:start={(event) => dispatch("startResize", event.detail)}
  />
</article>

<style lang="postcss">
  @reference "../../app.css";
  .note-container.focused {
    border-color: rgb(228 228 231 / 88%);
  }
  .note-container.editing-active {
    border-color: rgb(186 230 253 / 92%);
    box-shadow: 0 8px 24px rgb(0 0 0 / 35%);
  }
  .note-container.editing-active > header {
    background: rgb(125 211 252 / 10%);
  }
  .note-container {
    @apply flex flex-col;
  }
  .note-container.minimized > :not(.note-titlebar):not(.panel) {
    display: none;
  }
  .note-container.minimized .note-titlebar {
    height: 100%;
    border-radius: 0.45rem;
  }
  .note-container.minimized .note-title-status {
    display: none;
  }
  .note-container.linked-highlight {
    border-color: rgb(125 211 252 / 80%);
    animation: linked-note-pulse 1.8s ease-in-out infinite;
  }
  .note-container.linked-highlight.linked-from-terminal {
    border-color: rgb(129 140 248 / 50%);
    animation-name: linked-note-from-terminal-pulse;
  }
  .note-container.fullscreen {
    display: flex;
    flex-direction: column;
  }
  .paragraph-input {
    @apply block min-h-7 w-full resize-none overflow-hidden bg-transparent px-2 py-1 text-sm leading-6 text-zinc-100 outline-none placeholder:text-zinc-300/40;
    user-select: none;
  }
  .paragraph-input.text-editing {
    user-select: text;
  }
  .note-editor.editing .paragraph-input:focus {
    @apply rounded bg-white/[0.035];
  }
  .note-editor.block-selecting .paragraph-input {
    cursor: crosshair;
  }
  .paragraph-marker {
    @apply absolute left-1.5 top-2 grid h-4 w-4 cursor-grab grid-cols-[repeat(2,2px)] grid-rows-[repeat(2,2px)] place-content-center gap-x-[1.5px] gap-y-[3px] rounded text-zinc-300 opacity-35 transition-opacity hover:bg-white/10 hover:text-zinc-100 hover:opacity-100 active:cursor-grabbing disabled:pointer-events-none;
  }
  .paragraph-row {
    @apply mx-1 rounded-md border border-white/[0.055] bg-black/[0.06] transition-[border-color,background-color,opacity,transform] duration-150 hover:border-white/10 hover:bg-white/[0.035];
  }
  .paragraph-row.selected {
    @apply border-sky-300/30 bg-sky-300/[0.07];
  }
  .paragraph-row.dragging {
    @apply scale-[0.99] opacity-45;
  }
  .paragraph-row.moved {
    animation: paragraph-settle 220ms ease-out;
  }
  .paragraph-row + .paragraph-row {
    @apply mt-1.5;
  }
  .paragraph-hint {
    @apply pointer-events-none mx-3 mt-2 select-none text-[10px] text-zinc-300/35;
    user-select: none;
  }
  .paragraph-row:hover .paragraph-marker,
  .paragraph-marker.active {
    @apply opacity-100;
  }
  .paragraph-marker span {
    @apply h-0.5 w-0.5 rounded-full bg-current;
  }
  .paragraph-marker.selected {
    @apply bg-sky-300/15 text-sky-100 opacity-100;
  }
  .paragraph-send {
    @apply absolute right-1.5 top-1.5 z-10 inline-flex h-5 w-5 items-center justify-center rounded text-zinc-300 opacity-0 transition-[background-color,color,opacity] hover:bg-sky-300/15 hover:text-sky-100 focus-visible:bg-sky-300/15 focus-visible:text-sky-100 focus-visible:opacity-100 focus-visible:outline-none disabled:pointer-events-none;
  }
  .paragraph-row:hover .paragraph-send,
  .paragraph-send.active {
    @apply opacity-100;
  }
  .paragraph-send :global(svg) {
    @apply h-3.5 w-3.5;
  }
  .paragraph-menu {
    position: fixed;
    z-index: 1000;
    visibility: hidden;
    width: 15rem;
    overflow: hidden;
    border: 1px solid rgb(63 63 70);
    border-radius: 0.375rem;
    background: rgb(24 24 27 / 98%);
    padding: 0.25rem;
    box-shadow: 0 20px 25px -5px rgb(0 0 0 / 35%);
  }
  .paragraph-menu.positioned {
    visibility: visible;
  }
  .paragraph-menu button {
    @apply flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs;
    color: rgb(212 212 216);
  }
  .paragraph-menu button:hover {
    background: rgb(39 39 42);
    color: white;
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
    @apply flex h-9 shrink-0 items-center justify-end gap-2 rounded-b-lg border-t border-white/10 bg-black/15 px-2;
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
  @keyframes linked-note-from-terminal-pulse {
    0%,
    100% {
      box-shadow: 0 0 2px rgb(129 140 248 / 6%);
    }
    50% {
      box-shadow:
        0 0 10px rgb(129 140 248 / 55%),
        0 0 18px rgb(129 140 248 / 34%);
    }
  }
  @keyframes paragraph-settle {
    from {
      transform: translateY(5px);
      background-color: rgb(125 211 252 / 0.14);
    }
    to {
      transform: translateY(0);
    }
  }
</style>
