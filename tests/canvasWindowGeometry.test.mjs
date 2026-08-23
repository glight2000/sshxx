import assert from "node:assert/strict";
import test from "node:test";

import { resizeCanvasWindow } from "../src/lib/canvasWindowGeometry.ts";

const identity = (value) => value;

test("applies the same canvas minimum from every resize edge", () => {
  const start = { x: 100, y: 100, width: 500, height: 400 };
  const west = resizeCanvasWindow({
    start,
    startPointer: [100, 100],
    pointer: [590, 100],
    direction: "w",
    minWidth: 320,
    minHeight: 240,
    snapLeading: identity,
    snapTrailing: identity,
  });
  const east = resizeCanvasWindow({
    start,
    startPointer: [600, 500],
    pointer: [110, 500],
    direction: "e",
    minWidth: 320,
    minHeight: 240,
    snapLeading: identity,
    snapTrailing: identity,
  });
  assert.deepEqual(west, { x: 280, y: 100, width: 320, height: 400 });
  assert.deepEqual(east, { x: 100, y: 100, width: 320, height: 400 });
});

test("caps canvas geometry while preserving the stationary edge", () => {
  const resized = resizeCanvasWindow({
    start: { x: 100, y: 100, width: 500, height: 400 },
    startPointer: [100, 100],
    pointer: [-10_000, -10_000],
    direction: "nw",
    minWidth: 320,
    minHeight: 240,
    maxWidth: 4_000,
    maxHeight: 4_000,
    snapLeading: identity,
    snapTrailing: identity,
  });
  assert.deepEqual(resized, {
    x: -3_400,
    y: -3_500,
    width: 4_000,
    height: 4_000,
  });
});
