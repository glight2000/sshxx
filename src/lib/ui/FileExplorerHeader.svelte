<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { RefreshCwIcon, SettingsIcon } from "svelte-feather-icons";

  import CircleButton from "./CircleButton.svelte";
  import CircleButtons from "./CircleButtons.svelte";

  export let title: string;
  export let fullscreen: boolean;
  export let hasWriteAccess: boolean | undefined;

  const dispatch = createEventDispatcher<{
    close: void;
    toggleFullscreen: void;
    startMove: MouseEvent;
    reload: void;
    resetSplit: void;
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
      settingsOpen = false;
  }
</script>

<svelte:window on:mousedown|capture={closeSettingsOnOutsideClick} />

<header
  role="presentation"
  class="relative flex h-9 shrink-0 cursor-move select-none items-center border-b border-zinc-800"
  class:cursor-default={fullscreen}
  on:mousedown={(event) => {
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
    <span class="truncate">{title} · Files</span>
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
      on:click={() => (settingsOpen = !settingsOpen)}><SettingsIcon /></button
    >
    {#if settingsOpen}
      <div
        bind:this={settingsPanel}
        role="presentation"
        class="panel absolute right-2 top-8 z-30 w-52 p-1.5 text-left text-sm"
        on:mousedown|stopPropagation
      >
        <button
          type="button"
          class="settings-row"
          on:click={() => {
            dispatch("resetSplit");
            settingsOpen = false;
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
    @apply block w-full rounded px-2 py-1.5 text-left text-xs text-zinc-300 hover:bg-zinc-700 hover:text-white;
  }
</style>
