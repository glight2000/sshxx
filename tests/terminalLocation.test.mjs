import assert from "node:assert/strict";
import { test } from "node:test";

import { parseOsc7Location } from "../src/lib/terminalLocation.ts";

test("parses an OSC 7 host and decoded working directory", () => {
  assert.deepEqual(
    parseOsc7Location("file://build.example.test/work/release%20candidate"),
    {
      workingDirectory: "/work/release candidate",
      workingDirectoryHost: "build.example.test",
    },
  );
});

test("rejects malformed and non-file OSC locations", () => {
  assert.equal(parseOsc7Location("https://example.test/work"), null);
  assert.equal(parseOsc7Location("not a URI"), null);
  assert.equal(parseOsc7Location("file://example.test/%zz"), null);
});
