<!-- @component Interactive terminal rendered with xterm.js -->
<script lang="ts" context="module">
  import { makeToast } from "$lib/toast";

  // Deduplicated terminal font loading.
  const waitForFonts = (() => {
    let state: "initial" | "loading" | "loaded" = "initial";
    const waitlist: (() => void)[] = [];

    return async function waitForFonts() {
      if (state === "loaded") return;
      else if (state === "initial") {
        const FontFaceObserver = (await import("fontfaceobserver")).default;
        state = "loading";
        try {
          await new FontFaceObserver("Fira Code VF").load();
        } catch (error) {
          makeToast({
            kind: "error",
            message: "Could not load terminal font.",
          });
        }
        state = "loaded";
        for (const fn of waitlist) fn();
      } else {
        await new Promise<void>((resolve) => {
          if (state === "loaded") resolve();
          else waitlist.push(resolve);
        });
      }
    };
  })();
</script>

<script lang="ts">
  import { browser } from "$app/environment";

  import { createEventDispatcher, onDestroy, onMount } from "svelte";
  import type { Terminal } from "@xterm/xterm";
  import { Buffer } from "buffer";
  import { FolderIcon, SettingsIcon } from "svelte-feather-icons";

  import themes, { isThemeName, type ThemeName } from "./themes";
  import BackgroundPicker from "./BackgroundPicker.svelte";
  import CanvasRelations, {
    type CanvasRelationItem,
  } from "./CanvasRelations.svelte";
  import CircleButton from "./CircleButton.svelte";
  import CircleButtons from "./CircleButtons.svelte";
  import InlineTitle from "./InlineTitle.svelte";
  import { settings } from "$lib/settings";
  import { parseOsc7Location } from "$lib/terminalLocation";
  import { TypeAheadAddon } from "$lib/typeahead";
  import { installXtermMouseCoordinateAdapter } from "$lib/xtermMouseCoordinates";

  /** Used to determine Cmd versus Ctrl keyboard shortcuts. */
  const isMac = browser && navigator.platform.startsWith("Mac");

  const dispatch = createEventDispatcher<{
    data: Uint8Array;
    uploadImage: File;
    close: void;
    duplicate: {
      workingDirectory: string;
      workingDirectoryHost: string;
      initialWorkingDirectoryHost: string;
    };
    toggleFullscreen: void;
    openFiles: string;
    appearance: {
      title: string;
      background: string;
      opacity: number;
      theme: string;
    };
    title: string;
    bringToFront: void;
    startMove: MouseEvent;
    focus: void;
    blur: void;
    navigateNote: CanvasRelationItem;
    unlinkNote: CanvasRelationItem;
    floatingChange: boolean;
  }>();

  const typeahead = new TypeAheadAddon();

  export let rows: number, cols: number;
  export let windowWidth = 0;
  export let windowHeight = 0;
  export let title = "";
  export let background = "";
  export let colorTheme = "";
  export let opacity = 80;
  export let hasWriteAccess: boolean | undefined;
  export let fullscreen = false;
  export let linkedNotes: CanvasRelationItem[] = [];
  export let linkedHighlight = false;
  export let paragraphDropActive = false;
  export let write: (data: string, replay?: boolean) => void; // bound function prop
  export let sendText: (data: string, execute?: boolean) => void;

  export let termEl: HTMLDivElement = null as any; // suppress "missing prop" warning
  let term: Terminal | null = null;
  let mouseCoordinateAdapter: { dispose(): void } | null = null;

  let legacyTheme = $settings.theme;
  let previewTheme: ThemeName | null = null;
  let pendingTheme: ThemeName | null = null;
  let draftTheme: ThemeName = legacyTheme;
  let themeMenuOpen = false;
  $: persistedTheme = isThemeName(colorTheme) ? colorTheme : legacyTheme;
  $: activeThemeName = previewTheme ?? pendingTheme ?? persistedTheme;
  $: theme = themes[activeThemeName];
  $: terminalTheme = { ...theme, background: background || theme.background };
  $: if (pendingTheme !== null && colorTheme === pendingTheme) {
    pendingTheme = null;
  }

  $: if (term) {
    // If the theme changes, update existing terminals' appearance.
    term.options.theme = terminalTheme;
    term.options.scrollback = $settings.scrollback;
  }

  let loaded = false;
  let focused = false;
  let titleEditing = false;
  let currentTitle = "Remote Terminal";
  let appearanceOpen = false;
  let appearanceButton: HTMLButtonElement;
  let appearancePanel: HTMLDivElement;
  let attention = false;
  let imageDragging = false;
  let suppressAttention = 0;
  let suppressInput = 0;
  let workingDirectory = ".";
  let workingDirectoryHost = "";
  let initialWorkingDirectoryHost = "";
  function requestAttention() {
    if (suppressAttention === 0 && !focused) attention = true;
    return true;
  }

  function updateAppearance(values: {
    title?: string;
    background?: string;
    opacity?: number;
    theme?: string;
  }) {
    dispatch("appearance", {
      title,
      background,
      opacity,
      theme: pendingTheme ?? persistedTheme,
      ...values,
    });
  }

  function openThemeMenu() {
    draftTheme = activeThemeName;
    previewTheme = null;
    themeMenuOpen = true;
  }

  function closeThemeMenu() {
    previewTheme = null;
    draftTheme = pendingTheme ?? persistedTheme;
    themeMenuOpen = false;
  }

  function selectTheme(themeName: ThemeName) {
    draftTheme = themeName;
    pendingTheme = themeName;
    previewTheme = themeName;
    updateAppearance({ theme: themeName });
  }

  function closeAppearanceOnOutsideClick(event: MouseEvent) {
    if (!appearanceOpen || !(event.target instanceof Node)) return;
    if (
      appearanceButton.contains(event.target) ||
      appearancePanel?.contains(event.target)
    ) {
      return;
    }
    closeThemeMenu();
    setAppearanceOpen(false);
  }

  function setAppearanceOpen(open: boolean) {
    if (appearanceOpen === open) return;
    appearanceOpen = open;
    dispatch("floatingChange", open);
  }

  function setFocused(isFocused: boolean) {
    if (isFocused && !focused) {
      focused = isFocused;
      attention = false;
      dispatch("focus");
    } else if (!isFocused && focused) {
      focused = isFocused;
      dispatch("blur");
    }
  }

  const supportedImageTypes = new Set([
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
  ]);
  const maxImageBytes = 20 << 20;

  function uploadImage(file: File) {
    if (!hasWriteAccess) {
      makeToast({ kind: "error", message: "No write permission." });
      return;
    }
    if (!supportedImageTypes.has(file.type)) {
      makeToast({
        kind: "error",
        message: "Paste a PNG, JPEG, WebP, or GIF image.",
      });
      return;
    }
    if (file.size === 0 || file.size > maxImageBytes) {
      makeToast({
        kind: "error",
        message: "Images must be between 1 byte and 20 MiB.",
      });
      return;
    }
    dispatch("uploadImage", file);
  }

  function handlePaste(event: ClipboardEvent) {
    if (event.target instanceof HTMLInputElement) return;
    const file = Array.from(event.clipboardData?.items ?? [])
      .find(
        (item) => item.kind === "file" && supportedImageTypes.has(item.type),
      )
      ?.getAsFile();
    if (!file) return;
    event.preventDefault();
    event.stopPropagation();
    uploadImage(file);
  }

  function handleDragOver(event: DragEvent) {
    if (
      !Array.from(event.dataTransfer?.items ?? []).some((item) =>
        supportedImageTypes.has(item.type),
      )
    ) {
      return;
    }
    event.preventDefault();
    imageDragging = true;
    if (event.dataTransfer) event.dataTransfer.dropEffect = "copy";
  }

  function handleDragLeave(event: DragEvent) {
    if (
      event.relatedTarget instanceof Node &&
      event.currentTarget instanceof Node &&
      event.currentTarget.contains(event.relatedTarget)
    ) {
      return;
    }
    imageDragging = false;
  }

  function handleDrop(event: DragEvent) {
    imageDragging = false;
    const file = Array.from(event.dataTransfer?.files ?? []).find((candidate) =>
      supportedImageTypes.has(candidate.type),
    );
    if (!file) return;
    event.preventDefault();
    event.stopPropagation();
    uploadImage(file);
  }

  type PendingWrite = {
    parts: string[];
    length: number;
    replay: boolean;
  };
  const pendingWrites: PendingWrite[] = [];
  const maxCombinedWriteBytes = 256 << 10;
  let writeInProgress = false;

  function flushPendingWrites() {
    if (!term || writeInProgress || pendingWrites.length === 0) return;
    const next = pendingWrites.shift()!;
    let data = next.parts.length === 1 ? next.parts[0] : next.parts.join("");
    if (!next.replay) data = typeahead.onBeforeProcessData(data);
    if (!data) {
      flushPendingWrites();
      return;
    }

    writeInProgress = true;
    if (next.replay) {
      suppressAttention += 1;
      suppressInput += 1;
      typeahead.beginInputSuppression();
    }
    const complete = () => {
      if (next.replay) {
        suppressAttention -= 1;
        suppressInput -= 1;
        typeahead.endInputSuppression();
      }
      writeInProgress = false;
      flushPendingWrites();
    };
    try {
      // Wait for xterm's public write callback before feeding the next batch.
      // This keeps full-screen TUIs from overwhelming its internal write queue.
      term.write(data, complete);
    } catch (error) {
      console.error("Could not write terminal output.", error);
      complete();
    }
  }

  write = (data: string, replay = false) => {
    if (!data) return;
    const pending = pendingWrites.at(-1);
    if (
      pending &&
      pending.replay === replay &&
      pending.length + data.length <= maxCombinedWriteBytes
    ) {
      pending.parts.push(data);
      pending.length += data.length;
    } else {
      pendingWrites.push({ parts: [data], length: data.length, replay });
    }
    flushPendingWrites();
  };

  $: term?.resize(cols, rows);

  onMount(async () => {
    const [{ Terminal }, { WebLinksAddon }, { WebglAddon }, { ImageAddon }] =
      await Promise.all([
        import("@xterm/xterm"),
        import("@xterm/addon-web-links"),
        import("@xterm/addon-webgl"),
        import("@xterm/addon-image"),
      ]);

    await waitForFonts();

    term = new Terminal({
      allowTransparency: false,
      cursorBlink: false,
      cursorStyle: "block",
      // This is the monospace font family configured in Tailwind.
      fontFamily:
        '"Fira Code VF", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace',
      fontSize: 14,
      fontWeight: 400,
      fontWeightBold: 500,
      lineHeight: 1.06,
      scrollback: $settings.scrollback,
      theme: terminalTheme,
    });

    // Keyboard shortcuts for natural text editing.
    term.attachCustomKeyEventHandler((event) => {
      if (event.key === "Enter" && event.shiftKey) {
        if (event.type === "keydown") {
          term?.clearSelection();
          // Ctrl+J/LF is the portable multiline-input binding used by Codex
          // and Claude Code, including terminals without enhanced key events.
          dispatch("data", new Uint8Array([0x0a]));
        }
        // Suppress the keypress event as well, otherwise xterm also emits CR.
        return false;
      }

      if (event.type !== "keydown") return true;

      const copyModifier = isMac ? event.metaKey : event.ctrlKey;
      if (copyModifier && !event.altKey && event.key.toLowerCase() === "v") {
        // Let the browser emit a ClipboardEvent instead of forwarding Ctrl+V
        // to the remote PTY. The container's capture handler routes images to
        // daemon storage, while xterm handles ordinary bracketed text paste.
        return false;
      }
      if (
        copyModifier &&
        !event.altKey &&
        event.key.toLowerCase() === "c" &&
        term?.hasSelection()
      ) {
        const selection = term.getSelection();
        navigator.clipboard
          .writeText(selection)
          .then(() => {
            term?.clearSelection();
            makeToast({ kind: "success", message: "Copied selection." });
          })
          .catch(() =>
            makeToast({ kind: "error", message: "Could not copy selection." }),
          );
        return false;
      }

      if (event.key === "Enter") {
        term?.clearSelection();
      }

      if (
        (isMac && event.metaKey && !event.ctrlKey && !event.altKey) ||
        (!isMac && !event.metaKey && event.ctrlKey && !event.altKey)
      ) {
        if (event.key === "ArrowLeft") {
          dispatch("data", new Uint8Array([0x01]));
          return false;
        } else if (event.key === "ArrowRight") {
          dispatch("data", new Uint8Array([0x05]));
          return false;
        } else if (event.key === "Backspace") {
          dispatch("data", new Uint8Array([0x15]));
          return false;
        }
      }
      return true;
    });

    term.loadAddon(new WebLinksAddon());
    term.loadAddon(new ImageAddon({ enableSizeReports: false }));

    term.open(termEl);
    mouseCoordinateAdapter = installXtermMouseCoordinateAdapter(term.element!);
    term.parser.registerOscHandler(9, requestAttention);
    term.parser.registerOscHandler(99, requestAttention);
    term.parser.registerOscHandler(777, requestAttention);
    term.parser.registerOscHandler(7, (value) => {
      const location = parseOsc7Location(value);
      if (location) {
        workingDirectory = location.workingDirectory;
        workingDirectoryHost = location.workingDirectoryHost;
        if (!initialWorkingDirectoryHost && workingDirectoryHost)
          initialWorkingDirectoryHost = workingDirectoryHost;
      }
      return true;
    });
    term.onBell(requestAttention);
    try {
      term.loadAddon(new WebglAddon());
    } catch (error) {
      console.warn(
        "WebGL renderer unavailable; using the DOM renderer.",
        error,
      );
    }

    term.resize(cols, rows);
    sendText = (data: string, execute = false) => {
      term?.paste(data);
      if (execute) dispatch("data", new Uint8Array([13]));
    };
    term.onTitleChange((title) => {
      currentTitle = title;
      dispatch("title", title);
    });

    const focusObserver = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (
          mutation.type === "attributes" &&
          mutation.attributeName === "class"
        ) {
          // The "focus" class is set directly by xterm.js, but there isn't any way to listen for it.
          const target = mutation.target as HTMLElement;
          const isFocused = target.classList.contains("focus");
          setFocused(isFocused);
        }
      }
    });
    focusObserver.observe(term.element!, { attributeFilter: ["class"] });

    loaded = true;
    flushPendingWrites();

    typeahead.reset();
    term.loadAddon(typeahead);

    const utf8 = new TextEncoder();
    term.onData((data: string) => {
      if (suppressInput > 0) return;
      dispatch("data", utf8.encode(data));
    });
    term.onBinary((data: string) => {
      if (suppressInput > 0) return;
      dispatch("data", Buffer.from(data, "binary"));
    });
  });

  onDestroy(() => {
    mouseCoordinateAdapter?.dispose();
    term?.dispose();
  });
</script>

<svelte:window on:mousedown|capture={closeAppearanceOnOutsideClick} />

<div
  role="presentation"
  class="term-container"
  class:focused={focused || titleEditing}
  class:windowed={windowHeight > 0}
  class:fullscreen
  class:linked-highlight={linkedHighlight}
  class:paragraph-drop-active={paragraphDropActive}
  style:background={terminalTheme.background}
  style:opacity={opacity / 100}
  style:width={windowWidth > 0 ? `${windowWidth}px` : undefined}
  style:height={windowHeight > 0 ? `${windowHeight}px` : undefined}
  on:mousedown={() => dispatch("bringToFront")}
  on:pointerdown={(event) => event.stopPropagation()}
  on:paste|capture={handlePaste}
  on:dragover={handleDragOver}
  on:dragleave={handleDragLeave}
  on:drop={handleDrop}
  on:wheel={(event) => {
    if (!event.ctrlKey) event.stopPropagation();
  }}
>
  {#if imageDragging}
    <div
      class="pointer-events-none absolute inset-2 z-30 flex items-center justify-center rounded-md border-2 border-dashed border-indigo-300 bg-zinc-950/85 text-sm font-medium text-indigo-100"
    >
      Drop image into terminal
    </div>
  {/if}
  <div
    role="presentation"
    data-canvas-titlebar
    class="terminal-titlebar flex h-9 shrink-0 select-none items-center"
    class:terminal-attention={attention && !focused}
    on:mousedown={(event) => !fullscreen && dispatch("startMove", event)}
  >
    <div class="flex h-full flex-1 items-center px-3">
      <CircleButtons>
        <!--
          TODO: This should be on:click, but that is not working due to the
          containing element's on:pointerdown `stopPropagation()` call.
        -->
        <CircleButton
          kind="red"
          disabled={!hasWriteAccess}
          ariaLabel="Close terminal"
          on:mousedown={(event) => event.button === 0 && dispatch("close")}
        />
        <CircleButton
          kind="purple"
          active={fullscreen}
          ariaLabel={fullscreen ? "Exit full screen" : "Full screen"}
          on:mousedown={(event) =>
            event.button === 0 && dispatch("toggleFullscreen")}
        />
        <CircleButton
          kind="blue"
          disabled={!hasWriteAccess}
          ariaLabel="Duplicate terminal"
          on:mousedown={(event) =>
            event.button === 0 &&
            dispatch("duplicate", {
              workingDirectory,
              workingDirectoryHost,
              initialWorkingDirectoryHost,
            })}
        />
      </CircleButtons>
    </div>
    <div
      class="flex h-full w-0 flex-grow-[4] items-center justify-center gap-1.5 overflow-hidden whitespace-nowrap px-2 text-center text-sm font-medium text-zinc-300"
    >
      <InlineTitle
        value={title}
        fallback={currentTitle}
        disabled={!hasWriteAccess}
        ariaLabel="Terminal title"
        on:change={(event) => updateAppearance({ title: event.detail })}
        on:editingChange={(event) => {
          titleEditing = event.detail;
          if (event.detail) {
            attention = false;
            dispatch("focus");
          } else {
            dispatch("blur");
          }
        }}
      />
    </div>
    <div class="relative flex h-full flex-1 items-center justify-end pr-2">
      <button
        type="button"
        class="rounded p-1 text-zinc-400 hover:bg-zinc-700 hover:text-zinc-100"
        class:opacity-40={!hasWriteAccess}
        disabled={!hasWriteAccess}
        aria-label="Browse files"
        title="Browse files"
        on:mousedown|stopPropagation={(event) => {
          if (event.button === 0 && hasWriteAccess)
            dispatch("openFiles", workingDirectory);
        }}
      >
        <FolderIcon class="h-4 w-4" />
      </button>
      <button
        bind:this={appearanceButton}
        type="button"
        class="rounded p-1 text-zinc-400 hover:bg-zinc-700 hover:text-zinc-100"
        aria-label="Terminal appearance"
        on:mousedown|stopPropagation={(event) => {
          if (event.button === 0) {
            if (appearanceOpen) closeThemeMenu();
            setAppearanceOpen(!appearanceOpen);
          }
        }}
      >
        <SettingsIcon class="h-4 w-4" />
      </button>
      {#if appearanceOpen}
        <div
          bind:this={appearancePanel}
          role="presentation"
          class="panel absolute right-2 top-8 z-20 w-60 space-y-3 p-3 text-left text-sm"
          on:mousedown|stopPropagation
        >
          {#if themeMenuOpen}
            <div class="flex items-center justify-between">
              <span class="font-medium text-zinc-200">Color theme</span>
              <button
                type="button"
                class="rounded px-1.5 py-0.5 text-xs text-zinc-400 hover:bg-zinc-700 hover:text-zinc-100"
                on:mousedown|stopPropagation={(event) => {
                  if (event.button === 0) closeThemeMenu();
                }}>Back</button
              >
            </div>
            <div
              role="presentation"
              class="theme-list"
              on:wheel={(event) => {
                if (!event.ctrlKey) event.stopPropagation();
              }}
              on:mouseleave={() => (previewTheme = null)}
            >
              {#each Object.entries(themes) as [themeName, candidate] (themeName)}
                <button
                  type="button"
                  class:theme-selected={draftTheme === themeName}
                  class="theme-option"
                  disabled={!hasWriteAccess}
                  on:mouseenter={() => (previewTheme = themeName as ThemeName)}
                  on:focus={() => (previewTheme = themeName as ThemeName)}
                  on:mousedown|stopPropagation={(event) => {
                    if (event.button === 0 && hasWriteAccess) {
                      selectTheme(themeName as ThemeName);
                    }
                  }}
                >
                  <span
                    class="theme-preview"
                    style:background-color={candidate.background}
                    style:color={candidate.foreground}>Aa</span
                  >
                  <span class="flex-1 truncate">{themeName}</span>
                  {#if draftTheme === themeName}<span aria-hidden="true">✓</span
                    >{/if}
                </button>
              {/each}
            </div>
          {:else}
            <button
              type="button"
              class="menu-row"
              disabled={!hasWriteAccess}
              on:mousedown|stopPropagation={(event) => {
                if (event.button === 0 && hasWriteAccess) openThemeMenu();
              }}
            >
              <span>Color theme</span>
              <span class="flex min-w-0 items-center gap-1 text-zinc-400">
                <span class="max-w-28 truncate">{persistedTheme}</span>
                <span aria-hidden="true">›</span>
              </span>
            </button>
            <BackgroundPicker
              value={background}
              fallbackColor={theme.background || "#000000"}
              allowNone
              disabled={!hasWriteAccess}
              on:change={(event) =>
                updateAppearance({ background: event.detail })}
            />
            <label class="block">
              <span class="mb-1 flex justify-between">
                <span>Opacity</span><span>{opacity}%</span>
              </span>
              <input
                type="range"
                min="20"
                max="100"
                value={opacity}
                disabled={!hasWriteAccess}
                class="w-full accent-indigo-500"
                on:input={(event) =>
                  updateAppearance({ opacity: +event.currentTarget.value })}
              />
            </label>
          {/if}
        </div>
      {/if}
    </div>
  </div>
  <div
    role="presentation"
    class="terminal-host inline-block w-full pb-0 pl-1 pr-1 pt-1 transition-opacity duration-500"
    bind:this={termEl}
    style:opacity={loaded ? 1.0 : 0.0}
  ></div>
  {#if linkedNotes.length}
    <div class="terminal-relations">
      <CanvasRelations
        items={linkedNotes}
        disabled={!hasWriteAccess}
        on:navigate={(event) => dispatch("navigateNote", event.detail)}
        on:remove={(event) => dispatch("unlinkNote", event.detail)}
      />
    </div>
  {/if}
</div>

<style lang="postcss">
  @reference "../../app.css";

  .term-container {
    @apply relative isolate inline-block rounded-lg border border-zinc-700;
    transition:
      transform 200ms,
      opacity 200ms;
  }

  .term-container.windowed {
    display: flex;
    flex-direction: column;
  }

  .term-container.windowed .terminal-host {
    position: relative;
    z-index: 0;
    min-height: 0;
    flex: 1;
    overflow: hidden;
  }

  .term-container.fullscreen {
    display: flex;
    width: 100% !important;
    height: 100% !important;
    flex-direction: column;
  }

  .term-container.fullscreen > :global(.xterm) {
    flex: 1;
  }

  .term-container.fullscreen .terminal-host {
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .terminal-titlebar {
    position: relative;
    z-index: 10;
    isolation: isolate;
    border-radius: 0.45rem 0.45rem 0 0;
  }

  .terminal-titlebar > * {
    position: relative;
    z-index: 1;
  }

  .terminal-titlebar.terminal-attention::before {
    content: "";
    position: absolute;
    z-index: 0;
    inset: 0;
    border-radius: inherit;
    background: linear-gradient(
      110deg,
      #ff4d6d,
      #ffb703,
      #5eead4,
      #60a5fa,
      #c084fc,
      #ff4d6d
    );
    background-size: 250% 100%;
    animation: terminal-attention 2.4s ease-in-out infinite;
    pointer-events: none;
  }

  @keyframes terminal-attention {
    0%,
    100% {
      opacity: 0.18;
      background-position: 0% 50%;
    }
    50% {
      opacity: 0.58;
      background-position: 100% 50%;
    }
  }

  .term-container.focused,
  .term-container:focus-within {
    border-color: rgb(129 140 248 / 80%);
  }

  .term-container.linked-highlight {
    border-color: rgb(228 228 231 / 50%);
    animation: linked-terminal-pulse 1.8s ease-in-out infinite;
  }

  .term-container.paragraph-drop-active {
    border-color: rgb(125 211 252 / 85%);
    box-shadow: 0 0 12px rgb(125 211 252 / 38%);
  }
  .term-container.paragraph-drop-active::after {
    content: "Release to paste at the terminal cursor";
    @apply pointer-events-none absolute bottom-3 left-1/2 z-30 -translate-x-1/2 whitespace-nowrap rounded-md border border-sky-300/40 bg-zinc-950/90 px-2 py-1 text-[11px] font-medium text-sky-100 shadow-lg;
  }

  .terminal-relations {
    @apply absolute bottom-1.5 right-2 z-20 max-w-[65%];
  }

  @keyframes linked-terminal-pulse {
    0%,
    100% {
      box-shadow: 0 0 2px rgb(228 228 231 / 6%);
    }
    50% {
      box-shadow:
        0 0 10px rgb(228 228 231 / 55%),
        0 0 18px rgb(228 228 231 / 34%);
    }
  }

  .term-container:not(.focused) :global(.xterm) {
    @apply cursor-default;
  }

  .term-container :global(.xterm-viewport) {
    scrollbar-width: thin;
    scrollbar-color: rgb(113 113 122 / 65%) transparent;
  }

  .term-container :global(.xterm-viewport::-webkit-scrollbar) {
    width: 6px;
  }

  .term-container :global(.xterm-viewport::-webkit-scrollbar-thumb) {
    border-radius: 999px;
    background: rgb(113 113 122 / 65%);
  }

  .menu-row {
    @apply flex w-full items-center justify-between rounded px-2 py-1.5 text-left;
    @apply hover:bg-zinc-700 disabled:cursor-not-allowed disabled:opacity-50;
  }

  .theme-list {
    @apply max-h-56 space-y-1 overflow-y-auto overscroll-contain pr-1;
    scrollbar-width: thin;
    scrollbar-color: rgb(113 113 122 / 70%) transparent;
  }

  .theme-option {
    @apply flex w-full items-center gap-2 rounded px-1.5 py-1 text-left text-zinc-300;
    @apply hover:bg-zinc-700 focus:bg-zinc-700 focus:outline-none;
    @apply disabled:cursor-not-allowed disabled:opacity-50;
  }

  .theme-option.theme-selected {
    @apply bg-indigo-500/15 text-indigo-100;
  }

  .theme-preview {
    @apply inline-flex h-6 w-8 shrink-0 items-center justify-center rounded border border-white/10 font-mono text-xs;
  }
</style>
