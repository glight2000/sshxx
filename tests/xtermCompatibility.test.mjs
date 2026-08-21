import assert from "node:assert/strict";
import { test } from "node:test";

import {
  currentTypeAheadAttributes,
  supportsTypeAheadAttributes,
} from "../src/lib/xtermCompatibility.ts";

test("detects and reads the isolated xterm TypeAhead capability", () => {
  const attributes = { marker: "attributes" };
  const terminal = {
    _core: { _inputHandler: { getAttrData: () => attributes } },
  };
  assert.equal(supportsTypeAheadAttributes(terminal), true);
  assert.equal(currentTypeAheadAttributes(terminal), attributes);
});

test("rejects incompatible xterm internals without dereferencing them", () => {
  const terminal = {};
  assert.equal(supportsTypeAheadAttributes(terminal), false);
  assert.throws(() => currentTypeAheadAttributes(terminal), /does not expose/);
});
