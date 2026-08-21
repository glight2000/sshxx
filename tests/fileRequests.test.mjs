import assert from "node:assert/strict";
import { test } from "node:test";
import { TextEncoder } from "node:util";

import { FileRequestClient } from "../src/lib/fileRequests.ts";

const identityEncrypt = {
  segment: async (_stream, _offset, data) => data,
};

test("correlates and validates encrypted filesystem responses", async () => {
  const sent = [];
  const client = new FileRequestClient(
    identityEncrypt,
    () => true,
    (message) => sent.push(message),
  );
  const result = client.request(7, 2, { operation: "list", path: "/tmp" });
  await Promise.resolve();
  const [requestId, , responseStream] = sent[0].fileRequest.slice(2, 5);
  assert.equal(
    client.handleResponse(
      requestId,
      responseStream,
      new TextEncoder().encode(
        JSON.stringify({ ok: true, operation: "list", path: "/tmp" }),
      ),
    ),
    true,
  );
  assert.deepEqual(await result, {
    ok: true,
    operation: "list",
    path: "/tmp",
  });
});

test("rejects pending work on disconnect and ignores mismatched streams", async () => {
  const sent = [];
  const client = new FileRequestClient(
    identityEncrypt,
    () => true,
    (message) => sent.push(message),
  );
  const result = client.request(7, 2, { operation: "read", path: "/tmp/a" });
  await Promise.resolve();
  const [requestId] = sent[0].fileRequest.slice(2, 3);
  assert.equal(client.handleResponse(requestId, 1n, new Uint8Array()), false);
  client.rejectAll("Connection closed.");
  await assert.rejects(result, /Connection closed/);
});
