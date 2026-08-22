import assert from "node:assert/strict";
import { test } from "node:test";

import {
  canvasPanButton,
  canvasSelectionButton,
} from "../src/lib/canvasMouseButtons.ts";

test("canvas mouse buttons retain the default selection and pan mapping", () => {
  assert.equal(canvasSelectionButton(false), 0);
  assert.equal(canvasPanButton(false), 2);
});

test("canvas mouse buttons exchange selection and pan as one setting", () => {
  assert.equal(canvasSelectionButton(true), 2);
  assert.equal(canvasPanButton(true), 0);
});
