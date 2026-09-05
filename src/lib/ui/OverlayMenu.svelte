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
    class="pointer-events-auto fixed inset-0 z-50 grid place-items-center bg-black/20 backdrop-blur-sm"
    on:pointerdown|stopPropagation
    on:pointerup|stopPropagation
    on:pointermove|stopPropagation
    on:mousedown|stopPropagation
    on:mouseup|stopPropagation
    on:mousemove|stopPropagation
    on:touchstart|stopPropagation
    on:touchmove|stopPropagation
    on:touchend|stopPropagation
    on:wheel|stopPropagation
    on:click|stopPropagation={(event) => {
      if (event.target === event.currentTarget) close();
    }}
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
        class="panel relative h-screen max-h-screen overflow-y-auto overscroll-contain rounded-none px-6 py-10 sm:h-auto sm:rounded-2xl sm:py-6"
      >
        {#if showCloseButton}
          <button
            class="ui-icon-button absolute top-4 right-4"
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
