import assert from "node:assert/strict";
import { test } from "node:test";

import {
  constrainTerminalResize,
  TERMINAL_MAX_COLS,
  TERMINAL_MAX_ROWS,
  TERMINAL_MAX_WINDOW_SIZE,
  TERMINAL_MIN_COLS,
  TERMINAL_MIN_ROWS,
} from "../src/lib/terminalGeometry.ts";

const base = {
  left: 100,
  top: 200,
  right: 815,
  bottom: 723,
  startWidth: 715,
  startHeight: 523,
  startRows: 26,
  startCols: 79,
  cellWidth: 8,
  cellHeight: 15,
};

test("applies the same minimum size from every terminal edge", () => {
  const north = constrainTerminalResize({
    ...base,
    direction: "n",
    top: 1_000,
  });
  const south = constrainTerminalResize({
    ...base,
    direction: "s",
    bottom: 0,
  });
  const west = constrainTerminalResize({
    ...base,
    direction: "w",
    left: 1_000,
  });
  const east = constrainTerminalResize({
    ...base,
    direction: "e",
    right: 0,
  });

  assert.equal(north.height, south.height);
  assert.equal(north.rows, TERMINAL_MIN_ROWS);
  assert.equal(south.rows, TERMINAL_MIN_ROWS);
  assert.equal(north.y + north.height, base.bottom);
  assert.equal(south.y, base.top);
  assert.equal(west.width, east.width);
  assert.equal(west.cols, TERMINAL_MIN_COLS);
  assert.equal(east.cols, TERMINAL_MIN_COLS);
  assert.equal(west.x + west.width, base.right);
  assert.equal(east.x, base.left);
});

test("caps oversized terminal windows and PTY dimensions", () => {
  const resized = constrainTerminalResize({
    ...base,
    direction: "se",
    right: 20_000,
    bottom: 20_000,
  });
  assert.equal(resized.width, TERMINAL_MAX_WINDOW_SIZE);
  assert.equal(resized.height, TERMINAL_MAX_WINDOW_SIZE);
  assert.ok(resized.cols <= TERMINAL_MAX_COLS);
  assert.ok(resized.rows <= TERMINAL_MAX_ROWS);
});
