<script lang="ts">
  import { browser } from "$app/environment";
  import { createEventDispatcher, onDestroy } from "svelte";
  import {
    CodeIcon,
    EyeIcon,
    RefreshCwIcon,
    SettingsIcon,
  } from "svelte-feather-icons";

  import type { WsCustomWindow } from "$lib/protocol";
  import { resolveCustomComponentUrl } from "$lib/customComponentUrl";
  import BackgroundPicker from "./BackgroundPicker.svelte";
  import CircleButton from "./CircleButton.svelte";
  import CircleButtons from "./CircleButtons.svelte";
  import CodeEditor from "./CodeEditor.svelte";
  import InlineTitle from "./InlineTitle.svelte";

  export let customWindow: WsCustomWindow;
  export let fullscreen = false;
  export let interactionLocked = false;
  export let hasWriteAccess: boolean | undefined;

  const dispatch = createEventDispatcher<{
    close: void;
    toggleFullscreen: void;
    bringToFront: void;
    startMove: MouseEvent;
    focus: void;
    blur: void;
    update: Partial<
      Pick<
        WsCustomWindow,
        "title" | "background" | "source" | "showPreview" | "url" | "useUrl"
      >
    >;
    floatingChange: boolean;
  }>();

  let mode: "source" | "preview" = customWindow.showPreview
    ? "preview"
    : "source";
  let observedShowPreview = customWindow.showPreview;
  let sourceDraft = customWindow.source;
  let observedSource = customWindow.source;
  let useUrl = customWindow.useUrl;
  let observedUseUrl = customWindow.useUrl;
  let urlDraft = customWindow.url;
  let observedUrl = customWindow.url;
  let renderRevision = 0;
  let settingsOpen = false;
  let settingsButton: HTMLButtonElement;
  let settingsPanel: HTMLDivElement;
  let sourceTimer: number | undefined;
  let urlTimer: number | undefined;
  let formatting = false;
  let previewUrl = resolveCustomComponentUrl(
    urlDraft,
    browser ? window.location.href : "http://localhost/",
  );

  $: if (customWindow.source !== observedSource) {
    observedSource = customWindow.source;
    sourceDraft = customWindow.source;
  }
  $: if (customWindow.showPreview !== observedShowPreview) {
    observedShowPreview = customWindow.showPreview;
    mode = observedShowPreview ? "preview" : "source";
    if (observedShowPreview) renderRevision += 1;
  }
  $: if (customWindow.url !== observedUrl) {
    observedUrl = customWindow.url;
    urlDraft = customWindow.url;
  }
  $: if (customWindow.useUrl !== observedUseUrl) {
    observedUseUrl = customWindow.useUrl;
    useUrl = customWindow.useUrl;
    if (mode === "preview") renderRevision += 1;
  }
  $: previewUrl = resolveCustomComponentUrl(
    urlDraft,
    browser ? window.location.href : "http://localhost/",
  );

  function setSettingsOpen(open: boolean) {
    if (settingsOpen === open) return;
    settingsOpen = open;
    dispatch("floatingChange", open);
  }

  function closeSettingsOnOutsideClick(event: MouseEvent) {
    if (
      settingsOpen &&
      event.target instanceof Node &&
      !settingsButton?.contains(event.target) &&
      !settingsPanel?.contains(event.target)
    )
      setSettingsOpen(false);
  }

  function commitSource() {
    if (sourceTimer !== undefined) {
      window.clearTimeout(sourceTimer);
      sourceTimer = undefined;
    }
    if (sourceDraft === observedSource) return;
    observedSource = sourceDraft;
    dispatch("update", { source: sourceDraft });
  }

  function sourceChanged(value: string) {
    sourceDraft = value;
    if (sourceTimer !== undefined) window.clearTimeout(sourceTimer);
    sourceTimer = window.setTimeout(commitSource, 180);
  }

  function commitUrl() {
    if (urlTimer !== undefined) {
      window.clearTimeout(urlTimer);
      urlTimer = undefined;
    }
    const next = urlDraft.trim();
    if (next === observedUrl) return;
    observedUrl = next;
    urlDraft = next;
    dispatch("update", { url: next });
  }

  function urlChanged(value: string) {
    urlDraft = value;
    if (urlTimer !== undefined) window.clearTimeout(urlTimer);
    urlTimer = window.setTimeout(commitUrl, 180);
  }

  function setUseUrl(next: boolean) {
    if (!hasWriteAccess || useUrl === next) return;
    commitSource();
    commitUrl();
    observedUseUrl = next;
    useUrl = next;
    dispatch("update", { useUrl: next });
  }

  function showPreview() {
    commitSource();
    commitUrl();
    if (useUrl && previewUrl.error) return;
    observedShowPreview = true;
    mode = "preview";
    renderRevision += 1;
    dispatch("update", { showPreview: true });
  }

  function showSource() {
    observedShowPreview = false;
    mode = "source";
    dispatch("update", { showPreview: false });
  }

  function toggleMode() {
    if (!hasWriteAccess) return;
    if (mode === "source") showPreview();
    else showSource();
  }

  function sandboxedDocument(source: string) {
    const policy = `<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src * data: blob: 'unsafe-inline' 'unsafe-eval'; connect-src * data: blob:; img-src * data: blob:; media-src * data: blob:; style-src * 'unsafe-inline'; font-src * data:; frame-src 'none'; child-src 'none'; object-src 'none'; base-uri 'none'">`;
    const head = source.match(/<head(?:\s[^>]*)?>/i);
    return head
      ? `${source.slice(0, head.index! + head[0].length)}${policy}${source.slice(head.index! + head[0].length)}`
      : `${policy}${source}`;
  }

  async function formatSource() {
    if (!hasWriteAccess || formatting) return;
    formatting = true;
    try {
      const [{ format }, htmlPlugin] = await Promise.all([
        import("prettier/standalone"),
        import("prettier/plugins/html"),
      ]);
      sourceDraft = await format(sourceDraft, {
        parser: "html",
        plugins: [htmlPlugin.default],
      });
      commitSource();
    } catch (error) {
      console.warn("Could not format custom component source.", error);
    } finally {
      formatting = false;
    }
  }

  onDestroy(() => {
    commitSource();
    commitUrl();
    if (settingsOpen) dispatch("floatingChange", false);
  });
</script>

<svelte:window on:mousedown|capture={closeSettingsOnOutsideClick} />

<section
  role="presentation"
  aria-label={customWindow.title || "Custom component"}
  class="custom-window flex overflow-visible rounded-xl border border-transparent shadow-lg shadow-black/20"
  class:fullscreen
  style:width={`${customWindow.width}px`}
  style:height={`${customWindow.height}px`}
  style:background={customWindow.background || "#18181b"}
  tabindex="-1"
  on:focusin={() => dispatch("focus")}
  on:focusout={(event) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null))
      dispatch("blur");
  }}
  on:mousedown={() => dispatch("focus")}
>
  <header
    role="presentation"
    data-canvas-titlebar
    class="relative flex h-9 shrink-0 cursor-default select-none items-center border-b border-zinc-700/80 bg-zinc-950/50"
    on:mousedown|stopPropagation={(event) => {
      dispatch("bringToFront");
      if (event.button === 0 && !fullscreen) dispatch("startMove", event);
    }}
  >
    <div class="flex h-full flex-1 items-center px-3">
      <CircleButtons>
        <CircleButton
          kind="red"
          disabled={!hasWriteAccess}
          ariaLabel="Close custom component"
          on:mousedown={(event) => event.button === 0 && dispatch("close")}
        />
        <CircleButton
          kind="purple"
          active={fullscreen}
          ariaLabel={fullscreen ? "Exit full screen" : "Full screen"}
          on:mousedown={(event) =>
            event.button === 0 && dispatch("toggleFullscreen")}
        />
      </CircleButtons>
    </div>
    <div
      class="flex h-full w-0 flex-grow-[4] items-center justify-center overflow-hidden whitespace-nowrap px-2 text-center text-sm font-medium text-zinc-300"
    >
      <InlineTitle
        value={customWindow.title}
        fallback="Custom component"
        suffix=" · Custom"
        disabled={!hasWriteAccess}
        ariaLabel="Custom component title"
        on:change={(event) => dispatch("update", { title: event.detail })}
      />
    </div>
    <div
      class="relative flex h-full flex-1 items-center justify-end gap-0.5 pr-2"
    >
      {#if mode === "preview"}
        <button
          class="header-button"
          title="Render again"
          aria-label="Render again"
          on:mousedown|stopPropagation
          on:click={() => (renderRevision += 1)}><RefreshCwIcon /></button
        >
      {/if}
      <button
        class="header-button"
        disabled={!hasWriteAccess}
        title={mode === "source" ? "Show rendered component" : "Edit content"}
        aria-label={mode === "source"
          ? "Show rendered component"
          : "Edit content"}
        on:mousedown|stopPropagation
        on:click={toggleMode}
        >{#if mode === "source"}<EyeIcon />{:else}<CodeIcon />{/if}</button
      >
      <button
        bind:this={settingsButton}
        class="header-button"
        title="Custom component settings"
        aria-label="Custom component settings"
        on:mousedown|stopPropagation
        on:click={() => setSettingsOpen(!settingsOpen)}><SettingsIcon /></button
      >
      {#if settingsOpen}
        <div
          role="presentation"
          bind:this={settingsPanel}
          class="panel absolute right-2 top-8 z-30 w-60 p-3 text-left text-sm"
          on:mousedown|stopPropagation
        >
          <BackgroundPicker
            value={customWindow.background || "#18181b"}
            disabled={!hasWriteAccess}
            on:change={(event) =>
              dispatch("update", { background: event.detail })}
          />
        </div>
      {/if}
    </div>
  </header>

  <div
    class="relative min-h-0 flex-1 overflow-hidden rounded-b-[11px]"
    on:wheel|stopPropagation
  >
    {#if mode === "source"}
      <div class="source-layout">
        <div
          class="source-kind"
          role="group"
          aria-label="Component content type"
        >
          <button
            type="button"
            class:active={!useUrl}
            disabled={!hasWriteAccess}
            on:click={() => setUseUrl(false)}>HTML / JavaScript</button
          >
          <button
            type="button"
            class:active={useUrl}
            disabled={!hasWriteAccess}
            on:click={() => setUseUrl(true)}>URL</button
          >
          {#if !useUrl}
            <button
              class="format-button"
              type="button"
              disabled={!hasWriteAccess || formatting}
              on:click={formatSource}
              >{formatting ? "Formatting…" : "Format"}</button
            >
          {/if}
        </div>
        <div class="source-notice" role="note">
          {#if useUrl}
            The URL is shared, but every client loads it independently. Do not
            include credentials or bearer tokens. The target site may refuse to
            be embedded.
          {:else}
            HTML and JavaScript run independently in every client that opens the
            preview. Avoid non-idempotent actions and never place secrets in
            this shared source.
          {/if}
        </div>
        {#if useUrl}
          <div class="url-editor">
            <label>
              <span>Page URL</span>
              <input
                type="url"
                value={urlDraft}
                placeholder="https://example.com/dashboard"
                readonly={!hasWriteAccess}
                spellcheck="false"
                on:input={(event) => urlChanged(event.currentTarget.value)}
              />
            </label>
            <p class:error={Boolean(previewUrl.error)}>
              {previewUrl.error ||
                "The page will run in an isolated iframe without access to sshxx."}
            </p>
          </div>
        {:else}
          <div class="min-h-0 flex-1">
            <CodeEditor
              value={sourceDraft}
              filename="component.html"
              readOnly={!hasWriteAccess}
              onChange={sourceChanged}
              insertText={() => ({ ok: false })}
              previewTextDrop={() => false}
              cancelTextDropPreview={() => {}}
            />
          </div>
        {/if}
      </div>
    {:else}
      {#if useUrl && previewUrl.error}
        <div class="preview-error" role="alert">{previewUrl.error}</div>
      {:else}
        {#key renderRevision}
          {#if useUrl}
            <iframe
              class="h-full w-full border-0 bg-white"
              class:pointer-events-none={interactionLocked}
              title={customWindow.title || "Custom component preview"}
              sandbox="allow-scripts allow-forms allow-modals allow-downloads"
              referrerpolicy="no-referrer"
              src={previewUrl.url}
              on:focus={() => dispatch("focus")}
            ></iframe>
          {:else}
            <iframe
              class="h-full w-full border-0 bg-white"
              class:pointer-events-none={interactionLocked}
              title={customWindow.title || "Custom component preview"}
              sandbox="allow-scripts allow-forms allow-modals allow-downloads"
              referrerpolicy="no-referrer"
              srcdoc={sandboxedDocument(sourceDraft)}
              on:focus={() => dispatch("focus")}
            ></iframe>
          {/if}
        {/key}
      {/if}
    {/if}
  </div>
  <div class="custom-window-border" aria-hidden="true"></div>
</section>

<style lang="postcss">
  @reference "../../app.css";
  .custom-window {
    @apply relative flex-col text-zinc-200;
    --custom-window-border-color: rgb(63 63 70);
  }
  .custom-window.fullscreen {
    @apply h-full w-full;
  }
  .custom-window:focus-within {
    box-shadow: 0 0 0 1px rgb(129 140 248 / 0.45);
  }
  .custom-window:focus-within > .custom-window-border {
    border-color: rgb(129 140 248 / 0.95);
  }
  .custom-window-border {
    position: absolute;
    z-index: 20;
    inset: -1px;
    border: 1px solid var(--custom-window-border-color);
    border-radius: inherit;
    pointer-events: none;
  }
  .header-button {
    @apply inline-flex h-7 w-7 items-center justify-center rounded-md text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100 disabled:cursor-not-allowed disabled:opacity-35;
  }
  .header-button :global(svg) {
    @apply h-4 w-4;
  }
  .source-layout {
    @apply flex h-full min-h-0 flex-col bg-zinc-950;
  }
  .source-notice {
    @apply shrink-0 border-b border-amber-700/30 bg-amber-950/35 px-3 py-2 text-xs leading-5 text-amber-100/80 select-none;
  }
  .source-kind {
    @apply flex shrink-0 items-center gap-1 border-b border-zinc-800 bg-zinc-900 px-2 py-1.5;
  }
  .source-kind button {
    @apply rounded-md px-2.5 py-1 text-xs text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100 disabled:cursor-not-allowed disabled:opacity-40;
  }
  .source-kind button.active {
    @apply bg-zinc-700 text-zinc-100;
  }
  .source-kind .format-button {
    @apply ml-auto border border-zinc-600 bg-zinc-800 text-zinc-200;
  }
  .url-editor {
    @apply flex min-h-0 flex-1 flex-col gap-2 bg-zinc-950 p-3;
  }
  .url-editor label {
    @apply flex flex-col gap-2 text-xs font-medium text-zinc-300;
  }
  .url-editor input {
    @apply w-full rounded-md border border-zinc-700 bg-zinc-900 px-3 py-2 font-mono text-sm text-zinc-100 outline-none placeholder:text-zinc-600 focus:border-indigo-400 focus:ring-2 focus:ring-indigo-500/20 read-only:cursor-default;
  }
  .url-editor p {
    @apply text-xs leading-5 text-zinc-500;
  }
  .url-editor p.error {
    @apply text-rose-300;
  }
  .preview-error {
    @apply flex h-full w-full items-center justify-center bg-zinc-950 p-5 text-center text-sm text-rose-300;
  }
</style>
