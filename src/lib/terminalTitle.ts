const TERMINAL_TITLE_SPINNER_FRAMES = new Set([
  "⠋",
  "⠙",
  "⠹",
  "⠸",
  "⠼",
  "⠴",
  "⠦",
  "⠧",
  "⠇",
  "⠏",
]);

export type TerminalTitleParts = {
  activity: string;
  title: string;
};

/** Keep a leading CLI activity spinner out of the user-editable title. */
export function splitTerminalTitle(value: string): TerminalTitleParts {
  const normalized = value.trim();
  const [first = ""] = [...normalized];
  if (
    TERMINAL_TITLE_SPINNER_FRAMES.has(first) &&
    /^\s/u.test(normalized.slice(first.length))
  ) {
    return {
      activity: first,
      title: normalized.slice(first.length).trimStart(),
    };
  }
  return { activity: "", title: normalized };
}
