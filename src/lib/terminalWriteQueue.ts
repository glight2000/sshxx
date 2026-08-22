export type TerminalWriteQueueState = {
  queuedCharacters: number;
  queuedChunks: number;
};

type WriteGroup = {
  remaining: number;
  replay: boolean;
  started: boolean;
  resolve: () => void;
};

type WriteChunk = {
  data: string;
  replay: boolean;
  group: WriteGroup;
};

export type TerminalWriteQueueOptions = {
  chunkCharacters?: number;
  schedule?: (callback: () => void) => number;
  cancel?: (handle: number) => void;
  transform?: (data: string, replay: boolean) => string;
  onReplayStart?: () => void;
  onReplayEnd?: () => void;
  onStateChange?: (state: TerminalWriteQueueState) => void;
  onError?: (error: unknown) => void;
};

type TerminalWriteSink = (data: string, complete: () => void) => void;

const DEFAULT_CHUNK_CHARACTERS = 64 << 10;

/**
 * Feed xterm in bounded chunks and wait for its public write callback before
 * scheduling the next chunk. The promise returned by write() resolves only
 * after every character from that call has reached the renderer.
 */
export class TerminalWriteQueue {
  readonly #chunkCharacters: number;
  readonly #schedule: (callback: () => void) => number;
  readonly #cancel: (handle: number) => void;
  readonly #transform: (data: string, replay: boolean) => string;
  readonly #onReplayStart: () => void;
  readonly #onReplayEnd: () => void;
  readonly #onStateChange: (state: TerminalWriteQueueState) => void;
  readonly #onError: (error: unknown) => void;

  #sink: TerminalWriteSink | null = null;
  #chunks: WriteChunk[] = [];
  #activeChunk: WriteChunk | null = null;
  #writing = false;
  #scheduled: number | null = null;
  #queuedCharacters = 0;
  #disposed = false;

  constructor(options: TerminalWriteQueueOptions = {}) {
    this.#chunkCharacters = options.chunkCharacters ?? DEFAULT_CHUNK_CHARACTERS;
    if (
      !Number.isSafeInteger(this.#chunkCharacters) ||
      this.#chunkCharacters <= 0
    )
      throw new Error("Terminal write chunk size must be a positive integer.");
    this.#schedule =
      options.schedule ??
      ((callback) => window.requestAnimationFrame(callback));
    this.#cancel =
      options.cancel ?? ((handle) => window.cancelAnimationFrame(handle));
    this.#transform = options.transform ?? ((data) => data);
    this.#onReplayStart = options.onReplayStart ?? (() => undefined);
    this.#onReplayEnd = options.onReplayEnd ?? (() => undefined);
    this.#onStateChange = options.onStateChange ?? (() => undefined);
    this.#onError = options.onError ?? (() => undefined);
  }

  setSink(sink: TerminalWriteSink) {
    if (this.#disposed) return;
    this.#sink = sink;
    this.#drain();
  }

  write(data: string, replay = false): Promise<void> {
    if (!data || this.#disposed) return Promise.resolve();
    const pieces = splitTerminalWrite(data, this.#chunkCharacters);
    return new Promise<void>((resolve) => {
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
      this.#notify();
      this.#drain();
    });
  }

  dispose() {
    if (this.#disposed) return;
    this.#disposed = true;
    if (this.#scheduled !== null) this.#cancel(this.#scheduled);
    this.#scheduled = null;

    const groups = new Set(this.#chunks.map((chunk) => chunk.group));
    if (this.#activeChunk) groups.add(this.#activeChunk.group);
    this.#chunks = [];
    this.#activeChunk = null;
    this.#writing = false;
    this.#queuedCharacters = 0;
    for (const group of groups) this.#finishGroup(group);
    this.#notify();
  }

  #drain() {
    if (
      this.#disposed ||
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
    } catch (error) {
      this.#onError(error);
    }
    if (!data) {
      this.#completeChunk(chunk);
      return;
    }

    this.#writing = true;
    this.#activeChunk = chunk;
    let completed = false;
    const complete = () => {
      if (completed) return;
      completed = true;
      this.#writing = false;
      this.#activeChunk = null;
      this.#completeChunk(chunk);
    };
    try {
      this.#sink(data, complete);
    } catch (error) {
      this.#onError(error);
      complete();
    }
  }

  #completeChunk(chunk: WriteChunk) {
    if (chunk.group.remaining < 0) return;
    this.#queuedCharacters = Math.max(
      0,
      this.#queuedCharacters - chunk.data.length,
    );
    chunk.group.remaining -= 1;
    if (chunk.group.remaining === 0) this.#finishGroup(chunk.group);
    this.#notify();
    if (this.#disposed || this.#chunks.length === 0) return;
    this.#scheduled = this.#schedule(() => {
      this.#scheduled = null;
      this.#drain();
    });
  }

  #finishGroup(group: WriteGroup) {
    if (group.remaining < 0) return;
    group.remaining = -1;
    if (group.started && group.replay) this.#onReplayEnd();
    group.resolve();
  }

  #notify() {
    this.#onStateChange({
      queuedCharacters: this.#queuedCharacters,
      queuedChunks: this.#chunks.length + (this.#writing ? 1 : 0),
    });
  }
}

/** Split without leaving a UTF-16 surrogate pair across xterm writes. */
export function splitTerminalWrite(data: string, maxCharacters: number) {
  if (!Number.isSafeInteger(maxCharacters) || maxCharacters <= 0)
    throw new Error("Terminal write chunk size must be a positive integer.");
  const pieces: string[] = [];
  for (let start = 0; start < data.length;) {
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

function isHighSurrogate(value: number) {
  return value >= 0xd800 && value <= 0xdbff;
}

function isLowSurrogate(value: number) {
  return value >= 0xdc00 && value <= 0xdfff;
}
