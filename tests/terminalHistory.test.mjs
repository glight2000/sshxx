import assert from "node:assert/strict";
import { test } from "node:test";

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
