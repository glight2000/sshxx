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
  assert.equal(input.classify(sample(100, 2300), "auto"), "mouse");
  assert.equal(input.classify(sample(90, 2700, 0, 4), "auto"), "trackpad");
  assert.equal(
    input.classify(sample(1, 2800), "mouse"),
    "mouse",
    "high-resolution mouse override",
  );
  assert.equal(input.classify(sample(3, 2900, 1), "trackpad"), "trackpad");
  assert.equal(
    input.classify(sample(120, 2901), "auto"),
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

test("a brief pause and a large integer delta cannot turn a trackpad pan into zoom", () => {
  const input = new WheelInputClassifier();
  const events = [
    sample(1, 0, 0, 12),
    sample(2, 60, 0, 8),
    sample(80, 300),
    sample(2, 320, 0, 3),
    sample(120, 900),
  ];
  assert.deepEqual(
    events.map((event) =>
      wheelDestination(false, input.classify(event, "auto")),
    ),
    ["pan", "pan", "pan", "pan", "pan"],
  );
});

test("later horizontal evidence corrects an ambiguous first sample", () => {
  const input = new WheelInputClassifier();
  assert.equal(input.classify(sample(80, 0), "auto"), "mouse");
  assert.equal(input.classify(sample(120, 3000), "auto"), "mouse");
  assert.equal(input.classify(sample(120, 3020, 0, 3), "auto"), "trackpad");
  assert.equal(input.classify(sample(240, 3040), "auto"), "trackpad");
});

test("trackpad memory expires and does not override line wheels, settings or a reset clock", () => {
  const input = new WheelInputClassifier();
  assert.equal(input.classify(sample(1, 0), "auto"), "trackpad");
  assert.equal(input.classify(sample(3, 400, 1), "auto"), "mouse");
  assert.equal(input.classify(sample(1, 800), "auto"), "trackpad");
  assert.equal(input.classify(sample(120, 2400), "auto"), "mouse");
  assert.equal(input.classify(sample(1, 3000), "auto"), "trackpad");
  assert.equal(input.classify(sample(120, 0), "auto"), "mouse");
  assert.equal(input.classify(sample(1, 50), "auto"), "trackpad");
  assert.equal(input.classify(sample(120, 60), "mouse"), "mouse");
  assert.equal(input.classify(sample(120, 70), "auto"), "mouse");
});

test("empty samples neither identify a device nor extend trackpad memory", () => {
  const input = new WheelInputClassifier();
  input.classify(sample(0, 0), "auto");
  assert.equal(input.classify(sample(120, 20), "auto"), "mouse");
  input.classify(sample(1, 400), "auto");
  input.classify(sample(0, 1600), "auto");
  assert.equal(input.classify(sample(120, 2000), "auto"), "mouse");
});

test("ambiguous new gestures cannot keep stale trackpad evidence alive indefinitely", () => {
  const input = new WheelInputClassifier();
  assert.equal(input.classify(sample(1, 0), "auto"), "trackpad");
  assert.equal(input.classify(sample(120, 500), "auto"), "trackpad");
  assert.equal(input.classify(sample(120, 1000), "auto"), "trackpad");
  assert.equal(input.classify(sample(120, 1501), "auto"), "mouse");
  assert.equal(input.classify(sample(1, 2000), "auto"), "trackpad");
  assert.equal(input.classify(sample(3, 2400, 1), "auto"), "mouse");
  assert.equal(input.classify(sample(120, 2800), "auto"), "mouse");
});
