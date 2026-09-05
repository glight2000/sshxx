//! Snapshot and restore sessions from serialized state.

use std::collections::{BTreeMap, HashSet};

use anyhow::{ensure, Context, Result};
use prost::Message;
use sshx_core::{
    proto::{
        SerializedCustomWindow, SerializedFileWindow, SerializedNote, SerializedPage,
        SerializedSession, SerializedShell, SshProfileCollection,
    },
    Sid, Uid, SSH_PROFILE_FORMAT_VERSION,
};

use super::validation::{
    normalize_linked_shell_ids, normalize_note_canvas_links, normalize_note_paragraphs,
    validate_custom_source_total, validate_custom_window, validate_file_editor_total,
    validate_file_window, validate_note_content, validate_page_name, validate_terminal_window_size,
};
use super::{Metadata, Session, State};
use crate::web::protocol::{WsCustomWindow, WsFileWindow, WsNote, WsPage, WsWinsize};

/// Persist at most this many bytes of output in storage, per shell.
const SHELL_SNAPSHOT_BYTES: u64 = 1 << 15; // 32 KiB

const MAX_SNAPSHOT_SIZE: usize = 1 << 26; // 64 MiB

impl Session {
    /// Snapshot the session, returning a compressed representation.
    pub fn snapshot(&self) -> Result<Vec<u8>> {
        let pages = self.pages.borrow();
        let ids = self.counter.get_current_values();
        let winsizes: BTreeMap<Sid, WsWinsize> = self.source.borrow().iter().cloned().collect();
        let message = SerializedSession {
            encrypted_zeros: self.metadata().encrypted_zeros.clone(),
            shells: self
                .shells
                .read()
                .iter()
                .map(|(sid, shell)| {
                    // Prune off data until its total length is at most `SHELL_SNAPSHOT_BYTES`.
                    let mut prefix = 0;
                    let mut chunk_offset = shell.chunk_offset;
                    let mut byte_offset = shell.byte_offset;

                    for i in 0..shell.data.len() {
                        if shell.seqnum - byte_offset > SHELL_SNAPSHOT_BYTES {
                            prefix += 1;
                            chunk_offset += 1;
                            byte_offset += shell.data[i].len() as u64;
                        } else {
                            break;
                        }
                    }

                    let winsize = winsizes.get(sid).cloned().unwrap_or_default();
                    let shell = SerializedShell {
                        seqnum: shell.seqnum,
                        data: shell.data[prefix..].to_vec(),
                        chunk_offset,
                        byte_offset,
                        closed: shell.closed,
                        winsize_x: winsize.x,
                        winsize_y: winsize.y,
                        winsize_rows: winsize.rows.into(),
                        winsize_cols: winsize.cols.into(),
                        title: winsize.title,
                        background: winsize.background,
                        opacity: winsize.opacity.into(),
                        page_id: winsize.page_id,
                        theme: winsize.theme,
                        width: winsize.width.into(),
                        height: winsize.height.into(),
                        minimized: winsize.minimized,
                    };
                    (sid.0, shell)
                })
                .collect(),
            next_sid: ids.0 .0,
            next_uid: ids.1 .0,
            name: self.metadata().name.clone(),
            write_password_hash: self.metadata().write_password_hash.clone(),
            daemon_version: self.metadata().daemon_version.clone(),
            terminal_host_version: self.metadata().terminal_host_version.clone(),
            daemon_capabilities: self.metadata().daemon_capabilities.clone(),
            notes: self
                .notes
                .borrow()
                .iter()
                .map(|(id, note)| {
                    (
                        id.0,
                        SerializedNote {
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
                            minimized: note.minimized,
                        },
                    )
                })
                .collect(),
            pages: pages
                .iter()
                .map(|page| SerializedPage {
                    id: page.id,
                    name: page.name.clone(),
                })
                .collect(),
            ssh_profiles: self.ssh_profile_collection().profiles,
            file_windows: self
                .file_windows
                .borrow()
                .iter()
                .map(|(id, window)| SerializedFileWindow {
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
                    minimized: window.minimized,
                })
                .collect(),
            custom_windows: self
                .custom_windows
                .borrow()
                .iter()
                .map(|(id, window)| SerializedCustomWindow {
                    id: id.0,
                    page_id: window.page_id,
                    title: window.title.clone(),
                    background: window.background.clone(),
                    x: window.x,
                    y: window.y,
                    width: window.width.into(),
                    height: window.height.into(),
                    source: window.source.clone(),
                    show_preview: window.show_preview,
                    url: window.url.clone(),
                    use_url: window.use_url,
                    minimized: window.minimized,
                })
                .collect(),
        };
        let data = message.encode_to_vec();
        ensure!(data.len() < MAX_SNAPSHOT_SIZE, "snapshot too large");
        Ok(zstd::bulk::compress(&data, 3)?)
    }

    /// Restore the session from a previous compressed snapshot.
    pub fn restore(data: &[u8]) -> Result<Self> {
        let data = zstd::bulk::decompress(data, MAX_SNAPSHOT_SIZE)?;
        let message = SerializedSession::decode(&*data)?;

        let metadata = Metadata {
            encrypted_zeros: message.encrypted_zeros,
            name: message.name,
            write_password_hash: message.write_password_hash,
            daemon_version: message.daemon_version,
            terminal_host_version: message.terminal_host_version,
            daemon_capabilities: message.daemon_capabilities,
        };

        let session = Self::new(metadata);
        let pages = if message.pages.is_empty() {
            vec![WsPage {
                id: 1,
                name: "Page 1".into(),
            }]
        } else {
            let mut seen = HashSet::new();
            message
                .pages
                .into_iter()
                .map(|page| {
                    ensure!(
                        page.id != 0 && seen.insert(page.id),
                        "invalid or duplicate page ID"
                    );
                    validate_page_name(&page.name)?;
                    Ok(WsPage {
                        id: page.id,
                        name: page.name,
                    })
                })
                .collect::<Result<Vec<_>>>()?
        };
        let page_ids = pages.iter().map(|page| page.id).collect::<HashSet<_>>();
        let mut shells = session.shells.write();
        let mut winsizes = Vec::new();
        for (sid, shell) in message.shells {
            let winsize = WsWinsize {
                x: shell.winsize_x,
                y: shell.winsize_y,
                rows: shell.winsize_rows.try_into().context("rows overflow")?,
                cols: shell.winsize_cols.try_into().context("cols overflow")?,
                title: shell.title,
                background: shell.background,
                opacity: if shell.opacity == 0 {
                    80
                } else {
                    shell.opacity.try_into().context("opacity overflow")?
                },
                page_id: shell.page_id.max(1),
                theme: shell.theme,
                width: shell.width.try_into().context("width overflow")?,
                height: shell.height.try_into().context("height overflow")?,
                generation: 0,
                minimized: shell.minimized,
            };
            ensure!(
                page_ids.contains(&winsize.page_id),
                "terminal references a missing page"
            );
            validate_terminal_window_size(winsize.width, winsize.height)?;
            winsizes.push((Sid(sid), winsize));
            let shell = State {
                seqnum: shell.seqnum,
                data: shell.data,
                chunk_offset: shell.chunk_offset,
                byte_offset: shell.byte_offset,
                closed: shell.closed,
                notify: Default::default(),
            };
            shells.insert(Sid(sid), shell);
        }
        drop(shells);
        session.source.send_replace(winsizes);
        let shell_layout = session.source.borrow();
        let mut restored_notes = message
            .notes
            .into_iter()
            .map(|(id, note)| -> Result<(Sid, WsNote)> {
                let page_id = note.page_id.max(1);
                let paragraphs = normalize_note_paragraphs(&note.text, note.paragraphs);
                let note = WsNote {
                    x: note.x,
                    y: note.y,
                    width: if note.width == 0 {
                        384
                    } else {
                        note.width.try_into().context("note width overflow")?
                    },
                    height: if note.height == 0 {
                        224
                    } else {
                        note.height.try_into().context("note height overflow")?
                    },
                    text: paragraphs.join("\n"),
                    paragraphs,
                    linked_shell_ids: normalize_linked_shell_ids(
                        note.linked_shell_ids,
                        page_id,
                        &shell_layout,
                    ),
                    linked_note_ids: note.linked_note_ids.into_iter().map(Sid).collect(),
                    linked_file_window_ids: note
                        .linked_file_window_ids
                        .into_iter()
                        .map(Sid)
                        .collect(),
                    title: note.title,
                    background: note.background,
                    opacity: if note.opacity == 0 {
                        80
                    } else {
                        note.opacity.try_into().context("note opacity overflow")?
                    },
                    page_id,
                    minimized: note.minimized,
                };
                ensure!(
                    page_ids.contains(&note.page_id),
                    "note references a missing page"
                );
                validate_note_content(&note)?;
                Ok((Sid(id), note))
            })
            .collect::<Result<Vec<_>>>()?;
        drop(shell_layout);
        let file_windows = message
            .file_windows
            .into_iter()
            .map(|window| -> Result<(Sid, WsFileWindow)> {
                let state = WsFileWindow {
                    shell_id: Sid(window.shell_id),
                    page_id: window.page_id.max(1),
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
                    minimized: window.minimized,
                };
                validate_file_window(&state)?;
                ensure!(
                    page_ids.contains(&state.page_id),
                    "file browser references a missing page"
                );
                ensure!(
                    session
                        .source
                        .borrow()
                        .iter()
                        .any(|(id, _)| *id == state.shell_id),
                    "file browser references a missing terminal"
                );
                Ok((Sid(window.id), state))
            })
            .collect::<Result<Vec<_>>>()?;
        validate_file_editor_total(&file_windows)?;
        let custom_windows = message
            .custom_windows
            .into_iter()
            .map(|window| -> Result<(Sid, WsCustomWindow)> {
                let state = WsCustomWindow {
                    page_id: window.page_id.max(1),
                    title: window.title,
                    background: if window.background.is_empty() {
                        "#18181b".into()
                    } else {
                        window.background
                    },
                    x: window.x,
                    y: window.y,
                    width: window
                        .width
                        .try_into()
                        .context("custom component width overflow")?,
                    height: window
                        .height
                        .try_into()
                        .context("custom component height overflow")?,
                    source: window.source,
                    show_preview: window.show_preview,
                    url: window.url,
                    use_url: window.use_url,
                    minimized: window.minimized,
                };
                validate_custom_window(&state)?;
                ensure!(
                    page_ids.contains(&state.page_id),
                    "custom component references a missing page"
                );
                Ok((Sid(window.id), state))
            })
            .collect::<Result<Vec<_>>>()?;
        validate_custom_source_total(&custom_windows)?;
        normalize_note_canvas_links(&mut restored_notes, &file_windows);
        session.notes.send_replace(restored_notes);
        session.file_windows.send_replace(file_windows);
        session.custom_windows.send_replace(custom_windows);
        *session.next_page_id.lock() = pages
            .iter()
            .map(|page| page.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        session.pages.send_replace(pages);
        session.restore_ssh_profiles(SshProfileCollection {
            format_version: SSH_PROFILE_FORMAT_VERSION,
            profiles: message.ssh_profiles,
        })?;
        session
            .counter
            .set_current_values(Sid(message.next_sid), Uid(message.next_uid));

        Ok(session)
    }
}
