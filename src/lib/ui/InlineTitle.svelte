<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";

  export let value = "";
  export let fallback: string;
  export let suffix = "";
  export let disabled = false;
  export let ariaLabel = "Window title";

  const dispatch = createEventDispatcher<{
    change: string;
    editingChange: boolean;
  }>();

  let editing = false;
  let draft = "";
  let input: HTMLInputElement;

  async function beginEditing() {
    if (disabled || editing) return;
    draft = value || fallback;
    editing = true;
    dispatch("editingChange", true);
    await tick();
    input.focus({ preventScroll: true });
    input.select();
  }

  function finishEditing(commit: boolean) {
    if (!editing) return;
    const next = draft.trim();
    editing = false;
    dispatch("editingChange", false);
    if (commit && next !== value) dispatch("change", next);
  }
</script>

<div
  role="presentation"
  class="inline-title min-w-0 flex-1 cursor-default select-none"
  title={disabled ? value || fallback : "Double-click to rename"}
  on:dblclick|stopPropagation={beginEditing}
>
  {#if editing}
    <input
      bind:this={input}
      bind:value={draft}
      maxlength="100"
      aria-label={ariaLabel}
      class="title-input"
      on:mousedown|stopPropagation
      on:keydown|stopPropagation={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          finishEditing(true);
        } else if (event.key === "Escape") {
          event.preventDefault();
          finishEditing(false);
        }
      }}
      on:copy|stopPropagation
      on:cut|stopPropagation
      on:paste|stopPropagation
      on:blur={() => finishEditing(true)}
    />
  {:else}
    <span class="block truncate">{value || fallback}{suffix}</span>
  {/if}
</div>

<style lang="postcss">
  @reference "../../app.css";
  .title-input {
    @apply h-7 w-full cursor-text select-text rounded border border-indigo-400/60 bg-zinc-950/90 px-2 text-center text-sm text-zinc-100 outline-none ring-2 ring-indigo-500/25;
  }
</style>
