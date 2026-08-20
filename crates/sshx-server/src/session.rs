//! Core logic for sshxx sessions, independent of message transport.

use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use bytes::Bytes;
use parking_lot::{Mutex, RwLock, RwLockWriteGuard};
use sshx_core::{
    proto::{
        server_update::ServerMessage, NewShell, SequenceNumbers, WorkspaceNote, WorkspacePage,
        WorkspaceShell, WorkspaceState,
    },
    IdCounter, Sid, Uid, WORKSPACE_FORMAT_VERSION,
};
use tokio::sync::{broadcast, watch, Notify};
use tokio::time::Instant;
use tokio_stream::wrappers::{errors::BroadcastStreamRecvError, BroadcastStream, WatchStream};
use tokio_stream::Stream;
use tracing::{debug, warn};

use crate::utils::Shutdown;
use crate::web::protocol::{WsNote, WsPage, WsServer, WsUser, WsWinsize};

mod snapshot;

/// Store a rolling buffer with at most this quantity of output, per shell.
const SHELL_STORED_BYTES: u64 = 1 << 21; // 2 MiB

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

    /// Watch channel source for the ordered list of notes on the canvas.
    notes: watch::Sender<Vec<(Sid, WsNote)>>,

    /// Watch channel source for the ordered list of named canvas pages.
    pages: watch::Sender<Vec<WsPage>>,

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
            notes: watch::channel(Vec::new()).0,
            pages: watch::channel(vec![WsPage {
                id: 1,
                name: "Page 1".into(),
            }])
            .0,
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

    /// Receive a notification every time the named pages change.
    pub fn subscribe_pages(&self) -> impl Stream<Item = Vec<WsPage>> + Unpin {
        WatchStream::new(self.pages.subscribe())
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

    /// Receive a notification whenever locally persistable workspace state changes.
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
                    background: note.background.clone(),
                    opacity: note.opacity.into(),
                    page_id: note.page_id,
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
        if workspace.shells.len() > 100 || workspace.notes.len() > 100 || workspace.pages.len() > 50
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
        let mut source = Vec::with_capacity(workspace.shells.len());
        let mut requests = Vec::with_capacity(workspace.shells.len());
        for shell in workspace.shells {
            if shell.id == 0 || !ids.insert(shell.id) {
                bail!("workspace contains an invalid or duplicate item ID");
            }
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
            };
            if winsize.rows == 0 || winsize.cols == 0 {
                bail!("terminal dimensions must be positive");
            }
            validate_title(&winsize.title)?;
            validate_color(&winsize.background)?;
            validate_opacity(winsize.opacity)?;
            if !page_ids.contains(&winsize.page_id) {
                bail!("terminal references a missing page");
            }

            let id = Sid(shell.id);
            let page_id = winsize.page_id;
            max_id = max_id.max(shell.id);
            shells.insert(id, State::default());
            source.push((id, winsize));
            requests.push(NewShell {
                id: shell.id,
                x: shell.x,
                y: shell.y,
                source_id: None,
                page_id,
            });
        }

        let mut notes = Vec::with_capacity(workspace.notes.len());
        for note in workspace.notes {
            if note.id == 0 || !ids.insert(note.id) {
                bail!("workspace contains an invalid or duplicate item ID");
            }
            let note_state = WsNote {
                x: note.x,
                y: note.y,
                width: note.width.try_into().context("note width overflow")?,
                height: note.height.try_into().context("note height overflow")?,
                text: note.text,
                background: note.background,
                opacity: note.opacity.try_into().context("note opacity overflow")?,
                page_id: if note.page_id == 0 { 1 } else { note.page_id },
            };
            if note_state.text.len() > 10_000 {
                bail!("note text is too long");
            }
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

        let next_id = max_id
            .checked_add(1)
            .context("workspace item ID overflow")?;
        *self.shells.write() = shells;
        self.source.send_replace(source);
        self.notes.send_replace(notes);
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

    /// Subscribe for chunks from a shell, until it is closed.
    pub fn subscribe_chunks(
        &self,
        id: Sid,
        mut chunknum: u64,
    ) -> impl Stream<Item = (bool, u64, Vec<Bytes>)> + '_ {
        async_stream::stream! {
            let mut replay = true;
            while !self.shutdown.is_terminated() {
                // We absolutely cannot hold `shells` across an await point,
                // since that would cause deadlocks.
                let (seqnum, chunks, notified) = {
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
                        chunks = shell.data[start..].to_vec();
                        chunknum = current_chunks;
                    }
                    (seqnum, chunks, notified)
                };

                if !chunks.is_empty() {
                    yield (replay, seqnum, chunks);
                }
                replay = false;
                tokio::select! {
                    _ = notified => (),
                    _ = self.terminated() => return,
                }
            }
        }
    }

    /// Add a new shell to the session.
    pub fn add_shell(
        &self,
        id: Sid,
        center: (i32, i32),
        page_id: u32,
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

        let _guard = match self.shells.write().entry(id) {
            Occupied(_) => bail!("shell already exists with id={id}"),
            Vacant(v) => v.insert(State::default()),
        };
        self.source.send_modify(|source| {
            let winsize = WsWinsize {
                x: center.0,
                y: center.1,
                page_id,
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
        self.pending_restored_shells.lock().remove(&id);
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

    /// Add a new note to the canvas.
    pub fn add_note(&self, id: Sid, position: (i32, i32), page_id: u32) -> Result<()> {
        if !self.page_exists(page_id) {
            bail!("cannot add note to missing page");
        }
        self.notes.send_modify(|notes| {
            notes.push((
                id,
                WsNote {
                    x: position.0,
                    y: position.1,
                    width: 384,
                    height: 224,
                    text: String::new(),
                    background: "#4b4534".into(),
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
    pub fn update_note(&self, id: Sid, page_id: u32, note: Option<WsNote>) -> Result<()> {
        self.check_note_page(id, page_id)?;
        if let Some(note) = &note {
            if note.text.len() > 10_000 {
                bail!("note text is too long");
            }
            validate_color(&note.background)?;
            validate_opacity(note.opacity)?;
            if note.page_id != page_id {
                bail!("note update cannot move between pages");
            }
            if !(240..=2_000).contains(&note.width) || !(160..=2_000).contains(&note.height) {
                bail!("note dimensions are out of range");
            }
        }
        let preserve_live_text = self.note_editors.read().contains_key(&id);
        let mut found = false;
        self.notes.send_modify(|notes| {
            if let Some(idx) = notes.iter().position(|(note_id, _)| *note_id == id) {
                let (_, old_note) = notes.remove(idx);
                let mut next_note = note.unwrap_or_else(|| old_note.clone());
                if preserve_live_text {
                    next_note.text = old_note.text;
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
        self.notes.send_if_modified(|notes| {
            if let Some((_, note)) = notes.iter_mut().find(|(note_id, _)| *note_id == id) {
                note.text = text.clone();
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

fn validate_title(title: &str) -> Result<()> {
    if title.len() > 100 {
        bail!("terminal title is too long");
    }
    Ok(())
}

fn validate_color(color: &str) -> Result<()> {
    if !color.is_empty()
        && !(color.len() == 7
            && color.starts_with('#')
            && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        bail!("invalid background color");
    }
    Ok(())
}

fn validate_opacity(opacity: u8) -> Result<()> {
    if !(20..=100).contains(&opacity) {
        bail!("opacity must be between 20 and 100");
    }
    Ok(())
}

fn validate_page_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.len() > 100 {
        bail!("page name must contain between 1 and 100 bytes");
    }
    Ok(())
}
