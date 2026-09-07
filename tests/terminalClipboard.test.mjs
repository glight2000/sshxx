import assert from "node:assert/strict";
import test from "node:test";
import { pasteTerminalText } from "../src/lib/terminalClipboard.ts";

test("owns a large text paste without browser residue or duplicate delivery", () => {
  const text = "large pasted text 中文\n".repeat(3000);
  const target = {};
  const sent = [];
  const event = {
    target,
    clipboardData: { types: ["text/plain"], getData: () => text },
    defaultPrevented: false,
    preventDefault() {
      this.defaultPrevented = true;
    },
    stopPropagation() {
      this.stopped = true;
    },
  };
  const terminal = {
    element: { contains: (element) => element === target },
    paste: (data) => sent.push(data),
  };
  assert.equal(pasteTerminalText(event, terminal), true);
  assert.equal(event.defaultPrevented, true);
  assert.equal(event.stopped, true);
  assert.deepEqual(sent, [text]);
  assert.equal(pasteTerminalText(event, terminal), false);
  assert.equal(sent.length, 1);
  event.defaultPrevented = false;
  event.target = {}; // title/settings inputs are not the terminal input surface
  assert.equal(pasteTerminalText(event, terminal), false);
  assert.equal(event.defaultPrevented, false);
});
