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
  /** Structured paragraph blocks. `text` remains as a legacy/search projection. */
  paragraphs: string[];
  /** Terminals associated with this note. The relation is stored only here. */
  linkedShellIds: Sid[];
  /** Other notes associated with this note. Incoming links are derived. */
  linkedNoteIds: Sid[];
  /** File editor windows associated with this note. */
  linkedFileWindowIds: Sid[];
  /** User-defined title, or empty to use the generated note label. */
  title: string;
  background: string;
  opacity: number;
  pageId: number;
};

/** Shared state for a filesystem browser attached to a terminal. */
export type WsFileWindow = {
  shellId: Sid;
  pageId: number;
  path: string;
  title: string;
  background: string;
  x: number;
  y: number;
  width: number;
  height: number;
  currentPath: string;
  expandedPaths: string[];
  selectedPath: string;
  selectedKind: "" | FileTreeEntry["kind"];
  treeScrollTop: number;
  editorPath: string;
  editorStream: bigint;
  editorData: Uint8Array;
  editorDirty: boolean;
  sidebarWidth: number;
  treeRevision: number;
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
  theme: string;
  backgroundEnabled: boolean;
  background: string;
};

export type FileTreeEntry = {
  name: string;
  path: string;
  kind: "directory" | "file" | "symlink" | "other";
  size: number;
};

export type FileOperationRequest = {
  operation:
    | "list"
    | "read"
    | "write"
    | "createFile"
    | "createDirectory"
    | "rename"
    | "move"
    | "delete";
  path: string;
  destination?: string;
  content?: string;
  encoding?: "utf8" | "utf16le" | "utf16be" | "base64";
  recursive?: boolean;
};

export type FileOperationResponse = {
  ok: boolean;
  operation: FileOperationRequest["operation"];
  path: string;
  error?: string;
  entries?: FileTreeEntry[];
  content?: string;
  encoding?: "utf8" | "utf16le" | "utf16be" | "base64";
  size?: number;
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
  fileWindows?: [Sid, WsFileWindow][];
  pages?: WsPage[];
  sshProfiles?: WsSshProfile[];
  noteEditing?: [Sid, number, Uid | null];
  noteText?: [Sid, number, string];
  noteParagraphs?: [Sid, number, string[]];
  chunks?: [Sid, number, boolean, number, Uint8Array[]];
  hear?: [Uid, string, string];
  shellLatency?: number | bigint;
  fileResponse?: [string, bigint, Uint8Array];
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
  cloneWindowedAt?: [
    Sid,
    string,
    string,
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
  createAt?: [
    Sid,
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
  close?: [Sid, number];
  move?: [Sid, number, WsWinsize | null];
  moveCanvasItems?: [
    number,
    number,
    [Sid, number, number][],
    [Sid, number, number][],
    [Sid, number, number][],
  ];
  createNote?: [number, number, number];
  createNoteSized?: [number, number, number, number, number];
  closeNote?: [Sid, number];
  updateNote?: [Sid, number, WsNote | null];
  setNoteEditing?: [Sid, number, boolean];
  updateNoteText?: [Sid, number, string];
  updateNoteParagraphs?: [Sid, number, string[]];
  createFileWindow?: [
    Sid,
    number,
    string,
    string,
    number,
    number,
    number,
    number,
  ];
  closeFileWindow?: [Sid, number];
  updateFileWindow?: [Sid, number, WsFileWindow | null];
  createPage?: string;
  renamePage?: [number, string];
  upsertSshProfile?: WsSshProfile;
  deleteSshProfile?: string;
  data?: [Sid, number, Uint8Array, bigint];
  uploadImage?: [
    Sid,
    number,
    string,
    string,
    bigint,
    bigint,
    bigint,
    Uint8Array,
    boolean,
  ];
  fileRequest?: [Sid, number, string, bigint, bigint, Uint8Array];
  subscribe?: [Sid, number, number];
  chat?: string;
  ping?: bigint;
};
