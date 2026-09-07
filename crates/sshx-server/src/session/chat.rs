//! Bounded, daemon-persisted room history and attachment metadata validation.
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{ensure, Context, Result};
use sshx_core::proto::{WorkspaceAttachment, WorkspaceChatMessage};
use sshx_core::{rand_alphanumeric, Uid};

use super::{Session, WsServer};

pub(super) fn validate_attachments(items: &[WorkspaceAttachment], limit: usize) -> Result<()> {
    ensure!(items.len() <= limit, "too many attachments");
    let mut ids = HashSet::new();
    for item in items {
        ensure!(
            item.id.len() == 32
                && item
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && ids.insert(&item.id),
            "invalid or duplicate attachment ID"
        );
        ensure!(
            !item.name.is_empty()
                && item.name.len() <= 255
                && !item.name.chars().any(char::is_control),
            "invalid attachment name"
        );
        ensure!(
            item.size > 0 && item.size <= 20 << 20,
            "attachment must be between 1 byte and 20 MiB"
        );
        ensure!(
            matches!(
                item.media_type.as_str(),
                "image/png"
                    | "image/jpeg"
                    | "image/gif"
                    | "image/webp"
                    | "image/avif"
                    | "video/mp4"
                    | "video/webm"
                    | "application/octet-stream"
            ),
            "unsupported attachment media type"
        );
    }
    Ok(())
}

impl Session {
    /// Change note attachments independently from live text and window geometry.
    pub fn update_note_attachments(
        &self,
        id: sshx_core::Sid,
        page_id: u32,
        attachments: Vec<WorkspaceAttachment>,
    ) -> Result<()> {
        self.check_note_page(id, page_id)?;
        validate_attachments(&attachments, 32)?;
        let mut found = false;
        self.notes.send_modify(|notes| {
            if let Some((_, note)) = notes
                .iter_mut()
                .find(|(note_id, note)| *note_id == id && note.page_id == page_id)
            {
                note.attachments = Some(attachments);
                found = true;
            }
        });
        ensure!(found, "note no longer exists on this page");
        self.workspace_changed();
        Ok(())
    }
    /// Latest 500 room records, oldest first. Attachment bodies are never included.
    pub fn chat_history(&self) -> Vec<WorkspaceChatMessage> {
        self.chat_history.lock().clone()
    }

    pub(super) fn restore_chat_history(&self, records: Vec<WorkspaceChatMessage>) -> Result<()> {
        ensure!(records.len() <= 500, "too many chat records");
        let mut ids = HashSet::new();
        for record in &records {
            ensure!(
                record.sent_at <= 8_640_000_000_000_000,
                "invalid chat timestamp"
            );
            ensure!(
                !record.id.is_empty() && record.id.len() <= 64 && ids.insert(&record.id),
                "invalid chat record ID"
            );
            ensure!(
                record.name.len() <= 200 && record.text.len() <= 8192,
                "chat record is too large"
            );
            validate_attachments(&record.attachments, 8)?;
        }
        *self.chat_history.lock() = records;
        Ok(())
    }

    /// Persist a validated message and broadcast both current and legacy events.
    pub fn send_chat_with_attachments(
        &self,
        uid: Uid,
        text: &str,
        attachments: Vec<WorkspaceAttachment>,
    ) -> Result<()> {
        ensure!(
            text.len() <= 8192 && (!text.trim().is_empty() || !attachments.is_empty()),
            "chat message must not be empty or exceed 8 KiB"
        );
        validate_attachments(&attachments, 8)?;
        // Preserve legacy permission semantics: authenticated readers can chat.
        // Uploading a new attachment is separately restricted to workspace writers.
        let name = self
            .users
            .read()
            .get(&uid)
            .context("user not found")?
            .name
            .clone();
        let message = WorkspaceChatMessage {
            id: rand_alphanumeric(32),
            name: name.clone(),
            text: text.into(),
            sent_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
            attachments,
        };
        let mut history = self.chat_history.lock();
        history.push(message.clone());
        if history.len() > 500 {
            history.remove(0);
        }
        // Broadcast while holding the history lock, preserving append order.
        self.broadcast.send(WsServer::ChatMessage(message)).ok();
        self.broadcast
            .send(WsServer::Hear(uid, name, text.into()))
            .ok();
        drop(history);
        self.workspace_changed();
        Ok(())
    }
}
