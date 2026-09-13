import assert from "node:assert/strict";
import { test } from "node:test";
import {
  HISTORY_SCROLL_INTERVAL_MS,
  terminalHistoryScroll,
} from "../src/lib/terminalCheckpoint/historyScroll.ts";

const settle = async () => {
  await Promise.resolve();
  await Promise.resolve();
};

function harness(t, load = async () => {}) {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  let now = 0;
  const starts = [];
  const scroll = terminalHistoryScroll(
    async () => {
      starts.push(now);
      await load();
    },
    () => now,
  );
  t.after(() => scroll.dispose());
  return {
    scroll,
    starts,
    async advance(ms) {
      now += ms;
      t.mock.timers.tick(ms);
      await settle();
    },
  };
}

test("only a not-at-top to at-top transition loads, immediately and once", async (t) => {
  const { scroll, starts, advance } = harness(t);
  scroll.update(0);
  scroll.update(0);
  assert.deepEqual(starts, [], "initial top is not a request for all history");
  scroll.update(8);
  scroll.update(1);
  assert.deepEqual(starts, [], "near the top is not the top");
  scroll.update(0);
  assert.deepEqual(starts, [0], "no second wheel or key event is necessary");
  await settle();
  for (let i = 0; i < 50; i++) scroll.update(0);
  await advance(1000);
  assert.deepEqual(starts, [0], "remaining at the top does not keep loading");
});

test("scrollbar drag top edges coalesce and start at least 300 ms apart", async (t) => {
  assert.equal(HISTORY_SCROLL_INTERVAL_MS, 300);
  const { scroll, starts, advance } = harness(t);
  scroll.reset(12);
  scroll.update(0);
  await settle();
  scroll.update(12); // Prepending history moves the viewport away from the top.
  scroll.update(0); // The dragged thumb moves it back to the top.
  for (let i = 0; i < 50; i++) {
    scroll.update(1);
    scroll.update(0);
  }
  await advance(299);
  assert.deepEqual(starts, [0]);
  await advance(1);
  assert.deepEqual(starts, [0, 300]);
  scroll.update(12);
  scroll.update(0);
  await advance(300);
  assert.deepEqual(starts, [0, 300, 600]);
});

test("a slow history request never overlaps the next top-edge request", async (t) => {
  let resolve;
  const { scroll, starts, advance } = harness(
    t,
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  scroll.reset(10);
  scroll.update(0);
  scroll.update(10);
  scroll.update(0);
  await advance(1000);
  assert.deepEqual(starts, [0]);
  resolve();
  await settle();
  assert.deepEqual(starts, [0, 1000]);
  resolve();
  await settle();
  await advance(1000);
  assert.equal(
    starts.length,
    2,
    "a slow request does not accumulate a backlog",
  );
});

test("scrolling away cancels a throttled top edge", async (t) => {
  const { scroll, starts, advance } = harness(t);
  scroll.reset(10);
  scroll.update(0);
  await settle();
  scroll.update(10);
  scroll.update(0);
  await advance(100);
  scroll.update(1);
  await advance(400);
  assert.deepEqual(starts, [0]);
  scroll.update(0);
  assert.deepEqual(starts, [0, 500], "a later genuine top edge still loads");
});

test("reset cancels old timers and ignores old in-flight completions", async (t) => {
  const pending = [];
  const { scroll, starts, advance } = harness(
    t,
    () =>
      new Promise((done) => {
        pending.push(done);
      }),
  );
  scroll.reset(10);
  scroll.update(0);
  pending.shift()();
  await settle();
  scroll.update(10);
  scroll.update(0);
  scroll.reset(10);
  await advance(400);
  assert.deepEqual(starts, [0]);
  scroll.update(0);
  const old = pending.shift();
  scroll.reset(10);
  scroll.update(0);
  scroll.update(10);
  scroll.update(0);
  old();
  await advance(400);
  assert.deepEqual(
    starts,
    [0, 400, 400],
    "old completion cannot unlock a new load",
  );
  pending.shift()();
  await settle();
  assert.deepEqual(starts, [0, 400, 400, 800]);
  pending.shift()();
});

test("dispose cancels timers, pending edges and future updates", async (t) => {
  const { scroll, starts, advance } = harness(t);
  scroll.reset(10);
  scroll.update(0);
  await settle();
  scroll.update(10);
  scroll.update(0);
  scroll.dispose();
  await advance(1000);
  scroll.reset(10);
  scroll.update(0);
  assert.deepEqual(starts, [0]);
});

test("a failed request cannot cause a retry loop or an unhandled rejection", async (t) => {
  const { scroll, starts, advance } = harness(t, async () => {
    throw new Error("history no longer retained");
  });
  scroll.reset(10);
  scroll.update(0);
  scroll.update(1);
  scroll.update(0);
  await settle();
  await advance(1000);
  assert.deepEqual(starts, [0]);
});
