import assert from "node:assert/strict";
import { test } from "node:test";

import {
  arrangeNewCanvasItem,
  arrangeNewCanvasItemNear,
} from "../src/lib/arrange.ts";
import {
  GRID_EDGE_GAP,
  GRID_SIZE,
  MINIMIZED_WINDOW_HEIGHT,
  gridAlignedRect,
  gridLeadingEdge,
  gridSpanSize,
  gridTrailingEdge,
} from "../src/lib/grid.ts";

test("minimized canvas windows occupy exactly one grid unit", () => {
  assert.equal(MINIMIZED_WINDOW_HEIGHT, GRID_SIZE);
});

test("leading and trailing anchors use exact one-tenth grid offsets", () => {
  const rawLeading = 33;
  const rawTrailing = 87;
  const nearestLeading = Math.round(rawLeading / GRID_SIZE) * GRID_SIZE;
  const nearestTrailing = Math.round(rawTrailing / GRID_SIZE) * GRID_SIZE;

  assert.equal(GRID_EDGE_GAP / GRID_SIZE, 0.1);
  assert.equal(gridLeadingEdge(rawLeading) - nearestLeading, GRID_EDGE_GAP);
  assert.equal(nearestTrailing - gridTrailingEdge(rawTrailing), GRID_EDGE_GAP);
});

test("grid-spanning window sizes retain both visual edge insets", () => {
  assert.equal(gridSpanSize(2), 2 * GRID_SIZE - 2 * GRID_EDGE_GAP);
  assert.equal(gridSpanSize(3), 3 * GRID_SIZE - 2 * GRID_EDGE_GAP);
});

test("new item aligns both edges to the inset canvas grid", () => {
  const rect = gridAlignedRect({
    x: 13,
    y: -37,
    width: 384,
    height: 224,
  });

  assert.equal(rect.x, gridLeadingEdge(13));
  assert.equal(rect.y, gridLeadingEdge(-37));
  assert.equal(rect.x + rect.width, gridTrailingEdge(rect.x + 384));
  assert.equal(rect.y + rect.height, gridTrailingEdge(rect.y + 224));
});

test("arrangement uses the requested item dimensions", () => {
  const existing = [{ x: 0, y: 0, width: 720, height: 520 }];
  const position = arrangeNewCanvasItem(existing, 384, 224);
  const overlaps =
    position.x - 16 < 720 &&
    position.x + 384 + 16 > 0 &&
    position.y - 16 < 520 &&
    position.y + 224 + 16 > 0;
  assert.equal(overlaps, false);
});

test("source-derived arrangement prefers a nearby adjacent vacancy", () => {
  const source = { x: 80, y: 120, width: 400, height: 240 };
  const position = arrangeNewCanvasItemNear([source], 320, 200, source);

  assert.deepEqual(position, { x: 80, y: 400 });
});

test("source-derived arrangement stays nearby when overlap is unavoidable", () => {
  const source = { x: 0, y: 0, width: 400, height: 240 };
  const blockers = [source];
  for (let x = -1_200; x <= 1_200; x += 200) {
    for (let y = -1_000; y <= 1_000; y += 200) {
      blockers.push({ x, y, width: 200, height: 200 });
    }
  }

  const position = arrangeNewCanvasItemNear(blockers, 320, 200, source);
  assert.deepEqual(position, { x: 40, y: 40 });
});
