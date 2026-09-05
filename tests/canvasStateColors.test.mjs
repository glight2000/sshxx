import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const css = readFileSync(new URL("../src/app.css", import.meta.url), "utf8");

test("light mode reserves a visible real border for all four component types", () => {
  assert.match(
    css,
    /:root\[data-color-mode="light"\]\s+:is\(\.term-container, \.note-container, \.file-window, \.custom-window-border\)\s*\{\s*border-width: 2px;\s*\}/,
  );
});

const channels = (hex) => hex.match(/[a-f\d]{2}/gi).map((v) => parseInt(v, 16));
function contrast(foreground, background, alpha = 1) {
  const bg = channels(background);
  const fg = channels(foreground).map(
    (v, i) => v * alpha + bg[i] * (1 - alpha),
  );
  const luminance = (rgb) =>
    rgb
      .map((v) => {
        const s = v / 255;
        return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
      })
      .reduce((sum, v, i) => sum + v * [0.2126, 0.7152, 0.0722][i], 0);
  const values = [luminance(fg), luminance(bg)].sort((a, b) => b - a);
  return (values[0] + 0.05) / (values[1] + 0.05);
}

test("light canvas borders remain distinguishable at default 80% window opacity", () => {
  const block = css.match(/:root\[data-color-mode="light"\]\s*\{([^}]+)\}/)[1];
  const colors = Object.fromEntries(
    [...block.matchAll(/--([\w-]+):\s*(#[a-f\d]{6})/gi)].map((m) => [
      m[1],
      m[2],
    ]),
  );
  for (const key of ["canvas-focus", "canvas-note-focus", "canvas-selected"]) {
    assert.ok(contrast(colors[key], "#eceef2", 0.8) >= 3, key);
  }
});

test("all modes and components share a solid-core glow and one breathing animation", () => {
  assert.match(css, /0 0 3px 1px var\(--canvas-state-color\)/);
  assert.equal([...css.matchAll(/@keyframes canvas-state-pulse/g)].length, 1);
  assert.match(css, /0 0 1px var\(--canvas-state-color\)/);
  assert.match(css, /0 0 4px 1px var\(--canvas-state-color\)/);
  for (const file of ["ui/XTerm", "ui/Note", "ui/FileExplorer", "Session"]) {
    const source = readFileSync(
      new URL(`../src/lib/${file}.svelte`, import.meta.url),
      "utf8",
    );
    assert.match(source, /animation: canvas-state-pulse /, file);
    assert.match(source, /box-shadow: var\(--canvas-state-glow\)/, file);
    assert.doesNotMatch(source, /@keyframes canvas-.*pulse/, file);
  }
});
