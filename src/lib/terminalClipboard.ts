import type { Terminal } from "@xterm/xterm";

/** Own text paste once: xterm brackets it; the browser must not insert it again. */
export function pasteTerminalText(
  event: ClipboardEvent,
  terminal: Terminal | null,
) {
  if (
    !terminal?.element?.contains(event.target as Node | null) ||
    !event.clipboardData?.types.includes("text/plain") ||
    event.defaultPrevented
  )
    return false;
  event.preventDefault();
  event.stopPropagation();
  terminal.paste(event.clipboardData.getData("text/plain"));
  return true;
}
