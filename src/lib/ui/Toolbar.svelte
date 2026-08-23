<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import {
    ChevronDownIcon,
    CodeIcon,
    FileTextIcon,
    MessageSquareIcon,
    SearchIcon,
    SettingsIcon,
    WifiIcon,
  } from "svelte-feather-icons";
  import logo from "$lib/assets/logo.svg";
  import { settings } from "$lib/settings";
  import type { WsSshProfile, WsUser } from "$lib/protocol";
  import { createSshProfileDraft } from "$lib/sshProfiles";
  import NameList from "./NameList.svelte";
  import SshProfileEditor from "./SshProfileEditor.svelte";
  import SshProfileList from "./SshProfileList.svelte";

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
    createCustom: void;
    chat: void;
    search: void;
    settings: void;
    networkInfo: void;
  }>();
  let connectionsOpen = false;
  let editorOpen = false;
  let editingProfile: WsSshProfile | null = null;
  let connectionControl: HTMLElement;

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
            <SshProfileList
              {profiles}
              on:select={(event) => {
                connectionsOpen = false;
                dispatch("createSsh", event.detail);
              }}
              on:edit={(event) => openEditor(event.detail)}
              on:delete={(event) => dispatch("deleteSshProfile", event.detail)}
              on:add={() =>
                openEditor(createSshProfileDraft(profiles, $settings.theme))}
            />
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
        on:click={() => dispatch("createCustom")}
        disabled={!connected || !hasWriteAccess}
        title="Create custom component"
        aria-label="Create custom component"
        ><CodeIcon strokeWidth={1.5} /></button
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
    <div class="v-divider"></div>
    <div class="flex items-center gap-1">
      <button
        class="icon-button"
        aria-label="Chat"
        title="Chat"
        on:click={() => dispatch("chat")}
        ><MessageSquareIcon strokeWidth={1.5} />{#if newMessages}<div
            class="activity"
          ></div>{/if}</button
      >
      {#if users.length > 0}<NameList {users} />{/if}
    </div>
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
