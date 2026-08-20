import assert from "node:assert/strict";
import { test } from "node:test";

import themes, { defaultTheme, isThemeName } from "../src/lib/ui/themes.ts";

const ansiColors = [
  "foreground",
  "background",
  "black",
  "red",
  "green",
  "yellow",
  "blue",
  "magenta",
  "cyan",
  "white",
  "brightBlack",
  "brightRed",
  "brightGreen",
  "brightYellow",
  "brightBlue",
  "brightMagenta",
  "brightCyan",
  "brightWhite",
];

test("ships complete and valid terminal palettes", () => {
  assert.equal(Object.keys(themes).length, 13);
  assert.equal(isThemeName(defaultTheme), true);

  for (const [name, theme] of Object.entries(themes)) {
    assert.equal(isThemeName(name), true);
    for (const color of ansiColors) {
      assert.match(theme[color], /^#[0-9a-f]{6}$/i, `${name}.${color}`);
    }
  }
});
