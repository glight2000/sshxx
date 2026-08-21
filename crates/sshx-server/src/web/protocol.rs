//! Serializable types sent and received by the web server.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sshx_core::{Sid, Uid};

/// Real-time message conveying the position and size of a terminal.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WsWinsize {
    /// The top-left x-coordinate of the window, offset from origin.
    pub x: i32,
    /// The top-left y-coordinate of the window, offset from origin.
    pub y: i32,
    /// The number of rows in the window.
    pub rows: u16,
    /// The number of columns in the terminal.
    pub cols: u16,
    /// Exact canvas window width, or zero for legacy content sizing.
    #[serde(default)]
    pub width: u16,
    /// Exact canvas window height, or zero for legacy content sizing.
    #[serde(default)]
    pub height: u16,
    /// User-defined title override, or an empty string to use the terminal
    /// title.
    #[serde(default)]
    pub title: String,
    /// User-defined terminal background color, or an empty string to use the
    /// theme.
    #[serde(default)]
    pub background: String,
    /// Window opacity as a percentage.
    #[serde(default = "default_opacity")]
    pub opacity: u8,
    /// Canvas page containing this terminal.
    #[serde(default = "default_page_id")]
    pub page_id: u32,
    /// Per-terminal color theme, or empty for a legacy client default.
    #[serde(default)]
    pub theme: String,
}

fn default_opacity() -> u8 {
    80
}

fn default_page_id() -> u32 {
    1
}

fn default_note_width() -> u16 {
    384
}

fn default_note_height() -> u16 {
    224
}

fn default_file_sidebar_width() -> u16 {
    332
}

impl Default for WsWinsize {
    fn default() -> Self {
        WsWinsize {
            x: 0,
            y: 0,
            rows: 24,
            cols: 80,
            width: 0,
            height: 0,
            title: String::new(),
            background: String::new(),
            opacity: default_opacity(),
            page_id: default_page_id(),
            theme: String::new(),
        }
    }
}

/// Shared state for a note placed on the infinite canvas.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WsNote {
    /// Top-left x-coordinate on the canvas.
    pub x: i32,
    /// Top-left y-coordinate on the canvas.
    pub y: i32,
    /// Width of the note in canvas pixels.
    #[serde(default = "default_note_width")]
    pub width: u16,
    /// Height of the note in canvas pixels.
    #[serde(default = "default_note_height")]
    pub height: u16,
    /// Editable note contents.
    pub text: String,
    /// Structured block contents. Empty means legacy newline-delimited text.
    #[serde(default)]
    pub paragraphs: Vec<String>,
    /// Terminal IDs associated with this note.
    #[serde(default)]
    pub linked_shell_ids: Vec<Sid>,
    /// Other note IDs associated with this note. Incoming links are derived.
    #[serde(default)]
    pub linked_note_ids: Vec<Sid>,
    /// File editor window IDs associated with this note.
    #[serde(default)]
    pub linked_file_window_ids: Vec<Sid>,
    /// Note background color as a CSS hex value.
    pub background: String,
    /// Note opacity as a percentage.
    pub opacity: u8,
    /// Canvas page containing this note.
    #[serde(default = "default_page_id")]
    pub page_id: u32,
}

/// Shared state for a filesystem browser attached to a terminal.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WsFileWindow {
    /// Terminal whose local or SSH filesystem this window browses.
    pub shell_id: Sid,
    /// Canvas page containing this window.
    pub page_id: u32,
    /// Initial directory shown when a client creates the browser view.
    pub path: String,
    /// User-visible terminal-derived label.
    pub title: String,
    /// Top-left canvas coordinates.
    pub x: i32,
    /// Top-left vertical canvas coordinate.
    pub y: i32,
    /// Canvas width.
    pub width: u16,
    /// Canvas height.
    pub height: u16,
    /// Directory currently used as the browser's working location.
    #[serde(default)]
    pub current_path: String,
    /// Expanded directory paths in the shared tree.
    #[serde(default)]
    pub expanded_paths: Vec<String>,
    /// Currently selected file or directory path.
    #[serde(default)]
    pub selected_path: String,
    /// Kind of the selected tree entry, or empty when nothing is selected.
    #[serde(default)]
    pub selected_kind: String,
    /// Shared vertical scroll offset for the filesystem tree.
    #[serde(default)]
    pub tree_scroll_top: u32,
    /// Text file whose shared editor buffer is active.
    #[serde(default)]
    pub editor_path: String,
    /// Unique AES-CTR stream used for the current encrypted editor buffer.
    #[serde(default)]
    pub editor_stream: u64,
    /// End-to-end encrypted UTF-8 editor contents.
    #[serde(default)]
    pub editor_data: Bytes,
    /// Whether the shared editor buffer differs from the file on disk.
    #[serde(default)]
    pub editor_dirty: bool,
    /// Shared width of the file-tree pane within the browser window.
    #[serde(default = "default_file_sidebar_width")]
    pub sidebar_width: u16,
    /// Change token replaced after filesystem mutations.
    #[serde(default)]
    pub tree_revision: u32,
}

/// A named page in the shared workspace.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WsPage {
    /// Stable page identifier.
    pub id: u32,
    /// User-visible page name.
    pub name: String,
}

/// Authentication behavior for a reusable SSH connection.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum WsSshAuthMethod {
    /// Let OpenSSH use its configuration and normal authentication order.
    #[default]
    Default,
    /// Prefer keys exposed by an SSH agent.
    Agent,
    /// Use a specific private-key file.
    KeyFile,
    /// Prompt for a password inside the terminal without storing it.
    Password,
}

/// Reusable SSH connection metadata. Passwords are intentionally absent.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WsSshProfile {
    /// Stable profile identifier.
    pub id: String,
    /// User-visible unique connection name.
    pub name: String,
    /// OpenSSH host name, address, or config alias.
    pub host: String,
    /// TCP port for the SSH service.
    pub port: u16,
    /// Optional remote username.
    pub username: String,
    /// Authentication method passed to OpenSSH.
    pub auth_method: WsSshAuthMethod,
    /// Private key path on the daemon host, when applicable.
    pub key_path: String,
    /// Whether OpenSSH may accept a new host key on first use.
    pub accept_new_host_key: bool,
    /// Default terminal color theme for this connection.
    #[serde(default)]
    pub theme: String,
    /// Whether the profile's background overrides its theme.
    #[serde(default)]
    pub background_enabled: bool,
    /// Default terminal background color when the override is enabled.
    #[serde(default)]
    pub background: String,
}

/// Real-time message providing information about a user.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WsUser {
    /// The user's display name.
    pub name: String,
    /// Live coordinates of the mouse cursor, if available.
    pub cursor: Option<(i32, i32)>,
    /// Canvas page containing the live cursor.
    #[serde(default = "default_page_id")]
    pub page_id: u32,
    /// Currently focused terminal window ID.
    pub focus: Option<Sid>,
    /// Whether the user has write permissions in the session.
    pub can_write: bool,
}

/// A real-time message sent from the server over WebSocket.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum WsServer {
    /// Initial message with user ID, session name, server version, and daemon
    /// version.
    Hello(Uid, String, String, String),
    /// The user's authentication was invalid.
    InvalidAuth(),
    /// A snapshot of all current users in the session.
    Users(Vec<(Uid, WsUser)>),
    /// Info about a single user in the session: joined, left, or changed.
    UserDiff(Uid, Option<WsUser>),
    /// Notification when the set of open shells has changed.
    Shells(Vec<(Sid, WsWinsize)>),
    /// Snapshot of notes on the shared canvas.
    Notes(Vec<(Sid, WsNote)>),
    /// Shared filesystem browser window layouts.
    FileWindows(Vec<(Sid, WsFileWindow)>),
    /// Snapshot of named canvas pages.
    Pages(Vec<WsPage>),
    /// Snapshot of reusable SSH connection profiles.
    SshProfiles(Vec<WsSshProfile>),
    /// Current editor of a note, or none after editing ends.
    NoteEditing(Sid, u32, Option<Uid>),
    /// Character-level live text update for a note.
    NoteText(Sid, u32, String),
    /// Character-level live structured paragraph update for a note.
    NoteParagraphs(Sid, u32, Vec<String>),
    /// Subscription results, in the form of terminal data chunks.
    Chunks(Sid, u32, bool, u64, Vec<Bytes>),
    /// Get a chat message tuple `(uid, name, text)` from the room.
    Hear(Uid, String, String),
    /// Forward a latency measurement between the server and backend shell.
    ShellLatency(u64),
    /// End-to-end encrypted filesystem response from sshxx-daemon.
    FileResponse(String, u64, Bytes),
    /// Echo back a timestamp, for the the client's own latency measurement.
    Pong(u64),
    /// Alert the client of an application error.
    Error(String),
}

/// A real-time message sent from the client over WebSocket.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum WsClient {
    /// Authenticate the user's encryption key by zeros block and write password
    /// (if provided).
    Authenticate(Bytes, Option<Bytes>),
    /// Set the name of the current user.
    SetName(String),
    /// Send real-time information about the user's cursor.
    SetCursor(u32, Option<(i32, i32)>),
    /// Set the currently focused shell.
    SetFocus(Option<(Sid, u32)>),
    /// Create a new shell.
    Create(i32, i32, u32),
    /// Create a new shell with an explicit initial terminal size.
    CreateSized(i32, i32, u16, u16, u32),
    /// Create a shell with explicit size and per-terminal color theme.
    CreateStyled(i32, i32, u16, u16, u32, String),
    /// Create a shell with exact canvas and PTY dimensions.
    CreateWindowed(i32, i32, u16, u16, u16, u16, u32, String),
    /// Create a shell using a saved SSH connection profile.
    CreateSsh(String, i32, i32, u16, u16, u32),
    /// Create an SSH shell with explicit size and per-terminal color theme.
    CreateSshStyled(String, i32, i32, u16, u16, u32, String),
    /// Create an SSH shell with exact canvas and PTY dimensions.
    CreateSshWindowed(String, i32, i32, u16, u16, u16, u16, u32, String),
    /// Create a shell using another shell as its working-directory source.
    Clone(Sid, i32, i32, u32),
    /// Clone a shell with an explicit initial terminal size.
    CloneSized(Sid, i32, i32, u16, u16, u32),
    /// Clone a shell with explicit size and per-terminal color theme.
    CloneStyled(Sid, i32, i32, u16, u16, u32, String),
    /// Clone a shell with exact canvas and PTY dimensions.
    CloneWindowed(Sid, i32, i32, u16, u16, u16, u16, u32, String),
    /// Clone a shell at an explicit local or remote working directory.
    CreateAt(Sid, String, i32, i32, u16, u16, u16, u16, u32, String),
    /// Close a specific shell.
    Close(Sid, u32),
    /// Move a shell window to a new position and focus it.
    Move(Sid, u32, Option<WsWinsize>),
    /// Create a note at a canvas position.
    CreateNote(i32, i32, u32),
    /// Create a note with an explicit initial canvas size.
    CreateNoteSized(i32, i32, u16, u16, u32),
    /// Close a note.
    CloseNote(Sid, u32),
    /// Update a note, or bring it to the front when no value is supplied.
    UpdateNote(Sid, u32, Option<WsNote>),
    /// Claim or release the editor state for a note.
    SetNoteEditing(Sid, u32, bool),
    /// Update note text immediately as the user types.
    UpdateNoteText(Sid, u32, String),
    /// Update structured note paragraphs immediately as the user types.
    UpdateNoteParagraphs(Sid, u32, Vec<String>),
    /// Open a shared filesystem browser for a terminal.
    CreateFileWindow(Sid, u32, String, String, i32, i32, u16, u16),
    /// Close a shared filesystem browser.
    CloseFileWindow(Sid, u32),
    /// Update a filesystem browser, or bring it to the top when absent.
    UpdateFileWindow(Sid, u32, Option<WsFileWindow>),
    /// Create a named canvas page.
    CreatePage(String),
    /// Rename an existing canvas page.
    RenamePage(u32, String),
    /// Create or update a reusable SSH connection profile.
    UpsertSshProfile(WsSshProfile),
    /// Delete a reusable SSH connection profile by stable ID.
    DeleteSshProfile(String),
    /// Add user data to a given shell.
    Data(Sid, u32, Bytes, u64),
    /// Upload one encrypted image chunk for a local daemon shell.
    UploadImage(Sid, u32, String, String, u64, u64, u64, Bytes, bool),
    /// End-to-end encrypted filesystem request for a terminal's host.
    FileRequest(Sid, u32, String, u64, u64, Bytes),
    /// Subscribe to a shell, starting at a given chunk index.
    Subscribe(Sid, u32, u64),
    /// Send a a chat message to the room.
    Chat(String),
    /// Send a ping to the server, for latency measurement.
    Ping(u64),
}
