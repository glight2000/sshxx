<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import {
    AlertTriangleIcon,
    ChevronDownIcon,
    RefreshCwIcon,
  } from "svelte-feather-icons";

  import { settings, updateSettings } from "$lib/settings";
  import OverlayMenu from "./OverlayMenu.svelte";
  import themes, { type ThemeName } from "./themes";
  import type { ColorModePreference } from "$lib/colorMode";

  export let open: boolean;
  export let serverVersion: string;
  export let daemonVersion: string;
  export let hasWriteAccess: boolean | undefined;
  export let systemActionsAvailable: boolean;
  export let systemActionPending: boolean;

  const dispatch = createEventDispatcher<{
    restartDaemon: void;
    restartTerminalHost: void;
  }>();

  function restartDaemon() {
    if (
      window.confirm(
        "Restart the daemon control channel? Hosted terminal processes will remain running.",
      )
    ) {
      dispatch("restartDaemon");
    }
  }

  function restartTerminalHost() {
    if (
      window.confirm(
        "Restart terminal host? This will terminate every running terminal process. Saved SSH terminals can be recreated, but local terminal processes cannot be recovered.",
      )
    ) {
      dispatch("restartTerminalHost");
    }
  }

  let inputName: string;
  let inputTheme: ThemeName;
  let inputScrollback: number;
  let inputSnapToGrid: boolean;
  let inputSwapCanvasMouseButtons: boolean;
  let inputColorMode: ColorModePreference;

  let initialized = false;
  $: (open, (initialized = false));
  $: if (!initialized) {
    initialized = true;
    inputName = $settings.name;
    inputTheme = $settings.theme;
    inputScrollback = $settings.scrollback;
    inputSnapToGrid = $settings.snapToGrid;
    inputSwapCanvasMouseButtons = $settings.swapCanvasMouseButtons;
    inputColorMode = $settings.colorMode;
  }
</script>

<OverlayMenu
  title="Settings"
  description="Customize the workspace and manage its runtime."
  showCloseButton
  {open}
  on:close
>
  <div class="flex flex-col gap-4">
    <div class="item">
      <div>
        <p class="item-title">Snap to grid</p>
        <p class="item-subtitle">
          Snap moved top-left and resized bottom-right corners to the canvas
          grid.
        </p>
      </div>
      <label class="flex items-center gap-2 py-2 text-sm cursor-pointer">
        <input
          type="checkbox"
          class="h-4 w-4 accent-indigo-500"
          bind:checked={inputSnapToGrid}
          on:change={() => updateSettings({ snapToGrid: inputSnapToGrid })}
        />
        Enabled
      </label>
    </div>
    <div class="item runtime-item">
      <div>
        <p class="item-title">Runtime</p>
        <p class="item-subtitle">
          Restart daemon-owned runtime services without granting the browser
          operating-system service permissions.
        </p>
      </div>
      <div class="runtime-actions">
        <button
          type="button"
          class="runtime-button"
          disabled={hasWriteAccess !== true ||
            !systemActionsAvailable ||
            systemActionPending}
          on:click={restartDaemon}
        >
          <RefreshCwIcon size="15" />
          Restart daemon
        </button>
        <button
          type="button"
          class="runtime-button danger"
          disabled={hasWriteAccess !== true ||
            !systemActionsAvailable ||
            systemActionPending}
          on:click={restartTerminalHost}
        >
          <AlertTriangleIcon size="15" />
          Restart terminal host
        </button>
        {#if !systemActionsAvailable}
          <span class="runtime-hint"
            >Update server and daemon to use runtime controls.</span
          >
        {:else if hasWriteAccess !== true}
          <span class="runtime-hint">Write access is required.</span>
        {/if}
      </div>
    </div>
    <div class="item">
      <div>
        <p class="item-title">Canvas mouse buttons</p>
        <p class="item-subtitle">
          Exchange blank-canvas selection and movement. Component controls and
          middle-button movement are unchanged.
        </p>
      </div>
      <label class="flex items-center gap-2 py-2 text-sm cursor-pointer">
        <input
          type="checkbox"
          class="h-4 w-4 accent-indigo-500"
          bind:checked={inputSwapCanvasMouseButtons}
          on:change={() =>
            updateSettings({
              swapCanvasMouseButtons: inputSwapCanvasMouseButtons,
            })}
        />
        Swap buttons
      </label>
    </div>
    <div class="item">
      <div>
        <p class="item-title">Appearance</p>
        <p class="item-subtitle">
          Choose the application UI mode. Terminal and note colors are not
          changed.
        </p>
      </div>
      <div class="relative">
        <ChevronDownIcon
          class="absolute top-[11px] right-2.5 w-4 h-4 text-zinc-400"
        />
        <select
          class="input-common !pr-5"
          bind:value={inputColorMode}
          on:change={() => updateSettings({ colorMode: inputColorMode })}
        >
          <option value="system">Follow system</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </div>
    </div>
    <div class="item">
      <div>
        <p class="item-title">Name</p>
        <p class="item-subtitle">Choose how you appear to other users.</p>
      </div>
      <div>
        <input
          class="input-common"
          placeholder="Your name"
          bind:value={inputName}
          maxlength="50"
          on:input={() => {
            if (inputName.length >= 2) {
              updateSettings({ name: inputName });
            }
          }}
        />
      </div>
    </div>
    <div class="item">
      <div>
        <p class="item-title">Color palette</p>
        <p class="item-subtitle">
          Default color theme for newly created terminals.
        </p>
      </div>
      <div class="relative">
        <ChevronDownIcon
          class="absolute top-[11px] right-2.5 w-4 h-4 text-zinc-400"
        />
        <select
          class="input-common !pr-5"
          bind:value={inputTheme}
          on:change={() => updateSettings({ theme: inputTheme })}
        >
          {#each Object.keys(themes) as themeName (themeName)}
            <option value={themeName}>{themeName}</option>
          {/each}
        </select>
      </div>
    </div>
    <div class="item">
      <div>
        <p class="item-title">Scrollback</p>
        <p class="item-subtitle">
          Lines of previous text displayed in the terminal window.
        </p>
      </div>
      <div>
        <input
          type="number"
          class="input-common"
          bind:value={inputScrollback}
          on:input={() => {
            if (inputScrollback >= 0) {
              updateSettings({ scrollback: inputScrollback });
            }
          }}
          step="100"
        />
      </div>
    </div>
    <!-- <div class="item">
      <div>
        <p class="item-title">Cursor style</p>
        <p class="item-subtitle">Style of live cursors.</p>
      </div>
      <div class="text-red-500">Coming soon</div>
    </div> -->
  </div>

  <!-- svelte-ignore missing-declaration -->
  <div class="mt-6 flex flex-col items-end text-xs leading-5 text-zinc-400">
    <div class="inline-flex flex-col items-end">
      <span>sshxx-client v{__APP_VERSION__}</span>
      <span>sshxx-server v{serverVersion}</span>
      <span>sshxx-daemon v{daemonVersion}</span>
    </div>
    <a
      class="underline decoration-zinc-600 underline-offset-2 hover:text-zinc-300"
      target="_blank"
      rel="noreferrer"
      href="https://github.com/ekzhang/sshx">Based on sshx</a
    >
  </div>
</OverlayMenu>

<style lang="postcss">
  @reference "../../app.css";

  .item {
    @apply bg-zinc-800/25 rounded-lg p-4 flex gap-4 flex-col sm:flex-row items-start;
  }

  .item > div:first-child {
    @apply flex-1;
  }

  .item-title {
    @apply font-medium text-zinc-200 mb-1;
  }

  .item-subtitle {
    @apply text-sm text-zinc-400;
  }

  .input-common {
    @apply w-52 px-3 py-2 text-sm rounded-md bg-transparent hover:bg-white/5;
    @apply border border-zinc-700 outline-none focus:ring-2 focus:ring-indigo-500/50;
    @apply appearance-none transition-colors;
  }

  .runtime-item {
    @apply border border-zinc-700/60;
  }

  .runtime-actions {
    @apply w-full sm:w-52 flex flex-col gap-2;
  }

  .runtime-button {
    @apply inline-flex items-center justify-center gap-2 rounded-md border border-zinc-600;
    @apply px-3 py-2 text-sm text-zinc-200 bg-zinc-800/70 transition-colors;
    @apply hover:bg-zinc-700/80 disabled:cursor-not-allowed disabled:opacity-40;
  }

  .runtime-button.danger {
    @apply border-red-800/80 text-red-200 hover:bg-red-950/70;
  }

  .runtime-hint {
    @apply text-xs leading-4 text-zinc-500;
  }

  select.input-common {
    background-color: var(--control-bg);
    color: var(--control-text);
  }

  select.input-common option {
    background-color: var(--control-bg);
    color: var(--control-text);
  }
</style>
