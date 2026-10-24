import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import {
  copyTerminalSelection,
  pasteTerminalText,
  pasteText,
} from "../src/lib/terminalClipboard.ts";
import { TerminalHistory } from "../src/lib/terminalHistory.ts";
import xterm from "@xterm/xterm";

test("actual xterm key binding reports confirmed writes and preserves changed or failed selections", async () => {
  const source = readFileSync(
    new URL("../src/lib/ui/XTerm.svelte", import.meta.url),
    "utf8",
  );
  const start = source.indexOf("term.attachCustomKeyEventHandler(");
  const end = source.indexOf("term.loadAddon(", start);
  assert.ok(start >= 0 && end > start);
  const binding = source.slice(start, end);
  for (const isMac of [false, true]) {
    let handler, finish, fail;
    let selection = "copied text";
    const toasts = [];
    const terminal = {
      attachCustomKeyEventHandler: (callback) => (handler = callback),
      hasSelection: () => selection.length > 0,
      getSelection: () => selection,
      clearSelection: () => (selection = ""),
    };
    const clipboard = {
      writeText: () =>
        new Promise((resolve, reject) => {
          finish = resolve;
          fail = reject;
        }),
    };
    new Function(
      "term",
      "navigator",
      "isMac",
      "copyTerminalSelection",
      "makeToast",
      `const suppressInput = 0, destroyed = false; ${binding}`,
    )(terminal, { clipboard }, isMac, copyTerminalSelection, (v) =>
      toasts.push(v),
    );
    const shortcut = () => {
      const event = new Event("keydown", { cancelable: true });
      Object.assign(event, { key: "c", ctrlKey: !isMac, metaKey: isMac });
      assert.equal(handler(event), false);
      assert.equal(event.defaultPrevented, true);
    };
    shortcut();
    assert.deepEqual(toasts, []);
    selection = "newer selection";
    finish();
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(selection, "newer selection");
    assert.deepEqual(
      toasts.map((v) => v.kind),
      ["success"],
    );
    shortcut();
    fail(new Error("denied"));
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(selection, "newer selection");
    assert.deepEqual(
      toasts.map((v) => v.kind),
      ["success", "error"],
    );
    shortcut();
    finish();
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(selection, "");
  }
});

test("copy cancels the native shortcut before an asynchronous write can be overwritten", async () => {
  let clipboardText = "old clipboard";
  let selection = "new selection 中文";
  let finish;
  const event = new Event("keydown", { cancelable: true });
  const copied = copyTerminalSelection(
    event,
    { getSelection: () => selection },
    {
      writeText(text) {
        assert.equal(event.defaultPrevented, true);
        return new Promise((resolve) => {
          finish = () => {
            clipboardText = text;
            resolve();
          };
        });
      },
    },
  );
  assert.equal(event.defaultPrevented, true);
  selection = "another selection while the write is pending";
  finish();
  assert.equal(await copied, "new selection 中文");
  // Model the browser's default action after xterm's key handler returns.
  if (!event.defaultPrevented) clipboardText = "old textarea contents";
  assert.equal(clipboardText, "new selection 中文");
  assert.equal(selection, "another selection while the write is pending");
});

test("copy rejection is a failure and preserves the terminal selection", async () => {
  const event = new Event("keydown", { cancelable: true });
  const terminal = { getSelection: () => "keep selected" };
  await assert.rejects(
    copyTerminalSelection(event, terminal, {
      writeText: () => Promise.reject(new Error("permission denied")),
    }),
    /permission denied/,
  );
  assert.equal(event.defaultPrevented, true);
  assert.equal(terminal.getSelection(), "keep selected");
});

test("copy without the async API uses xterm's native copy and reports refusals", async () => {
  let allowed = true;
  let copies = 0;
  const terminal = {
    getSelection: () => "native selection",
    element: {
      ownerDocument: {
        execCommand(command) {
          assert.equal(command, "copy");
          copies++;
          return allowed;
        },
      },
    },
  };
  const copy = () =>
    copyTerminalSelection(
      new Event("keydown", { cancelable: true }),
      terminal,
      undefined,
    );
  assert.equal(await copy(), "native selection");
  allowed = false;
  await assert.rejects(copy(), /Clipboard access is unavailable/);
  assert.equal(copies, 2);
});

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
