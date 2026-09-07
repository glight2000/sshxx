<script lang="ts">
  import { MOBILE_TERMINAL_KEYS } from "$lib/mobileTerminalKeys";
  export let disabled = false;
  export let send: (key: string) => void;
  let open = false;
  let root: HTMLDivElement;
</script>

<svelte:window
  on:pointerdown|capture={(event) => {
    if (event.target instanceof Node && !root?.contains(event.target))
      open = false;
  }}
  on:keydown={(event) => {
    if (event.key === "Escape") open = false;
  }}
/>

<div class="terminal-keys" bind:this={root}>
  <div class="arrows" role="group" aria-label="Terminal arrow keys">
    {#each ["↑", "↓", "←", "→"] as key}
      <button
        type="button"
        {disabled}
        aria-label={`Send ${key} to terminal`}
        on:mousedown|preventDefault
        on:click={() => !disabled && send(key)}>{key}</button
      >
    {/each}
    <button
      type="button"
      aria-label="Terminal special keys"
      aria-expanded={open}
      on:mousedown|preventDefault
      on:click={() => (open = !open)}>Keys</button
    >
  </div>
  {#if open}
    <div class="keypad" role="group" aria-label="Terminal special keys">
      {#each Object.keys(MOBILE_TERMINAL_KEYS).slice(4) as key}
        <button
          type="button"
          {disabled}
          aria-label={`Send ${key} to terminal`}
          title={key === "Ctrl+C"
            ? "Interrupt the foreground program (not copy)"
            : `Send ${key} immediately`}
          on:mousedown|preventDefault
          on:click={() => !disabled && send(key)}>{key}</button
        >
      {/each}
    </div>
  {/if}
</div>

<style>
  .terminal-keys {
    min-width: 0;
  }
  .arrows {
    display: flex;
    gap: 4px;
  }
  .arrows button {
    flex: 1;
  }
  .keypad {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 4px;
    max-height: min(180px, 25dvh);
    overflow-y: auto;
    overscroll-behavior: contain;
    touch-action: pan-y;
    margin-top: 4px;
  }
  button {
    min-width: 40px;
    min-height: 40px;
    padding: 4px;
    border: 1px solid var(--reader-border, var(--surface-border));
    border-radius: 7px;
    background: var(--reader-bg, var(--surface-bg));
    color: var(--reader-fg, var(--app-text));
    font-size: 12px;
    touch-action: manipulation;
  }
  .arrows button:not(:last-child) {
    font-size: 18px;
  }
  button:disabled {
    opacity: 0.4;
  }
  button:focus-visible {
    outline: 2px solid var(--surface-accent);
  }
</style>
