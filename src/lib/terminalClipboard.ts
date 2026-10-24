import type { Terminal } from "@xterm/xterm";

/** Own a copy shortcut before starting an asynchronous clipboard write.
 * Returning false from xterm's key handler alone does not cancel the browser's
 * native copy, which can overwrite this write after the selection is cleared.
 */
export async function copyTerminalSelection(
  event: KeyboardEvent,
  terminal: Terminal,
  clipboard: Pick<Clipboard, "writeText"> | undefined,
) {
  event.preventDefault();
  event.stopPropagation();
  const selection = terminal.getSelection();
  if (!selection) throw new Error("No terminal selection to copy.");

  if (clipboard?.writeText) {
    await clipboard.writeText(selection);
  } else {
    // The synchronous copy event uses xterm's selection handler, including on
    // browsers without the async Clipboard API. Never treat a refusal as success.
    if (!terminal.element?.ownerDocument.execCommand("copy")) {
      throw new Error("Clipboard access is unavailable.");
    }
  }
  return selection;
}

/** Restore negotiated paste behavior without injecting CSI into a possibly
 * incomplete OSC/DCS parser. Unknown legacy state keeps xterm's own behavior.
 */
export function pasteText(terminal: Terminal, text: string, mode?: boolean) {
  if (mode === undefined || mode === terminal.modes.bracketedPasteMode) {
    terminal.paste(text);
    return;
  }
  const normalized = text.replace(/\r?\n/g, "\r");
  terminal.input(mode ? `\x1b[200~${normalized}\x1b[201~` : normalized, true);
  if (terminal.textarea) terminal.textarea.value = "";
}

/** Own text paste once: xterm brackets it; the browser must not insert it again. */
export function pasteTerminalText(
  event: ClipboardEvent,
  terminal: Terminal | null,
  mode?: boolean,
) {
  if (
    !terminal?.element?.contains(event.target as Node | null) ||
    !event.clipboardData?.types.includes("text/plain") ||
    event.defaultPrevented
  )
    return false;
  event.preventDefault();
  event.stopPropagation();
  pasteText(terminal, event.clipboardData.getData("text/plain"), mode);
  return true;
}
