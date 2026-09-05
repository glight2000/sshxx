import assert from "node:assert/strict";
import test from "node:test";
import { surfaceBackground, surfaceTone } from "../src/lib/ui/surfaceTheme.ts";

test("legacy backgrounds follow UI mode; explicit colors remain independent", () => {
  for (const legacy of ["#3f3f46", "#111113", "#18181b"]) {
    assert.equal(surfaceBackground(legacy.toUpperCase(), legacy), "");
    assert.equal(surfaceTone(surfaceBackground(legacy, legacy)), undefined);
    assert.equal(surfaceBackground("#ffffff", legacy), "#ffffff");
  }
  assert.equal(surfaceTone("#ffffff"), "light");
  assert.equal(surfaceTone("#FFFF00"), "light");
  assert.equal(surfaceTone("#101020"), "dark");
  assert.equal(surfaceTone("invalid"), undefined);
});
