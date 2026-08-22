export type TerminalLocation = {
  workingDirectory: string;
  workingDirectoryHost: string;
};

/** Parse a shell-reported OSC 7 file URI without trusting it as an SSH target. */
export function parseOsc7Location(value: string): TerminalLocation | null {
  try {
    const url = new URL(value);
    if (url.protocol !== "file:") return null;
    return {
      workingDirectory: decodeURIComponent(url.pathname) || ".",
      workingDirectoryHost: url.hostname,
    };
  } catch {
    return null;
  }
}
