import assert from "node:assert/strict";
import test from "node:test";

import { BACKGROUND_PRESETS } from "../src/lib/ui/backgroundPresets.ts";

function luminance(color) {
  const channels = color
    .slice(1)
    .match(/.{2}/g)
    .map((channel) => Number.parseInt(channel, 16) / 255)
    .map((channel) =>
      channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
    );
  return 0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2];
}

function contrast(left, right) {
  const values = [luminance(left), luminance(right)].sort((a, b) => b - a);
  return (values[0] + 0.05) / (values[1] + 0.05);
}

test("provides 24 distinct, high-contrast background presets", () => {
  assert.equal(BACKGROUND_PRESETS.length, 24);
  assert.equal(new Set(BACKGROUND_PRESETS.map(({ color }) => color)).size, 24);
  for (const preset of BACKGROUND_PRESETS) {
    assert.match(preset.color, /^#[0-9a-f]{6}$/);
    assert.ok(contrast(preset.color, "#e4e4e7") >= 10, preset.name);
  }
});
