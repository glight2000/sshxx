import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import {
  groupNavigationItems,
  navigationItemKey,
  mobileAssociationTargets,
} from "../src/lib/mobileNavigation.ts";
import {
  installMobileCanvas,
  touchCamera,
} from "../src/lib/action/mobileCanvas.ts";
import { mobileViewport } from "../src/lib/action/mobilePage.ts";

test("only phone detail pages hide the shared component titlebar controls", () => {
  const read = (file) =>
    readFileSync(
      new URL(`../src/lib/ui/${file}.svelte`, import.meta.url),
      "utf8",
    );
  assert.match(
    read("MobileNavigator"),
    /:global\(main\.mobile-detail \.canvas-fullscreen \[data-canvas-titlebar\]\)\s*\{\s*display: none !important;/,
  );
  for (const name of [
    "XTerm",
    "Note",
    "CustomComponent",
    "FileExplorerHeader",
  ]) {
    assert.match(
      read(name),
      /data-canvas-titlebar/,
      `${name} participates in the same phone chrome rule`,
    );
  }
});

test("phone focus and relation navigation do not raise shared desktop windows", () => {
  const session = readFileSync(
    new URL("../src/lib/Session.svelte", import.meta.url),
    "utf8",
  );
  for (const name of ["bringFileWindowToFront", "bringCustomWindowToFront"]) {
    assert.match(
      session,
      new RegExp(
        `function ${name}\\([^)]*\\) \\{\\s*if \\(mobileAvailable\\) return;`,
      ),
    );
  }
  assert.match(
    session,
    /on:bringToFront=\{\(\) => \{\s*if \(mobileAvailable \|\| !hasWriteAccess\) return;/,
  );
  assert.match(session, /on:bringToFront=\{\(\) =>\s*!mobileAvailable &&/);
  assert.match(
    session,
    /function navigateCanvasRelation[^]*?if \(mobileAvailable\) \{\s*void openMobileItem/,
  );
});

test("phone page follows the visible keyboard viewport and removes listeners on teardown", () => {
  const original = globalThis.window;
  const viewport = Object.assign(new EventTarget(), {
    offsetTop: 0,
    height: 844,
    offsetLeft: 0,
    width: 390,
  });
  globalThis.window = Object.assign(new EventTarget(), {
    visualViewport: viewport,
    innerHeight: 844,
    innerWidth: 390,
  });
  const values = new Map();
  const node = {
    style: {
      setProperty: (k, v) => values.set(k, v),
      removeProperty: (k) => values.delete(k),
    },
  };
  const action = mobileViewport(node, true);
  try {
    assert.equal(values.get("--mobile-viewport-height"), "844px");
    viewport.height = 420;
    viewport.offsetTop = 30;
    viewport.dispatchEvent(new Event("resize"));
    assert.equal(values.get("--mobile-viewport-height"), "420px");
    assert.equal(values.get("--mobile-viewport-width"), "390px");
    assert.equal(values.get("--mobile-viewport-top"), "30px");
    action.destroy();
    viewport.dispatchEvent(new Event("resize"));
    assert.equal(values.size, 0);
  } finally {
    action.destroy();
    globalThis.window = original;
  }
});

test("phone associations filter self, duplicates, reverse links and other pages", () => {
  const items = [
    { id: 1, kind: "note", pageId: 1 },
    { id: 2, kind: "note", pageId: 1 },
    { id: 3, kind: "note", pageId: 1 },
    { id: 4, kind: "note", pageId: 1 },
    { id: 1, kind: "terminal", pageId: 1 },
    { id: 2, kind: "terminal", pageId: 1 },
    { id: 3, kind: "terminal", pageId: 2 },
    { id: 1, kind: "custom", pageId: 1 },
    { id: 1, kind: "file", pageId: 1 },
    { id: 2, kind: "file", pageId: 1 },
  ];
  const note = { pageId: 1, linkedShellIds: [1], linkedFileWindowIds: [1] };
  assert.deepEqual(
    mobileAssociationTargets(items, 1, note, [2, 3]).map(navigationItemKey),
    ["note:4", "terminal:2", "file:2"],
  );
  assert.deepEqual(mobileAssociationTargets(items, 1, undefined, []), []);
});

test("mobile navigation groups all pages and kinds without mutating shared data", () => {
  const pages = [
    { id: 1, name: "First" },
    { id: 2, name: "Second" },
    { id: 3, name: "Empty" },
  ];
  const items = Array.from({ length: 110 }, (_, id) => ({
    id,
    kind: "note",
    pageId: 2,
    minimized: true,
  }));
  items.push(
    { id: 1, kind: "terminal", pageId: 1 },
    { id: 1, kind: "file", pageId: 1 },
    { id: 1, kind: "custom", pageId: 999 },
  );
  const snapshot = structuredClone(items);
  const groups = groupNavigationItems(pages, items);
  assert.deepEqual(
    groups.map((g) => g.items.length),
    [2, 110, 0],
  );
  assert.notEqual(navigationItemKey(items[110]), navigationItemKey(items[111]));
  assert.deepEqual(items, snapshot);
});

test("phone camera pans and pinches around the fingers in the existing world coordinate system", () => {
  const p = (x, y) => ({ x, y });
  assert.deepEqual(
    touchCamera({ center: [10, 20], zoom: 0.5 }, [p(20, 30)], [p(30, 50)]),
    { center: [-10, -20], zoom: 0.5 },
  );
  // World anchor is inside the scaled transform, so it cancels from this equation.
  const pinch = touchCamera(
    { center: [10, 20], zoom: 1 },
    [p(100, 100), p(200, 100)],
    [p(50, 110), p(250, 110)],
  );
  assert.deepEqual(pinch, { center: [85, 65], zoom: 2 });
  assert.equal(
    touchCamera(
      { center: [0, 0], zoom: 1 },
      [p(0, 0), p(100, 0)],
      [p(0, 0), p(1, 0)],
    ).zoom,
    0.35,
  );
  assert.equal(
    touchCamera(
      { center: [0, 0], zoom: 1 },
      [p(0, 0), p(0, 0)],
      [p(0, 0), p(1, 0)],
    ).zoom,
    1,
  );
});

test("phone overview distinguishes taps, drags and pinch; cancellation and cleanup never focus a component", () => {
  const originalWindow = globalThis.window;
  globalThis.window = new EventTarget();
  const frames = new Map();
  let frameId = 0;
  window.requestAnimationFrame = (callback) => {
    frames.set(++frameId, callback);
    return frameId;
  };
  window.cancelAnimationFrame = (id) => frames.delete(id);
  const paint = () => {
    const callbacks = [...frames.values()];
    frames.clear();
    callbacks.forEach((callback) => callback());
  };
  const node = new EventTarget();
  const captured = new Set();
  node.setPointerCapture = (id) => captured.add(id);
  node.hasPointerCapture = (id) => captured.has(id);
  node.releasePointerCapture = (id) => captured.delete(id);
  let enabled = true,
    taps = 0,
    writes = 0,
    previews = 0;
  let preview;
  let view = { center: [0, 0], zoom: 1 };
  const send = (type, id, x = 0, pointerType = "touch") => {
    const event = new Event(type, { cancelable: true });
    Object.assign(event, {
      pointerId: id,
      pointerType,
      clientX: x,
      clientY: 0,
    });
    (type === "pointerdown" ? node : window).dispatchEvent(event);
    return event.defaultPrevented;
  };
  const dispose = installMobileCanvas(
    node,
    () => enabled,
    () => view,
    (next, settled) => {
      if (settled) {
        view = next;
        writes++;
      } else {
        preview = next;
        previews++;
      }
    },
    () => taps++,
  );
  try {
    assert.equal(send("pointerdown", 1, 0, "mouse"), false);
    send("pointerdown", 1);
    send("pointerup", 1);
    assert.equal(taps, 1);
    send("pointerdown", 2);
    for (let x = 7; x <= 30; x++) send("pointermove", 2, x);
    assert.equal(frames.size, 1, "coalesce high-frequency touch samples");
    assert.equal(writes, 0, "do not update the component tree while dragging");
    assert.equal(previews, 0);
    paint();
    assert.equal(previews, 1);
    assert.deepEqual(preview.center, [-30, 0]);
    send("pointerup", 2, 30);
    assert.equal(writes, 1);
    assert.equal(frames.size, 0);
    assert.equal(taps, 1);
    assert.deepEqual(view.center, [-30, 0]);
    send("pointerdown", 3);
    send("pointerdown", 4, 100);
    send("pointermove", 4, 200);
    send("pointerup", 4, 200);
    send("pointerup", 3);
    assert.equal(taps, 1);
    assert.equal(view.zoom, 2);
    send("pointerdown", 5);
    send("pointercancel", 5);
    assert.equal(taps, 1);
    enabled = false;
    assert.equal(send("pointerdown", 6), false);
    const before = writes;
    send("pointermove", 6, 50);
    assert.equal(writes, before);
    enabled = true;
    send("pointerdown", 7);
    send("pointermove", 7, 30);
    assert.equal(frames.size, 1);
    dispose();
    assert.equal(frames.size, 0, "teardown cancels queued paints");
    assert.equal(captured.size, 0);
    send("pointerup", 7);
    assert.equal(taps, 1);
  } finally {
    dispose();
    globalThis.window = originalWindow;
  }
});
