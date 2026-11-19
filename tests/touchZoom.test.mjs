import assert from "node:assert/strict";
import test from "node:test";
import { TouchZoom } from "../src/lib/action/touchZoom.ts";

function fixture(t) {
  const originals = Object.fromEntries(
    [
      "window",
      "document",
      "ResizeObserver",
      "requestAnimationFrame",
      "cancelAnimationFrame",
    ].map((key) => [key, globalThis[key]]),
  );
  const node = new EventTarget();
  const document = new EventTarget();
  document.body = {};
  Object.assign(node, {
    parentElement: document.body,
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 800, height: 600 }),
    classList: { add() {}, remove() {}, toggle() {} },
  });
  const window = new EventTarget();
  window.navigator = { platform: "Linux" };
  const frames = new Map();
  let frameId = 0;
  Object.assign(globalThis, {
    window,
    document,
    ResizeObserver: class {
      observe() {}
      disconnect() {}
    },
    requestAnimationFrame: (callback) => {
      frames.set(++frameId, callback);
      return frameId;
    },
    cancelAnimationFrame: (id) => frames.delete(id),
  });
  const camera = new TouchZoom(node);
  t.after(() => {
    camera.destroy();
    Object.assign(globalThis, originals);
  });
  const wheel = (deltaX, deltaY, timeStamp, props = {}) => {
    const event = new Event("wheel", { cancelable: true });
    Object.defineProperty(event, "timeStamp", { value: timeStamp });
    Object.assign(event, {
      deltaX,
      deltaY,
      deltaMode: 0,
      buttons: 0,
      clientX: 400,
      clientY: 300,
      ...props,
    });
    return event;
  };
  return { camera, wheel, frames };
}

for (const first of ["horizontal", "vertical"]) {
  test(`a ${first} start allows both axes and turns at identical wheel timestamps`, (t) => {
    const { camera, wheel } = fixture(t);
    const x = wheel(20, 0, 100);
    const y = wheel(0, 30, 100);
    for (const event of first === "horizontal" ? [x, y] : [y, x])
      camera.wheel(event, "pan");
    assert.deepEqual(camera.center, [10, 15]);
    camera.wheel(wheel(-8, 12, 100), "pan");
    assert.deepEqual(camera.center, [6, 21]);
    camera.wheel(wheel(0, -10, 110), "pan");
    camera.wheel(wheel(6, 0, 120), "pan");
    assert.deepEqual(camera.center, [9, 16]);
    assert.equal(camera.zoom, 1);
    assert.ok(x.defaultPrevented && y.defaultPrevented);
  });
}

test("the same wheel event moves once even when two camera listeners receive it", (t) => {
  const { camera, wheel, frames } = fixture(t);
  const pan = wheel(12, 8, 100);
  camera.wheel(pan, "pan");
  camera.wheel(pan, "pan");
  assert.deepEqual(camera.center, [6, 4]);
  const zoom = wheel(0, 4, 110, { ctrlKey: true });
  window.dispatchEvent(zoom);
  const once = { center: [...camera.center], zoom: camera.zoom };
  camera.wheel(zoom, "zoom");
  assert.deepEqual({ center: camera.center, zoom: camera.zoom }, once);
  assert.equal(frames.size, 1, "camera paints remain batched per frame");
  camera.destroy();
  assert.equal(frames.size, 0);
});
