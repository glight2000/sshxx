<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";

  import FileWindowDialog from "./FileWindowDialog.svelte";

  export let open = false;
  export let source = "";
  export let initialDestination = "";
  export let busy = false;

  const dispatch = createEventDispatcher<{ close: void; move: string }>();
  let input: HTMLInputElement;
  let destination = "";

  $: if (open) {
    destination = initialDestination;
    void tick().then(() => {
      input?.focus();
      input?.select();
    });
  }
  $: validation = validateDestination(destination);

  function validateDestination(value: string) {
    const candidate = value.trim();
    if (!candidate) return "Enter a destination folder.";
    if (candidate.includes("\0") || /[\u0000-\u001f]/.test(candidate))
      return "The destination path contains unsupported characters.";
    if (candidate.length > 16_384) return "The destination path is too long.";
    return "";
  }

  function submit() {
    if (validation || busy) return;
    dispatch("move", destination.trim());
  }
</script>

<FileWindowDialog
  {open}
  title="Move item"
  description={`Move ${source}`}
  {busy}
  maxWidth={620}
  on:close={() => !busy && dispatch("close")}
>
  <form class="space-y-4" on:submit|preventDefault={submit}>
    <label class="block">
      <span class="mb-1.5 block text-sm text-zinc-400">Destination folder</span>
      <input
        bind:this={input}
        bind:value={destination}
        class="w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 font-mono text-sm text-zinc-100 outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/25"
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
        on:keydown={(event) => event.key === "Escape" && dispatch("close")}
      />
      {#if destination && validation}<span
          class="mt-1.5 block text-xs text-red-300">{validation}</span
        >{/if}
      <span class="mt-1.5 block text-xs text-zinc-500"
        >The original name is preserved. Moving across filesystems may not be
        supported by the remote host.</span
      >
    </label>
    <div class="flex justify-end gap-2">
      <button
        type="button"
        class="secondary-button"
        disabled={busy}
        on:click={() => dispatch("close")}>Cancel</button
      >
      <button
        type="submit"
        class="primary-button"
        disabled={busy || !!validation}>{busy ? "Moving…" : "Move"}</button
      >
    </div>
  </form>
</FileWindowDialog>

<style lang="postcss">
  @reference "../../app.css";
  .primary-button {
    @apply rounded-md bg-indigo-600 px-4 py-2 text-sm text-white hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-40;
  }
  .secondary-button {
    @apply rounded-md border border-zinc-700 px-4 py-2 text-sm text-zinc-300 hover:bg-zinc-800 disabled:opacity-40;
  }
</style>
