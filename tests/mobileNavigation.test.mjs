import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { mobileAssociationTargets } from "../src/lib/mobileNavigation.ts";
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
    read("MobileWorkspaceControls"),
    /:global\(main\.mobile-detail \.canvas-fullscreen \[data-canvas-titlebar\]\)\s*\{\s*display: none !important;/,
  );
  assert.match(
    read("MobileWorkspaceControls"),
    /:global\(main\.mobile-detail \.desktop-toolbar\),\s*:global\(main\.mobile-detail \.desktop-page-pager\)\s*\{\s*display: none;/,
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
    /function navigateCanvasRelation[^]*?if \(mobileAvailable\) \{[^]*?selectCanvasItem\(target\)/,
  );
  assert.doesNotMatch(
    session,
    /openMobileItem|mobileListEnabled|mobileReturnToList/,
  );
  assert.match(session, /if \(key\) focusCanvasItem\(key\)/);
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
    const pinch = new Event("touchmove", { cancelable: true });
    Object.assign(pinch, { touches: [{}, {}] });
    window.dispatchEvent(pinch);
    assert.equal(pinch.defaultPrevented, true);
    const scroll = new Event("touchmove", { cancelable: true });
    Object.assign(scroll, { touches: [{}] });
    window.dispatchEvent(scroll);
    assert.equal(scroll.defaultPrevented, false);
    action.update(false);
    const desktop = new Event("gesturechange", { cancelable: true });
    window.dispatchEvent(desktop);
    assert.equal(desktop.defaultPrevented, false);
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
    mobileAssociationTargets(items, 1, note, [2, 3]).map(
      (item) => `${item.kind}:${item.id}`,
    ),
    ["note:4", "terminal:2", "file:2"],
  );
  assert.deepEqual(mobileAssociationTargets(items, 1, undefined, []), []);
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

test("phone gesture ownership: focus, native content, pinch, fullscreen and cleanup", () => {
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
  let enabled = true,
    full = false,
    native = false,
    taps = 0,
    writes = 0,
    previews = 0;
  let preview;
  let view = { center: [0, 0], zoom: 1 };
  const send = (type, ...positions) => {
    const event = new Event(type, { cancelable: true });
    Object.assign(event, {
      touches: positions.map((x, identifier) => ({
        identifier,
        clientX: x,
        clientY: 0,
      })),
    });
    (type === "touchstart" ? node : window).dispatchEvent(event);
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
    () => full,
    () => native,
  );
  try {
    enabled = false;
    assert.equal(send("touchstart", 0), false, "desktop untouched");
    enabled = true;
    send("touchstart", 0);
    send("touchend");
    assert.equal(taps, 1);
    assert.equal(writes, 0, "tap never moves camera");
    send("touchstart", 0);
    for (let x = 7; x <= 30; x++) send("touchmove", x);
    assert.equal(frames.size, 1, "coalesce touch samples");
    assert.equal(writes, 0);
    assert.equal(previews, 0);
    paint();
    assert.deepEqual(preview.center, [-30, 0]);
    send("touchend");
    assert.equal(writes, 1);
    assert.equal(taps, 1);
    native = true;
    assert.equal(
      send("touchstart", 0),
      false,
      "focused content receives single touch",
    );
    assert.equal(send("touchmove", 10), false);
    send("touchend");
    assert.equal(writes, 1);
    send("touchstart", 0);
    assert.equal(
      send("touchstart", 0, 100),
      true,
      "pinch takes over focused content",
    );
    send("touchmove", 0, 200);
    send("touchend", 0);
    send("touchmove", 80);
    send("touchend");
    assert.equal(view.zoom, 2);
    assert.equal(writes, 2);
    assert.equal(taps, 1);
    const previous = structuredClone(view);
    full = true;
    assert.equal(
      send("touchstart", 0),
      false,
      "fullscreen native scroll preserved",
    );
    assert.equal(send("touchstart", 0, 100), true);
    assert.equal(send("touchmove", 10, 250), true);
    send("touchend", 10);
    send("touchend");
    assert.equal(writes, 2);
    assert.deepEqual(view, previous, "fullscreen never changes camera");
    full = native = false;
    send("touchstart", 0);
    send("touchcancel");
    assert.equal(taps, 1);
    send("touchstart", 0);
    send("touchmove", 30);
    assert.equal(frames.size, 1);
    dispose();
    assert.equal(frames.size, 0, "teardown cancels queued paints");
    send("touchend");
    assert.equal(taps, 1);
  } finally {
    dispose();
    globalThis.window = originalWindow;
  }
});
