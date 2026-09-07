import assert from "node:assert/strict";
import test from "node:test";
import { mobileRelationPress } from "../src/lib/action/mobileRelationPress.ts";

test("phone association tap, hold, scrolling, cancellation and teardown stay distinct", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const original = globalThis.window;
  globalThis.window = new EventTarget();
  const button = new EventTarget();
  let taps = 0,
    holds = 0;
  const action = mobileRelationPress(button, {
    tap: () => taps++,
    hold: () => holds++,
  });
  const send = (type, x = 0, detail = 1) => {
    const event = new Event(type, { cancelable: true });
    Object.assign(event, {
      pointerId: 1,
      pointerType: "touch",
      clientX: x,
      clientY: 0,
      detail,
    });
    (type === "pointerdown" || type === "click" || type === "contextmenu"
      ? button
      : window
    ).dispatchEvent(event);
  };
  try {
    send("pointerdown");
    send("pointerup");
    send("click");
    assert.equal(taps, 1);
    send("pointerdown");
    t.mock.timers.tick(500);
    send("pointerup");
    send("click");
    assert.equal(holds, 1);
    assert.equal(taps, 1);
    send("pointerdown");
    send("pointermove", 20);
    t.mock.timers.tick(500);
    send("pointerup");
    send("click");
    assert.equal(holds, 1);
    assert.equal(taps, 1);
    send("pointerdown");
    send("pointercancel");
    t.mock.timers.tick(500);
    send("click");
    assert.equal(holds, 1);
    assert.equal(taps, 1);
    send("click", 0, 0);
    assert.equal(taps, 2, "keyboard activation remains available");
    send("pointerdown");
    send("contextmenu");
    t.mock.timers.tick(500);
    send("pointerup");
    send("click");
    assert.equal(holds, 2);
    assert.equal(taps, 2);
    send("pointerdown");
    action.destroy();
    t.mock.timers.tick(500);
    send("click");
    assert.equal(holds, 2);
    assert.equal(taps, 2);
  } finally {
    action.destroy();
    globalThis.window = original;
  }
});
