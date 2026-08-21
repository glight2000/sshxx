<script lang="ts">
  import { fade } from "svelte/transition";

  import type { WsUser } from "$lib/protocol";
  import { userToHue } from "./LiveCursor.svelte";

  export let users: [number, WsUser][];

  const visibleUserLimit = 6;
  let expanded = false;
  let root: HTMLElement;

  $: sortedUsers = [...users].sort((a, b) => a[0] - b[0]);
  $: visibleUsers = sortedUsers.slice(0, visibleUserLimit);
  $: overflowCount = Math.max(0, sortedUsers.length - visibleUserLimit);
  $: if (overflowCount === 0) expanded = false;

  function nameToInitial(name: string): string {
    const first = Array.from(name.trim())[0];
    return first?.toLocaleUpperCase() ?? "?";
  }

  function closeOnOutsidePointer(event: PointerEvent) {
    if (
      expanded &&
      event.target instanceof Node &&
      !root?.contains(event.target)
    ) {
      expanded = false;
    }
  }
</script>

<svelte:window on:pointerdown={closeOnOutsidePointer} />

<div class="relative flex items-center" bind:this={root}>
  <div class="flex items-center -space-x-1.5">
    {#each visibleUsers as [id, user] (id)}
      <div
        class="user-avatar"
        style:background="hsl({userToHue(id, user.name)}, 68%, 48%)"
        title={user.name}
        aria-label={user.name}
        transition:fade|local={{ duration: 160 }}
      >
        {nameToInitial(user.name)}
      </div>
    {/each}
  </div>

  {#if overflowCount > 0}
    <button
      type="button"
      class="overflow-button"
      class:active={expanded}
      aria-label="Show all {sortedUsers.length} online users"
      aria-expanded={expanded}
      title="{sortedUsers.length} users online"
      on:click={() => (expanded = !expanded)}
    >
      +{overflowCount}
    </button>

    {#if expanded}
      <div class="user-menu" transition:fade|local={{ duration: 120 }}>
        <div class="menu-heading">
          <span>Online users</span>
          <span>{sortedUsers.length}</span>
        </div>
        <div class="max-h-72 overflow-y-auto p-1">
          {#each sortedUsers as [id, user] (id)}
            <div class="user-row">
              <div
                class="user-avatar shrink-0"
                style:background="hsl({userToHue(id, user.name)}, 68%, 48%)"
              >
                {nameToInitial(user.name)}
              </div>
              <span class="truncate">{user.name}</span>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {/if}
</div>

<style lang="postcss">
  @reference "../../app.css";

  .user-avatar {
    @apply relative flex h-7 w-7 select-none items-center justify-center rounded-full border-2 border-zinc-900 text-xs font-semibold text-white shadow-sm;
  }
  .overflow-button {
    @apply relative ml-1 flex h-7 min-w-7 items-center justify-center rounded-full border border-zinc-700 bg-zinc-800 px-1.5 text-[11px] font-semibold text-zinc-300 transition-colors hover:border-zinc-600 hover:bg-zinc-700 hover:text-zinc-100;
  }
  .overflow-button.active {
    @apply border-indigo-500/60 bg-indigo-500/20 text-indigo-200;
  }
  .user-menu {
    @apply absolute right-0 top-[calc(100%+0.6rem)] z-50 w-64 overflow-hidden rounded-lg border border-zinc-700 bg-zinc-900 shadow-2xl shadow-black/50;
  }
  .menu-heading {
    @apply flex items-center justify-between border-b border-zinc-800 px-3 py-2 text-xs font-medium text-zinc-400;
  }
  .user-row {
    @apply flex items-center gap-2.5 rounded-md px-2 py-1.5 text-sm text-zinc-200 hover:bg-zinc-800;
  }
</style>
