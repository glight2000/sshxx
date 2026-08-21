<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { XIcon } from "svelte-feather-icons";
  import { fade, scale } from "svelte/transition";

  export let open = false;
  export let title: string;
  export let description = "";
  export let busy = false;
  export let maxWidth = 640;

  const dispatch = createEventDispatcher<{ close: void }>();

  function close() {
    if (!busy) dispatch("close");
  }

  function handleKeydown(event: KeyboardEvent) {
    if (open && event.key === "Escape") close();
  }
</script>

<svelte:window on:keydown={handleKeydown} />

{#if open}
  <div
    role="presentation"
    class="absolute inset-0 z-50 flex items-center justify-center rounded-xl bg-black/55 p-5 backdrop-blur-sm"
    on:pointerdown|stopPropagation
    on:pointerup|stopPropagation
    on:pointermove|stopPropagation
    on:mousedown|stopPropagation
    on:mouseup|stopPropagation
    on:mousemove|stopPropagation
    on:wheel|stopPropagation
    on:click|stopPropagation={(event) => {
      if (event.target === event.currentTarget) close();
    }}
    transition:fade={{ duration: 120 }}
  >
    <div
      role="dialog"
      aria-modal="true"
      aria-label={title}
      class="relative max-h-[calc(100%-24px)] w-full overflow-y-auto rounded-xl border border-zinc-700 bg-[var(--app-surface-solid)] p-5 shadow-2xl shadow-black/75"
      style:max-width={`${maxWidth}px`}
      transition:scale={{ duration: 150, start: 0.97 }}
    >
      <button
        class="absolute right-3 top-3 rounded p-1.5 text-zinc-500 hover:bg-zinc-800 hover:text-zinc-100 disabled:opacity-40"
        aria-label="Close {title}"
        disabled={busy}
        on:click={close}><XIcon class="h-4 w-4" /></button
      >
      <div class="mb-5 pr-8">
        <h2 class="text-lg font-medium text-zinc-100">{title}</h2>
        {#if description}<p class="mt-1 text-sm text-zinc-500">
            {description}
          </p>{/if}
      </div>
      <slot />
    </div>
  </div>
{/if}
