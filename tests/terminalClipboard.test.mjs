import assert from "node:assert/strict";
import test from "node:test";
import { pasteTerminalText, pasteText } from "../src/lib/terminalClipboard.ts";
import { TerminalHistory } from "../src/lib/terminalHistory.ts";
import xterm from "@xterm/xterm";

test("real xterm: retained replay loses negotiation, checkpoint restores one complete paste", async () => {
  const live = new xterm.Terminal();
  const restored = new xterm.Terminal();
  try {
    const history = new TerminalHistory(2 * 1024 * 1024);
    const start = "\x1b[?2004h";
    const output = "synthetic output\r\n".repeat(130_000);
    await new Promise((done) => live.write(start + output, done));
    history.append(1, start, true);
    history.append(1, output, true);
    await new Promise((done) => restored.write(history.read(1), done));
    assert.equal(live.modes.bracketedPasteMode, true);
    assert.equal(restored.modes.bracketedPasteMode, false);
    const sent = [];
    restored.onData((data) => sent.push(data));
    for (const size of [1023, 1024, 1025, 65536]) {
      sent.length = 0;
      const text = "x".repeat(size) + "中文\nnext";
      pasteText(restored, text, history.pasteMode(1));
      restored.input("z", true);
      assert.deepEqual(sent, [
        "\x1b[200~" + text.replace(/\n/g, "\r") + "\x1b[201~",
        "z",
      ]);
    }
    const plain = [];
    live.onData((data) => plain.push(data));
    pasteText(live, "plain\ntext", false);
    assert.deepEqual(plain, ["plain\rtext"]);
    history.delete(1);
    assert.equal(history.pasteMode(1), undefined);
  } finally {
    live.dispose();
    restored.dispose();
  }
});

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
