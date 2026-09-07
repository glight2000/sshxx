<script lang="ts">
  import { settings, updateSettings } from "$lib/settings";
  import {
    WORKSPACE_SHORTCUTS,
    formatShortcut,
    normalizeShortcutBindings,
    shortcutFromEvent,
    type WorkspaceShortcut,
  } from "$lib/workspaceShortcuts";

  let recording: WorkspaceShortcut | null = null;
  let error = "";
  function save(id: WorkspaceShortcut, binding: string | null) {
    updateSettings({
      shortcutBindings: { ...$settings.shortcutBindings, [id]: binding },
    });
    recording = null;
    error = "";
  }
  function capture(event: KeyboardEvent, id: WorkspaceShortcut) {
    if (recording !== id) return;
    if (
      event.key === "Tab" &&
      !event.ctrlKey &&
      !event.altKey &&
      !event.metaKey
    ) {
      recording = null;
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      recording = null;
      error = "";
      return;
    }
    if (event.repeat || ["Control", "Alt", "Shift", "Meta"].includes(event.key))
      return;
    const binding = shortcutFromEvent(event);
    if (!binding) {
      error =
        "Use Ctrl, Alt or Meta with a key, or F1–F12. Finish IME composition first.";
      return;
    }
    const conflict = WORKSPACE_SHORTCUTS.find(
      (item) =>
        item.id !== id && $settings.shortcutBindings[item.id] === binding,
    );
    if (conflict) {
      error = `Already assigned to ${conflict.label}. Disable or change that shortcut first.`;
      return;
    }
    save(id, binding);
  }
</script>

<details
  class="shortcuts"
  on:toggle={() => {
    recording = null;
    error = "";
  }}
>
  <summary>Keyboard shortcuts <span>Desktop · This browser only</span></summary>
  <p>
    Click a shortcut, then press the new combination. Escape cancels. Changes
    are saved immediately and are not sent to the server.
  </p>
  <p>
    Shortcuts take priority inside terminals and editors. Browser/system
    shortcuts may intercept keys before this page; choose another combination if
    needed. Embedded website frames do not forward keys here.
  </p>
  <div class="bindings" data-shortcut-recorder>
    {#each WORKSPACE_SHORTCUTS as action (action.id)}
      <div class="binding">
        <span>{action.label}</span>
        <button
          class="key"
          class:recording={recording === action.id}
          aria-label={`Change shortcut: ${action.label}`}
          aria-pressed={recording === action.id}
          on:click={() => {
            recording = action.id;
            error = "";
          }}
          on:blur={() => {
            if (recording === action.id) recording = null;
          }}
          on:keydown={(event) => capture(event, action.id)}
        >
          {recording === action.id
            ? "Press keys…"
            : formatShortcut($settings.shortcutBindings[action.id])}
        </button>
        <button
          class="disable"
          aria-label={`Disable shortcut: ${action.label}`}
          disabled={$settings.shortcutBindings[action.id] === null}
          on:click={() => save(action.id, null)}>Disable</button
        >
      </div>
    {/each}
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <footer>
    <span>Page numbers follow the pager order. Ctrl + 0 selects page 10.</span
    ><button
      on:click={() => {
        updateSettings({
          shortcutBindings: normalizeShortcutBindings(undefined),
        });
        recording = null;
        error = "";
      }}>Restore defaults</button
    >
  </footer>
</details>

<style>
  .shortcuts {
    border-radius: 8px;
    padding: 16px;
    background: var(--surface-subtle);
    color: var(--app-text);
  }
  summary {
    cursor: pointer;
    font-weight: 500;
  }
  summary span {
    margin-left: 8px;
    font-size: 12px;
    color: var(--surface-muted);
  }
  p,
  footer span {
    font-size: 12px;
    opacity: 0.75;
    margin-top: 8px;
  }
  .bindings {
    margin-top: 12px;
  }
  .binding {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border-bottom: 1px solid var(--surface-border);
  }
  .binding > span {
    flex: 1;
    min-width: 0;
    font-size: 13px;
  }
  button {
    min-height: 34px;
    border: 1px solid var(--surface-border);
    border-radius: 6px;
    padding: 4px 8px;
    font-size: 12px;
  }
  .key {
    min-width: 132px;
    background: var(--control-bg);
    color: var(--control-text);
  }
  .key.recording,
  button:focus-visible {
    outline: 2px solid var(--surface-accent);
    outline-offset: 1px;
  }
  button:disabled {
    opacity: 0.35;
  }
  .error {
    color: var(--surface-danger);
    opacity: 1;
  }
  footer {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
  }
  footer span {
    flex: 1;
  }
</style>
