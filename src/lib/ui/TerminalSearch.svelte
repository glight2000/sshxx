<script lang="ts" context="module">
  export type CanvasSearchItem = {
    id: number;
    kind: "terminal" | "note" | "file" | "custom";
    pageId: number;
    pageName: string;
    title: string;
    content: string;
  };
</script>

<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import {
    ChevronRightIcon,
    CodeIcon,
    FileIcon,
    FileTextIcon,
    LayersIcon,
    SearchIcon,
    TerminalIcon,
  } from "svelte-feather-icons";

  export let open: boolean;
  export let items: CanvasSearchItem[];

  const dispatch = createEventDispatcher<{
    close: void;
    select: CanvasSearchItem;
  }>();
  let query = "";
  let selected = 0;
  let input: HTMLInputElement;
  let buttons: HTMLButtonElement[] = [];

  $: filtered = items
    .filter((item) =>
      `${item.title} ${item.content} ${item.pageName} ${item.id}`
        .toLowerCase()
        .includes(query.toLowerCase()),
    )
    .slice(0, 100);
  $: if (selected >= filtered.length)
    selected = Math.max(0, filtered.length - 1);
  $: {
    selected;
    tick().then(() => buttons[selected]?.scrollIntoView({ block: "nearest" }));
  }
  $: if (open) {
    query = "";
    selected = 0;
    tick().then(() => input?.focus());
  }

  function choose(index: number) {
    const item = filtered[index];
    if (item) dispatch("select", item);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      dispatch("close");
    } else if (event.key === "ArrowDown") {
      selected = Math.min(selected + 1, filtered.length - 1);
      event.preventDefault();
    } else if (event.key === "ArrowUp") {
      selected = Math.max(selected - 1, 0);
      event.preventDefault();
    } else if (event.key === "Enter") {
      choose(selected);
      event.preventDefault();
    }
  }
</script>

{#if open}
  <div
    role="presentation"
    class="fixed inset-0 pointer-events-auto z-40"
    on:mousedown|self={() => dispatch("close")}
  >
    <div
      class="panel absolute top-20 left-1/2 w-[32rem] max-w-[calc(100vw-2rem)] -translate-x-1/2 p-2 shadow-2xl"
    >
      <div class="relative">
        <SearchIcon class="absolute left-3 top-2.5 h-4 w-4 text-zinc-500" />
        <input
          bind:this={input}
          bind:value={query}
          on:keydown={handleKeydown}
          placeholder="Find canvas components across all pages…"
          class="w-full rounded-md border border-zinc-700 bg-zinc-950 py-2 pl-9 pr-3 text-sm outline-none focus:ring-2 focus:ring-indigo-500/50"
        />
      </div>
      <div class="mt-2 max-h-[min(37.5rem,calc(100vh-8rem))] overflow-y-auto">
        {#each filtered as item, index (`${item.kind}-${item.id}`)}
          <button
            bind:this={buttons[index]}
            class="flex w-full items-center gap-2.5 rounded-md px-3 py-2.5 text-left text-sm {index ===
            selected
              ? 'bg-indigo-700'
              : 'hover:bg-zinc-800'}"
            on:mouseenter={() => (selected = index)}
            on:click={() => choose(index)}
          >
            {#if item.kind === "terminal"}
              <TerminalIcon class="h-4 w-4 shrink-0" />
            {:else if item.kind === "note"}
              <FileTextIcon class="h-4 w-4 shrink-0 text-amber-300" />
            {:else if item.kind === "file"}
              <FileIcon class="h-4 w-4 shrink-0 text-sky-300" />
            {:else}
              <CodeIcon class="h-4 w-4 shrink-0 text-violet-300" />
            {/if}
            <span class="min-w-0 flex-1">
              <span class="block truncate font-medium">{item.title}</span>
              <span
                class="mt-0.5 flex min-w-0 items-center gap-1 text-xs {index ===
                selected
                  ? 'text-indigo-100/80'
                  : 'text-zinc-400'}"
                title={`Page: ${item.pageName}`}
              >
                <LayersIcon class="h-3 w-3 shrink-0" />
                <span class="shrink-0">Page</span>
                <ChevronRightIcon class="h-3 w-3 shrink-0 opacity-60" />
                <span class="truncate">{item.pageName}</span>
              </span>
            </span>
            <span
              class="shrink-0 text-xs {index === selected
                ? 'text-indigo-100/70'
                : 'text-zinc-500'}">#{item.id}</span
            >
          </button>
        {:else}
          <p class="px-3 py-6 text-center text-sm text-zinc-500">
            No matching canvas components
          </p>
        {/each}
      </div>
    </div>
  </div>
{/if}
