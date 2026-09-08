import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const session = readFileSync("src/lib/Session.svelte", "utf8");

test("page switching keeps every canvas component mounted", () => {
  assert.match(session, /\{#each pages as page \(page\.id\)\}/);

  for (const collection of [
    "shells",
    "notes",
    "fileWindows",
    "customWindows",
  ]) {
    assert.match(
      session,
      new RegExp(
        `\\{#each ${collection}\\.filter\\(\\(\\[, \\w+\\]\\) => \\w+\\.pageId === page\\.id\\)`,
      ),
    );
    assert.doesNotMatch(
      session,
      new RegExp(
        `\\{#each ${collection}\\.filter\\(\\(\\[, \\w+\\]\\) => \\w+\\.pageId === activePageId\\)`,
      ),
    );
  }

  assert.match(
    session,
    /class:canvas-page-active=\{page\.id === activePageId\}/,
  );
  assert.doesNotMatch(session, /\{#key activePageId\}/);
  assert.match(session, /pageVisible=\{page\.id === activePageId\}/);
});

test("page crossfades use per-page cameras, including terminal scale and touch previews", () => {
  assert.match(session, /class="canvas-world canvas-page-layer/);
  assert.match(session, /bind:this=\{pageWorlds\[page.id\]\}/);
  assert.match(session, /bind:this=\{pageGrids\[page.id\]\}/);
  assert.match(
    session,
    /style:transform=\{canvasWorldTransform\(\s*view.center,\s*view.zoom,/,
  );
  assert.match(session, /canvasZoom=\{view.zoom\}/);
  assert.match(session, /const canvasWorld = pageWorlds\[activePageId\]/);
  assert.match(session, /const canvasGrid = pageGrids\[activePageId\]/);
  assert.doesNotMatch(
    session,
    /querySelector<HTMLElement>\("\.canvas-world"\)/,
  );
  assert.doesNotMatch(
    session,
    /transform: translate3d\(var\(--canvas-world-x\)/,
  );
  assert.match(session, /opacity 400ms ease/);
});
