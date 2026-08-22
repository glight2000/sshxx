import assert from "node:assert/strict";
import test from "node:test";

import { canvasPageMoveView } from "../src/lib/canvasPageMove.ts";

test("centers the target page on all moved canvas items", () => {
  assert.deepEqual(
    canvasPageMoveView(
      [
        { x: 100, y: 200, width: 400, height: 200 },
        { x: 700, y: 500, width: 200, height: 300 },
      ],
      952,
      1000,
      1,
    ),
    { center: [500, 500], zoom: 0.95 },
  );
});

test("keeps the target page zoom when the moved group already fits", () => {
  assert.deepEqual(
    canvasPageMoveView(
      [{ x: -100, y: 50, width: 200, height: 100 }],
      1200,
      800,
      0.8,
    ),
    { center: [0, 100], zoom: 0.8 },
  );
});

test("does not invent a view for an empty move", () => {
  assert.equal(canvasPageMoveView([], 1200, 800, 1), null);
});
