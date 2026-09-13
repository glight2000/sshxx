export type TerminalWriteQueueState = {
  queuedCharacters: number;
  queuedChunks: number;
};

export type TerminalWriteResult = "written" | "failed" | "disposed";

export class TerminalWriteFailure extends Error {
  readonly reason: "timeout" | "capacity" | "transform" | "sink";

  constructor(reason: TerminalWriteFailure["reason"], message: string) {
    super(message);
    this.reason = reason;
    this.name = "TerminalWriteFailure";
  }
}

type WriteGroup = {
  remaining: number;
  replay: boolean;
  started: boolean;
  resolve: (result: TerminalWriteResult) => void;
};

type WriteChunk = {
  data: string;
  replay: boolean;
  group: WriteGroup;
};

export type TerminalWriteQueueOptions = {
  chunkCharacters?: number;
  writeTimeoutMs?: number;
  maxQueuedCharacters?: number;
  maxQueuedChunks?: number;
  now?: () => number;
  schedule?: (callback: () => void) => number;
  cancel?: (handle: number) => void;
  scheduleTimeout?: (callback: () => void, timeoutMs: number) => number;
  cancelTimeout?: (handle: number) => void;
  transform?: (data: string, replay: boolean) => string;
  onReplayStart?: () => void;
  onReplayEnd?: () => void;
  onStateChange?: (state: TerminalWriteQueueState) => void;
  onError?: (error: unknown) => void;
  onWriteTimeout?: (error: TerminalWriteTimeoutError) => void;
  onWriteFailure?: (error: TerminalWriteFailure) => void;
};

type TerminalWriteSink = (data: string, complete: () => void) => void;

const DEFAULT_CHUNK_CHARACTERS = 64 << 10;
export const DEFAULT_TERMINAL_WRITE_TIMEOUT_MS = 15_000;
export const MAX_TERMINAL_QUEUED_CHARACTERS = 4 << 20;
export const MAX_TERMINAL_QUEUED_CHUNKS = 4096;

export class TerminalWriteTimeoutError extends TerminalWriteFailure {
  readonly timeoutMs: number;
  readonly chunkCharacters: number;

  constructor(timeoutMs: number, chunkCharacters: number) {
    super(
      "timeout",
      `Terminal renderer did not complete a ${chunkCharacters}-character write within ${timeoutMs} ms.`,
    );
    this.name = "TerminalWriteTimeoutError";
    this.timeoutMs = timeoutMs;
    this.chunkCharacters = chunkCharacters;
  }
}

/**
 * Feed xterm in bounded chunks and wait for its public write callback before
 * scheduling the next chunk. Settlement releases the caller's lock; only a
 * "written" result confirms processing. Failure and teardown are not success.
 */
export class TerminalWriteQueue {
  readonly #chunkCharacters: number;
  readonly #writeTimeoutMs: number;
  readonly #maxQueuedCharacters: number;
  readonly #maxQueuedChunks: number;
  readonly #now: () => number;
  readonly #schedule: (callback: () => void) => number;
  readonly #cancel: (handle: number) => void;
  readonly #scheduleTimeout: (
    callback: () => void,
    timeoutMs: number,
  ) => number;
  readonly #cancelTimeout: (handle: number) => void;
  readonly #transform: (data: string, replay: boolean) => string;
  readonly #onReplayStart: () => void;
  readonly #onReplayEnd: () => void;
  readonly #onStateChange: (state: TerminalWriteQueueState) => void;
  readonly #onError: (error: unknown) => void;
  readonly #onWriteTimeout: (error: TerminalWriteTimeoutError) => void;
  readonly #onWriteFailure: (error: TerminalWriteFailure) => void;

  #sink: TerminalWriteSink | null = null;
  #chunks: WriteChunk[] = [];
  #activeChunk: WriteChunk | null = null;
  #writing = false;
  #scheduled: number | null = null;
  #writeTimeout: number | null = null;
  #sinkTimeout: number | null = null;
  #queuedCharacters = 0;
  #disposed = false;
  #failed = false;
  #pendingSince: number | null = null;
  #lastCompletedAt: number | null = null;

  get diagnostics() {
    return {
      queuedCharacters: this.#queuedCharacters,
      queuedChunks: this.#chunks.length + (this.#writing ? 1 : 0),
      pendingSince: this.#pendingSince,
      lastCompletedAt: this.#lastCompletedAt,
      failed: this.#failed,
    };
  }

  constructor(options: TerminalWriteQueueOptions = {}) {
    this.#chunkCharacters = options.chunkCharacters ?? DEFAULT_CHUNK_CHARACTERS;
    if (
      !Number.isSafeInteger(this.#chunkCharacters) ||
      this.#chunkCharacters <= 0
    )
      throw new Error("Terminal write chunk size must be a positive integer.");
    this.#writeTimeoutMs =
      options.writeTimeoutMs ?? DEFAULT_TERMINAL_WRITE_TIMEOUT_MS;
    if (
      !Number.isSafeInteger(this.#writeTimeoutMs) ||
      this.#writeTimeoutMs <= 0
    )
      throw new Error("Terminal write timeout must be a positive integer.");
    this.#maxQueuedCharacters =
      options.maxQueuedCharacters ?? MAX_TERMINAL_QUEUED_CHARACTERS;
    this.#maxQueuedChunks =
      options.maxQueuedChunks ?? MAX_TERMINAL_QUEUED_CHUNKS;
    for (const limit of [this.#maxQueuedCharacters, this.#maxQueuedChunks]) {
      if (!Number.isSafeInteger(limit) || limit <= 0)
        throw new Error("Terminal queue limits must be positive integers.");
    }
    this.#now = options.now ?? Date.now;
    // Parsing/ACK progress must not depend on visible rendering frames.
    // Background tabs pause requestAnimationFrame entirely.
    this.#schedule =
      options.schedule ?? ((callback) => window.setTimeout(callback, 0));
    this.#cancel = options.cancel ?? ((handle) => window.clearTimeout(handle));
    this.#scheduleTimeout =
      options.scheduleTimeout ??
      ((callback, timeoutMs) => window.setTimeout(callback, timeoutMs));
    this.#cancelTimeout =
      options.cancelTimeout ?? ((handle) => window.clearTimeout(handle));
    this.#transform = options.transform ?? ((data) => data);
    this.#onReplayStart = options.onReplayStart ?? (() => undefined);
    this.#onReplayEnd = options.onReplayEnd ?? (() => undefined);
    this.#onStateChange = options.onStateChange ?? (() => undefined);
    this.#onError = options.onError ?? (() => undefined);
    this.#onWriteTimeout = options.onWriteTimeout ?? (() => undefined);
    this.#onWriteFailure = options.onWriteFailure ?? (() => undefined);
  }

  setSink(sink: TerminalWriteSink) {
    if (this.#disposed || this.#failed) return;
    if (this.#sinkTimeout !== null) this.#cancelTimeout(this.#sinkTimeout);
    this.#sinkTimeout = null;
    this.#sink = sink;
    this.#drain();
  }

  write(data: string, replay = false): Promise<TerminalWriteResult> {
    if (this.#disposed) return Promise.resolve("disposed");
    if (this.#failed) return Promise.resolve("failed");
    if (!data) return Promise.resolve("written");
    if (data.length > this.#maxQueuedCharacters - this.#queuedCharacters) {
      this.#fail(queueCapacityError());
      return Promise.resolve("failed");
    }
    let pieces: string[];
    try {
      pieces = splitTerminalWrite(
        data,
        this.#chunkCharacters,
        this.#maxQueuedChunks - this.#chunks.length - Number(this.#writing),
      );
    } catch {
      this.#fail(queueCapacityError());
      return Promise.resolve("failed");
    }
    this.#pendingSince ??= this.#now();
    return new Promise<TerminalWriteResult>((resolve) => {
      const group: WriteGroup = {
        remaining: pieces.length,
        replay,
        started: false,
        resolve,
      };
      for (const piece of pieces) {
        this.#chunks.push({ data: piece, replay, group });
      }
      this.#queuedCharacters += data.length;
      if (!this.#sink && this.#sinkTimeout === null) {
        this.#sinkTimeout = this.#scheduleTimeout(() => {
          this.#sinkTimeout = null;
          this.#fail(
            new TerminalWriteTimeoutError(
              this.#writeTimeoutMs,
              this.#queuedCharacters,
            ),
          );
        }, this.#writeTimeoutMs);
      }
      this.#notify();
      this.#drain();
    });
  }

  dispose() {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#cancelPending();

    this.#finishPending("disposed");
  }

  #cancelPending() {
    if (this.#scheduled !== null) this.#cancel(this.#scheduled);
    this.#scheduled = null;
    if (this.#writeTimeout !== null) this.#cancelTimeout(this.#writeTimeout);
    this.#writeTimeout = null;
    if (this.#sinkTimeout !== null) this.#cancelTimeout(this.#sinkTimeout);
    this.#sinkTimeout = null;
  }

  #finishPending(result: TerminalWriteResult) {
    const groups = new Set(this.#chunks.map((chunk) => chunk.group));
    if (this.#activeChunk) groups.add(this.#activeChunk.group);
    this.#chunks = [];
    this.#activeChunk = null;
    this.#writing = false;
    this.#queuedCharacters = 0;
    this.#pendingSince = null;
    for (const group of groups) this.#finishGroup(group, result);
    this.#notify();
  }

  #drain() {
    if (
      this.#disposed ||
      this.#failed ||
      !this.#sink ||
      this.#writing ||
      this.#scheduled !== null
    )
      return;
    const chunk = this.#chunks.shift();
    if (!chunk) return;
    if (!chunk.group.started) {
      chunk.group.started = true;
      if (chunk.replay) this.#onReplayStart();
    }

    let data = "";
    try {
      data = this.#transform(chunk.data, chunk.replay);
    } catch {
      // Keep the active group reachable for cleanup even though it was shifted.
      this.#activeChunk = chunk;
      this.#fail(
        new TerminalWriteFailure(
          "transform",
          "Terminal output processing failed.",
        ),
      );
      return;
    }
    if (!data) {
      this.#completeChunk(chunk);
      return;
    }

    this.#writing = true;
    this.#activeChunk = chunk;
    let completed = false;
    const complete = () => {
      if (completed || this.#disposed || this.#failed) return;
      completed = true;
      if (this.#writeTimeout !== null) this.#cancelTimeout(this.#writeTimeout);
      this.#writeTimeout = null;
      this.#writing = false;
      this.#activeChunk = null;
      this.#lastCompletedAt = this.#now();
      this.#completeChunk(chunk);
    };
    this.#writeTimeout = this.#scheduleTimeout(() => {
      if (completed) return;
      completed = true;
      this.#writeTimeout = null;
      this.#fail(
        new TerminalWriteTimeoutError(this.#writeTimeoutMs, data.length),
      );
    }, this.#writeTimeoutMs);
    try {
      this.#sink(data, complete);
    } catch {
      this.#fail(
        new TerminalWriteFailure("sink", "Terminal renderer rejected output."),
      );
    }
  }

  #fail(error: TerminalWriteFailure) {
    if (this.#disposed || this.#failed) return;
    this.#failed = true;
    this.#cancelPending();
    this.#finishPending("failed");
    // Report controlled metadata, not parser exception strings that may contain
    // terminal text. Never automatically retry input as part of output recovery.
    this.#onWriteFailure(error);
    if (error instanceof TerminalWriteTimeoutError) this.#onWriteTimeout(error);
    else this.#onError(error);
  }

  #completeChunk(chunk: WriteChunk) {
    if (chunk.group.remaining < 0) return;
    this.#queuedCharacters = Math.max(
      0,
      this.#queuedCharacters - chunk.data.length,
    );
    chunk.group.remaining -= 1;
    if (this.#queuedCharacters === 0) this.#pendingSince = null;
    if (chunk.group.remaining === 0) this.#finishGroup(chunk.group);
    this.#notify();
    if (this.#disposed || this.#chunks.length === 0) return;
    this.#scheduled = this.#schedule(() => {
      this.#scheduled = null;
      this.#drain();
    });
  }

  #finishGroup(group: WriteGroup, result: TerminalWriteResult = "written") {
    if (group.remaining < 0) return;
    group.remaining = -1;
    if (group.started && group.replay) this.#onReplayEnd();
    group.resolve(result);
  }

  #notify() {
    this.#onStateChange({
      queuedCharacters: this.#queuedCharacters,
      queuedChunks: this.#chunks.length + (this.#writing ? 1 : 0),
    });
  }
}

/** Split without leaving a UTF-16 surrogate pair across xterm writes. */
export function splitTerminalWrite(
  data: string,
  maxCharacters: number,
  maxChunks = Number.POSITIVE_INFINITY,
) {
  if (!Number.isSafeInteger(maxCharacters) || maxCharacters <= 0)
    throw new Error("Terminal write chunk size must be a positive integer.");
  const pieces: string[] = [];
  for (let start = 0; start < data.length;) {
    if (pieces.length >= maxChunks) throw queueCapacityError();
    let end = Math.min(start + maxCharacters, data.length);
    if (
      end < data.length &&
      end > start &&
      isHighSurrogate(data.charCodeAt(end - 1)) &&
      isLowSurrogate(data.charCodeAt(end))
    ) {
      end -= 1;
    }
    // A one-character chunk cannot be shortened without making progress.
    if (end === start) end = Math.min(start + 2, data.length);
    pieces.push(data.slice(start, end));
    start = end;
  }
  return pieces;
}

function queueCapacityError() {
  return new TerminalWriteFailure(
    "capacity",
    "Terminal output queue exceeded its memory budget.",
  );
}

function isHighSurrogate(value: number) {
  return value >= 0xd800 && value <= 0xdbff;
}

function isLowSurrogate(value: number) {
  return value >= 0xdc00 && value <= 0xdfff;
}
