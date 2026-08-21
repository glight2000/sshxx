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
