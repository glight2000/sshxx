import assert from "node:assert/strict";
import test from "node:test";
import {
  WheelInputClassifier,
  isWheelInputMode,
  wheelDestination,
} from "../src/lib/wheelInput.ts";

const sample = (deltaY, timeStamp, deltaMode = 0, deltaX = 0) => ({
  deltaX,
  deltaY,
  timeStamp,
  deltaMode,
});

test("wheel device detection is a bounded gesture guess, with explicit overrides", () => {
  const input = new WheelInputClassifier();
  assert.equal(input.classify(sample(3, 0, 1), "auto"), "mouse");
  assert.equal(input.classify(sample(120, 250), "auto"), "mouse");
  assert.equal(input.classify(sample(2.5, 500), "auto"), "trackpad");
  assert.equal(
    input.classify(sample(120, 520), "auto"),
    "trackpad",
    "acceleration must not change a gesture into zoom",
  );
  assert.equal(
    input.classify(sample(1, 670), "auto"),
    "trackpad",
    "momentum tail belongs to the same gesture",
  );
  assert.equal(input.classify(sample(100, 900), "auto"), "mouse");
  assert.equal(input.classify(sample(90, 1200, 0, 4), "auto"), "trackpad");
  assert.equal(
    input.classify(sample(1, 1300), "mouse"),
    "mouse",
    "high-resolution mouse override",
  );
  assert.equal(input.classify(sample(3, 1400, 1), "trackpad"), "trackpad");
  assert.equal(
    input.classify(sample(120, 1401), "auto"),
    "mouse",
    "returning to auto starts a fresh gesture",
  );
  for (const value of [undefined, null, "", "touch", 1])
    assert.equal(isWheelInputMode(value), false);
  for (const value of ["auto", "mouse", "trackpad"])
    assert.equal(isWheelInputMode(value), true);
});

test("wheels follow focus; trackpads follow the gesture's starting position", () => {
  assert.equal(wheelDestination(true, "mouse"), "component");
  assert.equal(wheelDestination(true, "trackpad"), "component");
  assert.equal(wheelDestination(false, "mouse"), "zoom");
  assert.equal(wheelDestination(false, "trackpad"), "pan");
  assert.equal(wheelDestination(true, "trackpad", false), "pan");
  assert.equal(wheelDestination(true, "mouse", false), "component");
});
