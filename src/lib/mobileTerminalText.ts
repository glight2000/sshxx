import type { Terminal, IBufferCell, ITheme } from "@xterm/xterm";

export const MOBILE_TEXT_LINES = 2000;
export const MOBILE_TEXT_CHARACTERS = 128 * 1024;
export const MOBILE_TEXT_INTERVAL = 250;
export const MOBILE_INPUT_CHARACTERS = 16 * 1024;
export const MOBILE_TEXT_STYLE_RUNS = 1024;
const MAX_COLUMNS = 4096;
const MAX_CELLS = 128 * 1024;

export type TerminalTextSnapshot = {
  text: string;
  runs: { start: number; end: number; style: string }[];
  simplified: boolean;
  truncated: boolean;
  alternate: boolean;
};

/** A bounded projection of the existing parser, never an append-only transcript. */
export function readTerminalText(
  terminal: Pick<Terminal, "buffer" | "cols"> &
    Partial<Pick<Terminal, "options">>,
): TerminalTextSnapshot {
  const buffer = terminal.buffer.active;
  const parts: string[] = [];
  const styles: TerminalTextSnapshot["runs"] = [];
  let simplified = false;
  const cell = buffer.getNullCell();
  const boldBright = terminal.options?.drawBoldTextInBrightColors ?? true;
  let remaining = MOBILE_TEXT_CHARACTERS;
  let cells = MAX_CELLS;
  let continuation = false;
  let truncated = terminal.cols > MAX_COLUMNS;
  let index = buffer.length - 1;
  const first = Math.max(0, buffer.length - MOBILE_TEXT_LINES);
  for (; index >= first; index--) {
    const line = buffer.getLine(index);
    if (!line) continue;
    // Read bounded cells rather than concatenate an arbitrarily large run of
    // combining characters via translateToString before truncating it.
    const characters: string[] = [];
    const separator = parts.length && !continuation ? "\n" : "";
    let available = remaining - separator.length;
    let clipped = false;
    for (
      let column = Math.min(terminal.cols, line.length, MAX_COLUMNS) - 1;
      column >= 0;
      column--
    ) {
      if (cells-- <= 0) {
        clipped = true;
        break;
      }
      const value = line.getCell(column, cell);
      if (value?.getWidth() === 0) continue; // Wide-character placeholder.
      let chars = value?.getChars() || " ";
      if (!continuation && !characters.length && chars === " ") continue;
      if (chars.length > available) {
        chars = chars.slice(-available);
        if (/^[\uDC00-\uDFFF]/.test(chars)) chars = chars.slice(1);
        clipped = true;
      }
      // Concealed content must stay concealed even when older styling is dropped.
      if (value?.isInvisible()) chars = " ".repeat(chars.length);
      characters.push(chars);
      available -= chars.length;
      if (value && !simplified) {
        const style = terminalCellStyle(value, boldBright);
        const end = MOBILE_TEXT_CHARACTERS - available;
        const start = end - chars.length;
        const last = styles.at(-1);
        if (style && last?.style === style && last.end === start)
          last.end = end;
        else if (style) {
          if (styles.length < MOBILE_TEXT_STYLE_RUNS)
            styles.push({ start, end, style });
          else simplified = true;
        }
      }
      if (clipped || available === 0) {
        clipped ||= column > 0;
        break;
      }
    }
    const text = characters.reverse().join("");
    // The viewport can contain unused rows below the last output/cursor.
    if (
      !clipped &&
      !parts.length &&
      !text &&
      index > buffer.baseY + buffer.cursorY
    )
      continue;
    truncated ||= clipped;
    parts.push(text + separator);
    remaining -= text.length + separator.length;
    continuation = line.isWrapped;
    if (clipped || remaining <= 1) break;
  }
  const text = parts.reverse().join("");
  const runs: TerminalTextSnapshot["runs"] = [];
  let offset = 0;
  for (const run of styles.reverse()) {
    const start = text.length - run.end,
      end = text.length - run.start;
    if (start > offset) runs.push({ start: offset, end: start, style: "" });
    runs.push({ start, end, style: run.style });
    offset = end;
  }
  if (offset < text.length)
    runs.push({ start: offset, end: text.length, style: "" });
  return {
    text,
    runs,
    simplified,
    truncated: truncated || index >= 0,
    alternate: buffer.type === "alternate",
  };
}

/** One dirty bit and one timer; output parsing/ACK never waits for this view. */
export function watchTerminalText(
  terminal: Pick<Terminal, "buffer" | "cols" | "onWriteParsed" | "onResize"> &
    Partial<Pick<Terminal, "options">>,
  publish: (snapshot: TerminalTextSnapshot) => void,
) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let dirty = false;
  let paused = false;
  let disposed = false;
  const schedule = () => {
    if (disposed || paused || !dirty || timer !== undefined) return;
    timer = setTimeout(() => {
      timer = undefined;
      if (disposed || paused) return;
      dirty = false;
      publish(readTerminalText(terminal));
    }, MOBILE_TEXT_INTERVAL);
  };
  const invalidate = () => {
    dirty = true;
    schedule();
  };
  const listeners = [
    terminal.onWriteParsed(invalidate),
    terminal.onResize(invalidate),
    terminal.buffer.onBufferChange(invalidate),
  ];
  invalidate();
  return {
    setPaused(value: boolean) {
      paused = value;
      if (paused) {
        clearTimeout(timer);
        timer = undefined;
      } else schedule();
    },
    dispose() {
      disposed = true;
      clearTimeout(timer);
      timer = undefined;
      listeners.forEach((listener) => listener.dispose());
    },
  };
}

const palette = [
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite",
] as const;

/** Theme values become CSS variables, never markup or arbitrary CSS rules. */
export function mobileTerminalThemeCss(theme: ITheme): string {
  const color = (value: string | undefined, fallback: string) =>
    value && /^#[\da-f]{6}(?:[\da-f]{2})?$/i.test(value) ? value : fallback;
  const foreground = color(theme.foreground, "#d8d8d8");
  const background = color(theme.background, "#181818");
  return [
    `--reader-fg:${foreground}`,
    `--reader-bg:${background}`,
    `--reader-cursor:${color(theme.cursor, foreground)}`,
    `--reader-selection:${color(theme.selectionBackground, "color-mix(in srgb, var(--reader-fg) 28%, var(--reader-bg))")}`,
    `--reader-selection-fg:${color(theme.selectionForeground, foreground)}`,
    ...palette.map(
      (key, index) => `--reader-ansi-${index}:${color(theme[key], foreground)}`,
    ),
  ].join(";");
}

export function terminalCellColor(
  value: number,
  rgb: boolean,
  indexed: boolean,
): string {
  if (!Number.isInteger(value) || value < 0) return "";
  if (rgb)
    return value <= 0xffffff ? `#${value.toString(16).padStart(6, "0")}` : "";
  if (!indexed || value > 255) return "";
  if (value < 16) return `var(--reader-ansi-${value})`;
  if (value >= 232) {
    const level = 8 + (value - 232) * 10;
    return `rgb(${level},${level},${level})`;
  }
  const levels = [0, 95, 135, 175, 215, 255];
  const index = value - 16;
  return `rgb(${levels[Math.floor(index / 36)]},${levels[Math.floor(index / 6) % 6]},${levels[index % 6]})`;
}

/** Read already-parsed public attributes; this is not another ANSI parser. */
export function terminalCellStyle(
  cell: IBufferCell,
  boldBright = true,
): string {
  if (cell.isAttributeDefault()) return "";
  let fg = cell.getFgColor();
  if (boldBright && cell.isBold() && cell.isFgPalette() && fg < 8) fg += 8;
  let foreground = terminalCellColor(fg, cell.isFgRGB(), cell.isFgPalette());
  let background = terminalCellColor(
    cell.getBgColor(),
    cell.isBgRGB(),
    cell.isBgPalette(),
  );
  if (cell.isInverse())
    [foreground, background] = [
      background || "var(--reader-bg)",
      foreground || "var(--reader-fg)",
    ];
  const decoration = [
    cell.isUnderline() ? "underline" : "",
    cell.isStrikethrough() ? "line-through" : "",
  ]
    .filter(Boolean)
    .join(" ");
  return [
    foreground ? `color:${foreground}` : "",
    background ? `background-color:${background}` : "",
    cell.isBold() ? "font-weight:700" : "",
    cell.isItalic() ? "font-style:italic" : "",
    cell.isDim() ? "opacity:0.6" : "",
    decoration ? `text-decoration-line:${decoration}` : "",
  ]
    .filter(Boolean)
    .join(";");
}
