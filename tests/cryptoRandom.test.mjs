import assert from "node:assert/strict";
import { test } from "node:test";

import { randomEncryptedStream, randomHex } from "../src/lib/fileRequests.ts";

test("creates fixed-width cryptographic identifiers", () => {
  assert.match(randomHex(16), /^[0-9a-f]{32}$/);
  assert.throws(() => randomHex(0), /positive integer/);
});

test("reserves the high bit for encrypted stream numbers", () => {
  const stream = randomEncryptedStream();
  assert.notEqual(stream, 0n);
  assert.equal(stream & 0x8000000000000000n, 0x8000000000000000n);
});
