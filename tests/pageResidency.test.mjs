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
});
