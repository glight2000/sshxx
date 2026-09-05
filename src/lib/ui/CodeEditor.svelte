<script lang="ts" context="module">
  export type TextInsertResult = { ok: boolean; message?: string };
  export type TextInsertPosition = { x: number; y: number };
</script>

<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { basicSetup, EditorView } from "codemirror";
  import { languages } from "@codemirror/language-data";

  export let value: string;
  export let filename: string;
  export let readOnly = false;
  export let onChange: (value: string) => void;
  export let insertText: (
    text: string,
    position?: TextInsertPosition,
  ) => TextInsertResult;
  export let previewTextDrop: (position: TextInsertPosition) => boolean;
  export let cancelTextDropPreview: () => void;

  let host: HTMLDivElement;
  let view: EditorView | null = null;
  let appliedValue = value;
  let dropPreviewActive = false;
  let dropPreviewSelection: { anchor: number; head: number } | null = null;

  function insertAtSelection(
    text: string,
    position?: TextInsertPosition,
  ): TextInsertResult {
    if (!view || readOnly)
      return { ok: false, message: "The text editor is not writable." };
    const point = position ? view.posAtCoords(position) : null;
    const selection = view.state.selection.main;
    const from = point ?? selection.from;
    const to = point ?? selection.to;
    dropPreviewActive = false;
    dropPreviewSelection = null;
    view.dispatch({
      changes: { from, to, insert: text },
      selection: { anchor: from + text.length },
      scrollIntoView: true,
    });
    view.focus();
    return { ok: true };
  }

  function previewAt(position: TextInsertPosition) {
    if (!view || readOnly) return false;
    const point = view.posAtCoords(position);
    if (point === null) return false;
    if (!dropPreviewSelection) {
      const selection = view.state.selection.main;
      dropPreviewSelection = {
        anchor: selection.anchor,
        head: selection.head,
      };
    }
    dropPreviewActive = true;
    view.dispatch({ selection: { anchor: point }, scrollIntoView: true });
    view.focus();
    return true;
  }

  function cancelPreview() {
    if (view && dropPreviewSelection) {
      view.dispatch({ selection: dropPreviewSelection });
    }
    dropPreviewSelection = null;
    dropPreviewActive = false;
  }

  $: if (view && value !== appliedValue) {
    view.dispatch({
      changes: { from: 0, to: view.state.doc.length, insert: value },
    });
    appliedValue = value;
  }

  onMount(async () => {
    const extensions: any[] = [
      basicSetup,
      EditorView.editable.of(!readOnly),
      EditorView.theme({
        "&": {
          height: "100%",
          backgroundColor: "var(--control-bg)",
          color: "var(--control-text)",
        },
        ".cm-scroller": {
          overflow: "auto",
          fontFamily: "Fira Code VF, monospace",
        },
        ".cm-gutters": {
          backgroundColor: "var(--surface-bg)",
          color: "var(--surface-muted)",
          border: "none",
        },
        ".cm-activeLine, .cm-activeLineGutter": {
          backgroundColor: "var(--surface-subtle)",
        },
        ".cm-cursor, .cm-dropCursor": {
          borderLeftColor: "var(--control-text)",
        },
        "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection":
          {
            backgroundColor: "var(--surface-selection)",
          },
        ".cm-panels, .cm-tooltip": {
          backgroundColor: "var(--app-surface-solid)",
          color: "var(--app-text)",
          borderColor: "var(--surface-border)",
        },
      }),
      EditorView.updateListener.of((update) => {
        if (!update.docChanged) return;
        appliedValue = update.state.doc.toString();
        onChange(appliedValue);
      }),
    ];
    const description = languages.find((candidate) =>
      candidate.filename ? candidate.filename.test(filename) : false,
    );
    if (description) {
      try {
        extensions.push(await description.load());
      } catch (error) {
        console.warn(`Could not load syntax support for ${filename}.`, error);
      }
    }
    view = new EditorView({ doc: value, extensions, parent: host });
    insertText = insertAtSelection;
    previewTextDrop = previewAt;
    cancelTextDropPreview = cancelPreview;
  });

  onDestroy(() => {
    view?.destroy();
    insertText = () => ({ ok: false, message: "The text editor is closed." });
    previewTextDrop = () => false;
    cancelTextDropPreview = () => {};
  });
</script>

<div
  class="h-full min-h-0 overflow-hidden"
  class:drop-preview={dropPreviewActive}
  bind:this={host}
  on:wheel={(event) => {
    if (!event.ctrlKey) event.stopPropagation();
  }}
></div>

<style>
  div :global(.cm-editor) {
    height: 100%;
  }
  div.drop-preview :global(.cm-editor) {
    outline: 2px solid rgb(125 211 252 / 70%);
    outline-offset: -2px;
  }
  div.drop-preview :global(.cm-cursor) {
    border-left-color: #bae6fd;
    border-left-width: 2px;
    filter: drop-shadow(0 0 4px rgb(125 211 252 / 90%));
  }
</style>
