<script lang="ts">
  import { onMount, tick } from "svelte";
  import type { ITheme, Terminal } from "@xterm/xterm";
  import { TerminalIcon, SendIcon } from "svelte-feather-icons";
  import {
    mobileTerminalThemeCss,
    MOBILE_INPUT_CHARACTERS,
    watchTerminalText,
    type TerminalTextSnapshot,
  } from "$lib/mobileTerminalText";

  export let terminal: Terminal;
  export let theme: ITheme = {};
  export let writable: boolean;
  export let blocked = "";
  export let send: (text: string) => boolean;

  let snapshot: TerminalTextSnapshot = {
    text: "",
    runs: [],
    simplified: false,
    truncated: false,
    alternate: false,
  };
  let reader: HTMLDivElement;
  let draft = "";
  let selected = false;
  let follow = true;
  let interacting = false;
  let syncPause = () => {};
  function resume() {
    reader.ownerDocument.getSelection()?.removeAllRanges();
    follow = true;
    reader.scrollTop = reader.scrollHeight;
    syncPause();
  }

  onMount(() => {
    let alive = true;
    const doc = reader.ownerDocument;
    const observer = watchTerminalText(terminal, async (next) => {
      const scroll = reader.scrollTop;
      snapshot = next;
      await tick();
      if (!alive) return;
      reader.scrollTop = follow ? reader.scrollHeight : scroll;
    });
    syncPause = () => {
      if (doc.hidden) interacting = false;
      const selection = doc.getSelection();
      selected =
        !!selection &&
        !selection.isCollapsed &&
        (reader.contains(selection.anchorNode) ||
          reader.contains(selection.focusNode));
      observer.setPaused(doc.hidden || selected || interacting || !follow);
    };
    const release = () => {
      interacting = false;
      syncPause();
    };
    const resize = new ResizeObserver(() => {
      if (follow && !selected && !interacting)
        reader.scrollTop = reader.scrollHeight;
    });
    resize.observe(reader);
    doc.addEventListener("selectionchange", syncPause);
    doc.addEventListener("visibilitychange", syncPause);
    window.addEventListener("pointerup", release);
    window.addEventListener("pointercancel", release);
    syncPause();
    return () => {
      alive = false;
      observer.dispose();
      resize.disconnect();
      doc.removeEventListener("selectionchange", syncPause);
      doc.removeEventListener("visibilitychange", syncPause);
      window.removeEventListener("pointerup", release);
      window.removeEventListener("pointercancel", release);
      syncPause = () => {};
    };
  });
</script>

<section
  class="mobile-terminal-reader"
  style={mobileTerminalThemeCss(theme)}
  aria-label="Terminal text and input"
>
  <div class="status">
    <span class="terminal-icon" aria-hidden="true"
      ><TerminalIcon size="14" /></span
    >
    <span
      >{selected
        ? "Selection paused text updates"
        : !follow
          ? "Reading earlier output · Latest to resume"
          : snapshot.alternate
            ? "Current application screen · not a transcript"
            : "Terminal text"}{snapshot.truncated
        ? " · Recent output only"
        : ""}{snapshot.simplified ? " · Older styling simplified" : ""}</span
    >
    {#if selected || !follow}<button type="button" on:click={resume}
        >Latest</button
      >{/if}
  </div>
  <div
    bind:this={reader}
    class="output"
    data-mobile-terminal-reader
    role="textbox"
    aria-readonly="true"
    aria-multiline="true"
    aria-label="Terminal output; hold text to select and copy"
    tabindex="0"
    on:contextmenu|stopPropagation
    on:pointerdown={() => {
      interacting = true;
      syncPause();
    }}
    on:scroll={() => {
      follow =
        reader.scrollHeight - reader.scrollTop - reader.clientHeight < 32;
      syncPause();
    }}
  >
    {#each snapshot.runs as run}<span style={run.style}
        >{snapshot.text.slice(run.start, run.end)}</span
      >{/each}
  </div>
  <form
    on:submit|preventDefault={() => {
      if (
        !writable ||
        blocked ||
        !draft.length ||
        draft.length > MOBILE_INPUT_CHARACTERS
      )
        return;
      if (send(draft)) draft = "";
    }}
  >
    {#if blocked}<p role="status">{blocked}</p>{:else if !writable}<p>
        Read-only access
      </p>{/if}
    <div class="composer">
      <span class="prompt" aria-hidden="true">❯</span>
      <textarea
        aria-label="Terminal input"
        placeholder="Message or command…"
        rows="2"
        maxlength={MOBILE_INPUT_CHARACTERS}
        bind:value={draft}
        disabled={!writable}
        autocapitalize="off"
        autocomplete="off"
        spellcheck="false"></textarea>
      <button
        type="submit"
        disabled={!writable || !!blocked || !draft.length}
        aria-label="Send text and Enter"
        title="Send text and Enter"
        ><SendIcon size="18" /><span>Send</span></button
      >
    </div>
    <p>
      Enter adds a line · Send pastes text then presses Enter · {draft.length}/{MOBILE_INPUT_CHARACTERS}
    </p>
  </form>
</section>

<style>
  .mobile-terminal-reader {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
    --reader-muted: color-mix(in srgb, var(--reader-fg) 65%, var(--reader-bg));
    --reader-border: color-mix(in srgb, var(--reader-fg) 18%, var(--reader-bg));
    --reader-subtle: color-mix(in srgb, var(--reader-fg) 5%, var(--reader-bg));
    background: var(--reader-bg);
    color: var(--reader-fg);
  }
  .status {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 34px;
    padding: 5px 14px;
    gap: 8px;
    font-size: 11px;
    color: var(--reader-muted);
    background: var(--reader-subtle);
    border-bottom: 1px solid var(--reader-border);
  }
  .status > span:not(.terminal-icon) {
    flex: 1;
  }
  .status :global(svg) {
    flex-shrink: 0;
  }
  .output :global(::selection) {
    background: var(--reader-selection);
    color: var(--reader-selection-fg);
  }
  .output {
    flex: 1;
    min-height: 0;
    overflow: auto;
    overscroll-behavior: contain;
    touch-action: pan-y;
    user-select: text;
    -webkit-user-select: text;
    -webkit-touch-callout: default;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-family: "Fira Code VF", ui-monospace, monospace;
    font-variant-ligatures: none;
    font-size: 14px;
    line-height: 1.55;
    padding: 12px 14px 18px;
    scrollbar-width: thin;
    scrollbar-color: var(--reader-border) transparent;
    outline: none;
  }
  form {
    flex-shrink: 0;
    padding: 10px 12px 8px;
    border-top: 1px solid var(--reader-border);
    background: var(--reader-subtle);
  }
  .composer {
    display: flex;
    gap: 8px;
    align-items: stretch;
    padding: 8px;
    border: 1px solid var(--reader-border);
    border-radius: 10px;
    background: var(--reader-bg);
  }
  .composer:focus-within {
    border-color: var(--reader-cursor);
  }
  .prompt {
    padding: 5px 0 0 2px;
    color: var(--reader-cursor);
    font-family: monospace;
    user-select: none;
  }
  textarea {
    flex: 1;
    min-width: 0;
    max-height: 120px;
    resize: none;
    padding: 4px 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--reader-fg);
    caret-color: var(--reader-cursor);
    font-family: "Fira Code VF", ui-monospace, monospace;
    font-size: 16px;
  }
  button {
    min-height: 36px;
    padding: 4px 10px;
    border-radius: 8px;
    color: var(--reader-fg);
    background: var(--reader-subtle);
  }
  .composer button {
    min-height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    align-self: center;
    color: var(--reader-bg);
    background: var(--reader-fg);
    font-size: 12px;
  }
  button:disabled {
    opacity: 0.45;
  }
  button:focus-visible {
    outline: 2px solid var(--reader-cursor);
    outline-offset: 2px;
  }
  textarea::placeholder {
    color: var(--reader-muted);
  }
  p {
    margin-top: 4px;
    font-size: 10px;
    color: var(--reader-muted);
  }
</style>
