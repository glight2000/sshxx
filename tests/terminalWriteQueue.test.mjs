import assert from "node:assert/strict";
import test from "node:test";

import {
  splitTerminalWrite,
  TerminalWriteQueue,
} from "../src/lib/terminalWriteQueue.ts";

test("splits large terminal writes without breaking surrogate pairs", () => {
  assert.deepEqual(splitTerminalWrite("ab😀cd", 3), ["ab", "😀c", "d"]);
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
