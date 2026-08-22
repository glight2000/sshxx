//! Core logic for sshxx sessions, independent of message transport.

use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use parking_lot::{Mutex, RwLock, RwLockWriteGuard};
use sshx_core::{
    proto::{
        server_update::ServerMessage, NewShell, SequenceNumbers, SshProfile, SshProfileCollection,
        WorkspaceFileWindow, WorkspaceNote, WorkspacePage, WorkspaceShell, WorkspaceState,
    },
    IdCounter, Sid, Uid, SSH_PROFILE_FORMAT_VERSION, WORKSPACE_FORMAT_VERSION,
};
use tokio::sync::{broadcast, watch, Notify};
use tokio::time::Instant;
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream, WatchStream};
use tokio_stream::Stream;
use tracing::{debug, warn};

use crate::utils::Shutdown;
use crate::web::protocol::{
    WsFileWindow, WsNote, WsPage, WsServer, WsSshProfile, WsUser, WsWinsize,
};

mod snapshot;
mod validation;

use validation::{
    normalize_linked_shell_ids, normalize_note_canvas_links, normalize_note_paragraphs,
    proto_profile_from_ws, validate_color, validate_file_editor_total, validate_file_window,
    validate_linked_file_window_ids, validate_linked_note_ids, validate_linked_shell_ids,
    validate_note_content, validate_opacity, validate_optional_ssh_profile_id, validate_page_name,
    validate_paragraphs, validate_ssh_profile, validate_terminal_window_size, validate_theme,
    validate_title, ws_profile_from_proto,
};

/// Store a rolling buffer with at most this quantity of output, per shell.
const SHELL_STORED_BYTES: u64 = 1 << 21; // 2 MiB
const SHELL_SEND_BATCH_BYTES: usize = 256 << 10;

/// Static metadata for this session.
#[derive(Debug, Clone)]
pub struct Metadata {
    /// Used to validate that clients have the correct encryption key.
    pub encrypted_zeros: Bytes,

    /// Name of the session (human-readable).
    pub name: String,

    /// Password for write access to the session.
    pub write_password_hash: Option<Bytes>,

    /// Version of the daemon hosting the terminal processes.
    pub daemon_version: String,

    /// Optional protocol capabilities explicitly advertised by the daemon.
    pub daemon_capabilities: Vec<String>,
}

/// In-memory state for a single sshxx session.
#[derive(Debug)]
pub struct Session {
    /// Static metadata for this session.
    metadata: Metadata,

    /// In-memory state for the session.
    shells: RwLock<HashMap<Sid, State>>,

    /// Metadata for currently connected users.
    users: RwLock<HashMap<Uid, WsUser>>,

    /// Atomic counter to get new, unique IDs.
    counter: IdCounter,

    /// Timestamp of the last backend client message from an active connection.
    last_accessed: Mutex<Instant>,

    /// Watch channel source for the ordered list of open shells and sizes.
    source: watch::Sender<Vec<(Sid, WsWinsize)>>,

    /// Durable SSH profile identity for each remote shell. Profile secrets stay
    /// in the separately encrypted daemon-owned SSH profile collection.
    shell_ssh_profiles: RwLock<HashMap<Sid, String>>,

    /// Watch channel source for the ordered list of notes on the canvas.
    notes: watch::Sender<Vec<(Sid, WsNote)>>,

    /// Watch channel source for shared filesystem browser layouts.
    file_windows: watch::Sender<Vec<(Sid, WsFileWindow)>>,

    /// Watch channel source for the ordered list of named canvas pages.
    pages: watch::Sender<Vec<WsPage>>,

    /// Reusable SSH connection profiles owned and persisted by the daemon.
    ssh_profiles: watch::Sender<Vec<WsSshProfile>>,

    /// User currently editing each note. This transient state is not persisted.
    note_editors: RwLock<HashMap<Sid, Uid>>,

    /// Revision counter notifying the daemon that workspace metadata changed.
    workspace_revision: watch::Sender<u64>,

    /// Restored shells that have not yet been acknowledged by the daemon.
    pending_restored_shells: Mutex<HashSet<Sid>>,

    /// Broadcasts updates to all WebSocket clients.
    ///
    /// Every update inside this channel must be of idempotent form, since
    /// messages may arrive before or after any snapshot of the current session
    /// state. Duplicated events should remain consistent.
    broadcast: broadcast::Sender<WsServer>,

    /// Sender end of a channel that buffers messages for the client.
    update_tx: async_channel::Sender<ServerMessage>,

    /// Receiver end of a channel that buffers messages for the client.
    update_rx: async_channel::Receiver<ServerMessage>,

    /// Triggered from metadata events when an immediate snapshot is needed.
    sync_notify: Notify,

    /// Set when this session has been closed and removed.
    shutdown: Shutdown,
}

/// Internal state for each shell.
#[derive(Default, Debug)]
struct State {
    /// Sequence number, indicating how many bytes have been received.
    seqnum: u64,

    /// Terminal data chunks.
    data: Vec<Bytes>,

    /// Number of pruned data chunks before `data[0]`.
    chunk_offset: u64,

    /// Number of bytes in pruned data chunks.
    byte_offset: u64,

    /// Set when this shell is terminated.
    closed: bool,

    /// Updated when any of the above fields change.
    notify: Arc<Notify>,
}

impl Session {
    /// Construct a new session.
    pub fn new(metadata: Metadata) -> Self {
        let now = Instant::now();
        let (update_tx, update_rx) = async_channel::bounded(256);
        Session {
            metadata,
            shells: RwLock::new(HashMap::new()),
            users: RwLock::new(HashMap::new()),
            counter: IdCounter::default(),
            last_accessed: Mutex::new(now),
            source: watch::channel(Vec::new()).0,
            shell_ssh_profiles: RwLock::new(HashMap::new()),
            notes: watch::channel(Vec::new()).0,
            file_windows: watch::channel(Vec::new()).0,
            pages: watch::channel(vec![WsPage {
                id: 1,
                name: "Page 1".into(),
            }])
            .0,
            ssh_profiles: watch::channel(Vec::new()).0,
            note_editors: RwLock::new(HashMap::new()),
            workspace_revision: watch::channel(0).0,
            pending_restored_shells: Mutex::new(HashSet::new()),
            broadcast: broadcast::channel(64).0,
            update_tx,
            update_rx,
            sync_notify: Notify::new(),
            shutdown: Shutdown::new(),
        }
    }

    /// Returns the metadata for this session.
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Gives access to the ID counter for obtaining new IDs.
    pub fn counter(&self) -> &IdCounter {
        &self.counter
    }

    /// Return the sequence numbers for current shells.
    pub fn sequence_numbers(&self) -> SequenceNumbers {
        let shells = self.shells.read();
        let pending = self.pending_restored_shells.lock();
        let mut map = HashMap::with_capacity(shells.len());
        for (key, value) in &*shells {
            if !value.closed && !pending.contains(key) {
                map.insert(key.0, value.seqnum);
            }
        }
        SequenceNumbers { map }
    }

    /// Receive a notification on broadcasted message events.
    pub fn subscribe_broadcast(
        &self,
    ) -> impl Stream<Item = Result<WsServer, BroadcastStreamRecvError>> + Unpin {
        BroadcastStream::new(self.broadcast.subscribe())
    }

    /// Receive a notification every time the set of shells is changed.
    pub fn subscribe_shells(&self) -> impl Stream<Item = Vec<(Sid, WsWinsize)>> + Unpin {
        WatchStream::new(self.source.subscribe())
    }

    /// Receive a notification every time the set of notes is changed.
    pub fn subscribe_notes(&self) -> impl Stream<Item = Vec<(Sid, WsNote)>> + Unpin {
        WatchStream::new(self.notes.subscribe())
    }

    /// Receive every shared filesystem browser layout update.
    pub fn subscribe_file_windows(&self) -> impl Stream<Item = Vec<(Sid, WsFileWindow)>> + Unpin {
        WatchStream::new(self.file_windows.subscribe())
    }

    /// Receive a notification every time the named pages change.
    pub fn subscribe_pages(&self) -> impl Stream<Item = Vec<WsPage>> + Unpin {
        WatchStream::new(self.pages.subscribe())
    }

    /// Receive a notification every time reusable SSH profiles change.
    pub fn subscribe_ssh_profiles(&self) -> impl Stream<Item = Vec<WsSshProfile>> + Unpin {
        WatchStream::new(self.ssh_profiles.subscribe())
    }

    /// Restore daemon-owned SSH profiles while ignoring no individual records.
    pub fn restore_ssh_profiles(&self, collection: SshProfileCollection) -> Result<()> {
        if collection.format_version != SSH_PROFILE_FORMAT_VERSION {
            bail!(
                "unsupported SSH profile format version {}",
                collection.format_version
            );
        }
        if collection.profiles.len() > 100 {
            bail!("too many SSH profiles");
        }
        let mut profiles = Vec::with_capacity(collection.profiles.len());
        for profile in collection.profiles {
            let profile = ws_profile_from_proto(profile)?;
            validate_ssh_profile(&profile, &profiles)?;
            profiles.push(profile);
        }
        self.ssh_profiles.send_replace(profiles);
        Ok(())
    }

    /// Return the persistable representation of all SSH profiles.
    pub fn ssh_profile_collection(&self) -> SshProfileCollection {
        SshProfileCollection {
            format_version: SSH_PROFILE_FORMAT_VERSION,
            profiles: self
                .ssh_profiles
                .borrow()
                .iter()
                .cloned()
                .map(proto_profile_from_ws)
                .collect(),
        }
    }

    /// Add or update a reusable SSH connection profile.
    pub fn upsert_ssh_profile(&self, mut profile: WsSshProfile) -> Result<()> {
        profile.name = profile.name.trim().to_owned();
        profile.host = profile.host.trim().to_owned();
        profile.username = profile.username.trim().to_owned();
        let current = self.ssh_profiles.borrow().clone();
        let others = current
            .iter()
            .filter(|existing| existing.id != profile.id)
            .cloned()
            .collect::<Vec<_>>();
        validate_ssh_profile(&profile, &others)?;
        if current.len() >= 100 && !current.iter().any(|item| item.id == profile.id) {
            bail!("you can only save up to 100 SSH connections");
        }
        self.ssh_profiles.send_modify(|profiles| {
            if let Some(existing) = profiles.iter_mut().find(|item| item.id == profile.id) {
                *existing = profile.clone();
            } else {
                profiles.push(profile.clone());
            }
        });
        self.update_tx
            .try_send(ServerMessage::SshProfiles(self.ssh_profile_collection()))
            .context("failed to queue SSH profile persistence")?;
        Ok(())
    }

    /// Delete a reusable SSH connection profile by stable ID.
    pub fn delete_ssh_profile(&self, id: &str) -> Result<()> {
        let mut removed = false;
        self.ssh_profiles.send_modify(|profiles| {
            let old_len = profiles.len();
            profiles.retain(|profile| profile.id != id);
            removed = old_len != profiles.len();
        });
        if !removed {
            bail!("SSH connection does not exist");
        }
        self.update_tx
            .try_send(ServerMessage::SshProfiles(self.ssh_profile_collection()))
            .context("failed to queue SSH profile persistence")?;
        Ok(())
    }

    /// Resolve an SSH profile to the daemon transport representation.
    pub fn ssh_profile(&self, id: &str) -> Result<SshProfile> {
        self.ssh_profiles
            .borrow()
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
            .map(proto_profile_from_ws)
            .context("SSH connection does not exist")
    }

    /// Return the SSH profile identity associated with a terminal, if it is remote.
    pub fn shell_ssh_profile_id(&self, id: Sid) -> Option<String> {
        self.shell_ssh_profiles.read().get(&id).cloned()
    }

    /// Return the terminal's current canvas page for page-aware output
    /// delivery. This must be resolved for every batch because terminals can
    /// move without replacing their long-lived output subscription.
    pub fn shell_page(&self, id: Sid) -> Option<u32> {
        self.source
            .borrow()
            .iter()
            .find(|(shell_id, _)| *shell_id == id)
            .map(|(_, shell)| shell.page_id)
    }

    /// Return current note editors for initial WebSocket state.
    pub fn list_note_editors(&self) -> Vec<(Sid, u32, Uid)> {
        let notes = self.notes.borrow();
        self.note_editors
            .read()
            .iter()
            .filter_map(|(id, user_id)| {
                notes
                    .iter()
                    .find(|(note_id, _)| note_id == id)
                    .map(|(_, note)| (*id, note.page_id, *user_id))
            })
            .collect()
    }

    /// Receive a notification whenever locally persistable workspace state
    /// changes.
    pub fn subscribe_workspace(&self) -> impl Stream<Item = u64> + Unpin {
        WatchStream::new(self.workspace_revision.subscribe())
    }

    /// Return the current terminal and note metadata in portable form.
    pub fn workspace_state(&self) -> WorkspaceState {
        WorkspaceState {
            format_version: WORKSPACE_FORMAT_VERSION,
            shells: self
                .source
                .borrow()
                .iter()
                .map(|(id, shell)| WorkspaceShell {
                    id: id.0,
                    x: shell.x,
                    y: shell.y,
                    rows: shell.rows.into(),
                    cols: shell.cols.into(),
                    title: shell.title.clone(),
                    background: shell.background.clone(),
                    opacity: shell.opacity.into(),
                    page_id: shell.page_id,
                    theme: shell.theme.clone(),
                    width: shell.width.into(),
                    height: shell.height.into(),
                    ssh_profile_id: self
                        .shell_ssh_profiles
                        .read()
                        .get(id)
                        .cloned()
                        .unwrap_or_default(),
                })
                .collect(),
            notes: self
                .notes
                .borrow()
                .iter()
                .map(|(id, note)| WorkspaceNote {
                    id: id.0,
                    x: note.x,
                    y: note.y,
                    width: note.width.into(),
                    height: note.height.into(),
                    text: note.text.clone(),
                    paragraphs: note.paragraphs.clone(),
                    linked_shell_ids: note.linked_shell_ids.iter().map(|id| id.0).collect(),
                    linked_note_ids: note.linked_note_ids.iter().map(|id| id.0).collect(),
                    linked_file_window_ids: note
                        .linked_file_window_ids
                        .iter()
                        .map(|id| id.0)
                        .collect(),
                    title: note.title.clone(),
                    background: note.background.clone(),
                    opacity: note.opacity.into(),
                    page_id: note.page_id,
                })
                .collect(),
            file_windows: self
                .file_windows
                .borrow()
                .iter()
                .map(|(id, window)| WorkspaceFileWindow {
                    id: id.0,
                    shell_id: window.shell_id.0,
                    page_id: window.page_id,
                    path: window.path.clone(),
                    title: window.title.clone(),
                    background: window.background.clone(),
                    x: window.x,
                    y: window.y,
                    width: window.width.into(),
                    height: window.height.into(),
                    current_path: window.current_path.clone(),
                    expanded_paths: window.expanded_paths.clone(),
                    selected_path: window.selected_path.clone(),
                    selected_kind: window.selected_kind.clone(),
                    tree_scroll_top: window.tree_scroll_top,
                    editor_path: window.editor_path.clone(),
                    editor_stream: window.editor_stream,
                    editor_data: window.editor_data.clone(),
                    editor_dirty: window.editor_dirty,
                    sidebar_width: window.sidebar_width.into(),
                    tree_revision: window.tree_revision,
                })
                .collect(),
            pages: self
                .pages
                .borrow()
                .iter()
                .map(|page| WorkspacePage {
                    id: page.id,
                    name: page.name.clone(),
                })
                .collect(),
        }
    }

    /// Initialize this new session from a daemon-owned workspace snapshot.
    pub fn restore_workspace(&self, workspace: WorkspaceState) -> Result<Vec<NewShell>> {
        if workspace.format_version != WORKSPACE_FORMAT_VERSION {
            bail!(
                "unsupported workspace format version {}",
                workspace.format_version
            );
        }
        if workspace.shells.len() > 100
            || workspace.notes.len() > 100
            || workspace.file_windows.len() > 100
            || workspace.pages.len() > 50
        {
            bail!("workspace contains too many items");
        }

        let pages = if workspace.pages.is_empty() {
            vec![WsPage {
                id: 1,
                name: "Page 1".into(),
            }]
        } else {
            let mut page_ids = HashSet::new();
            workspace
                .pages
                .into_iter()
                .map(|page| {
                    if page.id == 0 || !page_ids.insert(page.id) {
                        bail!("workspace contains an invalid or duplicate page ID");
                    }
                    validate_page_name(&page.name)?;
                    Ok(WsPage {
                        id: page.id,
                        name: page.name,
                    })
                })
                .collect::<Result<Vec<_>>>()?
        };
        let page_ids = pages.iter().map(|page| page.id).collect::<HashSet<_>>();

        let mut ids = HashSet::new();
        let mut max_id = 0_u32;
        let mut shells = HashMap::with_capacity(workspace.shells.len());
        let mut shell_ssh_profiles = HashMap::new();
        let mut source = Vec::with_capacity(workspace.shells.len());
        let mut requests = Vec::with_capacity(workspace.shells.len());
        for shell in workspace.shells {
            if shell.id == 0 || !ids.insert(shell.id) {
                bail!("workspace contains an invalid or duplicate item ID");
            }
            validate_optional_ssh_profile_id(&shell.ssh_profile_id)
                .context("workspace contains an invalid SSH connection ID")?;
            let ssh_profile_id = shell.ssh_profile_id.clone();
            let winsize = WsWinsize {
                x: shell.x,
                y: shell.y,
                rows: shell.rows.try_into().context("terminal rows overflow")?,
                cols: shell.cols.try_into().context("terminal columns overflow")?,
                title: shell.title,
                background: shell.background,
                opacity: shell
                    .opacity
                    .try_into()
                    .context("terminal opacity overflow")?,
                page_id: if shell.page_id == 0 { 1 } else { shell.page_id },
                theme: shell.theme,
                generation: 0,
                width: shell.width.try_into().context("terminal width overflow")?,
                height: shell
                    .height
                    .try_into()
                    .context("terminal height overflow")?,
            };
            if winsize.rows == 0 || winsize.cols == 0 {
                bail!("terminal dimensions must be positive");
            }
            validate_title(&winsize.title)?;
            validate_color(&winsize.background)?;
            validate_opacity(winsize.opacity)?;
            validate_theme(&winsize.theme)?;
            validate_terminal_window_size(winsize.width, winsize.height)?;
            if !page_ids.contains(&winsize.page_id) {
                bail!("terminal references a missing page");
            }

            let id = Sid(shell.id);
            let page_id = winsize.page_id;
            let rows = winsize.rows;
            let cols = winsize.cols;
            let theme = winsize.theme.clone();
            let background = winsize.background.clone();
            let width = winsize.width;
            let height = winsize.height;
            max_id = max_id.max(shell.id);
            shells.insert(id, State::default());
            if !ssh_profile_id.is_empty() {
                shell_ssh_profiles.insert(id, ssh_profile_id.clone());
            }
            source.push((id, winsize));
            requests.push(NewShell {
                id: shell.id,
                x: shell.x,
                y: shell.y,
                source_id: None,
                page_id,
                rows: rows.into(),
                cols: cols.into(),
                ssh_profile: (!ssh_profile_id.is_empty())
                    .then(|| self.ssh_profile(&ssh_profile_id).ok())
                    .flatten(),
                theme,
                width: width.into(),
                height: height.into(),
                background,
                working_directory: String::new(),
                ssh_profile_id,
                copy_history: false,
            });
        }

        let mut notes = Vec::with_capacity(workspace.notes.len());
        for note in workspace.notes {
            if note.id == 0 || !ids.insert(note.id) {
                bail!("workspace contains an invalid or duplicate item ID");
            }
            let page_id = if note.page_id == 0 { 1 } else { note.page_id };
            let paragraphs = normalize_note_paragraphs(&note.text, note.paragraphs);
            let linked_shell_ids =
                normalize_linked_shell_ids(note.linked_shell_ids, page_id, &source);
            let note_state = WsNote {
                x: note.x,
                y: note.y,
                width: note.width.try_into().context("note width overflow")?,
                height: note.height.try_into().context("note height overflow")?,
                text: paragraphs.join("\n"),
                paragraphs,
                linked_shell_ids,
                linked_note_ids: note.linked_note_ids.into_iter().map(Sid).collect(),
                linked_file_window_ids: note.linked_file_window_ids.into_iter().map(Sid).collect(),
                title: note.title,
                background: note.background,
                opacity: note.opacity.try_into().context("note opacity overflow")?,
                page_id,
            };
            validate_note_content(&note_state)?;
            if !(240..=2_000).contains(&note_state.width)
                || !(160..=2_000).contains(&note_state.height)
            {
                bail!("note dimensions are out of range");
            }
            validate_color(&note_state.background)?;
            validate_opacity(note_state.opacity)?;
            if !page_ids.contains(&note_state.page_id) {
                bail!("note references a missing page");
            }

            max_id = max_id.max(note.id);
            notes.push((Sid(note.id), note_state));
        }

        let mut file_windows = Vec::with_capacity(workspace.file_windows.len());
        for window in workspace.file_windows {
            if window.id == 0 || !ids.insert(window.id) {
                bail!("workspace contains an invalid or duplicate item ID");
            }
            let state = WsFileWindow {
                shell_id: Sid(window.shell_id),
                page_id: if window.page_id == 0 {
                    1
                } else {
                    window.page_id
                },
                path: window.path,
                title: window.title,
                background: if window.background.is_empty() {
                    "#111113".into()
                } else {
                    window.background
                },
                x: window.x,
                y: window.y,
                width: window
                    .width
                    .try_into()
                    .context("file browser width overflow")?,
                height: window
                    .height
                    .try_into()
                    .context("file browser height overflow")?,
                current_path: window.current_path,
                expanded_paths: window.expanded_paths,
                selected_path: window.selected_path,
                selected_kind: window.selected_kind,
                tree_scroll_top: window.tree_scroll_top,
                editor_path: window.editor_path,
                editor_stream: window.editor_stream,
                editor_data: window.editor_data,
                editor_dirty: window.editor_dirty,
                sidebar_width: if window.sidebar_width == 0 {
                    332
                } else {
                    window
                        .sidebar_width
                        .try_into()
                        .context("file browser sidebar width overflow")?
                },
                tree_revision: window.tree_revision,
            };
            validate_file_window(&state)?;
            if !page_ids.contains(&state.page_id) {
                bail!("file browser references a missing page");
            }
            if !source.iter().any(|(id, _)| *id == state.shell_id) {
                bail!("file browser references a missing terminal");
            }
            max_id = max_id.max(window.id);
            file_windows.push((Sid(window.id), state));
        }
        validate_file_editor_total(&file_windows)?;
        normalize_note_canvas_links(&mut notes, &file_windows);

        let next_id = max_id
            .checked_add(1)
            .context("workspace item ID overflow")?;
        *self.shells.write() = shells;
        *self.shell_ssh_profiles.write() = shell_ssh_profiles;
        self.source.send_replace(source);
        self.notes.send_replace(notes);
        self.file_windows.send_replace(file_windows);
        self.pages.send_replace(pages);
        *self.pending_restored_shells.lock() = requests
            .iter()
            .map(|shell| Sid(shell.id))
            .collect::<HashSet<_>>();
        self.counter.set_current_values(Sid(next_id.max(1)), Uid(1));
        Ok(requests)
    }

    /// Number of active terminal windows in the session.
    pub fn shell_count(&self) -> usize {
        self.source.borrow().len()
    }

    /// Number of notes in the session.
    pub fn note_count(&self) -> usize {
        self.notes.borrow().len()
    }

    /// Number of shared filesystem browser windows in the session.
    pub fn file_window_count(&self) -> usize {
        self.file_windows.borrow().len()
    }

    /// Create a named canvas page and return its stable identifier.
    pub fn create_page(&self, requested_name: String) -> Result<u32> {
        let mut created_id = 0;
        self.pages.send_modify(|pages| {
            if pages.len() >= 50 {
                return;
            }
            created_id = pages
                .iter()
                .map(|page| page.id)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            let name = if requested_name.trim().is_empty() {
                format!("Page {}", pages.len() + 1)
            } else {
                requested_name.trim().to_owned()
            };
            if created_id != 0 && validate_page_name(&name).is_ok() {
                pages.push(WsPage {
                    id: created_id,
                    name,
                });
            } else {
                created_id = 0;
            }
        });
        if created_id == 0 {
            bail!("cannot create page");
        }
        self.workspace_changed();
        Ok(created_id)
    }

    /// Rename an existing canvas page.
    pub fn rename_page(&self, id: u32, name: String) -> Result<()> {
        let name = name.trim();
        validate_page_name(name)?;
        let mut found = false;
        self.pages.send_modify(|pages| {
            if let Some(page) = pages.iter_mut().find(|page| page.id == id) {
                page.name = name.to_owned();
                found = true;
            }
        });
        if !found {
            bail!("cannot rename missing page");
        }
        self.workspace_changed();
        Ok(())
    }

    /// Return whether a page ID currently exists.
    pub fn page_exists(&self, id: u32) -> bool {
        self.pages.borrow().iter().any(|page| page.id == id)
    }

    /// Ensure a terminal exists on the page claimed by a browser event.
    pub fn check_shell_page(&self, id: Sid, page_id: u32) -> Result<()> {
        match self
            .source
            .borrow()
            .iter()
            .find(|(shell_id, _)| *shell_id == id)
        {
            Some((_, shell)) if shell.page_id == page_id => Ok(()),
            Some(_) => bail!("terminal does not belong to the active page"),
            None => bail!("terminal with id={id} does not exist"),
        }
    }

    /// Ensure a terminal exists, regardless of which canvas page presents a
    /// component explicitly linked to it.
    pub fn check_shell_exists(&self, id: Sid) -> Result<()> {
        if self
            .source
            .borrow()
            .iter()
            .any(|(shell_id, _)| *shell_id == id)
        {
            Ok(())
        } else {
            bail!("terminal with id={id} does not exist")
        }
    }

    /// Ensure a note exists on the page claimed by a browser event.
    pub fn check_note_page(&self, id: Sid, page_id: u32) -> Result<()> {
        match self
            .notes
            .borrow()
            .iter()
            .find(|(note_id, _)| *note_id == id)
        {
            Some((_, note)) if note.page_id == page_id => Ok(()),
            Some(_) => bail!("note does not belong to the active page"),
            None => bail!("note with id={id} does not exist"),
        }
    }

    /// Ensure a filesystem browser exists on the page claimed by an event.
    pub fn check_file_window_page(&self, id: Sid, page_id: u32) -> Result<()> {
        match self
            .file_windows
            .borrow()
            .iter()
            .find(|(window_id, _)| *window_id == id)
        {
            Some((_, window)) if window.page_id == page_id => Ok(()),
            Some(_) => bail!("file browser does not belong to the active page"),
            None => bail!("file browser with id={id} does not exist"),
        }
    }

    /// Subscribe for chunks from a shell, until it is closed.
    pub fn subscribe_chunks(
        &self,
        id: Sid,
        generation: u32,
        mut chunknum: u64,
    ) -> impl Stream<Item = (bool, u64, Vec<Bytes>)> + '_ {
        async_stream::stream! {
            let mut replay = true;
            while !self.shutdown.is_terminated() {
                if self.shell_generation(id) != Some(generation) {
                    return;
                }
                // We absolutely cannot hold `shells` across an await point,
                // since that would cause deadlocks.
                let (seqnum, chunks, notified, caught_up) = {
                    let shells = self.shells.read();
                    let shell = match shells.get(&id) {
                        Some(shell) if !shell.closed => shell,
                        _ => return,
                    };
                    let notify = Arc::clone(&shell.notify);
                    let notified = async move { notify.notified().await };
                    let mut seqnum = shell.byte_offset;
                    let mut chunks = Vec::new();
                    let current_chunks = shell.chunk_offset + shell.data.len() as u64;
                    if chunknum < current_chunks {
                        let start = chunknum.saturating_sub(shell.chunk_offset) as usize;
                        seqnum += shell.data[..start].iter().map(|x| x.len() as u64).sum::<u64>();
                        let mut end = start;
                        let mut batch_bytes = 0usize;
                        while end < shell.data.len() {
                            let chunk_bytes = shell.data[end].len();
                            if end > start
                                && batch_bytes.saturating_add(chunk_bytes)
                                    > SHELL_SEND_BATCH_BYTES
                            {
                                break;
                            }
                            batch_bytes = batch_bytes.saturating_add(chunk_bytes);
                            end += 1;
                        }
                        chunks = shell.data[start..end].to_vec();
                        chunknum = shell.chunk_offset + end as u64;
                    }
                    (seqnum, chunks, notified, chunknum >= current_chunks)
                };

                if !chunks.is_empty() {
                    yield (replay, seqnum, chunks);
                    if caught_up {
                        replay = false;
                    }
                    // Re-read the current chunk boundary before waiting. A
                    // renderer acknowledgement can race with new PTY output,
                    // and notify_waiters intentionally stores no permit when
                    // this future has not yet been polled.
                    continue;
                }
                replay = false;
                tokio::select! {
                    _ = notified => (),
                    _ = self.terminated() => return,
                }
            }
        }
    }

    pub(crate) fn shell_generation(&self, id: Sid) -> Option<u32> {
        self.source
            .borrow()
            .iter()
            .find(|(shell_id, _)| *shell_id == id)
            .map(|(_, shell)| shell.generation)
    }

    /// Reset volatile output state while preserving the window and its SSH
    /// profile. This is used only when a persisted SSH terminal is recreated
    /// after terminal-host state loss.
    pub fn restart_shell(&self, id: Sid) -> Result<()> {
        let mut found = false;
        self.source.send_modify(|source| {
            if let Some((_, shell)) = source.iter_mut().find(|(shell_id, _)| *shell_id == id) {
                shell.generation = shell.generation.wrapping_add(1);
                found = true;
            }
        });
        if !found {
            bail!("cannot restart shell with id={id}, layout does not exist");
        }
        {
            let mut shell = self.get_shell_mut(id)?;
            shell.notify.notify_waiters();
            *shell = State::default();
        }
        self.sync_now();
        Ok(())
    }

    /// Add a new shell to the session.
    pub fn add_shell(
        &self,
        id: Sid,
        center: (i32, i32),
        page_id: u32,
        requested_terminal_size: (u16, u16),
        requested_window_size: (u16, u16),
        requested_style: (String, String, String),
    ) -> Result<Option<WsWinsize>> {
        use std::collections::hash_map::Entry::*;
        let restored = self.pending_restored_shells.lock().remove(&id);
        if restored {
            let winsize = self
                .source
                .borrow()
                .iter()
                .find(|(shell_id, _)| *shell_id == id)
                .map(|(_, winsize)| winsize.clone())
                .context("restored shell layout is missing")?;
            self.sync_now();
            return Ok(Some(winsize));
        }

        if !self.page_exists(page_id) {
            bail!("cannot add terminal to missing page");
        }
        let (requested_rows, requested_cols) = requested_terminal_size;
        let (requested_width, requested_height) = requested_window_size;
        let (requested_theme, requested_background, ssh_profile_id) = requested_style;
        let rows = if requested_rows == 0 {
            24
        } else {
            requested_rows
        };
        let cols = if requested_cols == 0 {
            80
        } else {
            requested_cols
        };
        if !(8..=500).contains(&rows) || !(32..=500).contains(&cols) {
            bail!("terminal dimensions are out of range");
        }
        if (requested_width != 0 && !(240..=4_000).contains(&requested_width))
            || (requested_height != 0 && !(160..=4_000).contains(&requested_height))
        {
            bail!("terminal window dimensions are out of range");
        }
        validate_theme(&requested_theme)?;
        validate_color(&requested_background)?;
        validate_optional_ssh_profile_id(&ssh_profile_id)?;

        let _guard = match self.shells.write().entry(id) {
            Occupied(_) => bail!("shell already exists with id={id}"),
            Vacant(v) => v.insert(State::default()),
        };
        if !ssh_profile_id.is_empty() {
            self.shell_ssh_profiles.write().insert(id, ssh_profile_id);
        }
        self.source.send_modify(|source| {
            let winsize = WsWinsize {
                x: center.0,
                y: center.1,
                page_id,
                rows,
                cols,
                width: requested_width,
                height: requested_height,
                theme: requested_theme,
                background: requested_background,
                generation: 0,
                ..Default::default()
            };
            source.push((id, winsize));
        });
        self.workspace_changed();
        Ok(None)
    }

    /// Terminates an existing shell.
    pub fn close_shell(&self, id: Sid) -> Result<()> {
        match self.shells.write().get_mut(&id) {
            Some(shell) if !shell.closed => {
                shell.closed = true;
                shell.notify.notify_waiters();
            }
            Some(_) => return Ok(()),
            None => bail!("cannot close shell with id={id}, does not exist"),
        }
        self.source.send_modify(|source| {
            source.retain(|(x, _)| *x != id);
        });
        let removed_file_window_ids = self
            .file_windows
            .borrow()
            .iter()
            .filter(|(_, window)| window.shell_id == id)
            .map(|(window_id, _)| *window_id)
            .collect::<HashSet<_>>();
        self.file_windows.send_modify(|windows| {
            windows.retain(|(_, window)| window.shell_id != id);
        });
        self.notes.send_modify(|notes| {
            for (_, note) in notes {
                note.linked_shell_ids.retain(|shell_id| *shell_id != id);
                note.linked_file_window_ids
                    .retain(|window_id| !removed_file_window_ids.contains(window_id));
            }
        });
        self.pending_restored_shells.lock().remove(&id);
        self.shell_ssh_profiles.write().remove(&id);
        self.workspace_changed();
        Ok(())
    }

    fn get_shell_mut(&self, id: Sid) -> Result<impl DerefMut<Target = State> + '_> {
        let shells = self.shells.write();
        match shells.get(&id) {
            Some(shell) if !shell.closed => {
                Ok(RwLockWriteGuard::map(shells, |s| s.get_mut(&id).unwrap()))
            }
            Some(_) => bail!("cannot update shell with id={id}, already closed"),
            None => bail!("cannot update shell with id={id}, does not exist"),
        }
    }

    /// Change the size of a terminal, notifying clients if necessary.
    pub fn move_shell(&self, id: Sid, page_id: u32, winsize: Option<WsWinsize>) -> Result<()> {
        self.check_shell_page(id, page_id)?;
        let _guard = self.get_shell_mut(id)?; // Ensures mutual exclusion.
        if let Some(winsize) = &winsize {
            validate_title(&winsize.title)?;
            validate_color(&winsize.background)?;
            validate_opacity(winsize.opacity)?;
            validate_theme(&winsize.theme)?;
            validate_terminal_window_size(winsize.width, winsize.height)?;
            if winsize.page_id != page_id {
                bail!("terminal update cannot move between pages");
            }
        }
        self.source.send_modify(|source| {
            if let Some(idx) = source.iter().position(|(sid, _)| *sid == id) {
                let (_, oldsize) = source.remove(idx);
                source.push((id, winsize.unwrap_or(oldsize)));
            }
        });
        self.workspace_changed();
        Ok(())
    }

    /// Move an explicitly selected set of canvas items to another page.
    ///
    /// Every source relationship is validated before any watch channel is
    /// mutated, so a stale or malformed client cannot leave a partial move.
    pub fn move_canvas_items(
        &self,
        source_page_id: u32,
        target_page_id: u32,
        terminals: Vec<(Sid, i32, i32)>,
        notes: Vec<(Sid, i32, i32)>,
        file_windows: Vec<(Sid, i32, i32)>,
    ) -> Result<()> {
        if source_page_id == target_page_id {
            bail!("canvas items are already on the target page");
        }
        if !self.page_exists(target_page_id) {
            bail!("cannot move canvas items to a missing page");
        }
        let item_count = terminals.len() + notes.len() + file_windows.len();
        if item_count == 0 || item_count > 300 {
            bail!("canvas page move contains an invalid number of items");
        }

        let terminal_ids = terminals
            .iter()
            .map(|(id, _, _)| *id)
            .collect::<HashSet<_>>();
        let note_ids = notes.iter().map(|(id, _, _)| *id).collect::<HashSet<_>>();
        let file_window_ids = file_windows
            .iter()
            .map(|(id, _, _)| *id)
            .collect::<HashSet<_>>();
        if terminal_ids.len() != terminals.len()
            || note_ids.len() != notes.len()
            || file_window_ids.len() != file_windows.len()
        {
            bail!("canvas page move contains duplicate items");
        }
        for id in &terminal_ids {
            self.check_shell_page(*id, source_page_id)?;
        }
        for id in &note_ids {
            self.check_note_page(*id, source_page_id)?;
        }
        for id in &file_window_ids {
            self.check_file_window_page(*id, source_page_id)?;
        }

        let terminal_positions = terminals
            .into_iter()
            .map(|(id, x, y)| (id, (x, y)))
            .collect::<HashMap<_, _>>();
        let note_positions = notes
            .into_iter()
            .map(|(id, x, y)| (id, (x, y)))
            .collect::<HashMap<_, _>>();
        let file_positions = file_windows
            .into_iter()
            .map(|(id, x, y)| (id, (x, y)))
            .collect::<HashMap<_, _>>();

        self.source.send_modify(|items| {
            for (id, state) in items {
                if let Some(&(x, y)) = terminal_positions.get(id) {
                    state.x = x;
                    state.y = y;
                    state.page_id = target_page_id;
                }
            }
        });
        self.notes.send_modify(|items| {
            for (id, state) in items {
                if let Some(&(x, y)) = note_positions.get(id) {
                    state.x = x;
                    state.y = y;
                    state.page_id = target_page_id;
                }
            }
        });
        self.file_windows.send_modify(|items| {
            for (id, state) in items {
                if let Some(&(x, y)) = file_positions.get(id) {
                    state.x = x;
                    state.y = y;
                    state.page_id = target_page_id;
                }
            }
        });
        for id in note_ids {
            if let Some(editor) = self.note_editors.read().get(&id).copied() {
                self.broadcast
                    .send(WsServer::NoteEditing(id, target_page_id, Some(editor)))
                    .ok();
            }
        }
        self.workspace_changed();
        Ok(())
    }

    /// Add a new note to the canvas.
    pub fn add_note(
        &self,
        id: Sid,
        position: (i32, i32),
        page_id: u32,
        requested_size: Option<(u16, u16)>,
    ) -> Result<()> {
        if !self.page_exists(page_id) {
            bail!("cannot add note to missing page");
        }
        let (width, height) = requested_size.unwrap_or((384, 224));
        if !(240..=2_000).contains(&width) || !(160..=2_000).contains(&height) {
            bail!("note dimensions are out of range");
        }
        self.notes.send_modify(|notes| {
            notes.push((
                id,
                WsNote {
                    x: position.0,
                    y: position.1,
                    width,
                    height,
                    text: String::new(),
                    paragraphs: vec![String::new()],
                    linked_shell_ids: Vec::new(),
                    linked_note_ids: Vec::new(),
                    linked_file_window_ids: Vec::new(),
                    title: String::new(),
                    background: "#3f3f46".into(),
                    opacity: 80,
                    page_id,
                },
            ));
        });
        self.workspace_changed();
        Ok(())
    }

    /// Close a note on the canvas.
    pub fn close_note(&self, id: Sid, page_id: u32) -> Result<()> {
        self.check_note_page(id, page_id)?;
        let mut found = false;
        self.notes.send_modify(|notes| {
            let len = notes.len();
            notes.retain(|(note_id, _)| *note_id != id);
            for (_, note) in notes.iter_mut() {
                note.linked_note_ids.retain(|note_id| *note_id != id);
            }
            found = notes.len() != len;
        });
        if !found {
            bail!("cannot close note with id={id}, does not exist");
        }
        if self.note_editors.write().remove(&id).is_some() {
            self.broadcast
                .send(WsServer::NoteEditing(id, page_id, None))
                .ok();
        }
        self.workspace_changed();
        Ok(())
    }

    /// Update a note, or move it to the top of the stacking order.
    pub fn update_note(&self, id: Sid, page_id: u32, mut note: Option<WsNote>) -> Result<()> {
        self.check_note_page(id, page_id)?;
        if let Some(note) = &mut note {
            note.paragraphs =
                normalize_note_paragraphs(&note.text, std::mem::take(&mut note.paragraphs));
            note.text = note.paragraphs.join("\n");
            validate_note_content(note)?;
            validate_color(&note.background)?;
            validate_opacity(note.opacity)?;
            if note.page_id != page_id {
                bail!("note update cannot move between pages");
            }
            if !(240..=2_000).contains(&note.width) || !(160..=2_000).contains(&note.height) {
                bail!("note dimensions are out of range");
            }
            validate_linked_shell_ids(&note.linked_shell_ids, page_id, &self.source.borrow())?;
            validate_linked_note_ids(&note.linked_note_ids, id, page_id, &self.notes.borrow())?;
            validate_linked_file_window_ids(
                &note.linked_file_window_ids,
                page_id,
                &self.file_windows.borrow(),
            )?;
        }
        let preserve_live_text = self.note_editors.read().contains_key(&id);
        let mut found = false;
        self.notes.send_modify(|notes| {
            if let Some(idx) = notes.iter().position(|(note_id, _)| *note_id == id) {
                let (_, old_note) = notes.remove(idx);
                let mut next_note = note.unwrap_or_else(|| old_note.clone());
                if preserve_live_text {
                    next_note.text = old_note.text;
                    next_note.paragraphs = old_note.paragraphs;
                }
                notes.push((id, next_note));
                found = true;
            }
        });
        if !found {
            bail!("cannot update note with id={id}, does not exist");
        }
        self.workspace_changed();
        Ok(())
    }

    /// Open one shared filesystem browser per terminal, or bring the existing
    /// one forward.
    #[allow(clippy::too_many_arguments)]
    pub fn open_file_window(
        &self,
        id: Sid,
        shell_id: Sid,
        page_id: u32,
        path: String,
        title: String,
        x: i32,
        y: i32,
        width: u16,
        height: u16,
    ) -> Result<()> {
        self.check_shell_page(shell_id, page_id)?;
        {
            let windows = self.file_windows.borrow();
            let existing = windows
                .iter()
                .any(|(_, window)| window.shell_id == shell_id && window.page_id == page_id);
            if !existing && windows.len() >= 100 {
                bail!("you can only create up to 100 file browsers");
            }
        }
        let window = WsFileWindow {
            shell_id,
            page_id,
            path,
            title,
            background: "#111113".into(),
            x,
            y,
            width,
            height,
            current_path: String::new(),
            expanded_paths: Vec::new(),
            selected_path: String::new(),
            selected_kind: String::new(),
            tree_scroll_top: 0,
            editor_path: String::new(),
            editor_stream: 0,
            editor_data: Bytes::new(),
            editor_dirty: false,
            sidebar_width: 332,
            tree_revision: 0,
        };
        validate_file_window(&window)?;
        self.file_windows.send_modify(|windows| {
            if let Some(index) = windows.iter().position(|(_, existing)| {
                existing.shell_id == shell_id && existing.page_id == page_id
            }) {
                let existing = windows.remove(index);
                windows.push(existing);
            } else {
                windows.push((id, window));
            }
        });
        self.workspace_changed();
        Ok(())
    }

    /// Close a shared filesystem browser window.
    pub fn close_file_window(&self, id: Sid, page_id: u32) -> Result<()> {
        self.check_file_window_page(id, page_id)?;
        self.file_windows.send_modify(|windows| {
            windows.retain(|(window_id, _)| *window_id != id);
        });
        self.notes.send_modify(|notes| {
            for (_, note) in notes {
                note.linked_file_window_ids
                    .retain(|window_id| *window_id != id);
            }
        });
        self.workspace_changed();
        Ok(())
    }

    /// Update a filesystem browser layout, or bring it to the stacking front.
    pub fn update_file_window(
        &self,
        id: Sid,
        page_id: u32,
        window: Option<WsFileWindow>,
    ) -> Result<()> {
        self.check_file_window_page(id, page_id)?;
        if let Some(window) = &window {
            validate_file_window(window)?;
            if window.page_id != page_id {
                bail!("file browser update cannot move between pages");
            }
            self.check_shell_exists(window.shell_id)?;
            let windows = self.file_windows.borrow();
            let projected = windows
                .iter()
                .filter(|(window_id, _)| *window_id != id)
                .map(|(_, state)| state.editor_data.len())
                .sum::<usize>()
                .saturating_add(window.editor_data.len());
            if projected > 48 << 20 {
                bail!("shared file editor buffers exceed the session limit");
            }
        }
        let mut found = false;
        self.file_windows.send_modify(|windows| {
            if let Some(index) = windows.iter().position(|(window_id, _)| *window_id == id) {
                let (_, old_window) = windows.remove(index);
                if window
                    .as_ref()
                    .is_some_and(|next| next.shell_id != old_window.shell_id)
                {
                    windows.insert(index, (id, old_window));
                    return;
                }
                windows.push((id, window.unwrap_or(old_window)));
                found = true;
            }
        });
        if !found {
            bail!("cannot update file browser with id={id}");
        }
        self.workspace_changed();
        Ok(())
    }

    /// Claim or release exclusive live editing state for a note.
    pub fn set_note_editing(
        &self,
        id: Sid,
        page_id: u32,
        user_id: Uid,
        editing: bool,
    ) -> Result<()> {
        self.check_note_page(id, page_id)?;
        let update = {
            let mut editors = self.note_editors.write();
            if editing {
                match editors.get(&id) {
                    Some(owner) if *owner != user_id => bail!("note is already being edited"),
                    Some(_) => return Ok(()),
                    None => {
                        editors.insert(id, user_id);
                        Some(user_id)
                    }
                }
            } else {
                match editors.get(&id) {
                    Some(owner) if *owner == user_id => {
                        editors.remove(&id);
                        None
                    }
                    Some(_) => bail!("note is being edited by another user"),
                    None => return Ok(()),
                }
            }
        };
        self.broadcast
            .send(WsServer::NoteEditing(id, page_id, update))
            .ok();
        Ok(())
    }

    /// Apply a live note text update from its current editor.
    pub fn update_note_text(
        &self,
        id: Sid,
        page_id: u32,
        user_id: Uid,
        text: String,
    ) -> Result<()> {
        self.check_note_page(id, page_id)?;
        if text.len() > 10_000 {
            bail!("note text is too long");
        }
        if self.note_editors.read().get(&id) != Some(&user_id) {
            bail!("note text can only be changed by its current editor");
        }
        let mut found = false;
        let paragraphs = normalize_note_paragraphs(&text, Vec::new());
        self.notes.send_if_modified(|notes| {
            if let Some((_, note)) = notes.iter_mut().find(|(note_id, _)| *note_id == id) {
                note.text = text.clone();
                note.paragraphs = paragraphs.clone();
                found = true;
            }
            false
        });
        if !found {
            bail!("cannot update note with id={id}, does not exist");
        }
        self.broadcast
            .send(WsServer::NoteText(id, page_id, text))
            .ok();
        self.workspace_changed_deferred();
        Ok(())
    }

    /// Apply a live structured paragraph update from the note's current editor.
    pub fn update_note_paragraphs(
        &self,
        id: Sid,
        page_id: u32,
        user_id: Uid,
        paragraphs: Vec<String>,
    ) -> Result<()> {
        self.check_note_page(id, page_id)?;
        let paragraphs = normalize_note_paragraphs("", paragraphs);
        validate_paragraphs(&paragraphs)?;
        if self.note_editors.read().get(&id) != Some(&user_id) {
            bail!("note paragraphs can only be changed by its current editor");
        }
        let text = paragraphs.join("\n");
        let mut found = false;
        self.notes.send_if_modified(|notes| {
            if let Some((_, note)) = notes.iter_mut().find(|(note_id, _)| *note_id == id) {
                note.text = text;
                note.paragraphs = paragraphs.clone();
                found = true;
            }
            false
        });
        if !found {
            bail!("cannot update note with id={id}, does not exist");
        }
        self.broadcast
            .send(WsServer::NoteParagraphs(id, page_id, paragraphs))
            .ok();
        self.workspace_changed_deferred();
        Ok(())
    }

    /// Receive new data into the session.
    pub fn add_data(&self, id: Sid, data: Bytes, seq: u64) -> Result<()> {
        let mut shell = self.get_shell_mut(id)?;

        if seq <= shell.seqnum && seq + data.len() as u64 > shell.seqnum {
            let start = shell.seqnum - seq;
            let segment = data.slice(start as usize..);
            debug!(%id, bytes = segment.len(), "adding data to shell");
            shell.seqnum += segment.len() as u64;
            shell.data.push(segment);

            // Prune old chunks if we've exceeded the maximum stored bytes.
            let mut stored_bytes = shell.seqnum - shell.byte_offset;
            if stored_bytes > SHELL_STORED_BYTES {
                let mut offset = 0;
                while offset < shell.data.len() && stored_bytes > SHELL_STORED_BYTES {
                    let bytes = shell.data[offset].len() as u64;
                    stored_bytes -= bytes;
                    shell.chunk_offset += 1;
                    shell.byte_offset += bytes;
                    offset += 1;
                }
                shell.data.drain(..offset);
            }

            shell.notify.notify_waiters();
        }

        Ok(())
    }

    /// List all the users in the session.
    pub fn list_users(&self) -> Vec<(Uid, WsUser)> {
        self.users
            .read()
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect()
    }

    /// Update a user in place by ID, applying a callback to the object.
    pub fn update_user(&self, id: Uid, f: impl FnOnce(&mut WsUser)) -> Result<()> {
        let updated_user = {
            let mut users = self.users.write();
            let user = users.get_mut(&id).context("user not found")?;
            f(user);
            user.clone()
        };
        self.broadcast
            .send(WsServer::UserDiff(id, Some(updated_user)))
            .ok();
        Ok(())
    }

    /// Add a new user, and return a guard that removes the user when dropped.
    pub fn user_scope(&self, id: Uid, can_write: bool) -> Result<impl Drop + '_> {
        use std::collections::hash_map::Entry::*;

        #[must_use]
        struct UserGuard<'a>(&'a Session, Uid);
        impl Drop for UserGuard<'_> {
            fn drop(&mut self) {
                self.0.remove_user(self.1);
            }
        }

        match self.users.write().entry(id) {
            Occupied(_) => bail!("user already exists with id={id}"),
            Vacant(v) => {
                let user = WsUser {
                    name: format!("User {id}"),
                    cursor: None,
                    page_id: 1,
                    focus: None,
                    can_write,
                };
                v.insert(user.clone());
                self.broadcast.send(WsServer::UserDiff(id, Some(user))).ok();
                Ok(UserGuard(self, id))
            }
        }
    }

    /// Remove an existing user.
    fn remove_user(&self, id: Uid) {
        if self.users.write().remove(&id).is_none() {
            warn!(%id, "invariant violation: removed user that does not exist");
        }
        let released = {
            let mut editors = self.note_editors.write();
            let released = editors
                .iter()
                .filter_map(|(note_id, owner)| (*owner == id).then_some(*note_id))
                .collect::<Vec<_>>();
            editors.retain(|_, owner| *owner != id);
            released
        };
        for note_id in released {
            if let Some((_, note)) = self.notes.borrow().iter().find(|(id, _)| *id == note_id) {
                self.broadcast
                    .send(WsServer::NoteEditing(note_id, note.page_id, None))
                    .ok();
            }
        }
        self.broadcast.send(WsServer::UserDiff(id, None)).ok();
    }

    /// Check if a user has write permission in the session.
    pub fn check_write_permission(&self, user_id: Uid) -> Result<()> {
        let users = self.users.read();
        let user = users.get(&user_id).context("user not found")?;
        if !user.can_write {
            bail!("No write permission");
        }
        Ok(())
    }

    /// Send a chat message into the room.
    pub fn send_chat(&self, id: Uid, msg: &str) -> Result<()> {
        // Populate the message with the current name in case it's not known later.
        let name = {
            let users = self.users.read();
            users.get(&id).context("user not found")?.name.clone()
        };
        self.broadcast
            .send(WsServer::Hear(id, name, msg.into()))
            .ok();
        Ok(())
    }

    /// Send a measurement of the shell latency.
    pub fn send_latency_measurement(&self, latency: u64) {
        self.broadcast.send(WsServer::ShellLatency(latency)).ok();
    }

    /// Forward an opaque, end-to-end encrypted filesystem response.
    pub fn send_file_response(&self, request_id: String, stream_num: u64, data: Bytes) {
        self.broadcast
            .send(WsServer::FileResponse(request_id, stream_num, data))
            .ok();
    }

    /// Broadcast a correlated lifecycle result. Each WebSocket forwards it
    /// only when it owns the matching pending request ID.
    pub fn send_system_action_response(
        &self,
        request_id: String,
        action: String,
        ok: bool,
        message: String,
    ) {
        self.broadcast
            .send(WsServer::SystemActionResult(
                request_id, action, ok, message,
            ))
            .ok();
    }

    /// Show a daemon-side operational error to connected browser clients.
    pub fn send_error(&self, message: String) {
        self.broadcast.send(WsServer::Error(message)).ok();
    }

    /// Register a backend client heartbeat, refreshing the timestamp.
    pub fn access(&self) {
        *self.last_accessed.lock() = Instant::now();
    }

    /// Returns the timestamp of the last backend client activity.
    pub fn last_accessed(&self) -> Instant {
        *self.last_accessed.lock()
    }

    /// Access the sender of the client message channel for this session.
    pub fn update_tx(&self) -> &async_channel::Sender<ServerMessage> {
        &self.update_tx
    }

    /// Access the receiver of the client message channel for this session.
    pub fn update_rx(&self) -> &async_channel::Receiver<ServerMessage> {
        &self.update_rx
    }

    /// Mark the session as requiring an immediate storage sync.
    ///
    /// This is needed for consistency when creating new shells, removing old
    /// shells, or updating the ID counter. If these operations are lost in a
    /// server restart, then the snapshot that contains them would be invalid
    /// compared to the current backend client state.
    ///
    /// Note that it is not necessary to do this all the time though, since that
    /// would put too much pressure on the database. Lost terminal data is
    /// already re-synchronized periodically.
    pub fn sync_now(&self) {
        self.sync_notify.notify_one();
    }

    fn workspace_changed(&self) {
        self.workspace_changed_deferred();
        self.sync_now();
    }

    fn workspace_changed_deferred(&self) {
        self.workspace_revision
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    /// Resolves when the session has been marked for an immediate sync.
    pub async fn sync_now_wait(&self) {
        self.sync_notify.notified().await
    }

    /// Send a termination signal to exit this session.
    pub fn shutdown(&self) {
        self.shutdown.shutdown()
    }

    /// Resolves when the session has received a shutdown signal.
    pub async fn terminated(&self) {
        self.shutdown.wait().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    fn session() -> Session {
        Session::new(Metadata {
            encrypted_zeros: Bytes::new(),
            name: "test".into(),
            write_password_hash: None,
            daemon_version: "test".into(),
            daemon_capabilities: Vec::new(),
        })
    }

    #[test]
    fn restart_shell_resets_output_but_preserves_layout_and_ssh_profile() {
        let session = session();
        let id = Sid(7);
        session
            .add_shell(
                id,
                (120, 240),
                1,
                (24, 80),
                (640, 480),
                ("sshxx-dark".into(), String::new(), "profile-1".into()),
            )
            .unwrap();
        session
            .add_data(id, Bytes::from_static(b"old output"), 0)
            .unwrap();

        session.restart_shell(id).unwrap();

        let source = session.source.borrow();
        let (_, window) = source.iter().find(|(shell_id, _)| *shell_id == id).unwrap();
        assert_eq!(window.generation, 1);
        assert_eq!((window.x, window.y), (120, 240));
        drop(source);
        assert_eq!(
            session.workspace_state().shells[0].ssh_profile_id,
            "profile-1"
        );
        let shell = session.shells.read();
        let state = shell.get(&id).unwrap();
        assert_eq!(state.seqnum, 0);
        assert!(state.data.is_empty());
        assert!(!state.closed);
    }

    #[tokio::test]
    async fn restart_shell_ends_old_output_generation_subscriptions() {
        let session = session();
        let id = Sid(8);
        session
            .add_shell(
                id,
                (0, 0),
                1,
                (24, 80),
                (640, 480),
                (String::new(), String::new(), "profile-2".into()),
            )
            .unwrap();
        let old_stream = session.subscribe_chunks(id, 0, 0);
        tokio::pin!(old_stream);

        session.restart_shell(id).unwrap();
        session
            .add_data(id, Bytes::from_static(b"new output"), 0)
            .unwrap();

        assert!(old_stream.next().await.is_none());
        let new_stream = session.subscribe_chunks(id, 1, 0);
        tokio::pin!(new_stream);
        let (_, sequence, chunks) = new_stream.next().await.unwrap();
        assert_eq!(sequence, 0);
        assert_eq!(chunks, vec![Bytes::from_static(b"new output")]);
    }
}
