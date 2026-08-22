import assert from "node:assert/strict";
import { test } from "node:test";

import { shouldReloadAfterPreloadError } from "../src/lib/preloadRecovery.ts";

function memoryStorage() {
  const values = new Map();
  return {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value),
  };
}

test("reloads once for a stale deployment chunk and prevents a tight loop", () => {
  const storage = memoryStorage();
  assert.equal(
    shouldReloadAfterPreloadError(storage, "https://sshxx.test/s/dev", 1_000),
    true,
  );
  assert.equal(
    shouldReloadAfterPreloadError(storage, "https://sshxx.test/s/dev", 2_000),
    false,
  );
  assert.equal(
    shouldReloadAfterPreloadError(storage, "https://sshxx.test/s/dev", 32_000),
    true,
  );
});

test("allows recovery independently for another session URL", () => {
  const storage = memoryStorage();
  assert.equal(shouldReloadAfterPreloadError(storage, "/s/a", 1_000), true);
  assert.equal(shouldReloadAfterPreloadError(storage, "/s/b", 2_000), true);
});
