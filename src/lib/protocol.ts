type Sid = number; // u32
type Uid = number; // u32

/** Position and size of a window, see the Rust version. */
export type WsWinsize = {
  x: number;
  y: number;
  rows: number;
  cols: number;
  title: string;
  background: string;
  opacity: number;
  pageId: number;
};

/** Shared state for a note on the infinite canvas. */
export type WsNote = {
  x: number;
  y: number;
  width: number;
  height: number;
  text: string;
  background: string;
  opacity: number;
  pageId: number;
};

/** A named canvas page shared by every viewer. */
export type WsPage = {
  id: number;
  name: string;
};

/** Information about a user, see the Rust version */
export type WsUser = {
  name: string;
  cursor: [number, number] | null;
  pageId: number;
  focus: number | null;
  canWrite: boolean;
};

/** Server message type, see the Rust version. */
export type WsServer = {
  hello?: [Uid, string, string, string];
  invalidAuth?: [];
  users?: [Uid, WsUser][];
  userDiff?: [Uid, WsUser | null];
  shells?: [Sid, WsWinsize][];
  notes?: [Sid, WsNote][];
  pages?: WsPage[];
  noteEditing?: [Sid, number, Uid | null];
  noteText?: [Sid, number, string];
  chunks?: [Sid, number, boolean, number, Uint8Array[]];
  hear?: [Uid, string, string];
  shellLatency?: number | bigint;
  pong?: number | bigint;
  error?: string;
};

/** Client message type, see the Rust version. */
export type WsClient = {
  authenticate?: [Uint8Array, Uint8Array | null];
  setName?: string;
  setCursor?: [number, [number, number] | null];
  setFocus?: [Sid, number] | null;
  create?: [number, number, number];
  clone?: [Sid, number, number, number];
  close?: [Sid, number];
  move?: [Sid, number, WsWinsize | null];
  createNote?: [number, number, number];
  closeNote?: [Sid, number];
  updateNote?: [Sid, number, WsNote | null];
  setNoteEditing?: [Sid, number, boolean];
  updateNoteText?: [Sid, number, string];
  createPage?: string;
  renamePage?: [number, string];
  data?: [Sid, number, Uint8Array, bigint];
  subscribe?: [Sid, number, number];
  chat?: string;
  ping?: bigint;
};
