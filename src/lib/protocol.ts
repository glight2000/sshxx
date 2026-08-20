type Sid = number; // u32
type Uid = number; // u32

/** Position and size of a window, see the Rust version. */
export type WsWinsize = {
  x: number;
  y: number;
  rows: number;
  cols: number;
  width: number;
  height: number;
  title: string;
  background: string;
  opacity: number;
  pageId: number;
  theme: string;
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

export type WsSshAuthMethod = "default" | "agent" | "keyFile" | "password";

/** Reusable SSH connection metadata. Passwords are intentionally not stored. */
export type WsSshProfile = {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authMethod: WsSshAuthMethod;
  keyPath: string;
  acceptNewHostKey: boolean;
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
  sshProfiles?: WsSshProfile[];
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
  createSized?: [number, number, number, number, number];
  createStyled?: [number, number, number, number, number, string];
  createWindowed?: [
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    string,
  ];
  createSsh?: [string, number, number, number, number, number];
  createSshStyled?: [string, number, number, number, number, number, string];
  createSshWindowed?: [
    string,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    string,
  ];
  clone?: [Sid, number, number, number];
  cloneSized?: [Sid, number, number, number, number, number];
  cloneStyled?: [Sid, number, number, number, number, number, string];
  cloneWindowed?: [
    Sid,
    number,
    number,
    number,
    number,
    number,
    number,
    number,
    string,
  ];
  close?: [Sid, number];
  move?: [Sid, number, WsWinsize | null];
  createNote?: [number, number, number];
  createNoteSized?: [number, number, number, number, number];
  closeNote?: [Sid, number];
  updateNote?: [Sid, number, WsNote | null];
  setNoteEditing?: [Sid, number, boolean];
  updateNoteText?: [Sid, number, string];
  createPage?: string;
  renamePage?: [number, string];
  upsertSshProfile?: WsSshProfile;
  deleteSshProfile?: string;
  data?: [Sid, number, Uint8Array, bigint];
  subscribe?: [Sid, number, number];
  chat?: string;
  ping?: bigint;
};
