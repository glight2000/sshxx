<script lang="ts">
  import { createEventDispatcher } from "svelte";

  import { BACKGROUND_PRESETS } from "./backgroundPresets";

  export let value: string;
  export let disabled = false;
  export let label = "Background";
  export let allowNone = false;
  export let fallbackColor = "#18181b";

  const dispatch = createEventDispatcher<{ change: string }>();
</script>

<div class:opacity-40={disabled} class="space-y-2">
  <div class="flex items-center justify-between gap-3">
    <span class="text-xs text-zinc-400">{label}</span>
    <input
      type="color"
      value={value || fallbackColor}
      {disabled}
      aria-label={`Custom ${label.toLowerCase()} color`}
      on:input={(event) => dispatch("change", event.currentTarget.value)}
    />
  </div>
  <div class="preset-grid" aria-label={`${label} presets`}>
    {#if allowNone}
      <button
        type="button"
        class="preset no-background"
        class:selected={value === ""}
        title="Use theme background"
        aria-label="Use theme background; disable custom background"
        aria-pressed={value === ""}
        {disabled}
        on:mousedown|stopPropagation
        on:click={() => dispatch("change", "")}
      ></button>
    {/if}
    {#each BACKGROUND_PRESETS as preset (preset.color)}
      <button
        type="button"
        class="preset"
        class:selected={value.toLowerCase() === preset.color}
        style:background-color={preset.color}
        title={preset.name}
        aria-label={`${preset.name} ${preset.color}`}
        aria-pressed={value.toLowerCase() === preset.color}
        {disabled}
        on:mousedown|stopPropagation
        on:click={() => dispatch("change", preset.color)}
      ></button>
    {/each}
  </div>
</div>

<style lang="postcss">
  @reference "../../app.css";
  .preset-grid {
    @apply grid grid-cols-5 gap-1.5;
  }
  .preset {
    @apply h-7 rounded-md border border-white/10 outline-none transition-transform hover:scale-105 hover:border-white/35 focus-visible:ring-2 focus-visible:ring-indigo-400 disabled:cursor-not-allowed;
  }
  .preset.selected {
    @apply border-white/80 ring-2 ring-indigo-400/75 ring-offset-1 ring-offset-zinc-900;
  }
  .no-background {
    position: relative;
    overflow: hidden;
    background: repeating-conic-gradient(#3f3f46 0 25%, #18181b 0 50%) 0 0 / 8px
      8px;
  }
  .no-background::after {
    content: "";
    position: absolute;
    left: 8%;
    top: 50%;
    width: 84%;
    height: 2px;
    border-radius: 999px;
    background: rgb(248 113 113 / 90%);
    box-shadow: 0 0 0 1px rgb(0 0 0 / 35%);
    transform: rotate(-32deg);
  }
</style>
