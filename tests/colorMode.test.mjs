import assert from "node:assert/strict";
import { test } from "node:test";

import {
  isColorModePreference,
  resolveColorMode,
} from "../src/lib/colorMode.ts";

test("validates persisted color mode preferences", () => {
  assert.equal(isColorModePreference("system"), true);
  assert.equal(isColorModePreference("light"), true);
  assert.equal(isColorModePreference("dark"), true);
  assert.equal(isColorModePreference("auto"), false);
  assert.equal(isColorModePreference(null), false);
});

test("resolves explicit and system color modes", () => {
  assert.equal(resolveColorMode("system", true), "dark");
  assert.equal(resolveColorMode("system", false), "light");
  assert.equal(resolveColorMode("light", true), "light");
  assert.equal(resolveColorMode("dark", false), "dark");
});
