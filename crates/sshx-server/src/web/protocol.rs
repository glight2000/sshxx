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
    /// User-defined title override, or an empty string to use the terminal title.
    #[serde(default)]
    pub title: String,
    /// User-defined terminal background color, or an empty string to use the theme.
    #[serde(default)]
    pub background: String,
    /// Window opacity as a percentage.
    #[serde(default = "default_opacity")]
    pub opacity: u8,
    /// Canvas page containing this terminal.
    #[serde(default = "default_page_id")]
    pub page_id: u32,
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

impl Default for WsWinsize {
    fn default() -> Self {
        WsWinsize {
            x: 0,
            y: 0,
            rows: 24,
            cols: 80,
            title: String::new(),
            background: String::new(),
            opacity: default_opacity(),
            page_id: default_page_id(),
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
    /// Note background color as a CSS hex value.
    pub background: String,
    /// Note opacity as a percentage.
    pub opacity: u8,
    /// Canvas page containing this note.
    #[serde(default = "default_page_id")]
    pub page_id: u32,
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
    /// Initial message with user ID, session name, server version, and daemon version.
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
    /// Snapshot of named canvas pages.
    Pages(Vec<WsPage>),
    /// Current editor of a note, or none after editing ends.
    NoteEditing(Sid, u32, Option<Uid>),
    /// Character-level live text update for a note.
    NoteText(Sid, u32, String),
    /// Subscription results, in the form of terminal data chunks.
    Chunks(Sid, u32, bool, u64, Vec<Bytes>),
    /// Get a chat message tuple `(uid, name, text)` from the room.
    Hear(Uid, String, String),
    /// Forward a latency measurement between the server and backend shell.
    ShellLatency(u64),
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
    /// Create a shell using another shell as its working-directory source.
    Clone(Sid, i32, i32, u32),
    /// Close a specific shell.
    Close(Sid, u32),
    /// Move a shell window to a new position and focus it.
    Move(Sid, u32, Option<WsWinsize>),
    /// Create a note at a canvas position.
    CreateNote(i32, i32, u32),
    /// Close a note.
    CloseNote(Sid, u32),
    /// Update a note, or bring it to the front when no value is supplied.
    UpdateNote(Sid, u32, Option<WsNote>),
    /// Claim or release the editor state for a note.
    SetNoteEditing(Sid, u32, bool),
    /// Update note text immediately as the user types.
    UpdateNoteText(Sid, u32, String),
    /// Create a named canvas page.
    CreatePage(String),
    /// Rename an existing canvas page.
    RenamePage(u32, String),
    /// Add user data to a given shell.
    Data(Sid, u32, Bytes, u64),
    /// Subscribe to a shell, starting at a given chunk index.
    Subscribe(Sid, u32, u64),
    /// Send a a chat message to the room.
    Chat(String),
    /// Send a ping to the server, for latency measurement.
    Ping(u64),
}
