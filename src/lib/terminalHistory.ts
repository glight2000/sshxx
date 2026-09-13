type TextHistory = {
  chunks: string[];
  start: number;
  length: number;
  pasteMode?: boolean;
};

/** Bounded terminal output retained only for remounting the local renderer. */
export class TerminalHistory {
  private readonly histories = new Map<number, TextHistory>();
  private readonly maxLength: number;

  constructor(maxLength: number) {
    if (!Number.isSafeInteger(maxLength) || maxLength <= 0)
      throw new Error("Terminal history limit must be a positive integer.");
    this.maxLength = maxLength;
  }

  append(id: number, data: string, pasteMode?: boolean) {
    if (!data) return;
    const history = this.histories.get(id) ?? {
      chunks: [],
      start: 0,
      length: 0,
    };
    if (!this.histories.has(id)) this.histories.set(id, history);
    history.pasteMode = pasteMode;
    history.chunks.push(data);
    history.length += data.length;

    while (history.length > this.maxLength) {
      const first = history.chunks[history.start];
      const overflow = history.length - this.maxLength;
      if (overflow >= first.length) {
        history.length -= first.length;
        // Release the text now; compaction only reclaims empty array slots.
        history.chunks[history.start] = "";
        history.start += 1;
      } else {
        let trim = overflow;
        // A retention boundary may land between a UTF-16 surrogate pair.
        // Discard the complete character, not a dangling low surrogate that
        // becomes a replacement glyph when a renderer is recreated.
        const previous = first.charCodeAt(trim - 1);
        const next = first.charCodeAt(trim);
        if (
          previous >= 0xd800 &&
          previous <= 0xdbff &&
          next >= 0xdc00 &&
          next <= 0xdfff
        )
          trim += 1;
        history.chunks[history.start] = first.slice(trim);
        history.length -= trim;
      }
    }

    if (history.start > 256 && history.start * 2 > history.chunks.length) {
      history.chunks = history.chunks.slice(history.start);
      history.start = 0;
    }
  }

  read(id: number) {
    const history = this.histories.get(id);
    return history ? history.chunks.slice(history.start).join("") : "";
  }

  delete(id: number) {
    this.histories.delete(id);
  }

  pasteMode(id: number) {
    return this.histories.get(id)?.pasteMode;
  }

  retain(ids: ReadonlySet<number>) {
    for (const id of this.histories.keys()) {
      if (!ids.has(id)) this.histories.delete(id);
    }
  }

  clear() {
    this.histories.clear();
  }
}
