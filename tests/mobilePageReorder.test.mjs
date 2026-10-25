import assert from "node:assert/strict";
import test from "node:test";
import { mobilePageReorder } from "../src/lib/action/mobilePageReorder.ts";

test("touch sorting requires a hold; tap, scroll, multitouch and cancellation remain distinct", (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const original = globalThis.window;
  globalThis.window = new EventTarget();
  const node = new EventTarget(),
    events = [];
  const action = mobilePageReorder(node, {
    start: () => {
      events.push("start");
      return true;
    },
    move: ({ clientX }) => events.push(clientX),
    end: (commit) => events.push(commit),
  });
  t.after(() => {
    action.destroy();
    globalThis.window = original;
  });
  const send = (type, x = 0, count = 1, outside = false) => {
    const event = new Event(type, { cancelable: true });
    const touches = Array.from({ length: count }, (_, identifier) => ({
      identifier,
      clientX: x,
      clientY: 0,
    }));
    Object.assign(event, { touches, changedTouches: touches });
    (!outside && (type === "touchstart" || type === "contextmenu")
      ? node
      : window
    ).dispatchEvent(event);
    return event;
  };
  send("touchstart");
  send("touchend");
  t.mock.timers.tick(500);
  assert.deepEqual(events, []);
  send("touchstart");
  assert.equal(send("touchmove", 20).defaultPrevented, false);
  t.mock.timers.tick(500);
  send("touchend", 20);
  assert.deepEqual(events, []);
  send("touchstart");
  t.mock.timers.tick(450);
  assert.equal(send("contextmenu").defaultPrevented, true);
  assert.equal(send("touchmove", 50).defaultPrevented, true);
  send("touchend", 70);
  assert.deepEqual(events, ["start", 50, 70, true]);
  events.length = 0;
  send("touchstart");
  t.mock.timers.tick(450);
  send("touchmove", 30, 2);
  assert.deepEqual(events, ["start", false]);
  events.length = 0;
  send("touchstart");
  send("touchstart", 100, 2, true);
  t.mock.timers.tick(500);
  assert.deepEqual(
    events,
    [],
    "an outside second finger cancels the pending hold",
  );
  send("touchstart");
  t.mock.timers.tick(450);
  send("touchstart", 100, 2, true);
  send("touchend");
  assert.deepEqual(
    events,
    ["start", false],
    "an outside second finger cancels an active drag without committing",
  );
  events.length = 0;
  send("touchstart");
  send("touchcancel");
  t.mock.timers.tick(500);
  assert.deepEqual(events, []);
  send("touchstart");
  action.destroy();
  t.mock.timers.tick(500);
  assert.deepEqual(events, []);
});
