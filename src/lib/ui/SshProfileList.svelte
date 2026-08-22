<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { Edit2Icon, PlusIcon, Trash2Icon } from "svelte-feather-icons";

  import type { WsSshProfile } from "$lib/protocol";

  export let profiles: WsSshProfile[];

  const dispatch = createEventDispatcher<{
    select: string;
    edit: WsSshProfile;
    delete: string;
    add: void;
  }>();

  function deleteProfile(profile: WsSshProfile) {
    if (window.confirm(`Delete SSH connection “${profile.name}”?`))
      dispatch("delete", profile.id);
  }
</script>

<div class="max-h-72 overflow-y-auto py-1">
  {#if profiles.length === 0}
    <p class="px-3 py-4 text-center text-xs text-zinc-500">
      No saved SSH connections
    </p>
  {:else}
    {#each profiles as profile (profile.id)}
      <div
        class="connection-item"
        role="menuitem"
        tabindex="0"
        on:click={() => dispatch("select", profile.id)}
        on:keydown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            dispatch("select", profile.id);
          }
        }}
      >
        <div class="min-w-0 flex-1">
          <p class="truncate text-sm text-zinc-200">{profile.name}</p>
          <p class="truncate text-xs text-zinc-500">
            {profile.username
              ? `${profile.username}@`
              : ""}{profile.host}:{profile.port}
          </p>
        </div>
        <button
          class="item-action"
          aria-label="Edit {profile.name}"
          title="Edit"
          on:click|stopPropagation={() => dispatch("edit", profile)}
          ><Edit2Icon /></button
        >
        <button
          class="item-action danger"
          aria-label="Delete {profile.name}"
          title="Delete"
          on:click|stopPropagation={() => deleteProfile(profile)}
          ><Trash2Icon /></button
        >
      </div>
    {/each}
  {/if}
</div>
<button class="add-connection" on:click={() => dispatch("add")}
  ><PlusIcon />Add SSH connection</button
>

<style lang="postcss">
  @reference "../../app.css";
  .connection-item {
    @apply mx-1 flex cursor-pointer items-center gap-1 rounded-md px-2 py-2 outline-none hover:bg-zinc-800 focus:bg-zinc-800;
  }
  .item-action {
    @apply inline-flex h-6 w-6 shrink-0 items-center justify-center rounded p-0 text-zinc-400 hover:bg-zinc-700 hover:text-zinc-100;
  }
  .item-action :global(svg) {
    @apply h-3.5 w-3.5;
  }
  .item-action.danger {
    @apply hover:bg-red-950 hover:text-red-300;
  }
  .add-connection {
    @apply flex w-full items-center gap-2 border-t border-zinc-700 px-3 py-2.5 text-sm text-zinc-300 hover:bg-zinc-800;
  }
  .add-connection :global(svg) {
    @apply h-4 w-4;
  }
</style>
