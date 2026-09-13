import {
  captureCheckpoint,
  captureHistoryRows,
  prependHistoryRows,
} from "./compat.mjs";

/** Daemon-only archive coordinates. Resizing/resetting invalidates old pages. */
export class CheckpointArchive {
  constructor(term) {
    this.term = term;
    this.epoch = 0;
    this.base = 0;
    this.buffer = null;
    this.trim = null;
    this.location = "";
    this.locationEvents = term.parser.registerOscHandler(7, (value) => {
      if (value.length <= 8192) this.location = value;
      return true;
    });
    this.colors = new Map();
    this.colorEvents = term._core._inputHandler.onColor((events) => {
      for (const event of events) {
        if (event.type === 1) this.colors.set(event.index, event);
        else if (event.type === 2) {
          if (event.index === undefined) this.colors.clear();
          else this.colors.delete(event.index);
        }
      }
    });
    this.observe();
  }

  observe() {
    const buffer = this.term._core._bufferService.buffers.normal;
    if (buffer === this.buffer) return;
    this.trim?.dispose();
    this.buffer = buffer;
    this.base = 0;
    this.epoch++;
    this.trim = buffer.lines.onTrim((count) => {
      this.base += count;
    });
  }

  resized() {
    this.epoch++;
    this.observe();
  }

  snapshot(history) {
    this.observe();
    const state = captureCheckpoint(this.term, history);
    state.colors = [...this.colors.values()];
    state.location = this.location;
    state.archive = {
      epoch: this.epoch,
      before: this.base + state.normal.first,
      available: this.base,
    };
    return state;
  }

  history(epoch, before, count) {
    this.observe();
    if (
      epoch !== this.epoch ||
      !Number.isSafeInteger(before) ||
      before <= this.base ||
      before > this.base + this.buffer.ybase
    ) {
      throw new Error("Terminal history range is no longer retained");
    }
    const end = before - this.base;
    const start = Math.max(0, end - Math.max(1, Math.min(count, 1000)));
    return {
      ...captureHistoryRows(this.term, start, end),
      epoch,
      before,
      start: this.base + start,
      available: this.base,
    };
  }

  dispose() {
    this.colorEvents.dispose();
    this.locationEvents.dispose();
    this.trim?.dispose();
    this.buffer = null;
  }
}

/** Browser-only loading policy. Reload creates a fresh instance; page switches do not. */
export class TerminalHistoryWindow {
  constructor(rows, limit) {
    this.limit = Math.max(0, Math.floor(limit));
    this.step = Math.max(8, Math.ceil(rows * 1.5));
    this.capacity = Math.min(Math.max(1, Math.ceil(rows / 2)), this.limit);
    this.archive = null;
    this.loading = false;
    this.listeners = [];
  }

  grow(used) {
    this.capacity = Math.min(
      this.limit,
      Math.max(
        this.capacity,
        Math.ceil((used + this.step) / this.step) * this.step,
      ),
    );
    return this.capacity;
  }

  restore(state, term) {
    this.dispose();
    this.archive = state.archive ?? null;
    if (term) {
      const buffers = term._core._bufferService.buffers;
      const normal = buffers.normal;
      this.listeners.push(
        normal.lines.onTrim((count) => {
          if (this.archive) this.archive.before += count;
        }),
        buffers.onBufferActivate(() => {
          if (buffers.normal !== normal) this.archive = null;
        }),
      );
    }
  }

  dispose() {
    this.listeners.forEach((listener) => listener.dispose());
    this.listeners = [];
  }

  apply(term, page) {
    if (
      !this.archive ||
      page.epoch !== this.archive.epoch ||
      page.before !== this.archive.before
    )
      return false;
    const added = page.lines.length;
    if (
      !Number.isSafeInteger(page.start) ||
      !Number.isSafeInteger(page.available) ||
      page.available < 0 ||
      page.start < page.available ||
      page.before - page.start !== added ||
      added === 0
    )
      return false;
    if (term.buffer.normal.baseY + added > this.limit) return false;
    this.grow(term.buffer.normal.baseY + added);
    prependHistoryRows(term, page, this.capacity);
    this.archive = {
      epoch: page.epoch,
      before: page.start,
      available: page.available,
    };
    return true;
  }
}
