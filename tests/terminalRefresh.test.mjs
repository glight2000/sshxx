import assert from "node:assert/strict";
import { test } from "node:test";
import { terminalRefresh } from "../src/lib/terminalRefresh.ts";

test("hidden pages cancel zoom paints and refresh only the final visible scale", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  let paints = 0;
  const refresh = terminalRefresh(() => paints++);
  refresh.update(1, true, false);
  refresh.update(1, true, true);
  t.mock.timers.tick(120);
  assert.equal(paints, 0);
  refresh.update(0.8, true, true);
  refresh.update(0.6, false, true);
  t.mock.timers.tick(120);
  assert.equal(paints, 0);
  refresh.update(0.6, true, true);
  refresh.update(0.7, true, true);
  t.mock.timers.tick(120);
  assert.equal(paints, 1);
  refresh.update(0.7, false, true);
  refresh.update(0.7, true, true);
  t.mock.timers.tick(120);
  assert.equal(paints, 1);
  // Returning to the already painted zoom must cancel an intermediate zoom.
  refresh.update(0.9, true, true);
  refresh.update(0.7, true, true);
  t.mock.timers.tick(120);
  assert.equal(paints, 1);
  refresh.update(1, true, true);
  refresh.dispose();
  refresh.update(2, true, true);
  t.mock.timers.tick(120);
  assert.equal(paints, 1);
});
