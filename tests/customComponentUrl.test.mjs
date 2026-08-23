import assert from "node:assert/strict";
import test from "node:test";

import { resolveCustomComponentUrl } from "../src/lib/customComponentUrl.ts";

test("accepts absolute HTTP URLs for a custom component", () => {
  assert.deepEqual(
    resolveCustomComponentUrl(
      "https://status.example.test/dashboard?q=1",
      "https://sshxx.example.test/s/dev",
    ),
    { url: "https://status.example.test/dashboard?q=1", error: "" },
  );
});

test("rejects recursive, unsupported, and incomplete custom component URLs", () => {
  const viewer = "https://sshxx.example.test/s/dev";
  assert.match(
    resolveCustomComponentUrl("https://sshxx.example.test/", viewer).error,
    /recursively/,
  );
  assert.match(
    resolveCustomComponentUrl("javascript:alert(1)", viewer).error,
    /HTTP and HTTPS/,
  );
  assert.match(resolveCustomComponentUrl("/relative", viewer).error, /valid/);
});
