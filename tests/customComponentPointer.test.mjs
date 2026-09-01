import assert from "node:assert/strict";
import test from "node:test";

import {
  CUSTOM_COMPONENT_POINTER_BRIDGE,
  CUSTOM_COMPONENT_POINTER_MESSAGE,
  CUSTOM_COMPONENT_SET_URL_MESSAGE,
  customComponentRequestedUrl,
  mapCustomComponentPointer,
} from "../src/lib/customComponentPointer.ts";

test("maps iframe pointer coordinates into shared component coordinates", () => {
  assert.deepEqual(
    mapCustomComponentPointer(
      {
        type: CUSTOM_COMPONENT_POINTER_MESSAGE,
        event: "click",
        x: 360,
        y: 242,
      },
      720,
      484,
      720,
      520,
    ),
    { x: 360, y: 278, clicked: true },
  );
});

test("bounds pointer messages and rejects malformed iframe input", () => {
  assert.deepEqual(
    mapCustomComponentPointer(
      {
        type: CUSTOM_COMPONENT_POINTER_MESSAGE,
        event: "move",
        x: -20,
        y: 900,
      },
      720,
      484,
      720,
      520,
    ),
    { x: 0, y: 520, clicked: false },
  );
  assert.equal(mapCustomComponentPointer({}, 1, 1, 1, 40), null);
  assert.match(CUSTOM_COMPONENT_POINTER_BRIDGE, /pointermove/);
  assert.match(CUSTOM_COMPONENT_POINTER_BRIDGE, /click/);
});

test("exposes an explicit shared URL request without inferring iframe navigation", () => {
  assert.equal(
    customComponentRequestedUrl({
      type: CUSTOM_COMPONENT_SET_URL_MESSAGE,
      url: "https://example.test/next",
    }),
    "https://example.test/next",
  );
  assert.equal(
    customComponentRequestedUrl({
      type: CUSTOM_COMPONENT_SET_URL_MESSAGE,
      url: 42,
    }),
    null,
  );
  assert.match(CUSTOM_COMPONENT_POINTER_BRIDGE, /setUrl/);
});
