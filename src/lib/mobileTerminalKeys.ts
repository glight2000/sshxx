export type MobileTerminalSendMode = "execute" | "paste" | "keys";

/** Standard xterm key sequences; these are input, never bracketed paste. */
export const MOBILE_TERMINAL_KEYS: Record<string, string> = {
  "↑": "\x1b[A",
  "↓": "\x1b[B",
  "←": "\x1b[D",
  "→": "\x1b[C",
  Enter: "\r",
  Tab: "\t",
  Esc: "\x1b",
  Backspace: "\x7f",
  Delete: "\x1b[3~",
  Home: "\x1b[H",
  End: "\x1b[F",
  PageUp: "\x1b[5~",
  PageDown: "\x1b[6~",
  "Ctrl+C": "\x03",
  "Ctrl+D": "\x04",
  "Ctrl+Z": "\x1a",
  "Ctrl+L": "\x0c",
  "Ctrl+A": "\x01",
  "Ctrl+E": "\x05",
  "Ctrl+R": "\x12",
  "Ctrl+U": "\x15",
  "Ctrl+W": "\x17",
};

export function mobileTerminalKey(key: string, applicationCursorKeys: boolean) {
  const sequence = Object.hasOwn(MOBILE_TERMINAL_KEYS, key)
    ? MOBILE_TERMINAL_KEYS[key]
    : null;
  if (sequence && applicationCursorKeys && /^\x1b\[[ABCDHF]$/.test(sequence))
    return sequence.replace("[", "O");
  return sequence;
}
