<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import {
    ChevronRightIcon,
    CodeIcon,
    FileTextIcon,
    SearchIcon,
    SettingsIcon,
    TerminalIcon,
  } from "svelte-feather-icons";

  import type { WsSshProfile } from "$lib/protocol";
  import { settings } from "$lib/settings";
  import { createSshProfileDraft } from "$lib/sshProfiles";
  import SshProfileEditor from "./SshProfileEditor.svelte";
  import SshProfileList from "./SshProfileList.svelte";

  export let open: boolean;
  export let x: number;
  export let y: number;
  export let connected: boolean;
  export let hasWriteAccess: boolean | undefined;
  export let profiles: WsSshProfile[];

  const dispatch = createEventDispatcher<{
    close: void;
    create: void;
    createSsh: string;
    saveSshProfile: WsSshProfile;
    deleteSshProfile: string;
    createNote: void;
    createCustom: void;
    search: void;
    settings: void;
  }>();

  let menu: HTMLDivElement;
  let menuX = 0;
  let menuY = 0;
  let submenuLeft = false;
  let positionKey = "";
  let connectionsOpen = false;
  let editorOpen = false;
  let editingProfile: WsSshProfile | null = null;

  $: nextPositionKey = open ? `${x}:${y}` : "closed";
  $: if (nextPositionKey !== positionKey) {
    positionKey = nextPositionKey;
    connectionsOpen = false;
    if (open) {
      menuX = x;
      menuY = y;
      void tick().then(positionMenu);
    }
  }

  function positionMenu() {
    if (!menu) return;
    const margin = 8;
    const bounds = menu.getBoundingClientRect();
    menuX = Math.max(
      margin,
      Math.min(x, window.innerWidth - bounds.width - margin),
    );
    menuY = Math.max(
      margin,
      Math.min(y, window.innerHeight - bounds.height - margin),
    );
    submenuLeft = menuX + bounds.width + 328 + margin > window.innerWidth;
  }

  function closeAndDispatch(
    action: "create" | "createNote" | "createCustom" | "search" | "settings",
  ) {
    dispatch("close");
    dispatch(action);
  }

  function createSsh(profileId: string) {
    dispatch("close");
    dispatch("createSsh", profileId);
  }

  function openEditor(profile: WsSshProfile) {
    dispatch("close");
    editingProfile = { ...profile };
    editorOpen = true;
  }

  function handleWindowPointerDown(event: PointerEvent) {
    if (open && event.target instanceof Node && !menu?.contains(event.target))
      dispatch("close");
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    if (!open || event.key !== "Escape") return;
    event.preventDefault();
    dispatch("close");
  }
</script>

<svelte:window
  on:pointerdown|capture={handleWindowPointerDown}
  on:keydown={handleWindowKeydown}
  on:resize={positionMenu}
/>

{#if open}
  <div
    bind:this={menu}
    class="canvas-context-menu"
    style:left="{menuX}px"
    style:top="{menuY}px"
    role="menu"
    aria-label="Canvas actions"
    tabindex="-1"
    on:contextmenu|preventDefault|stopPropagation
    on:pointerdown|stopPropagation
  >
    <button
      role="menuitem"
      disabled={!connected || !hasWriteAccess}
      on:click={() => closeAndDispatch("create")}
    >
      <TerminalIcon />
      <span>New default terminal</span>
    </button>

    <div class="submenu-control">
      <button
        role="menuitem"
        aria-haspopup="menu"
        aria-expanded={connectionsOpen}
        disabled={!connected || !hasWriteAccess}
        on:mouseenter={() => (connectionsOpen = true)}
        on:click={() => (connectionsOpen = !connectionsOpen)}
      >
        <TerminalIcon />
        <span>SSH connections</span>
        <ChevronRightIcon class="chevron" />
      </button>

      {#if connectionsOpen}
        <div
          class:open-left={submenuLeft}
          class="ssh-submenu"
          role="menu"
          aria-label="SSH connections"
        >
          <SshProfileList
            {profiles}
            on:select={(event) => createSsh(event.detail)}
            on:edit={(event) => openEditor(event.detail)}
            on:delete={(event) => {
              dispatch("close");
              dispatch("deleteSshProfile", event.detail);
            }}
            on:add={() =>
              openEditor(createSshProfileDraft(profiles, $settings.theme))}
          />
        </div>
      {/if}
    </div>

    <button
      role="menuitem"
      disabled={!connected || !hasWriteAccess}
      on:click={() => closeAndDispatch("createNote")}
    >
      <FileTextIcon class="note-icon" />
      <span>New note</span>
    </button>

    <button
      role="menuitem"
      disabled={!connected || !hasWriteAccess}
      on:click={() => closeAndDispatch("createCustom")}
    >
      <CodeIcon />
      <span>New custom component</span>
    </button>

    <div class="divider"></div>

    <button role="menuitem" on:click={() => closeAndDispatch("search")}>
      <SearchIcon />
      <span>Find terminal or note</span>
    </button>
    <button role="menuitem" on:click={() => closeAndDispatch("settings")}>
      <SettingsIcon />
      <span>Settings</span>
    </button>
  </div>
{/if}

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
  .canvas-context-menu {
    @apply fixed z-[70] w-64 rounded-lg border border-zinc-700 bg-zinc-900 p-1.5 shadow-2xl shadow-black/55 select-none;
  }
  .canvas-context-menu > button,
  .submenu-control > button {
    @apply flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-sm text-zinc-200 outline-none hover:bg-zinc-800 focus:bg-zinc-800 disabled:pointer-events-none disabled:opacity-40;
  }
  .canvas-context-menu button :global(svg) {
    @apply h-4 w-4 shrink-0;
  }
  .canvas-context-menu button :global(.note-icon) {
    @apply text-amber-300;
  }
  .canvas-context-menu button :global(.chevron) {
    @apply ml-auto h-3.5 w-3.5 text-zinc-500;
  }
  .submenu-control {
    @apply relative;
  }
  .ssh-submenu {
    @apply absolute left-[calc(100%+0.45rem)] top-0 z-[71] w-80 overflow-hidden rounded-lg border border-zinc-700 bg-zinc-900 shadow-2xl shadow-black/55;
  }
  .ssh-submenu.open-left {
    @apply left-auto right-[calc(100%+0.45rem)];
  }
  .divider {
    @apply my-1 border-t border-zinc-700/80;
  }
</style>
