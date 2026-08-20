<script lang="ts">
  import { createEventDispatcher } from "svelte";

  import type { WsSshProfile } from "$lib/protocol";
  import OverlayMenu from "./OverlayMenu.svelte";

  export let open: boolean;
  export let profile: WsSshProfile | null;
  export let existingProfiles: WsSshProfile[];

  const dispatch = createEventDispatcher<{ close: void; save: WsSshProfile }>();

  let draft: WsSshProfile;
  let error = "";
  let initializedFor = "";

  $: initializationKey = open ? (profile?.id ?? "new") : "closed";
  $: if (initializationKey !== initializedFor) {
    initializedFor = initializationKey;
    if (open && profile) draft = { ...profile };
    error = "";
  }

  function save() {
    draft = {
      ...draft,
      name: draft.name.trim(),
      host: draft.host.trim(),
      username: draft.username.trim(),
      keyPath: draft.keyPath.trim(),
      port: Number(draft.port),
    };
    if (!draft.name || !draft.host) {
      error = "Name and host are required.";
      return;
    }
    if (!Number.isInteger(draft.port) || draft.port < 1 || draft.port > 65535) {
      error = "Port must be between 1 and 65535.";
      return;
    }
    if (draft.authMethod === "keyFile" && !draft.keyPath) {
      error = "Private key path is required for key-file authentication.";
      return;
    }
    if (
      existingProfiles.some(
        (item) =>
          item.id !== draft.id &&
          item.name.toLocaleLowerCase() === draft.name.toLocaleLowerCase(),
      )
    ) {
      error = "Connection names must be unique.";
      return;
    }
    dispatch("save", draft);
  }
</script>

<OverlayMenu
  title={profile && existingProfiles.some((item) => item.id === profile.id)
    ? "Edit SSH Connection"
    : "Add SSH Connection"}
  description="Save OpenSSH connection settings on the daemon host."
  showCloseButton
  maxWidth={640}
  {open}
  on:close={() => dispatch("close")}
>
  {#if open && profile}
    <form class="flex flex-col gap-4" on:submit|preventDefault={save}>
      <label class="field"
        ><span>Connection name</span><input
          class="input-common"
          bind:value={draft.name}
          maxlength="100"
        /></label
      >
      <div class="grid grid-cols-[1fr_8rem] gap-3">
        <label class="field"
          ><span>Host or SSH config alias</span><input
            class="input-common"
            bind:value={draft.host}
            maxlength="255"
            placeholder="server.example.com"
          /></label
        >
        <label class="field"
          ><span>Port</span><input
            class="input-common"
            type="number"
            min="1"
            max="65535"
            bind:value={draft.port}
          /></label
        >
      </div>
      <label class="field"
        ><span>Username <small>(optional)</small></span><input
          class="input-common"
          bind:value={draft.username}
          maxlength="100"
          autocomplete="username"
        /></label
      >
      <label class="field">
        <span>Authentication</span>
        <select class="input-common" bind:value={draft.authMethod}>
          <option value="default">OpenSSH default / config</option>
          <option value="agent">SSH agent</option>
          <option value="keyFile">Private key file</option>
          <option value="password">Password prompt</option>
        </select>
      </label>
      {#if draft.authMethod === "keyFile"}
        <label class="field"
          ><span>Private key path on daemon host</span><input
            class="input-common"
            bind:value={draft.keyPath}
            maxlength="4096"
            placeholder="/home/user/.ssh/id_ed25519"
          /></label
        >
      {/if}
      {#if draft.authMethod === "password"}
        <p
          class="rounded-md border border-zinc-700 bg-zinc-800/50 px-3 py-2 text-xs text-zinc-400"
        >
          The password is never saved. OpenSSH asks for it inside the terminal
          each time.
        </p>
      {/if}
      <label
        class="flex items-center gap-2 text-sm text-zinc-300 cursor-pointer"
        ><input
          type="checkbox"
          class="h-4 w-4 accent-indigo-500"
          bind:checked={draft.acceptNewHostKey}
        />Automatically accept a host key on first connection</label
      >
      {#if error}<p class="text-sm text-red-400" role="alert">{error}</p>{/if}
      <div class="flex justify-end gap-2 pt-2">
        <button
          type="button"
          class="secondary-button"
          on:click={() => dispatch("close")}>Cancel</button
        >
        <button type="submit" class="primary-button">Save connection</button>
      </div>
    </form>
  {/if}
</OverlayMenu>

<style lang="postcss">
  @reference "../../app.css";
  .field {
    @apply flex flex-col gap-1.5 text-sm text-zinc-300;
  }
  .field small {
    @apply text-zinc-500;
  }
  .input-common {
    @apply w-full px-3 py-2 rounded-md bg-zinc-900 border border-zinc-700 outline-none focus:ring-2 focus:ring-indigo-500/50 focus:border-indigo-500 transition-colors;
  }
  .primary-button {
    @apply rounded-md bg-indigo-600 px-4 py-2 text-sm hover:bg-indigo-500 active:bg-indigo-700;
  }
  .secondary-button {
    @apply rounded-md border border-zinc-700 px-4 py-2 text-sm hover:bg-zinc-800 active:bg-zinc-700;
  }
</style>
