export const WORKSPACE_SHORTCUTS = [
  { id: "search", label: "Search components", binding: "Ctrl+Space" },
  { id: "previousPage", label: "Previous page", binding: "Ctrl+ArrowLeft" },
  { id: "nextPage", label: "Next page", binding: "Ctrl+ArrowRight" },
  { id: "firstPage", label: "First page", binding: "Ctrl+ArrowUp" },
  { id: "lastPage", label: "Last page", binding: "Ctrl+ArrowDown" },
  { id: "page1", label: "Page 1", binding: "Ctrl+Digit1" },
  { id: "page2", label: "Page 2", binding: "Ctrl+Digit2" },
  { id: "page3", label: "Page 3", binding: "Ctrl+Digit3" },
  { id: "page4", label: "Page 4", binding: "Ctrl+Digit4" },
  { id: "page5", label: "Page 5", binding: "Ctrl+Digit5" },
  { id: "page6", label: "Page 6", binding: "Ctrl+Digit6" },
  { id: "page7", label: "Page 7", binding: "Ctrl+Digit7" },
  { id: "page8", label: "Page 8", binding: "Ctrl+Digit8" },
  { id: "page9", label: "Page 9", binding: "Ctrl+Digit9" },
  { id: "page10", label: "Page 10", binding: "Ctrl+Digit0" },
] as const;

export type WorkspaceShortcut = (typeof WORKSPACE_SHORTCUTS)[number]["id"];
export type ShortcutBindings = Record<WorkspaceShortcut, string | null>;

const keyCode =
  /^(Key[A-Z]|Digit[0-9]|Arrow(?:Left|Right|Up|Down)|Space|Enter|Tab|Backspace|Delete|Home|End|PageUp|PageDown|F(?:[1-9]|1[0-2]))$/;

function validBinding(value: unknown): value is string {
  if (typeof value !== "string" || value.length > 60) return false;
  const match = /^(Ctrl\+)?(Alt\+)?(Shift\+)?(Meta\+)?([^+]+)$/.exec(value);
  return (
    !!match &&
    keyCode.test(match[5]) &&
    (!!(match[1] || match[2] || match[4]) || /^F\d+$/.test(match[5]))
  );
}

/** Missing/invalid values inherit defaults; null explicitly disables a binding. */
export function normalizeShortcutBindings(value: unknown): ShortcutBindings {
  const source =
    value && typeof value === "object" && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : {};
  const result = {} as ShortcutBindings;
  const used = new Set<string>();
  for (const { id, binding } of WORKSPACE_SHORTCUTS) {
    const candidate = Object.hasOwn(source, id) ? source[id] : undefined;
    const next =
      candidate === null ? null : validBinding(candidate) ? candidate : binding;
    // Corrupt/older data must never run two actions for one keypress.
    result[id] = next && used.has(next) ? null : next;
    if (next) used.add(next);
  }
  return result;
}

export function shortcutFromEvent(event: KeyboardEvent): string | null {
  if (
    event.isComposing ||
    event.key === "Process" ||
    event.key === "Dead" ||
    event.keyCode === 229 ||
    event.getModifierState?.("AltGraph")
  )
    return null;
  const code = event.code.replace(/^Numpad([0-9])$/, "Digit$1");
  const binding = `${event.ctrlKey ? "Ctrl+" : ""}${event.altKey ? "Alt+" : ""}${event.shiftKey ? "Shift+" : ""}${event.metaKey ? "Meta+" : ""}${code}`;
  return validBinding(binding) ? binding : null;
}

export function formatShortcut(binding: string | null): string {
  return binding === null
    ? "Disabled"
    : binding
        .replace(/Key([A-Z])$/, "$1")
        .replace(/Digit([0-9])$/, "$1")
        .replace("ArrowLeft", "←")
        .replace("ArrowRight", "→")
        .replace("ArrowUp", "↑")
        .replace("ArrowDown", "↓")
        .replaceAll("+", " + ");
}

export function shortcutPageId(
  action: WorkspaceShortcut,
  pages: readonly { id: number }[],
  activeId: number,
): number | null {
  const index = pages.findIndex((page) => page.id === activeId);
  if (action === "firstPage") return pages[0]?.id ?? null;
  if (action === "lastPage") return pages.at(-1)?.id ?? null;
  if (action === "previousPage") return index > 0 ? pages[index - 1].id : null;
  if (action === "nextPage")
    return index >= 0 ? (pages[index + 1]?.id ?? null) : null;
  if (action.startsWith("page"))
    return pages[Number(action.slice(4)) - 1]?.id ?? null;
  return null;
}

export function installWorkspaceShortcuts(
  read: () => ShortcutBindings,
  enabled: () => boolean,
  run: (action: WorkspaceShortcut) => void,
) {
  const keydown = (event: KeyboardEvent) => {
    if (!enabled() || event.defaultPrevented) return;
    const binding = shortcutFromEvent(event);
    if (!binding) return;
    const bindings = read();
    const action = WORKSPACE_SHORTCUTS.find(
      ({ id }) => bindings[id] === binding,
    )?.id;
    if (!action) return;
    // Capture before xterm/CodeMirror: navigation must never also reach the PTY.
    event.preventDefault();
    event.stopImmediatePropagation();
    if (!event.repeat) run(action);
  };
  window.addEventListener("keydown", keydown, { capture: true });
  return () =>
    window.removeEventListener("keydown", keydown, { capture: true });
}
