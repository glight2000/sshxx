import assert from "node:assert/strict";
import test from "node:test";
import {
  WORKSPACE_SHORTCUTS,
  normalizeShortcutBindings,
  shortcutFromEvent,
  formatShortcut,
  shortcutPageId,
  installWorkspaceShortcuts,
  installFocusEscape,
} from "../src/lib/workspaceShortcuts.ts";

test("only desktop Shift+Escape clears focus, without reaching the terminal", () => {
  const original = globalThis.window;
  globalThis.window = new EventTarget();
  let clears = 0,
    downstream = 0,
    enabled = true;
  const dispose = installFocusEscape(
    () => enabled,
    () => clears++,
  );
  window.addEventListener("keydown", () => downstream++);
  const send = (extra = {}) => {
    const e = new Event("keydown", { cancelable: true });
    Object.assign(e, { key: "Escape", shiftKey: false, ...extra });
    window.dispatchEvent(e);
    return e.defaultPrevented;
  };
  try {
    assert.equal(send(), false);
    assert.equal(clears, 0);
    assert.equal(downstream, 1);
    assert.equal(send({ shiftKey: true }), true);
    assert.equal(clears, 1);
    assert.equal(downstream, 1);
    for (const extra of [
      { ctrlKey: true },
      { altKey: true },
      { metaKey: true },
      { isComposing: true },
      { keyCode: 229 },
    ])
      assert.equal(send({ shiftKey: true, ...extra }), false);
    enabled = false;
    assert.equal(send({ shiftKey: true }), false);
    dispose();
    enabled = true;
    assert.equal(send({ shiftKey: true }), false);
    assert.equal(clears, 1);
  } finally {
    dispose();
    globalThis.window = original;
  }
});

const key = (code, extra = {}) => ({
  code,
  ctrlKey: true,
  altKey: false,
  shiftKey: false,
  metaKey: false,
  isComposing: false,
  keyCode: 0,
  ...extra,
});

test("shortcut settings migrate missing/invalid data and preserve explicit disable/custom bindings", () => {
  const defaults = normalizeShortcutBindings(undefined);
  assert.equal(Object.keys(defaults).length, 15);
  assert.equal(new Set(Object.values(defaults)).size, 15);
  for (const input of [null, [], 7, "bad", {}])
    assert.deepEqual(normalizeShortcutBindings(input), defaults);
  const custom = normalizeShortcutBindings({
    search: "Alt+KeyK",
    page1: null,
    nextPage: "garbage",
    lastPage: {},
    firstPage: "KeyA",
  });
  assert.equal(custom.search, "Alt+KeyK");
  assert.equal(custom.page1, null);
  assert.equal(custom.nextPage, defaults.nextPage);
  assert.equal(custom.firstPage, defaults.firstPage);
  const collision = normalizeShortcutBindings({ search: "Ctrl+Digit1" });
  assert.equal(collision.search, "Ctrl+Digit1");
  assert.equal(collision.page1, null);
  const inherited = Object.create({ search: null });
  assert.equal(normalizeShortcutBindings(inherited).search, defaults.search);
  assert.deepEqual(
    normalizeShortcutBindings(JSON.parse(JSON.stringify(custom))),
    custom,
  );
});

test("shortcuts use exact modifiers, ignore composition/AltGraph, and allow function-key bindings", () => {
  assert.equal(shortcutFromEvent(key("Space")), "Ctrl+Space");
  assert.equal(shortcutFromEvent(key("Numpad2")), "Ctrl+Digit2");
  assert.equal(
    shortcutFromEvent(key("KeyK", { shiftKey: true })),
    "Ctrl+Shift+KeyK",
  );
  assert.equal(
    shortcutFromEvent(key("KeyK", { ctrlKey: false, metaKey: true })),
    "Meta+KeyK",
  );
  assert.equal(shortcutFromEvent(key("F6", { ctrlKey: false })), "F6");
  for (const event of [
    key("KeyA", { ctrlKey: false }),
    key("Space", { isComposing: true }),
    key("KeyK", { keyCode: 229 }),
    key("KeyK", { key: "Dead" }),
    key("KeyK", { getModifierState: () => true }),
    key("ControlLeft"),
  ])
    assert.equal(shortcutFromEvent(event), null);
  assert.equal(formatShortcut("Ctrl+ArrowLeft"), "Ctrl + ←");
  assert.equal(formatShortcut("Alt+KeyK"), "Alt + K");
  assert.equal(formatShortcut(null), "Disabled");
});

test("page shortcuts follow pager order, not persistent IDs, and stop at boundaries", () => {
  const pages = [{ id: 12 }, { id: 3 }, { id: 90 }];
  assert.equal(shortcutPageId("page2", pages, 12), 3);
  assert.equal(shortcutPageId("firstPage", pages, 90), 12);
  assert.equal(shortcutPageId("lastPage", pages, 12), 90);
  assert.equal(shortcutPageId("previousPage", pages, 90), 3);
  assert.equal(shortcutPageId("nextPage", pages, 12), 3);
  assert.equal(shortcutPageId("previousPage", pages, 12), null);
  assert.equal(shortcutPageId("nextPage", pages, 90), null);
  assert.equal(shortcutPageId("nextPage", pages, 999), null);
  assert.equal(shortcutPageId("page9", pages, 12), null);
  assert.equal(
    shortcutPageId(
      "page10",
      Array.from({ length: 10 }, (_, i) => ({ id: 20 + i })),
      20,
    ),
    29,
  );
  for (const { id } of WORKSPACE_SHORTCUTS)
    assert.equal(shortcutPageId(id, [], 1), null);
});

test("capture prevents double terminal input, respects local rebinding/disabled mode, and cleans up", () => {
  const original = globalThis.window;
  globalThis.window = new EventTarget();
  let bindings = normalizeShortcutBindings(undefined),
    enabled = true,
    downstream = 0;
  const calls = [];
  const dispose = installWorkspaceShortcuts(
    () => bindings,
    () => enabled,
    (action) => calls.push(action),
  );
  window.addEventListener("keydown", () => downstream++);
  const send = (code, extra = {}) => {
    const e = new Event("keydown", { cancelable: true });
    Object.assign(e, key(code, extra));
    window.dispatchEvent(e);
    return e.defaultPrevented;
  };
  try {
    assert.equal(send("Space"), true);
    assert.deepEqual(calls, ["search"]);
    assert.equal(downstream, 0);
    assert.equal(send("Space", { repeat: true }), true);
    assert.equal(calls.length, 1);
    assert.equal(send("Space", { shiftKey: true }), false);
    assert.equal(send("Space", { isComposing: true }), false);
    enabled = false;
    assert.equal(
      send("ArrowRight"),
      false,
      "phone/modal/settings mode leaves keys alone",
    );
    enabled = true;
    bindings = normalizeShortcutBindings({
      search: "Alt+KeyK",
      nextPage: null,
    });
    assert.equal(send("Space"), false);
    assert.equal(send("ArrowRight"), false);
    assert.equal(send("KeyK", { ctrlKey: false, altKey: true }), true);
    assert.deepEqual(calls, ["search", "search"]);
    dispose();
    assert.equal(send("KeyK", { ctrlKey: false, altKey: true }), false);
  } finally {
    dispose();
    globalThis.window = original;
  }
});
