<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";

  import FileWindowDialog from "./FileWindowDialog.svelte";

  export let open = false;
  export let kind: "file" | "directory" = "file";
  export let destination = "";
  export let busy = false;
  export let mode: "create" | "rename" = "create";
  export let initialName = "";

  const dispatch = createEventDispatcher<{
    close: void;
    create: string;
    rename: string;
  }>();
  let input: HTMLInputElement;
  let name = "";

  $: if (open) {
    name = mode === "rename" ? initialName : "";
    void tick().then(() => {
      input?.focus();
      input?.select();
    });
  }
  $: validation = validateName(name);

  function validateName(value: string) {
    const candidate = value.trim();
    if (!candidate) return "Enter a name.";
    if (candidate === "." || candidate === "..")
      return "This name is reserved.";
    if (candidate.length > 255) return "The name is too long.";
    if (/[\\/\0\u0000-\u001f<>:"|?*]/.test(candidate))
      return "The name contains unsupported characters.";
    if (/[. ]$/.test(candidate))
      return "The name cannot end with a dot or space.";
    if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i.test(candidate))
      return "This name is reserved by Windows.";
    return "";
  }

  function submit() {
    if (validation || busy) return;
    dispatch(mode === "rename" ? "rename" : "create", name.trim());
  }
</script>

<FileWindowDialog
  {open}
  title={mode === "rename"
    ? `Rename ${kind}`
    : kind === "file"
      ? "Create file"
      : "Create folder"}
  description={mode === "rename"
    ? `Rename the item inside ${destination}`
    : `Create it inside ${destination}`}
  {busy}
  maxWidth={520}
  on:close={() => !busy && dispatch("close")}
>
  <form class="space-y-4" on:submit|preventDefault={submit}>
    <label class="block">
      <span class="mb-1.5 block text-sm text-zinc-400">Name</span>
      <input
        bind:this={input}
        bind:value={name}
        class="w-full rounded-md border border-zinc-700 bg-zinc-950 px-3 py-2 text-zinc-100 outline-none focus:border-indigo-500 focus:ring-2 focus:ring-indigo-500/25"
        autocomplete="off"
        spellcheck="false"
        disabled={busy}
        on:keydown={(event) => event.key === "Escape" && dispatch("close")}
      />
      {#if name && validation}<span class="mt-1.5 block text-xs text-red-300"
          >{validation}</span
        >{/if}
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
        disabled={busy || !!validation}
      >
        {busy
          ? mode === "rename"
            ? "Renaming…"
            : "Creating…"
          : mode === "rename"
            ? "Rename"
            : "Create"}
      </button>
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
