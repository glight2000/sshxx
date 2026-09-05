import assert from "node:assert/strict";
import test from "node:test";

import {
  splitTerminalWrite,
  TerminalWriteQueue,
} from "../src/lib/terminalWriteQueue.ts";

test("splits large terminal writes without breaking surrogate pairs", () => {
  assert.deepEqual(splitTerminalWrite("ab😀cd", 3), ["ab", "😀c", "d"]);
});

test("a renderer that never mounts cannot hold the subscription lock forever", async () => {
  let expire;
  let failure;
  const queue = new TerminalWriteQueue({
    scheduleTimeout(callback) {
      expire = callback;
      return 1;
    },
    cancelTimeout() {},
    onWriteTimeout(error) {
      failure = error;
    },
  });
  const completion = queue.write("pending replay", true);
  expire();
  await completion;
  assert.match(failure.message, /Terminal renderer did not complete/);
  queue.dispose();
});

test("default write scheduling progresses without animation frames", async () => {
  const previous = globalThis.window;
  globalThis.window = {
    setTimeout,
    clearTimeout,
    requestAnimationFrame() {
      throw new Error("background tab cannot schedule rendering frames");
    },
  };
  const writes = [];
  const queue = new TerminalWriteQueue({ chunkCharacters: 2 });
  try {
    queue.setSink((data, complete) => {
      writes.push(data);
      complete();
    });
    await queue.write("abcdef", true);
    assert.deepEqual(writes, ["ab", "cd", "ef"]);
  } finally {
    queue.dispose();
    if (previous === undefined) delete globalThis.window;
    else globalThis.window = previous;
  }
});

test("waits for each bounded renderer write before scheduling the next", async () => {
  const scheduled = [];
  const writes = [];
  const states = [];
  const queue = new TerminalWriteQueue({
    chunkCharacters: 4,
    schedule(callback) {
      scheduled.push(callback);
      return scheduled.length;
    },
    cancel() {},
    scheduleTimeout() {
      return 1;
    },
    cancelTimeout() {},
    onStateChange(state) {
      states.push(state);
    },
  });
  queue.setSink((data, complete) => writes.push({ data, complete }));

  let finished = false;
  const completion = queue.write("abcdefghij").then(() => (finished = true));
  assert.deepEqual(
    writes.map(({ data }) => data),
    ["abcd"],
  );
  assert.equal(states.at(-1).queuedCharacters, 10);

  writes[0].complete();
  assert.equal(finished, false);
  assert.equal(scheduled.length, 1);
  scheduled.shift()();
  assert.deepEqual(
    writes.map(({ data }) => data),
    ["abcd", "efgh"],
  );

  writes[1].complete();
  scheduled.shift()();
  writes[2].complete();
  await completion;
  assert.equal(finished, true);
  assert.deepEqual(states.at(-1), {
    queuedCharacters: 0,
    queuedChunks: 0,
  });
});

test("keeps replay suppression active for a complete logical write", async () => {
  const scheduled = [];
  const writes = [];
  const events = [];
  const queue = new TerminalWriteQueue({
    chunkCharacters: 3,
    schedule(callback) {
      scheduled.push(callback);
      return scheduled.length;
    },
    cancel() {},
    scheduleTimeout() {
      return 1;
    },
    cancelTimeout() {},
    onReplayStart: () => events.push("start"),
    onReplayEnd: () => events.push("end"),
  });
  queue.setSink((data, complete) => writes.push({ data, complete }));

  const completion = queue.write("abcdef", true);
  assert.deepEqual(events, ["start"]);
  writes[0].complete();
  assert.deepEqual(events, ["start"]);
  scheduled.shift()();
  writes[1].complete();
  await completion;
  assert.deepEqual(events, ["start", "end"]);
});

test("times out a stalled renderer write and releases replay suppression", async () => {
  const writes = [];
  const events = [];
  const errors = [];
  let timeoutCallback;
  let clearedTimeout = null;
  const queue = new TerminalWriteQueue({
    chunkCharacters: 4,
    writeTimeoutMs: 25,
    scheduleTimeout(callback, timeoutMs) {
      assert.equal(timeoutMs, 25);
      timeoutCallback = callback;
      return 7;
    },
    cancelTimeout(handle) {
      clearedTimeout = handle;
    },
    onReplayStart: () => events.push("start"),
    onReplayEnd: () => events.push("end"),
    onWriteTimeout: (error) => errors.push(error),
  });
  queue.setSink((data, complete) => writes.push({ data, complete }));

  const completion = queue.write("abcdefgh", true);
  assert.deepEqual(events, ["start"]);
  assert.equal(writes.length, 1);
  timeoutCallback();
  await completion;

  assert.deepEqual(events, ["start", "end"]);
  assert.equal(errors.length, 1);
  assert.equal(errors[0].name, "TerminalWriteTimeoutError");
  assert.equal(errors[0].chunkCharacters, 4);
  assert.equal(clearedTimeout, null);

  // The failed sink remains quarantined until its owning terminal component
  // is remounted. A late xterm callback must not restart the old queue.
  writes[0].complete();
  await queue.write("ignored after failure");
  assert.equal(writes.length, 1);
});
