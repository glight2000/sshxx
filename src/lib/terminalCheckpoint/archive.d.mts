import type { Terminal } from "@xterm/xterm";

export type ArchiveCursor = {
  epoch: number;
  before: number;
  available: number;
};
export class TerminalHistoryWindow {
  constructor(rows: number, limit: number);
  limit: number;
  step: number;
  capacity: number;
  archive: ArchiveCursor | null;
  loading: boolean;
  grow(used: number): number;
  restore(state: any, term?: Terminal): void;
  dispose(): void;
  apply(term: Terminal, page: any): boolean;
}
export class CheckpointArchive {
  constructor(term: any);
  snapshot(history: number): any;
  history(epoch: number, before: number, count: number): any;
  resized(): void;
  observe(): void;
  dispose(): void;
}
