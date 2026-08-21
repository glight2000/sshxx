<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import {
    ChevronDownIcon,
    Edit2Icon,
    FileTextIcon,
    MessageSquareIcon,
    PlusIcon,
    SearchIcon,
    SettingsIcon,
    Trash2Icon,
    WifiIcon,
  } from "svelte-feather-icons";
  import logo from "$lib/assets/logo.svg";
  import { settings } from "$lib/settings";
  import type { WsSshProfile, WsUser } from "$lib/protocol";
  import NameList from "./NameList.svelte";
  import SshProfileEditor from "./SshProfileEditor.svelte";

  export let connected: boolean;
  export let connectionStatus: "connected" | "connecting" | "unavailable";
  export let connectionDetail: string | null = null;
  export let hasWriteAccess: boolean | undefined;
  export let newMessages: boolean;
  export let profiles: WsSshProfile[];
  export let users: [number, WsUser][];

  const dispatch = createEventDispatcher<{
    create: void;
    createSsh: string;
    saveSshProfile: WsSshProfile;
    deleteSshProfile: string;
    createNote: void;
    chat: void;
    search: void;
    settings: void;
    networkInfo: void;
  }>();
  let connectionsOpen = false;
  let editorOpen = false;
  let editingProfile: WsSshProfile | null = null;
  let connectionControl: HTMLElement;

  function uniqueConnectionName() {
    const used = new Set(
      profiles.map((profile) => profile.name.toLocaleLowerCase()),
    );
    for (let suffix = 1; ; suffix += 1) {
      const name = suffix === 1 ? "SSH connection" : `SSH connection ${suffix}`;
      if (!used.has(name.toLocaleLowerCase())) return name;
    }
  }

  function newProfile(): WsSshProfile {
    const id = crypto.randomUUID
      ? crypto.randomUUID()
      : Array.from(crypto.getRandomValues(new Uint8Array(16)), (byte) =>
          byte.toString(16).padStart(2, "0"),
        ).join("");
    return {
      id,
      name: uniqueConnectionName(),
      host: "",
      port: 22,
      username: "",
      authMethod: "default",
      keyPath: "",
      acceptNewHostKey: true,
      theme: $settings.theme,
      backgroundEnabled: false,
      background: "#181818",
    };
  }

  function openEditor(profile: WsSshProfile) {
    connectionsOpen = false;
    editingProfile = { ...profile };
    editorOpen = true;
  }

  function handleWindowPointer(event: PointerEvent) {
    if (
      connectionsOpen &&
      event.target instanceof Node &&
      !connectionControl?.contains(event.target)
    )
      connectionsOpen = false;
  }

  function deleteProfile(profile: WsSshProfile) {
    if (window.confirm(`Delete SSH connection “${profile.name}”?`))
      dispatch("deleteSshProfile", profile.id);
  }

  $: networkTitle =
    connectionDetail ??
    (connectionStatus === "connected"
      ? "Connected"
      : connectionStatus === "connecting"
        ? "Connecting"
        : "Connection unavailable");
</script>

<svelte:window on:pointerdown={handleWindowPointer} />

<div class="panel inline-block px-3 py-2">
  <div class="flex items-center select-none">
    <a href="/" class="flex-shrink-0"
      ><img src={logo} alt="sshxx logo" class="h-8 w-8" /></a
    >
    <div class="v-divider"></div>

    <div class="flex space-x-1">
      <div class="relative flex" bind:this={connectionControl}>
        <button
          class="split-main"
          on:click={() => dispatch("create")}
          disabled={!connected || !hasWriteAccess}
          aria-label="Create a default terminal"
          title={!connected
            ? "Not connected"
            : hasWriteAccess === false
              ? "No write access"
              : "Create a default terminal"}>New terminal</button
        >
        <button
          class="split-arrow"
          class:active={connectionsOpen}
          on:click={() => (connectionsOpen = !connectionsOpen)}
          disabled={!connected || !hasWriteAccess}
          aria-label="SSH connections"
          aria-expanded={connectionsOpen}
          title="SSH connections"><ChevronDownIcon strokeWidth={1.5} /></button
        >
        {#if connectionsOpen}
          <div class="connection-menu">
            <div class="max-h-72 overflow-y-auto py-1">
              {#if profiles.length === 0}
                <p class="px-3 py-4 text-center text-xs text-zinc-500">
                  No saved SSH connections
                </p>
              {:else}
                {#each profiles as profile (profile.id)}
                  <div
                    class="connection-item"
                    role="button"
                    tabindex="0"
                    on:click={() => {
                      connectionsOpen = false;
                      dispatch("createSsh", profile.id);
                    }}
                    on:keydown={(event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        connectionsOpen = false;
                        dispatch("createSsh", profile.id);
                      }
                    }}
                  >
                    <div class="min-w-0 flex-1">
                      <p class="truncate text-sm text-zinc-200">
                        {profile.name}
                      </p>
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
                      on:click|stopPropagation={() => openEditor(profile)}
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
            <button
              class="add-connection"
              on:click={() => openEditor(newProfile())}
              ><PlusIcon />Add SSH connection</button
            >
          </div>
        {/if}
      </div>

      <button
        class="icon-button"
        on:click={() => dispatch("createNote")}
        disabled={!connected || !hasWriteAccess}
        title="Create note"><FileTextIcon strokeWidth={1.5} /></button
      >
      <button
        class="icon-button"
        aria-label="Chat"
        title="Chat"
        on:click={() => dispatch("chat")}
        ><MessageSquareIcon strokeWidth={1.5} />{#if newMessages}<div
            class="activity"
          ></div>{/if}</button
      >
      <button
        class="icon-button"
        aria-label="Find terminal or note"
        on:click={() => dispatch("search")}
        title="Find terminal or note"><SearchIcon strokeWidth={1.5} /></button
      >
    </div>

    <div class="v-divider"></div>
    <button
      class="icon-button"
      aria-label="Settings"
      title="Settings"
      on:click={() => dispatch("settings")}
      ><SettingsIcon strokeWidth={1.5} /></button
    >
    <div class="v-divider"></div>
    <button
      class="icon-button network-status {connectionStatus}"
      aria-label={networkTitle}
      title={networkTitle}
      on:click={() => dispatch("networkInfo")}
      ><WifiIcon strokeWidth={1.5} /></button
    >
    {#if users.length > 0}
      <div class="v-divider"></div>
      <NameList {users} />
    {/if}
  </div>
</div>

<SshProfileEditor
  open={editorOpen}
  profile={editingProfile}
  existingProfiles={profiles}
  on:close={() => (editorOpen = false)}
  on:save={(event) => {
    dispatch("saveSshProfile", event.detail);
    editorOpen = false;
  }}
/>

<style lang="postcss">
  @reference "../../app.css";
  .v-divider {
    @apply h-5 mx-2 border-l-4 border-zinc-800;
  }
  .icon-button {
    @apply relative inline-flex h-8 w-8 items-center justify-center rounded-md hover:bg-zinc-700 active:bg-indigo-700 transition-colors disabled:opacity-50 disabled:bg-transparent;
  }
  .icon-button :global(svg) {
    @apply h-5 w-5;
  }
  .network-status :global(svg) {
    @apply h-[22px] w-[22px];
  }
  .split-main,
  .split-arrow {
    @apply inline-flex h-8 items-center justify-center border border-zinc-700 bg-zinc-800/60 hover:bg-zinc-700 active:bg-indigo-700 transition-colors disabled:opacity-50 disabled:bg-zinc-800/30;
  }
  .split-main {
    @apply whitespace-nowrap rounded-l-md border-r-0 px-3 text-sm font-medium;
  }
  .split-arrow {
    @apply w-7 shrink-0 rounded-r-md p-0;
  }
  .split-arrow :global(svg) {
    @apply block h-3.5 w-3.5;
  }
  .split-arrow.active {
    @apply bg-zinc-700;
  }
  .connection-menu {
    @apply absolute left-0 top-[calc(100%+0.6rem)] z-50 w-80 overflow-hidden rounded-lg border border-zinc-700 bg-zinc-900 shadow-2xl shadow-black/50;
  }
  .connection-item {
    @apply mx-1 flex cursor-pointer items-center gap-1 rounded-md px-2 py-2 hover:bg-zinc-800 outline-none focus:bg-zinc-800;
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
  .activity {
    @apply absolute top-1 right-0.5 text-xs p-[4.5px] bg-red-500 rounded-full;
  }
  .network-status.connected {
    @apply text-emerald-300;
    animation: connected-glow 2.4s ease-in-out infinite;
  }
  .network-status.connecting {
    @apply text-amber-300;
    animation: connecting-pulse 1.2s ease-in-out infinite;
  }
  .network-status.unavailable {
    @apply text-red-400;
  }
  @keyframes connected-glow {
    50% {
      filter: drop-shadow(0 0 4px rgb(110 231 183 / 0.65));
    }
  }
  @keyframes connecting-pulse {
    50% {
      opacity: 0.42;
      transform: scale(0.9);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .network-status {
      animation: none !important;
    }
  }
</style>
