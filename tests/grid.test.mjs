import assert from "node:assert/strict";
import { test } from "node:test";

import { arrangeNewCanvasItem } from "../src/lib/arrange.ts";
import {
  GRID_EDGE_GAP,
  GRID_SIZE,
  gridAlignedRect,
  gridLeadingEdge,
  gridTrailingEdge,
} from "../src/lib/grid.ts";

test("leading and trailing anchors use exact one-tenth grid offsets", () => {
  const rawLeading = 33;
  const rawTrailing = 87;
  const nearestLeading = Math.round(rawLeading / GRID_SIZE) * GRID_SIZE;
  const nearestTrailing = Math.round(rawTrailing / GRID_SIZE) * GRID_SIZE;

  assert.equal(GRID_EDGE_GAP / GRID_SIZE, 0.1);
  assert.equal(gridLeadingEdge(rawLeading) - nearestLeading, GRID_EDGE_GAP);
  assert.equal(nearestTrailing - gridTrailingEdge(rawTrailing), GRID_EDGE_GAP);
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
