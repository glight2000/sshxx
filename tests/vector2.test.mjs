import assert from "node:assert/strict";
import test from "node:test";

import {
  addVector,
  clamp,
  divideVector,
  lerpVector,
  multiplyVector,
  subtractVector,
  vectorsEqual,
} from "../src/lib/vector2.ts";

test("local vector operations cover canvas pan and zoom math", () => {
  assert.deepEqual(addVector([2, 4], [3, -1]), [5, 3]);
  assert.deepEqual(subtractVector([2, 4], [3, -1]), [-1, 5]);
  assert.deepEqual(multiplyVector([2, -4], 0.5), [1, -2]);
  assert.deepEqual(divideVector([2, -4], 2), [1, -2]);
  assert.deepEqual(lerpVector([0, 10], [10, 30], 0.25), [2.5, 15]);
  assert.equal(vectorsEqual([1, 2], [1, 2]), true);
  assert.equal(vectorsEqual([1, 2], [2, 1]), false);
  assert.equal(clamp(3, 0, 2), 2);
});
