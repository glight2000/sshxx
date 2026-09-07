import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import {
  readTerminalText,
  watchTerminalText,
  MOBILE_TEXT_CHARACTERS,
  MOBILE_TEXT_INTERVAL,
  MOBILE_TEXT_STYLE_RUNS,
  terminalCellColor,
  terminalCellStyle,
  mobileTerminalThemeCss,
} from "../src/lib/mobileTerminalText.ts";

function fixture(lines, extra = {}) {
  const listeners = new Set();
  const event = (callback) => {
    listeners.add(callback);
    return { dispose: () => listeners.delete(callback) };
  };
  let reads = 0;
  const terminal = {
    cols: 80,
    buffer: {
      active: {
        length: lines.length,
        baseY: 0,
        cursorY: lines.length - 1,
        type: "normal",
        getNullCell: () => ({}),
        getLine(index) {
          reads++;
          const item = lines[index];
          if (item === undefined) return undefined;
          const cells = typeof item === "string" ? [...item] : item.cells;
          return {
            length: cells.length,
            isWrapped: item.wrapped ?? false,
            getCell(column) {
              return {
                ...cellStyle(
                  typeof cells[column] === "object" && cells[column] !== null
                    ? cells[column]
                    : {},
                ),
                getWidth: () => (cells[column] === null ? 0 : 1),
                getChars: () =>
                  typeof cells[column] === "object" && cells[column] !== null
                    ? cells[column].text
                    : cells[column],
              };
            },
          };
        },
        ...extra,
      },
      onBufferChange: event,
    },
    onWriteParsed: event,
    onResize: event,
  };
  return {
    terminal,
    listeners,
    reads: () => reads,
    emit: () => listeners.forEach((callback) => callback()),
  };
}

function cellStyle({
  fg = -1,
  bg = -1,
  rgb = false,
  bold = false,
  inverse = false,
  hidden = false,
  italic = false,
  underline = false,
} = {}) {
  return {
    isAttributeDefault: () =>
      fg === -1 &&
      bg === -1 &&
      !bold &&
      !inverse &&
      !hidden &&
      !italic &&
      !underline,
    getFgColor: () => fg,
    getBgColor: () => bg,
    isFgPalette: () => fg >= 0 && !rgb,
    isFgRGB: () => fg >= 0 && rgb,
    isBgPalette: () => bg >= 0 && !rgb,
    isBgRGB: () => bg >= 0 && rgb,
    isBold: () => +bold,
    isInverse: () => +inverse,
    isInvisible: () => +hidden,
    isItalic: () => +italic,
    isUnderline: () => +underline,
    isStrikethrough: () => 0,
    isDim: () => 0,
  };
}

test("reader colors follow terminal palettes, truecolor and safe theme variables", () => {
  assert.equal(terminalCellColor(1, false, true), "var(--reader-ansi-1)");
  assert.equal(terminalCellColor(196, false, true), "rgb(255,0,0)");
  assert.equal(terminalCellColor(232, false, true), "rgb(8,8,8)");
  assert.equal(terminalCellColor(255, false, true), "rgb(238,238,238)");
  assert.equal(terminalCellColor(0x123456, true, false), "#123456");
  assert.equal(terminalCellColor(0, true, false), "#000000");
  assert.equal(terminalCellColor(999, false, true), "");
  assert.equal(terminalCellColor(NaN, true, false), "");
  assert.equal(
    terminalCellStyle(cellStyle({ fg: 1, bold: true })),
    "color:var(--reader-ansi-9);font-weight:700",
  );
  assert.equal(
    terminalCellStyle(cellStyle({ fg: 1, bold: true }), false),
    "color:var(--reader-ansi-1);font-weight:700",
  );
  assert.equal(
    terminalCellStyle(cellStyle({ inverse: true })),
    "color:var(--reader-bg);background-color:var(--reader-fg)",
  );
  const css = mobileTerminalThemeCss({
    foreground: "#123456",
    background: "#abcdef",
    red: "#ff0000",
    blue: "red;display:none",
    selectionBackground: "#11223344",
  });
  assert.match(css, /--reader-bg:#abcdef/);
  assert.match(css, /--reader-ansi-1:#ff0000/);
  assert.match(css, /--reader-selection:#11223344/);
  assert.doesNotMatch(css, /display:none/);
});

test("styled spans preserve exact text, merge neighbors and remain bounded for pathological color changes", () => {
  const f = fixture([
    {
      cells: ["a", { text: "b", fg: 1 }, { text: "c", fg: 1 }, "d"],
      wrapped: false,
    },
  ]);
  const snapshot = readTerminalText(f.terminal);
  assert.equal(snapshot.text, "abcd");
  assert.deepEqual(snapshot.runs, [
    { start: 0, end: 1, style: "" },
    { start: 1, end: 3, style: "color:var(--reader-ansi-1)" },
    { start: 3, end: 4, style: "" },
  ]);
  const noise = fixture(
    Array.from({ length: 40 }, () => ({
      cells: Array.from({ length: 80 }, (_, i) => ({
        text: "x",
        fg: (i % 2) + 1,
      })),
      wrapped: false,
    })),
  );
  const bounded = readTerminalText(noise.terminal);
  assert.equal(bounded.simplified, true);
  assert.ok(bounded.runs.length <= 2 * MOBILE_TEXT_STYLE_RUNS + 1);
  assert.equal(
    bounded.runs.map((run) => bounded.text.slice(run.start, run.end)).join(""),
    bounded.text,
  );
  assert.ok(
    bounded.runs.every(
      (run) =>
        run.start >= 0 &&
        run.end <= bounded.text.length &&
        run.start <= run.end,
    ),
  );
  const concealed = readTerminalText(
    fixture([
      { cells: [{ text: "secret", hidden: true }, "!"], wrapped: false },
    ]).terminal,
  );
  assert.equal(concealed.text, "      !");
});

test("phone text is a current bounded snapshot, respecting soft wraps, wide cells, blank rows and alternate screens", () => {
  const f = fixture(
    [
      "hello ",
      { cells: ["世", null, "界", null], wrapped: true },
      "next",
      "",
      "",
    ],
    { cursorY: 2 },
  );
  assert.equal(readTerminalText(f.terminal).text, "hello 世界\nnext");
  f.terminal.buffer.active.type = "alternate";
  assert.equal(readTerminalText(f.terminal).alternate, true);
  const blank = fixture([]);
  assert.deepEqual(readTerminalText(blank.terminal), {
    text: "",
    runs: [],
    simplified: false,
    truncated: false,
    alternate: false,
  });
  const many = fixture(Array.from({ length: 100_000 }, (_, i) => `line ${i}`));
  const snapshot = readTerminalText(many.terminal);
  assert.equal(snapshot.truncated, true);
  assert.ok(snapshot.text.endsWith("line 99999"));
  assert.ok(many.reads() <= 2000);
  assert.ok(snapshot.text.length <= MOBILE_TEXT_CHARACTERS);
  // One pathological combining cell must not cause a second huge concatenation.
  const huge = fixture([
    { cells: ["x".repeat(MOBILE_TEXT_CHARACTERS * 4), "😀"], wrapped: false },
  ]);
  const limited = readTerminalText(huge.terminal);
  assert.equal(limited.truncated, true);
  assert.ok(limited.text.length <= MOBILE_TEXT_CHARACTERS);
  assert.ok(limited.text.endsWith("😀"));
  assert.ok(limited.text.isWellFormed());
  const crowded = fixture(Array(2000).fill("a".repeat(1000)));
  crowded.terminal.cols = 1000;
  assert.ok(
    readTerminalText(crowded.terminal).text.length <= MOBILE_TEXT_CHARACTERS,
  );
});

test("reader coalesces writes, pauses without queues, refreshes on resume, and disposes all subscriptions/timers", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const f = fixture(["before"]);
  const snapshots = [];
  const observer = watchTerminalText(f.terminal, (snapshot) =>
    snapshots.push(snapshot.text),
  );
  try {
    for (let i = 0; i < 1000; i++) f.emit();
    assert.equal(f.reads(), 0);
    t.mock.timers.tick(MOBILE_TEXT_INTERVAL);
    assert.deepEqual(snapshots, ["before"]);
    const baseline = f.reads();
    t.mock.timers.tick(10000);
    assert.equal(
      f.reads(),
      baseline,
      "idle reader does not poll or scan buffers",
    );
    observer.setPaused(true);
    for (let i = 0; i < 10000; i++) f.emit();
    t.mock.timers.tick(10000);
    assert.equal(f.reads(), baseline);
    observer.setPaused(false);
    t.mock.timers.tick(MOBILE_TEXT_INTERVAL);
    assert.equal(snapshots.length, 2, "no backlog of snapshots accumulates");
    f.emit();
    observer.dispose();
    assert.equal(f.listeners.size, 0);
    t.mock.timers.tick(10000);
    observer.setPaused(false);
    assert.equal(snapshots.length, 2);
  } finally {
    observer.dispose();
  }
});

test("phone reader keeps native text selection and local composer separate from terminal parsing and geometry", () => {
  const read = (path) =>
    readFileSync(new URL(`../${path}`, import.meta.url), "utf8");
  const reader = read("src/lib/ui/MobileTerminalReader.svelte");
  assert.match(reader, /on:contextmenu\|stopPropagation/);
  assert.match(reader, /maxlength=\{MOBILE_INPUT_CHARACTERS\}/);
  assert.doesNotMatch(
    reader,
    /\{@html|localStorage|\.resize\(|srocket|new Terminal/,
  );
  assert.match(reader, /observer\.dispose\(\)/);
  const xterm = read("src/lib/ui/XTerm.svelte");
  assert.match(xterm, /\{#if mobileDetail && loaded && term\}/);
  assert.match(
    xterm,
    /\.term-container\.mobile-reader \.terminal-host\s*\{\s*display: none;/,
  );
  assert.match(
    xterm,
    /writeQueue\.setSink\(\(data, complete\) => term!\.write\(data, complete\)\)/,
  );
});
