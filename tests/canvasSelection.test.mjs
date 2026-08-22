import assert from "node:assert/strict";
import test from "node:test";

import {
  canvasItemKey,
  marqueeRect,
  parseCanvasItemKey,
  rectsIntersect,
} from "../src/lib/canvasSelection.ts";

test("canvas item keys preserve kind and id", () => {
  const key = canvasItemKey("terminal", 42);
  assert.equal(key, "terminal:42");
  assert.deepEqual(parseCanvasItemKey(key), { kind: "terminal", id: 42 });
});

test("marquee rectangles normalize every drag direction", () => {
  assert.deepEqual(marqueeRect(20, 30, 5, 10), {
    left: 5,
    top: 10,
    right: 20,
    bottom: 30,
  });
});

test("marquee selection includes intersecting and touching components", () => {
  const marquee = marqueeRect(10, 10, 30, 30);
  assert.equal(
    rectsIntersect(marquee, { left: 20, top: 20, right: 40, bottom: 40 }),
    true,
  );
  assert.equal(
    rectsIntersect(marquee, { left: 30, top: 5, right: 50, bottom: 10 }),
    true,
  );
  assert.equal(
    rectsIntersect(marquee, { left: 31, top: 10, right: 50, bottom: 30 }),
    false,
  );
});
