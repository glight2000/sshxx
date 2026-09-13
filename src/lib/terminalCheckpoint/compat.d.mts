import type { Terminal } from "@xterm/xterm";

export const CHECKPOINT_FORMAT: string;
export const CHECKPOINT_MAX_CELLS: number;
/** Private schema is version locked and checked before renderer mutation. */
export function captureCheckpoint(term: any, history: number): any;
export function validateCheckpoint(state: unknown): any;
export function restoreCheckpoint(
  term: Terminal,
  state: unknown,
  scrollback: number,
): void;
export function parseCheckpointOutput(term: any, text: string): void;
export function captureHistoryRows(term: any, start: number, end: number): any;
export function prependHistoryRows(
  term: Terminal,
  page: unknown,
  scrollback: number,
): void;
