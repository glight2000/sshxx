import assert from "node:assert/strict";
import { test } from "node:test";
import xterm from "@xterm/xterm";

import { TerminalHistory } from "../src/lib/terminalHistory.ts";

test("retains only the newest terminal output", () => {
  const history = new TerminalHistory(8);
  history.append(1, "abc");
  history.append(1, "defgh");
  assert.equal(history.read(1), "abcdefgh");
  history.append(1, "ijk");
  assert.equal(history.read(1), "defghijk");
});

test("isolates terminal histories and supports cleanup", () => {
  const history = new TerminalHistory(16);
  history.append(1, "first");
  history.append(2, "second");
  history.delete(1);
  assert.equal(history.read(1), "");
  assert.equal(history.read(2), "second");
  history.clear();
  assert.equal(history.read(2), "");
});

test("releases histories that no longer belong to live terminals", () => {
  const history = new TerminalHistory(16);
  history.append(1, "first");
  history.append(2, "second");

  history.retain(new Set([2]));

  assert.equal(history.read(1), "");
  assert.equal(history.read(2), "second");
});

test("discarded chunks release text before array compaction", () => {
  const history = new TerminalHistory(16);
  let expected = "";
  for (let index = 0; index < 800; index++) {
    const chunk = String(index).padStart(4, "0");
    history.append(1, chunk);
    expected = (expected + chunk).slice(-16);
    assert.equal(history.read(1), expected);
    // Inspect references, not process RSS: GC scheduling is engine-dependent.
    const stored = history.histories.get(1);
    assert.equal(
      stored.chunks.reduce((sum, value) => sum + value.length, 0),
      expected.length,
    );
    assert.ok(
      stored.chunks.slice(0, stored.start).every((value) => value === ""),
    );
  }
  history.append(1, "a much larger chunk ending with the retained tail");
  assert.equal(history.read(1), "he retained tail");
});

test("trimming a retained replay never leaves half a Unicode character", () => {
  const history = new TerminalHistory(4);
  history.append(1, "a😀bcd");
  assert.equal(history.read(1), "bcd");
  history.append(1, "e");
  assert.equal(history.read(1), "bcde");
});

test("many retention cycles preserve isolation, the latest mode and bounded references", () => {
  const history = new TerminalHistory(64);
  history.append(2, "other viewer's terminal", false);
  for (let cycle = 0; cycle < 20_000; cycle++) {
    history.append(1, `${cycle}: output\r\n`, cycle % 2 === 0);
    const stored = history.histories.get(1);
    assert.ok(stored.length <= 64);
    assert.ok(stored.chunks.length <= 270);
  }
  assert.ok(history.read(1).endsWith("19999: output\r\n"));
  assert.equal(history.pasteMode(1), false);
  assert.equal(history.read(2), "other viewer's terminal");
  history.clear();
  assert.equal(history.histories.size, 0);
});

test("a raw retained tail is not a complete terminal checkpoint", async () => {
  const history = new TerminalHistory(128);
  const full = new xterm.Terminal({ cols: 20, rows: 3, scrollback: 10 });
  const cold = new xterm.Terminal({ cols: 20, rows: 3, scrollback: 10 });
  const data = "\x1b[31m" + "older\r\n".repeat(200) + "\x1b[2J\x1b[Hlatest";
  try {
    history.append(1, data);
    await new Promise((resolve) => full.write(data, resolve));
    await new Promise((resolve) => cold.write(history.read(1), resolve));
    const fullLine = full.buffer.active.getLine(full.buffer.active.baseY);
    const coldLine = cold.buffer.active.getLine(cold.buffer.active.baseY);
    assert.equal(fullLine.translateToString(true), "latest");
    assert.equal(coldLine.translateToString(true), "latest");
    // Identical visible text does not prove the same state. The old SGR was
    // trimmed, so tail-only replay has lost the still-active foreground color.
    assert.equal(fullLine.getCell(0).isFgPalette(), true);
    assert.equal(coldLine.getCell(0).isFgDefault(), true);
  } finally {
    full.dispose();
    cold.dispose();
    history.clear();
  }
});
