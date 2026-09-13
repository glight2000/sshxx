import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import headless from "@xterm/headless";
import web from "@xterm/xterm";
import { Encrypt, deriveCheckpointKey } from "../src/lib/encrypt.ts";
import { TerminalCheckpointClient } from "../src/lib/terminalCheckpoint/client.ts";
import { watchTerminalText } from "../src/lib/mobileTerminalText.ts";
import { terminalHistoryScroll } from "../src/lib/terminalCheckpoint/historyScroll.ts";
import {
  captureCheckpoint,
  parseCheckpointOutput,
  restoreCheckpoint,
  validateCheckpoint,
} from "../src/lib/terminalCheckpoint/compat.mjs";
import {
  CheckpointArchive,
  TerminalHistoryWindow,
} from "../src/lib/terminalCheckpoint/archive.mjs";

const options = { rows: 3, cols: 20, scrollback: 100, allowProposedApi: true };

test("terminal output epochs match Rust and cannot recover another incarnation's plaintext", async () => {
  const raw = new Uint8Array(16);
  const ctr = await crypto.subtle.importKey("raw", raw, "AES-CTR", false, [
    "encrypt",
  ]);
  const material = await crypto.subtle.importKey("raw", raw, "HKDF", false, [
    "deriveKey",
  ]);
  const encrypt = new Encrypt(ctr, ctr, material);
  const plaintext = new TextEncoder().encode("synthetic output");
  const first = await encrypt.outputSegment(
    new Uint8Array(16).fill(1),
    0x100000007n,
    0n,
    plaintext,
  );
  assert.equal(
    Buffer.from(first).toString("hex"),
    "ac80c57ee9df8e5fa71f7a6059a86228",
  );
  const next = await encrypt.outputSegment(
    new Uint8Array(16).fill(2),
    0x100000007n,
    0n,
    plaintext,
  );
  assert.notDeepEqual(next, first);
  assert.deepEqual(
    await encrypt.outputSegment(
      new Uint8Array(16).fill(2),
      0x100000007n,
      3n,
      next.slice(3),
    ),
    plaintext.slice(3),
  );
  assert.deepEqual(
    await encrypt.outputSegment(undefined, 0x100000007n, 0n, plaintext),
    await encrypt.segment(0x100000007n, 0n, plaintext),
  );
  await assert.rejects(
    encrypt.outputSegment(new Uint8Array(15), 1n, 0n, plaintext),
  );
});
const fixtures = JSON.parse(
  readFileSync(new URL("./fixtures/terminal-replay.json", import.meta.url)),
);

test("WebCrypto derives the same domain-separated checkpoint key as Rust", async () => {
  const key = await deriveCheckpointKey(new Uint8Array(16));
  const data = Uint8Array.from(
    Buffer.from(
      "1f2029956effdcdb39c74aef6a504af819c591f3580d3b1cf297be8bb5a6346526599c92",
      "hex",
    ),
  );
  const result = await crypto.subtle.decrypt(
    { name: "AES-GCM", iv: new Uint8Array(12) },
    key,
    data,
  );
  assert.equal(new TextDecoder().decode(result), "synthetic checkpoint");
});

function snapshot(term) {
  const result = captureCheckpoint(term, 100);
  // Link IDs are local identifiers; compare their actual URI below instead.
  return result;
}

function check(before, after, opts = options) {
  const source = new headless.Terminal(opts);
  const expected = new web.Terminal(opts);
  const restored = new web.Terminal(opts);
  try {
    parseCheckpointOutput(source, before);
    parseCheckpointOutput(expected, before);
    const saved = captureCheckpoint(source, 100);
    restoreCheckpoint(restored, saved, 100);
    parseCheckpointOutput(restored, after);
    parseCheckpointOutput(expected, after);
    assert.deepEqual(snapshot(restored), snapshot(expected));
  } finally {
    source.dispose();
    expected.dispose();
    restored.dispose();
  }
}

test("checkpoint dependencies are exactly the Web parser version", () => {
  for (const name of ["xterm", "headless"]) {
    assert.equal(
      JSON.parse(
        readFileSync(
          new URL(
            `../node_modules/@xterm/${name}/package.json`,
            import.meta.url,
          ),
        ),
      ).version,
      "6.0.0",
    );
  }
});

test("headless checkpoints continue shared replay fixtures at every chunk boundary", () => {
  const cases = Array.isArray(fixtures) ? fixtures : fixtures.cases;
  for (const fixture of cases) {
    for (let split = 0; split <= fixture.parts.length; split++) {
      check(
        fixture.parts.slice(0, split).join(""),
        fixture.parts.slice(split).join(""),
        { ...options, cols: fixture.cols ?? 20, rows: fixture.rows ?? 3 },
      );
    }
  }
});

test("preserves saved cursor, margins, modes and split controls", () => {
  const cases = [
    ["one\x1b7\r\ntwo\x1b[32m", "\x1b8!"],
    ["head\r\none\r\ntwo\x1b[2;3r\x1b[3;1H", "\r\nlatest"],
    ["text\x1b[3", "1mred"],
    ["\x1b]2;partial", " title\x07text"],
    ["\x1bP$q", "m\x1b\\text"],
    [
      "\x1b[?1049h\x1b[4h\x1b[?2004h\x1b[?1003h\x1b[?1006h",
      "TUI\x1b[?1049lprompt",
    ],
    ["\x1b[4:3mcurly\x1b[0m", "normal"],
    ["中😀é\x1b(0", "qqq\x1b(B"],
    ["\x1b]8;id=test;https://example.org\x07link", "more\x1b]8;;\x07end"],
    ["\x1b[?6h\x1b[?7l\x1b[?25l", "hello\r\nworld"],
  ];
  for (const [before, after] of cases) check(before, after);
});

test("small cold checkpoint includes only requested history without changing the source", () => {
  const term = new headless.Terminal(options);
  try {
    parseCheckpointOutput(
      term,
      Array.from({ length: 100 }, (_, i) => `line ${i}\r\n`).join(""),
    );
    const before = term.buffer.normal.length;
    const value = captureCheckpoint(term, 2);
    assert.equal(value.normal.lines.length, 5);
    assert.equal(value.normal.ybase, 2);
    assert.equal(term.buffer.normal.length, before);
    const viewer = new web.Terminal(options);
    try {
      restoreCheckpoint(viewer, value, 2);
      assert.equal(viewer.buffer.normal.length, 5);
      parseCheckpointOutput(viewer, "latest prompt");
      assert.match(
        viewer.buffer.active.getLine(4).translateToString(true),
        /latest prompt/,
      );
    } finally {
      viewer.dispose();
    }
  } finally {
    term.dispose();
  }
});

test("rejects incompatible and oversized state before resetting a viewer", () => {
  const term = new headless.Terminal(options);
  try {
    const state = captureCheckpoint(term, 2);
    assert.throws(() => validateCheckpoint({ ...state, format: "future/2" }));
    assert.throws(() => validateCheckpoint({ ...state, cols: 1000000 }));
    assert.throws(() =>
      validateCheckpoint({
        ...state,
        normal: { ...state.normal, lines: new Array(300001) },
      }),
    );
  } finally {
    term.dispose();
  }
});

test("checkpoint restore and prepend invalidate the Web viewport's cached scroll target", () => {
  const source = new headless.Terminal(options);
  const viewer = new web.Terminal(options);
  const archive = new CheckpointArchive(source);
  const policy = new TerminalHistoryWindow(3, 100);
  const queued = [];
  // An unopened Web terminal has no DOM viewport. Probe the adapter contract;
  // actual thumb/scrollable alignment is also checked in a running browser.
  const viewport = {
    _latestYDisp: 0,
    queueSync(...args) {
      queued.push({ target: this._latestYDisp, args });
    },
  };
  try {
    parseCheckpointOutput(source, "synthetic row\r\n".repeat(80));
    const state = archive.snapshot(policy.capacity);
    viewer._core._viewport = viewport;
    restoreCheckpoint(viewer, state, policy.capacity);
    policy.restore(state, viewer);
    assert.deepEqual(queued, [{ target: undefined, args: [] }]);
    // Model the scrollbar's cached zero after the user reaches the top. No
    // public scroll event is emitted by row insertion to repair this for us.
    viewport._latestYDisp = 0;
    viewer._core._bufferService.buffer.ydisp = 0;
    const page = archive.history(
      policy.archive.epoch,
      policy.archive.before,
      8,
    );
    assert.equal(policy.apply(viewer, page), true);
    assert.equal(viewer.buffer.normal.viewportY, 8);
    assert.deepEqual(queued, [
      { target: undefined, args: [] },
      { target: undefined, args: [] },
    ]);
  } finally {
    delete viewer._core._viewport;
    policy.dispose();
    archive.dispose();
    source.dispose();
    viewer.dispose();
  }
});

test("xterm top events load one page and prepending re-arms the next top edge", async () => {
  const source = new headless.Terminal(options);
  const viewer = new web.Terminal(options);
  const archive = new CheckpointArchive(source);
  const policy = new TerminalHistoryWindow(3, 100);
  let now = 0;
  let loads = 0;
  const results = [];
  const scroll = terminalHistoryScroll(
    async () => {
      loads++;
      const anchor = viewer.buffer.normal.getLine(0).translateToString();
      const page = archive.history(
        policy.archive.epoch,
        policy.archive.before,
        8,
      );
      results.push({
        applied: policy.apply(viewer, page),
        viewport: viewer.buffer.normal.viewportY,
        anchored:
          viewer.buffer.normal.getLine(8).translateToString() === anchor,
      });
      scroll.update(viewer.buffer.normal.viewportY);
    },
    () => now,
  );
  try {
    parseCheckpointOutput(source, "synthetic row\r\n".repeat(80));
    const state = archive.snapshot(policy.capacity);
    restoreCheckpoint(viewer, state, policy.capacity);
    policy.restore(state, viewer);
    scroll.reset(viewer.buffer.normal.viewportY);
    viewer.onScroll((position) => scroll.update(position));
    viewer.scrollLines(-1);
    assert.equal(loads, 0);
    viewer.scrollToTop();
    assert.equal(loads, 1, "reaching the top is sufficient");
    await Promise.resolve();
    now += 300;
    viewer.scrollToTop();
    assert.equal(loads, 2, "no extra down/up gesture needed after a prepend");
    await Promise.resolve();
    assert.equal(viewer.buffer.normal.viewportY, 8);
    assert.deepEqual(results, [
      { applied: true, viewport: 8, anchored: true },
      { applied: true, viewport: 8, anchored: true },
    ]);
  } finally {
    scroll.dispose();
    policy.dispose();
    archive.dispose();
    source.dispose();
    viewer.dispose();
  }
});

test("older pages prepend cells while live output and the reading anchor remain intact", () => {
  const source = new headless.Terminal(options);
  const viewer = new web.Terminal(options);
  const archive = new CheckpointArchive(source);
  const policy = new TerminalHistoryWindow(3, 100);
  try {
    parseCheckpointOutput(
      source,
      Array.from({ length: 80 }, (_, i) => `line ${i}\r\n`).join(""),
    );
    const state = archive.snapshot(policy.capacity);
    restoreCheckpoint(viewer, state, policy.capacity);
    policy.restore(state);
    viewer.options.scrollback = policy.grow(viewer.buffer.normal.baseY);
    parseCheckpointOutput(source, "live\r\n");
    parseCheckpointOutput(viewer, "live\r\n");
    viewer.scrollToTop();
    const anchor = viewer.buffer.normal
      .getLine(viewer.buffer.normal.viewportY)
      .translateToString();
    const page = archive.history(
      policy.archive.epoch,
      policy.archive.before,
      8,
    );
    assert.equal(policy.apply(viewer, page), true);
    assert.equal(
      viewer.buffer.normal
        .getLine(viewer.buffer.normal.viewportY)
        .translateToString(),
      anchor,
    );
    assert.equal(
      viewer.buffer.normal
        .getLine(viewer.buffer.normal.baseY + 1)
        .translateToString(true),
      "live",
    );
    assert.equal(
      policy.apply(viewer, page),
      false,
      "stale/duplicate page cannot be inserted twice",
    );
    const cursor = { ...policy.archive };
    source.resize(30, 3);
    archive.resized();
    assert.throws(() => archive.history(cursor.epoch, cursor.before, 8));
  } finally {
    archive.dispose();
    source.dispose();
    viewer.dispose();
  }
});

test("long-lived archive rollover is bounded and refuses a lost range", () => {
  const source = new headless.Terminal({ ...options, scrollback: 16 });
  const archive = new CheckpointArchive(source);
  try {
    parseCheckpointOutput(source, "a\r\nb\r\nc\r\nd\r\n");
    const cursor = archive.snapshot(1).archive;
    for (let i = 0; i < 2000; i++) parseCheckpointOutput(source, `${i}\r\n`);
    assert.ok(source.buffer.normal.length <= 19);
    assert.throws(() => archive.history(cursor.epoch, cursor.before, 8));
    const current = archive.snapshot(1).archive;
    const page = archive.history(current.epoch, current.before, 8);
    assert.equal(page.lines.length, 8);
  } finally {
    archive.dispose();
    source.dispose();
  }
});

test("checkpoint encryption verifies authentication tags and rejects oversized envelopes", async () => {
  const raw = new Uint8Array(16);
  const ctr = await crypto.subtle.importKey("raw", raw, "AES-CTR", false, [
    "encrypt",
  ]);
  const gcm = await crypto.subtle.importKey("raw", raw, "AES-GCM", false, [
    "decrypt",
  ]);
  const encrypt = new Encrypt(ctr, gcm);
  // NIST AES-128-GCM empty-plaintext vector with zero key and nonce.
  const tag = Uint8Array.from(
    Buffer.from("58e2fccefa7e3061367f1d57a4e7455a", "hex"),
  );
  assert.equal(
    (await encrypt.openCheckpoint(new Uint8Array(12), tag)).length,
    0,
  );
  tag[0] ^= 1;
  await assert.rejects(encrypt.openCheckpoint(new Uint8Array(12), tag));
  await assert.rejects(encrypt.openCheckpoint(new Uint8Array(11), tag));
  await assert.rejects(
    encrypt.openCheckpoint(
      new Uint8Array(12),
      new Uint8Array((4 << 20) + 65537),
    ),
  );
});

test("checkpoint requests correlate identity, reject mismatches and release work on disconnect", async () => {
  const sent = [];
  let payload;
  const client = new TerminalCheckpointClient(
    {
      async openCheckpoint() {
        return new TextEncoder().encode(JSON.stringify(payload));
      },
    },
    (message) => sent.push(message),
  );
  try {
    const result = client.request(60, 4, 2, 36);
    const requestId = sent[0].terminalCheckpoint[3];
    payload = {
      id: 60,
      requestId,
      generation: 2,
      sequence: 12,
      state: { format: "synthetic" },
    };
    await client.accept({
      ...payload,
      nonce: new Uint8Array(12),
      data: new Uint8Array(16),
    });
    assert.equal((await result).sequence, 12);
    const wrong = client.request(60, 4, 2, 36);
    const rejected = assert.rejects(wrong);
    await new Promise((resolve) => setTimeout(resolve, 80));
    const secondId = sent.at(-1).terminalCheckpoint[3];
    payload = {
      id: 61,
      requestId: secondId,
      generation: 2,
      sequence: 12,
      state: {},
    };
    await client.accept({
      id: 60,
      requestId: secondId,
      generation: 2,
      sequence: 12,
      nonce: new Uint8Array(12),
      data: new Uint8Array(16),
    });
    await rejected;
    const pending = client.request(60, 4, 2, 36);
    const disconnected = assert.rejects(pending, /connection closed/);
    client.reset();
    await disconnected;
    assert.equal(client.requests.size, 0);
    assert.equal(client.queue.length, 0);
  } finally {
    client.dispose();
  }
});

test("continuations also survive a checkpoint inside each character of compound controls", () => {
  const stream =
    "中😀\x1b]2;title\x1b\\\x1b[38:2::12:34:56mcolor\x1b7\x1b[2;3r\x1b[?1049happ\x1b[?1049l\x1b8!";
  for (let split = 0; split <= stream.length; split++)
    check(stream.slice(0, split), stream.slice(split));
});

test("restored palette, location, hyperlink lifetimes and idle mobile projection remain bounded", async () => {
  const source = new headless.Terminal(options);
  const viewer = new web.Terminal(options);
  const archive = new CheckpointArchive(source);
  let text = "";
  const observer = watchTerminalText(viewer, (value) => {
    text = value.text;
  });
  try {
    parseCheckpointOutput(
      source,
      "\x1b]7;file://example.test/work\x07\x1b]4;1;#abcdef\x07\x1b]8;id=link;https://example.org\x07visible\x1b]8;;\x07",
    );
    const state = archive.snapshot(2);
    assert.equal(state.location, "file://example.test/work");
    assert.deepEqual(state.colors, [
      { type: 1, index: 1, color: [171, 205, 239] },
    ]);
    for (let i = 0; i < 30; i++) restoreCheckpoint(viewer, state, 2);
    assert.ok(viewer._core._oscLinkService._dataByLinkId.size <= 1);
    await new Promise((resolve) => setTimeout(resolve, 280));
    assert.match(text, /visible/);
    const before = captureCheckpoint(viewer, 2);
    assert.throws(() =>
      restoreCheckpoint(
        viewer,
        { ...state, links: { 1: { uri: { invalid: true } } } },
        2,
      ),
    );
    assert.deepEqual(captureCheckpoint(viewer, 2), before);
  } finally {
    observer.dispose();
    archive.dispose();
    source.dispose();
    viewer.dispose();
  }
});

test("client retention trimming advances the archive cursor and reset invalidates it", () => {
  const source = new headless.Terminal(options);
  const viewer = new web.Terminal(options);
  const archive = new CheckpointArchive(source);
  const policy = new TerminalHistoryWindow(3, 2);
  try {
    parseCheckpointOutput(
      source,
      Array.from({ length: 30 }, (_, i) => `line ${i}\r\n`).join(""),
    );
    const state = archive.snapshot(2);
    restoreCheckpoint(viewer, state, 2);
    policy.restore(state, viewer);
    const before = policy.archive.before;
    parseCheckpointOutput(viewer, "new\r\nmore\r\n");
    assert.equal(policy.archive.before, before + 2);
    viewer.reset();
    assert.equal(policy.archive, null);
  } finally {
    policy.dispose();
    archive.dispose();
    source.dispose();
    viewer.dispose();
  }
});
