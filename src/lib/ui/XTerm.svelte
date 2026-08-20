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
  import { SettingsIcon } from "svelte-feather-icons";

  import themes from "./themes";
  import CircleButton from "./CircleButton.svelte";
  import CircleButtons from "./CircleButtons.svelte";
  import { settings } from "$lib/settings";
  import { TypeAheadAddon } from "$lib/typeahead";

  /** Used to determine Cmd versus Ctrl keyboard shortcuts. */
  const isMac = browser && navigator.platform.startsWith("Mac");

  const dispatch = createEventDispatcher<{
    data: Uint8Array;
    close: void;
    duplicate: void;
    appearance: { title: string; background: string; opacity: number };
    title: string;
    bringToFront: void;
    startMove: MouseEvent;
    focus: void;
    blur: void;
  }>();

  const typeahead = new TypeAheadAddon();

  export let rows: number, cols: number;
  export let title = "";
  export let background = "";
  export let opacity = 80;
  export let hasWriteAccess: boolean | undefined;
  export let write: (data: string, replay?: boolean) => void; // bound function prop

  export let termEl: HTMLDivElement = null as any; // suppress "missing prop" warning
  let term: Terminal | null = null;

  $: theme = themes[$settings.theme];
  $: terminalTheme = { ...theme, background: background || theme.background };

  $: if (term) {
    // If the theme changes, update existing terminals' appearance.
    term.options.theme = terminalTheme;
    term.options.scrollback = $settings.scrollback;
  }

  let loaded = false;
  let focused = false;
  let currentTitle = "Remote Terminal";
  let appearanceOpen = false;
  let appearanceButton: HTMLButtonElement;
  let appearancePanel: HTMLDivElement;
  let attention = false;
  let suppressAttention = 0;
  $: displayTitle = title || currentTitle;

  function requestAttention() {
    if (suppressAttention === 0 && !focused) attention = true;
    return true;
  }

  function updateAppearance(values: {
    title?: string;
    background?: string;
    opacity?: number;
  }) {
    dispatch("appearance", { title, background, opacity, ...values });
  }

  function closeAppearanceOnOutsideClick(event: MouseEvent) {
    if (!appearanceOpen || !(event.target instanceof Node)) return;
    if (
      appearanceButton.contains(event.target) ||
      appearancePanel?.contains(event.target)
    ) {
      return;
    }
    appearanceOpen = false;
  }

  function handleWheelSkipXTerm(event: WheelEvent) {
    event.preventDefault(); // Stop native macOS Chrome zooming on pinch.

    // We stop the event from propagating to the main `.xterm` terminal element,
    // so the xterm.js's event handlers do not fire and scroll the buffer.
    event.stopPropagation();

    // However, we still want it to propagate upward to our pan/zoom handlers,
    // so we re-dispatch the event higher up, skipping xterm.
    termEl?.dispatchEvent(new WheelEvent(event.type, event));
  }

  function setFocused(isFocused: boolean, cursorLayer: HTMLDivElement) {
    if (isFocused && !focused) {
      focused = isFocused;
      attention = false;
      cursorLayer.removeEventListener("wheel", handleWheelSkipXTerm);
      dispatch("focus");
    } else if (!isFocused && focused) {
      focused = isFocused;
      cursorLayer.addEventListener("wheel", handleWheelSkipXTerm);
      dispatch("blur");
    }
  }

  const preloadBuffer: [string, boolean][] = [];

  write = (data: string, replay = false) => {
    if (!data) return;
    if (!term) {
      // Before the terminal is loaded, push data into a buffer.
      preloadBuffer.push([data, replay]);
    } else {
      if (data && !replay) data = typeahead.onBeforeProcessData(data);
      if (replay) {
        suppressAttention += 1;
        term.write(data, () => (suppressAttention -= 1));
      } else {
        term.write(data);
      }
    }
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
    term.parser.registerOscHandler(9, requestAttention);
    term.parser.registerOscHandler(99, requestAttention);
    term.parser.registerOscHandler(777, requestAttention);
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
    term.onTitleChange((title) => {
      currentTitle = title;
      dispatch("title", title);
    });

    // Hack: We artificially disable scrolling when the terminal is not focused.
    // ("termEl" > div.terminal.xterm > div.xterm-screen)
    const screenEl = termEl.querySelector(".xterm-screen")! as HTMLDivElement;
    screenEl.addEventListener("wheel", handleWheelSkipXTerm);

    const focusObserver = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (
          mutation.type === "attributes" &&
          mutation.attributeName === "class"
        ) {
          // The "focus" class is set directly by xterm.js, but there isn't any way to listen for it.
          const target = mutation.target as HTMLElement;
          const isFocused = target.classList.contains("focus");
          setFocused(isFocused, screenEl);
        }
      }
    });
    focusObserver.observe(term.element!, { attributeFilter: ["class"] });

    loaded = true;
    for (const [data, replay] of preloadBuffer) {
      write(data, replay);
    }

    typeahead.reset();
    term.loadAddon(typeahead);

    const utf8 = new TextEncoder();
    term.onData((data: string) => {
      dispatch("data", utf8.encode(data));
    });
    term.onBinary((data: string) => {
      dispatch("data", Buffer.from(data, "binary"));
    });
  });

  onDestroy(() => term?.dispose());
</script>

<svelte:window on:mousedown|capture={closeAppearanceOnOutsideClick} />

<div
  role="presentation"
  class="term-container"
  class:focused
  class:terminal-attention={attention && !focused}
  style:background={background || theme.background}
  style:opacity={opacity / 100}
  on:mousedown={() => dispatch("bringToFront")}
  on:pointerdown={(event) => event.stopPropagation()}
>
  <div
    role="presentation"
    class="flex select-none"
    on:mousedown={(event) => dispatch("startMove", event)}
  >
    <div class="flex-1 flex items-center px-3">
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
          kind="blue"
          disabled={!hasWriteAccess}
          ariaLabel="Duplicate terminal"
          on:mousedown={(event) => event.button === 0 && dispatch("duplicate")}
        />
      </CircleButtons>
    </div>
    <div
      class="flex w-0 flex-grow-[4] items-center justify-center gap-1.5 overflow-hidden whitespace-nowrap p-2 text-center text-sm font-medium text-zinc-300"
    >
      <span class="truncate">{displayTitle}</span>
    </div>
    <div class="relative flex flex-1 justify-end pr-2">
      <button
        bind:this={appearanceButton}
        type="button"
        class="rounded p-1 text-zinc-400 hover:bg-zinc-700 hover:text-zinc-100"
        aria-label="Terminal appearance"
        on:mousedown|stopPropagation={(event) => {
          if (event.button === 0) appearanceOpen = !appearanceOpen;
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
          <label class="block">
            <span class="mb-1 block text-zinc-400">Title</span>
            <input
              value={title}
              maxlength="100"
              placeholder={currentTitle}
              disabled={!hasWriteAccess}
              class="w-full rounded border border-zinc-700 bg-zinc-950 px-2 py-1.5 outline-none focus:ring-2 focus:ring-indigo-500/50"
              on:change={(event) =>
                updateAppearance({ title: event.currentTarget.value })}
            />
          </label>
          <label class="flex items-center justify-between gap-3">
            Background
            <input
              type="color"
              value={background || theme.background}
              disabled={!hasWriteAccess}
              on:input={(event) =>
                updateAppearance({ background: event.currentTarget.value })}
            />
          </label>
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
        </div>
      {/if}
    </div>
  </div>
  <div
    role="presentation"
    class="inline-block px-4 py-2 transition-opacity duration-500"
    bind:this={termEl}
    style:opacity={loaded ? 1.0 : 0.0}
    on:wheel={(event) => {
      if (focused) {
        // Don't pan the page when scrolling while the terminal is selected.
        // Conversely, we manually disable terminal scrolling unless it is currently selected.
        event.stopPropagation();
      }
    }}
  ></div>
</div>

<style lang="postcss">
  @reference "../../app.css";

  .term-container {
    @apply relative isolate inline-block rounded-lg border border-zinc-700;
    transition:
      transform 200ms,
      opacity 200ms;
  }

  .term-container.terminal-attention::before {
    content: "";
    position: absolute;
    z-index: -1;
    inset: -2.5px;
    border-radius: 0.75rem;
    background: conic-gradient(
      from 0deg,
      #ff4d6d,
      #ffb703,
      #5eead4,
      #60a5fa,
      #c084fc,
      #ff4d6d
    );
    filter: blur(5px);
    animation: terminal-attention 2.2s ease-in-out infinite;
    pointer-events: none;
  }

  @keyframes terminal-attention {
    0%,
    100% {
      opacity: 0.3;
      transform: scale(0.9975);
    }
    50% {
      opacity: 0.95;
      transform: scale(1.0075);
    }
  }

  .term-container.focused,
  .term-container:focus-within {
    outline: 2px solid rgb(129 140 248 / 80%);
    outline-offset: -1px;
  }

  .term-container:not(.focused) :global(.xterm) {
    @apply cursor-default;
  }
</style>
