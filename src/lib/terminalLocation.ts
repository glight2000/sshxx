export type TerminalLocation = {
  workingDirectory: string;
  workingDirectoryHost: string;
};

/** Parse a shell-reported OSC 7 file URI without trusting it as an SSH target. */
export function parseOsc7Location(value: string): TerminalLocation | null {
  try {
    if (value.length > 8192 || /[?#]/.test(value)) return null;
    const url = new URL(value);
    if (url.protocol !== "file:") return null;
    const path = decodeURIComponent(url.pathname);
    if (
      !path ||
      Array.from(path).some(
        (c) => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127,
      )
    )
      return null;
    return {
      workingDirectory: path,
      workingDirectoryHost: url.hostname.toLowerCase().replace(/\.$/, ""),
    };
  } catch {
    return null;
  }
}
