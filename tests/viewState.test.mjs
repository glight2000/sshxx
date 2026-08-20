import assert from "node:assert/strict";
import { test } from "node:test";

import {
  LOCAL_VIEW_STATE_VERSION,
  localViewStateKey,
  parseLocalViewState,
} from "../src/lib/viewState.ts";

test("local view state is isolated by server and session", () => {
  const first = localViewStateKey("dev", "http://localhost:5173", null);
  const second = localViewStateKey(
    "dev",
    "tauri://localhost",
    "https://sshxx.example.test",
  );
  assert.notEqual(first, second);
  assert.match(first, /dev$/);
});

test("local view state validates and preserves page views", () => {
  const state = parseLocalViewState(
    JSON.stringify({
      version: LOCAL_VIEW_STATE_VERSION,
      activePageId: 2,
      pages: {
        1: { center: [12, -24], zoom: 0.75 },
        2: { center: [200, 100], zoom: 1.5 },
      },
    }),
  );
  assert.deepEqual(state, {
    version: LOCAL_VIEW_STATE_VERSION,
    activePageId: 2,
    pages: {
      1: { center: [12, -24], zoom: 0.75 },
      2: { center: [200, 100], zoom: 1.5 },
    },
  });
});

test("local view state rejects incompatible data and skips invalid pages", () => {
  assert.equal(parseLocalViewState("not json"), null);
  assert.equal(
    parseLocalViewState(
      JSON.stringify({ version: 99, activePageId: 1, pages: {} }),
    ),
    null,
  );
  assert.deepEqual(
    parseLocalViewState(
      JSON.stringify({
        version: LOCAL_VIEW_STATE_VERSION,
        activePageId: 1,
        pages: {
          1: { center: [0, 0], zoom: 1 },
          2: { center: [Number.MAX_VALUE, 0], zoom: 1 },
          3: { center: [0, 0], zoom: 10 },
        },
      }),
    )?.pages,
    { 1: { center: [0, 0], zoom: 1 } },
  );
});
