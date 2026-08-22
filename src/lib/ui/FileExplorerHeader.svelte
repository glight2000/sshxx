<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { RefreshCwIcon, SettingsIcon } from "svelte-feather-icons";

  import BackgroundPicker from "./BackgroundPicker.svelte";
  import CircleButton from "./CircleButton.svelte";
  import CircleButtons from "./CircleButtons.svelte";
  import InlineTitle from "./InlineTitle.svelte";

  export let title: string;
  export let background: string;
  export let fullscreen: boolean;
  export let hasWriteAccess: boolean | undefined;

  const dispatch = createEventDispatcher<{
    close: void;
    toggleFullscreen: void;
    bringToFront: void;
    startMove: MouseEvent;
    reload: void;
    resetSplit: void;
    title: string;
    background: string;
    floatingChange: boolean;
  }>();

  let settingsOpen = false;
  let settingsButton: HTMLButtonElement;
  let settingsPanel: HTMLDivElement;

  function closeSettingsOnOutsideClick(event: MouseEvent) {
    if (
      settingsOpen &&
      event.target instanceof Node &&
      !settingsButton?.contains(event.target) &&
      !settingsPanel?.contains(event.target)
    )
      setSettingsOpen(false);
  }

  function setSettingsOpen(open: boolean) {
    if (settingsOpen === open) return;
    settingsOpen = open;
    dispatch("floatingChange", open);
  }
</script>

<svelte:window on:mousedown|capture={closeSettingsOnOutsideClick} />

<header
  role="presentation"
  data-canvas-titlebar
  class="relative flex h-9 shrink-0 cursor-default select-none items-center border-b border-zinc-800"
  class:cursor-default={fullscreen}
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
        ariaLabel="Close file explorer"
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
      value={title}
      fallback="Files"
      suffix=" · Files"
      disabled={!hasWriteAccess}
      ariaLabel="File browser title"
      on:change={(event) => dispatch("title", event.detail)}
    />
  </div>
  <div
    class="relative flex h-full flex-1 items-center justify-end gap-0.5 pr-2"
  >
    <button
      class="header-button"
      title="Reload filesystem"
      aria-label="Reload filesystem"
      on:mousedown|stopPropagation
      on:click={() => dispatch("reload")}><RefreshCwIcon /></button
    >
    <button
      bind:this={settingsButton}
      class="header-button"
      title="File explorer settings"
      aria-label="File explorer settings"
      on:mousedown|stopPropagation
      on:click={() => setSettingsOpen(!settingsOpen)}><SettingsIcon /></button
    >
    {#if settingsOpen}
      <div
        bind:this={settingsPanel}
        role="presentation"
        class="panel absolute right-2 top-8 z-30 w-60 space-y-3 p-3 text-left text-sm"
        on:mousedown|stopPropagation
      >
        <BackgroundPicker
          value={background || "#18181b"}
          disabled={!hasWriteAccess}
          on:change={(event) => dispatch("background", event.detail)}
        />
        <button
          type="button"
          class="settings-row"
          on:click={() => {
            dispatch("resetSplit");
            setSettingsOpen(false);
          }}>Reset split layout</button
        >
      </div>
    {/if}
  </div>
</header>

<style lang="postcss">
  @reference "../../app.css";
  .header-button {
    @apply inline-flex h-7 w-7 items-center justify-center rounded-md text-zinc-400 hover:bg-zinc-800 hover:text-zinc-100 disabled:cursor-not-allowed disabled:opacity-35;
  }
  .header-button :global(svg) {
    @apply h-4 w-4;
  }
  .settings-row {
    @apply block w-full rounded border-t border-zinc-700/70 px-2 pt-2 text-left text-xs text-zinc-300 hover:text-white;
  }
</style>
