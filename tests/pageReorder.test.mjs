import assert from "node:assert/strict";
import test from "node:test";
import { pageReorder } from "../src/lib/action/pageReorder.ts";

function fixture(t) {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const originals = Object.fromEntries(
    [
      "window",
      "document",
      "Element",
      "requestAnimationFrame",
      "cancelAnimationFrame",
    ].map((key) => [key, globalThis[key]]),
  );
  let preview = null;
  const commits = [],
    frames = new Map();
  let frameId = 0;
  const initial = [1, 2, 3].map((id) => ({ id }));
  class Element extends EventTarget {
    constructor(id) {
      super();
      this.dataset = { canvasPageId: String(id) };
    }
    closest(selector) {
      if (selector === "[data-page-action]")
        return this.pageAction ? this : null;
      return this;
    }
    get offsetLeft() {
      return (
        (preview?.ids ?? [1, 2, 3]).indexOf(Number(this.dataset.canvasPageId)) *
        100
      );
    }
    get offsetWidth() {
      return 100;
    }
    getBoundingClientRect() {
      return { left: this.offsetLeft, top: 0, width: 100, height: 40 };
    }
  }
  const items = [1, 2, 3].map((id) => new Element(id));
  const node = new EventTarget();
  let captured = null;
  Object.assign(node, {
    setPointerCapture: (id) => {
      captured = id;
    },
    hasPointerCapture: (id) => captured === id,
    releasePointerCapture: () => {
      captured = null;
    },
    scrollLeft: 0,
    contains: (item) => items.includes(item),
    querySelectorAll: () =>
      (preview?.ids ?? [1, 2, 3]).map((id) => items[id - 1]),
    getBoundingClientRect: () => ({ left: 0, right: 300, top: 0, bottom: 40 }),
  });
  Object.assign(globalThis, {
    window: new EventTarget(),
    document: {
      documentElement: { style: { cursor: "auto", userSelect: "text" } },
    },
    Element,
    requestAnimationFrame: (callback) => {
      frames.set(++frameId, callback);
      return frameId;
    },
    cancelAnimationFrame: (id) => frames.delete(id),
  });
  let options = {
    pages: initial,
    enabled: true,
    preview: (value) => {
      preview = value;
    },
    commit: (value) => commits.push(value),
  };
  const action = pageReorder(node, options);
  t.after(() => {
    action.destroy();
    Object.assign(globalThis, originals);
  });
  const send = (type, props = {}) => {
    const event = new Event(type, { cancelable: true });
    Object.assign(event, {
      pointerId: 1,
      pointerType: "mouse",
      isPrimary: true,
      button: 0,
      clientX: 50,
      clientY: 20,
      detail: 1,
      ...props,
    });
    if (type === "pointerdown")
      Object.defineProperty(event, "target", { value: items[0] });
    (["pointerdown", "click", "dblclick", "lostpointercapture"].includes(type)
      ? node
      : window
    ).dispatchEvent(event);
    return event;
  };
  return {
    item: items[0],
    action,
    commits,
    frames,
    get captured() {
      return captured;
    },
    send,
    get preview() {
      return preview;
    },
    update(next) {
      options = { ...options, ...next };
      action.update(options);
    },
  };
}

test("the page actions button never starts sorting or suppresses its click", (t) => {
  const f = fixture(t);
  f.item.pageAction = true;
  f.send("pointerdown");
  f.send("pointermove", { clientX: 260 });
  f.send("pointerup", { clientX: 260 });
  assert.equal(f.preview, null);
  assert.deepEqual(f.commits, []);
  assert.equal(f.send("click").defaultPrevented, false);
});

test("drag threshold preserves clicks; dropping sends once and suppresses click/rename", (t) => {
  const f = fixture(t);
  f.send("pointerdown");
  f.send("pointermove", { clientX: 54 });
  f.send("pointerup");
  assert.equal(f.preview, null);
  assert.equal(f.send("click").defaultPrevented, false);
  f.send("pointerdown");
  f.send("pointermove", { clientX: 260 });
  assert.deepEqual(f.preview.ids, [2, 3, 1]);
  assert.equal(f.preview.drag.id, 1);
  assert.equal(f.captured, 1);
  f.send("pointerup", { clientX: 260 });
  assert.deepEqual(f.commits, [{ id: 1, beforeId: null }]);
  assert.equal(f.preview.drag, null);
  assert.equal(f.send("click").defaultPrevented, true);
  assert.equal(f.send("dblclick").defaultPrevented, true);
  assert.equal(f.send("click", { detail: 0 }).defaultPrevented, false);
  assert.equal(f.frames.size, 0);
  assert.equal(f.captured, null);
  assert.deepEqual(document.documentElement.style, {
    cursor: "auto",
    userSelect: "text",
  });
  f.update({ pages: [{ id: 2 }, { id: 3 }, { id: 1 }] });
  assert.equal(f.preview, null);
  t.mock.timers.tick(3000);
  assert.equal(f.commits.length, 1);
});

test("cancel, outside drop, disconnect and concurrent updates discard previews", (t) => {
  const f = fixture(t);
  for (const cancel of [
    () => f.send("keydown", { key: "Escape" }),
    () => f.send("pointercancel"),
    () => f.send("lostpointercapture"),
    () => f.send("blur"),
    () => f.send("resize"),
    () => f.send("pointerup", { clientY: -100 }),
    () => f.update({ enabled: false }),
    () => f.update({ pages: [{ id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }] }),
  ]) {
    f.update({ pages: [{ id: 1 }, { id: 2 }, { id: 3 }], enabled: true });
    f.send("pointerdown");
    f.send("pointermove", { clientX: 260 });
    cancel();
    assert.equal(f.preview, null);
    assert.equal(f.frames.size, 0);
    f.send("pointerup", { clientX: 260 });
    assert.deepEqual(f.commits, []);
  }
});

test("readonly and unrelated pointers cannot reorder; unacknowledged drops expire without retry", (t) => {
  const f = fixture(t);
  f.update({ enabled: false });
  f.send("pointerdown");
  f.send("pointermove", { clientX: 260 });
  assert.equal(f.preview, null);
  f.update({ enabled: true });
  f.send("pointerdown");
  f.send("pointermove", { pointerId: 2, clientX: 260 });
  assert.equal(f.preview, null);
  f.send("pointermove", { clientX: 260 });
  f.send("pointerup", { clientX: 260 });
  t.mock.timers.tick(2000);
  assert.equal(f.preview, null);
  assert.equal(f.commits.length, 1);
  f.send("pointerdown");
  f.send("pointermove", { clientX: 260 });
  f.action.destroy();
  assert.equal(f.preview, null);
  assert.equal(f.frames.size, 0);
  f.send("pointerup", { clientX: 260 });
  assert.equal(f.commits.length, 1);
});
