import assert from "node:assert/strict";
import { test } from "node:test";
import { TextEncoder } from "node:util";

import { FileRequestClient } from "../src/lib/fileRequests.ts";
import {
  WorkspaceMedia,
  mergeChatHistory,
  attachmentMetadata,
} from "../src/lib/workspaceMedia.ts";

const identityEncrypt = {
  segment: async (_stream, _offset, data) => data,
};

test("workspace media uses bounded chunks and releases preview reservations", async () => {
  const parts = [];
  const media = new WorkspaceMedia({
    requestAttachment: async (request) => {
      if (request.operation === "write") {
        const part = Buffer.from(request.content, "base64");
        parts.push(part);
        return { ok: true, size: request.offset + part.length };
      }
      const content = Buffer.concat(parts);
      return {
        ok: true,
        size: content.length,
        content: content
          .subarray(request.offset, request.offset + 65536)
          .toString("base64"),
      };
    },
  });
  const data = new Uint8Array(100000).fill(17);
  const signal = new AbortController().signal;
  const file = new File([data], "example.png", { type: "image/png" });
  const item = await media.upload(file, signal, () => {});
  assert.deepEqual(
    parts.map((part) => part.length),
    [65536, 34464],
  );
  const previews = [];
  for (let i = 0; i < 4; i++) previews.push(await media.open(item, signal));
  await assert.rejects(media.open(item, signal), /Close another/);
  previews[0].release();
  previews[0].release();
  previews.push(await media.open(item, signal));
  media.dispose();
  assert.throws(
    () =>
      attachmentMetadata(new File([data], "voice.ogg", { type: "audio/ogg" })),
    /Audio/,
  );
  assert.equal(
    attachmentMetadata(
      new File(["<script>"], "unsafe.html", { type: "text/html" }),
    ).mediaType,
    "application/octet-stream",
  );
  assert.throws(
    () => attachmentMetadata(new File([], "empty.txt")),
    /non-empty/,
  );
});

test("chat history is capped at 500 and replayed message IDs do not duplicate", () => {
  const records = Array.from({ length: 600 }, (_, i) => ({
    id: String(i),
    name: "Tester",
    text: String(i),
    sentAt: i,
    attachments: [],
  }));
  const history = mergeChatHistory(records, [records[599]]);
  assert.equal(history.length, 500);
  assert.equal(history[0].id, "100");
  assert.equal(history.at(-1).id, "599");
  const [record] = mergeChatHistory(
    [],
    [{ ...records[0], sentAt: 1780000000000n }],
  );
  assert.equal(typeof record.sentAt, "number");
  assert.equal(new Date(record.sentAt).getTime(), 1780000000000);
});

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
