import assert from "node:assert/strict";
import { test } from "node:test";

import { remapTerminalClientPoint } from "../src/lib/xtermMouseCoordinates.ts";

test("maps a transformed terminal point back into xterm layout coordinates", () => {
  const scale = 0.9431700053967927;
  assert.deepEqual(
    remapTerminalClientPoint(100 + 400 * scale, 50 + 240 * scale, {
      left: 100,
      top: 50,
      visualWidth: 800 * scale,
      visualHeight: 480 * scale,
      layoutWidth: 800,
      layoutHeight: 480,
    }).map((value) => Math.round(value)),
    [500, 290],
  );
});

test("leaves unscaled and invalid terminal coordinates unchanged", () => {
  assert.deepEqual(
    remapTerminalClientPoint(320, 180, {
      left: 100,
      top: 50,
      visualWidth: 800,
      visualHeight: 480,
      layoutWidth: 800,
      layoutHeight: 480,
    }),
    [320, 180],
  );
  assert.deepEqual(
    remapTerminalClientPoint(320, 180, {
      left: 100,
      top: 50,
      visualWidth: 0,
      visualHeight: 0,
      layoutWidth: 0,
      layoutHeight: 0,
    }),
    [320, 180],
  );
});
