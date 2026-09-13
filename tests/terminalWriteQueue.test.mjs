import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import ts from "typescript";
import { terminalBatchIsContinuous } from "../src/lib/terminalSubscription.ts";
import xterm from "@xterm/xterm";
import replayCases from "./fixtures/terminal-replay.json" with { type: "json" };

import {
  splitTerminalWrite,
  TerminalWriteQueue,
} from "../src/lib/terminalWriteQueue.ts";

function timers(options = {}) {
  return {
    schedule: (callback) => setTimeout(callback, 0),
    cancel: clearTimeout,
    scheduleTimeout: setTimeout,
    cancelTimeout: clearTimeout,
    ...options,
  };
}

test("the actual component-to-session write path only acknowledges successful parsing", async () => {
  const xtermSource = readFileSync(
    new URL("../src/lib/ui/XTerm.svelte", import.meta.url),
    "utf8",
  );
  const sessionSource = readFileSync(
    new URL("../src/lib/Session.svelte", import.meta.url),
    "utf8",
  );
  const writeSource = xtermSource.slice(
    xtermSource.indexOf("  write = async"),
    xtermSource.indexOf("  restoreState = async"),
  );
  const handlers = sessionSource.slice(
    sessionSource.indexOf("  async function writeTerminalData("),
    sessionSource.indexOf("  function subscribeTerminal("),
  );
  const compile = (source) =>
    ts.transpile(source, { target: ts.ScriptTarget.ES2022 });
  for (const mode of ["written", "failed", "disposed", "gap"]) {
    const queue = new TerminalWriteQueue(timers());
    let complete;
    queue.setSink((_data, callback) => {
      complete = callback;
      if (mode === "failed") throw new Error("synthetic sink failure");
      if (mode === "written") callback();
      if (mode === "disposed") queue.dispose();
    });
    const writer = new Function(
      "writeQueue",
      `let write, pasteMode; const historyWindow = null, term = null, destroyed = false; ${compile(writeSource)}; return write;`,
    )(queue);
    const sent = [];
    const recovered = [];
    let work;
    const context = {
      appendTerminalHistory() {},
      chunknums: { 7: mode === "gap" ? 1 : 0 },
      writers: { 7: writer },
      replayedWriters: { 7: writer },
      readTerminalHistory: () => "",
      terminalHistory: { pasteMode: () => undefined },
      shells: [[7, { pageId: 1, generation: 0 }]],
      locks: {
        7: (fn) => {
          work = fn();
        },
      },
      tick: async () => {},
      terminalSubscriptionTokens: { 7: 3 },
      terminalOutputEpochs: new Map(),
      terminalBatchIsContinuous,
      recoverTerminalRenderer: (...args) => recovered.push(args),
      encrypt: {
        outputSegment: async (_epoch, _stream, _offset, data) => data,
      },
      srocket: { send: (message) => sent.push(message) },
      terminalRenderFlowControl: true,
    };
    const handle = new Function(
      ...Object.keys(context),
      `${compile(handlers)}; return handleTerminalChunks;`,
    )(...Object.values(context));
    handle(
      7,
      1,
      0,
      false,
      0,
      [new TextEncoder().encode("synthetic output")],
      3,
      mode === "gap" ? 90 : 0,
    );
    await work;
    complete?.(); // Late callbacks must not revive an ACK after failure/disposal.
    await Promise.resolve();
    assert.equal(sent.length, mode === "written" ? 1 : 0, mode);
    assert.equal(recovered.length, mode === "gap" ? 1 : 0, mode);
    if (mode === "written")
      assert.deepEqual(sent[0], { renderedBatch: [7, 0, 3, 1] });
    queue.dispose();
  }
});

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

test("diagnostics distinguish a pending write, recovery, and cleanup without content", async () => {
  let complete;
  const queue = new TerminalWriteQueue({
    scheduleTimeout: () => 1,
    cancelTimeout() {},
  });
  queue.setSink((_, done) => {
    complete = done;
  });
  const pending = queue.write("synthetic private text", true);
  assert.equal(queue.diagnostics.queuedChunks, 1);
  assert.equal(typeof queue.diagnostics.pendingSince, "number");
  assert.equal(queue.diagnostics.lastCompletedAt, null);
  assert.equal(JSON.stringify(queue.diagnostics).includes("synthetic"), false);
  complete();
  await pending;
  assert.equal(queue.diagnostics.pendingSince, null);
  assert.equal(typeof queue.diagnostics.lastCompletedAt, "number");
  const next = queue.write("pending at unmount", true);
  queue.dispose();
  const disposed = queue.diagnostics;
  complete(); // a disposed renderer must not revive counters/timestamps
  await next;
  assert.deepEqual(queue.diagnostics, disposed);
  assert.equal(disposed.queuedChunks, 0);
});

for (const phase of ["transform", "sink"]) {
  test(`${phase} exceptions fail the write instead of reporting successful processing`, async () => {
    const events = [];
    const failures = [];
    const fail = () => {
      throw new Error("synthetic private output");
    };
    const queue = new TerminalWriteQueue(
      timers({
        transform: phase === "transform" ? fail : undefined,
        onReplayStart: () => events.push("start"),
        onReplayEnd: () => events.push("end"),
        onWriteFailure: (error) => failures.push(error),
      }),
    );
    try {
      queue.setSink(phase === "sink" ? fail : (_, complete) => complete());
      assert.equal(await queue.write("private", true), "failed");
      assert.deepEqual(events, ["start", "end"]);
      assert.equal(failures.length, 1);
      assert.equal(failures[0].reason, phase);
      assert.equal(failures[0].message.includes("private"), false);
      assert.equal(queue.diagnostics.queuedCharacters, 0);
      assert.equal(await queue.write("later"), "failed");
    } finally {
      queue.dispose();
    }
  });
}

test("pending character and chunk budgets release all waiters without retrying input", async () => {
  for (const limits of [{ maxQueuedCharacters: 5 }, { maxQueuedChunks: 1 }]) {
    let lateCallback;
    const failures = [];
    const queue = new TerminalWriteQueue(
      timers({
        ...limits,
        onWriteFailure: (error) => failures.push(error),
      }),
    );
    try {
      queue.setSink((_, complete) => {
        lateCallback = complete;
      });
      const first = queue.write("12345", true);
      assert.equal(await queue.write("6", true), "failed");
      assert.equal(await first, "failed");
      assert.equal(failures[0].reason, "capacity");
      const failed = queue.diagnostics;
      lateCallback();
      assert.deepEqual(queue.diagnostics, failed);
      assert.equal(failed.queuedChunks, 0);
    } finally {
      queue.dispose();
    }
  }
  // Splitting itself is bounded, even with tiny renderer chunks.
  const queue = new TerminalWriteQueue(
    timers({ chunkCharacters: 1, maxQueuedChunks: 2 }),
  );
  assert.equal(await queue.write("abc"), "failed");
  queue.dispose();
});

test("teardown cancels a scheduled chunk and reports disposal, not completion", async () => {
  let scheduled;
  const writes = [];
  const queue = new TerminalWriteQueue(
    timers({
      chunkCharacters: 1,
      schedule(callback) {
        scheduled = callback;
        return 1;
      },
      cancel() {},
    }),
  );
  queue.setSink((data, complete) => {
    writes.push(data);
    complete();
  });
  const pending = queue.write("ab", true);
  queue.dispose();
  scheduled();
  assert.equal(await pending, "disposed");
  assert.deepEqual(writes, ["a"]);
});

test("a suspended viewer does not stall a sibling; late callbacks cannot revive a failed renderer", async () => {
  let now = 0;
  let expire;
  let late;
  const events = [];
  const slow = new TerminalWriteQueue(
    timers({
      now: () => now,
      scheduleTimeout(callback) {
        expire = callback;
        return 1;
      },
      cancelTimeout() {},
      onReplayStart: () => events.push("start"),
      onReplayEnd: () => events.push("end"),
    }),
  );
  const fast = new TerminalWriteQueue(timers());
  const seen = [];
  try {
    slow.setSink((_, complete) => {
      late = complete;
    });
    fast.setSink((data, complete) => {
      seen.push(data);
      complete();
    });
    const pending = slow.write("old replay", true);
    now += 14 * 24 * 60 * 60 * 1000; // Scheduling resumes after a long suspension.
    assert.equal(slow.diagnostics.pendingSince, 0);
    assert.equal(await fast.write("latest shell prompt"), "written");
    expire();
    assert.equal(await pending, "failed");
    late();
    assert.equal(await fast.write("still live"), "written");
    assert.deepEqual(events, ["start", "end"]);
    assert.deepEqual(seen, ["latest shell prompt", "still live"]);
    assert.equal(slow.diagnostics.lastCompletedAt, null);
  } finally {
    slow.dispose();
    fast.dispose();
  }
});

for (const fixture of replayCases) {
  test(`Web parser conformance: ${fixture.name}`, async () => {
    const terminal = new xterm.Terminal({ cols: 20, rows: 3, scrollback: 10 });
    const queue = new TerminalWriteQueue(timers({ chunkCharacters: 7 }));
    try {
      queue.setSink((data, complete) => terminal.write(data, complete));
      // Queue live output before replay completes to exercise their ordering.
      const results = await Promise.all(
        fixture.parts.map((data, index) =>
          queue.write(data, index < fixture.parts.length - 1),
        ),
      );
      assert.ok(results.every((result) => result === "written"));
      const buffer = terminal.buffer.active;
      assert.deepEqual(
        Array.from(
          { length: 3 },
          (_, row) =>
            buffer.getLine(buffer.baseY + row)?.translateToString(true) ?? "",
        ),
        fixture.lines,
      );
      assert.deepEqual([buffer.cursorX, buffer.cursorY], fixture.cursor);
      assert.equal(terminal.modes.bracketedPasteMode, fixture.paste);
    } finally {
      queue.dispose();
      terminal.dispose();
    }
  });
}

test("a cold retained replay followed by live output reaches the real latest prompt", async () => {
  const terminal = new xterm.Terminal({ cols: 20, rows: 3, scrollback: 10 });
  const queue = new TerminalWriteQueue(timers());
  try {
    queue.setSink((data, complete) => terminal.write(data, complete));
    const retained = "old output\r\n".repeat(100_000);
    const replay = queue.write(retained, true);
    const live = queue.write("\x1b[2J\x1b[H$ latest", false);
    assert.deepEqual(await Promise.all([replay, live]), ["written", "written"]);
    assert.equal(
      terminal.buffer.active
        .getLine(terminal.buffer.active.baseY)
        .translateToString(true),
      "$ latest",
    );
    assert.equal(queue.diagnostics.queuedCharacters, 0);
    assert.ok(terminal.buffer.active.length <= 13);
  } finally {
    queue.dispose();
    terminal.dispose();
  }
});
