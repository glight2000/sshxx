const TERMINAL_TITLE_SPINNER = /^[⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏](?=\s)/u;

export type TerminalTitleParts = {
  activity: string;
  title: string;
};

/** Keep a leading CLI activity spinner out of the user-editable title. */
export function splitTerminalTitle(value: string): TerminalTitleParts {
  const normalized = value.trim();
  const activity = normalized.match(TERMINAL_TITLE_SPINNER)?.[0] ?? "";
  if (activity) {
    return {
      activity,
      title: normalized.slice(activity.length).trimStart(),
    };
  }
  return { activity: "", title: normalized };
}
