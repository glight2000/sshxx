<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { EyeIcon } from "svelte-feather-icons";

  import type { WsPage, WsSshProfile, WsUser } from "$lib/protocol";
  import Chat from "./Chat.svelte";
  import type { ChatRecord, WorkspaceAttachment } from "$lib/protocol";
  import type { WorkspaceMedia } from "$lib/workspaceMedia";
  import ChooseName from "./ChooseName.svelte";
  import NetworkInfo from "./NetworkInfo.svelte";
  import PagePager from "./PagePager.svelte";
  import Settings from "./Settings.svelte";
  import TerminalSearch, {
    type CanvasSearchItem,
  } from "./TerminalSearch.svelte";
  import Toolbar from "./Toolbar.svelte";

  export let connected: boolean;
  export let connectionStatus: "connected" | "connecting" | "unavailable";
  export let connectionDetail: string | null;
  export let failureStage: "server" | "session" | null;
  export let newMessages: boolean;
  export let hasWriteAccess: boolean | undefined;
  export let profiles: WsSshProfile[];
  export let users: [number, WsUser][];
  export let searchOpen: boolean;
  export let searchItems: CanvasSearchItem[];
  export let showNetworkInfo: boolean;
  export let serverLatency: number | null;
  export let shellLatency: number | null;
  export let showChat: boolean;
  export let userId: number;
  export let chatMessages: ChatRecord[];
  export let media: WorkspaceMedia | null;
  export let settingsOpen: boolean;
  export let serverVersion: string;
  export let daemonVersion: string;
  export let terminalHostVersion: string;
  export let systemActionsAvailable: boolean;
  export let runtimeInfo: {
    terminalHostVersion: string;
    releaseVersion: string;
    updateStatus: string;
  } | null;
  export let systemActionPending: boolean;
  export let pages: WsPage[];
  export let activePageId: number;
  export let canvasDropPageId: number | null;

  const dispatch = createEventDispatcher<{
    create: void;
    createSsh: string;
    saveSshProfile: WsSshProfile;
    deleteSshProfile: string;
    createNote: void;
    createCustom: void;
    toggleChat: void;
    openSettings: void;
    toggleSearch: void;
    selectSearch: CanvasSearchItem;
    toggleNetwork: void;
    chat: { text: string; attachments: WorkspaceAttachment[] };
    closeChat: void;
    closeSettings: void;
    restartDaemon: void;
    updateRuntime: void;
    restartTerminalHost: void;
    selectPage: number;
    createPage: void;
    renamePage: { id: number; name: string };
    deletePage: number;
  }>();
</script>

<div
  class="workspace-chrome absolute top-8 inset-x-0 z-10 flex justify-center pointer-events-none"
>
  <div class="desktop-toolbar contents">
    <Toolbar
      {connected}
      {connectionStatus}
      {connectionDetail}
      {newMessages}
      {hasWriteAccess}
      {profiles}
      {users}
      on:create={() => dispatch("create")}
      on:createSsh={(event) => dispatch("createSsh", event.detail)}
      on:saveSshProfile={(event) => dispatch("saveSshProfile", event.detail)}
      on:deleteSshProfile={(event) =>
        dispatch("deleteSshProfile", event.detail)}
      on:createNote={() => dispatch("createNote")}
      on:createCustom={() => dispatch("createCustom")}
      on:chat={() => dispatch("toggleChat")}
      on:settings={() => dispatch("openSettings")}
      on:search={() => dispatch("toggleSearch")}
      on:networkInfo={() => dispatch("toggleNetwork")}
    />
  </div>
  <TerminalSearch
    open={searchOpen}
    items={searchItems}
    on:close={() => dispatch("toggleSearch")}
    on:select={(event) => dispatch("selectSearch", event.detail)}
  />

  {#if showNetworkInfo}
    <div class="absolute top-20 translate-x-[116.5px]">
      <NetworkInfo
        status={connectionStatus === "connected"
          ? "connected"
          : connectionDetail
            ? failureStage === "session"
              ? "no-shell"
              : "no-server"
            : "no-server"}
        {serverLatency}
        {shellLatency}
        detail={connectionDetail}
      />
    </div>
  {/if}
</div>

{#if showChat}
  <div class="chat-surface">
    <Chat
      {media}
      {connected}
      canUpload={Boolean(hasWriteAccess && media)}
      messages={chatMessages}
      on:chat={(event) => dispatch("chat", event.detail)}
      on:close={() => dispatch("closeChat")}
    />
  </div>
{/if}

<Settings
  open={settingsOpen}
  {serverVersion}
  {daemonVersion}
  {terminalHostVersion}
  {hasWriteAccess}
  {systemActionsAvailable}
  {runtimeInfo}
  {systemActionPending}
  on:close={() => dispatch("closeSettings")}
  on:restartDaemon={() => dispatch("restartDaemon")}
  on:updateRuntime={() => dispatch("updateRuntime")}
  on:restartTerminalHost={() => dispatch("restartTerminalHost")}
/>

<ChooseName />

<div class="desktop-page-pager contents">
  <PagePager
    {pages}
    {activePageId}
    {canvasDropPageId}
    {hasWriteAccess}
    on:select={(event) => dispatch("selectPage", event.detail)}
    on:create={() => dispatch("createPage")}
    on:rename={(event) => dispatch("renamePage", event.detail)}
    on:delete={(event) => dispatch("deletePage", event.detail)}
  />
</div>

<div class="py-2">
  {#if userId && hasWriteAccess === false}
    <div
      class="bg-yellow-900 text-yellow-200 px-1 py-0.5 rounded inline-flex items-center gap-1"
    >
      <EyeIcon size="14" />
      <span class="text-xs">Read-only</span>
    </div>
  {/if}
</div>

<style>
  .chat-surface {
    position: absolute;
    inset: 0 0 0 auto;
    z-index: 40;
    width: min(390px, 100%);
    pointer-events: auto;
  }
  :global(.mobile-mode) .chat-surface {
    position: fixed;
    inset: auto;
    top: var(--mobile-viewport-top, 0px);
    left: var(--mobile-viewport-left, 0px);
    width: var(--mobile-viewport-width, 100vw);
    height: var(--mobile-viewport-height, 100dvh);
    z-index: 100;
  }
</style>
