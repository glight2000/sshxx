<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { EyeIcon } from "svelte-feather-icons";

  import type { WsPage, WsSshProfile, WsUser } from "$lib/protocol";
  import Chat, { type ChatMessage } from "./Chat.svelte";
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
  export let chatMessages: ChatMessage[];
  export let settingsOpen: boolean;
  export let serverVersion: string;
  export let daemonVersion: string;
  export let systemActionsAvailable: boolean;
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
    toggleChat: void;
    openSettings: void;
    toggleSearch: void;
    selectSearch: CanvasSearchItem;
    toggleNetwork: void;
    chat: string;
    closeChat: void;
    closeSettings: void;
    restartDaemon: void;
    restartTerminalHost: void;
    selectPage: number;
    createPage: void;
    renamePage: { id: number; name: string };
  }>();
</script>

<div
  class="absolute top-8 inset-x-0 z-10 flex justify-center pointer-events-none"
>
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
    on:deleteSshProfile={(event) => dispatch("deleteSshProfile", event.detail)}
    on:createNote={() => dispatch("createNote")}
    on:chat={() => dispatch("toggleChat")}
    on:settings={() => dispatch("openSettings")}
    on:search={() => dispatch("toggleSearch")}
    on:networkInfo={() => dispatch("toggleNetwork")}
  />

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
  <div
    class="absolute flex flex-col justify-end inset-y-4 right-4 z-10 w-80 pointer-events-none"
  >
    <Chat
      {userId}
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
  {hasWriteAccess}
  {systemActionsAvailable}
  {systemActionPending}
  on:close={() => dispatch("closeSettings")}
  on:restartDaemon={() => dispatch("restartDaemon")}
  on:restartTerminalHost={() => dispatch("restartTerminalHost")}
/>

<ChooseName />

<PagePager
  {pages}
  {activePageId}
  {canvasDropPageId}
  {hasWriteAccess}
  on:select={(event) => dispatch("selectPage", event.detail)}
  on:create={() => dispatch("createPage")}
  on:rename={(event) => dispatch("renamePage", event.detail)}
/>

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
