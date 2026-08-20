<script lang="ts">
  import { XIcon } from "svelte-feather-icons";
  import { createEventDispatcher } from "svelte";
  import { fade, scale } from "svelte/transition";

  const dispatch = createEventDispatcher<{ close: void }>();

  export let title: string;
  export let description: string;
  export let showCloseButton = false;
  export let maxWidth: number = 768; // screen-md
  export let open: boolean;

  function close() {
    dispatch("close");
  }

  function handleKeydown(event: KeyboardEvent) {
    if (open && event.key === "Escape") close();
  }
</script>

<svelte:window on:keydown={handleKeydown} />

{#if open}
  <div
    role="presentation"
    class="fixed inset-0 z-50 grid place-items-center bg-black/20 backdrop-blur-sm"
    on:click|self={close}
    transition:fade={{ duration: 150 }}
  >
    <div
      role="dialog"
      aria-modal="true"
      aria-label={title}
      class="w-full sm:w-[calc(100%-32px)]"
      style="max-width: {maxWidth}px"
      transition:scale={{ duration: 200, start: 0.95 }}
    >
      <div
        class="relative bg-[#111] sm:border border-zinc-800 px-6 py-10 sm:py-6
         h-screen sm:h-auto max-h-screen sm:rounded-lg overflow-y-auto"
      >
        {#if showCloseButton}
          <button
            class="absolute top-4 right-4 p-1 rounded hover:bg-zinc-700 active:bg-indigo-700 transition-colors"
            aria-label="Close {title}"
            on:click={close}
          >
            <XIcon class="h-5 w-5" />
          </button>
        {/if}

        <div class="mb-8 text-center">
          <h2 class="text-xl font-medium mb-2">{title}</h2>
          <p class="text-zinc-400">{description}</p>
        </div>

        <slot />
      </div>
    </div>
  </div>
{/if}
