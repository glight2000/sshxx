//! Bounded terminal replay shared by hosted and embedded PTYs.

use encoding_rs::{CoderResult, Decoder, UTF_8};
use sshx_core::{proto::TerminalData, Sid};
use sshxx_terminal_host::paste_mode::PasteMode;
use std::collections::VecDeque;
use std::time::Instant;
use tracing::{debug, warn};

use super::prev_char_boundary;
use crate::encrypt::Encrypt;

const CHUNK_BYTES: usize = 1 << 16;
const ROLLING_BYTES: usize = 8 << 20;
const PRUNE_BYTES: usize = 12 << 20;

pub(super) struct OutputBuffer {
    retained_modes: PasteMode,
    sent_modes: PasteMode,
    mode_checkpoints: VecDeque<(usize, PasteMode)>,
    content: String,
    offset: usize,
    sent: usize,
    sync_misses: u8,
    last_ack: Option<u64>,
    reported_gap: bool,
    decoder: Decoder,
    last_read: Option<Instant>,
    last_sent: Option<Instant>,
}

impl Default for OutputBuffer {
    fn default() -> Self {
        Self {
            retained_modes: PasteMode::fresh(),
            sent_modes: PasteMode::fresh(),
            mode_checkpoints: VecDeque::new(),
            content: String::new(),
            offset: 0,
            sent: 0,
            sync_misses: 0,
            last_ack: None,
            reported_gap: false,
            decoder: UTF_8.new_decoder(),
            last_read: None,
            last_sent: None,
        }
    }
}

impl OutputBuffer {
    pub fn reset_decoder(&mut self) {
        self.decoder = UTF_8.new_decoder();
        // A gap from a legacy host must not imply the default/off state.
        self.mode_checkpoints
            .push_back((self.offset + self.content.len(), PasteMode::default()));
    }

    pub fn restore_modes(&mut self, checkpoint: &[u8]) {
        if let Some(mode) = PasteMode::restore(checkpoint) {
            self.mode_checkpoints
                .push_back((self.offset + self.content.len(), mode));
        }
    }

    fn advance_modes(&self, mut mode: PasteMode, start: usize, end: usize) -> PasteMode {
        let mut cursor = start;
        for (offset, checkpoint) in &self.mode_checkpoints {
            if *offset < start || *offset >= end {
                continue;
            }
            mode.feed(
                self.content.as_bytes()[cursor - self.offset..*offset - self.offset]
                    .iter()
                    .copied(),
            );
            mode = checkpoint.clone();
            cursor = *offset;
        }
        mode.feed(
            self.content.as_bytes()[cursor - self.offset..end - self.offset]
                .iter()
                .copied(),
        );
        mode
    }

    pub fn append(&mut self, bytes: &[u8], finished: bool) {
        if !bytes.is_empty() {
            self.last_read = Some(Instant::now());
        }
        self.content
            .reserve(self.decoder.max_utf8_buffer_length(bytes.len()).unwrap());
        let (result, _, _) = self
            .decoder
            .decode_to_string(bytes, &mut self.content, finished);
        debug_assert!(result == CoderResult::InputEmpty);
    }

    /// Sync is an acknowledgement, unlike `sent`, which only means queued.
    /// Retention remains bounded even when an acknowledgement never advances.
    pub fn sync(&mut self, id: Sid, sequence: u64, recovery: bool) -> Option<String> {
        debug!(terminal_id = id.0, sync_sequence = sequence,
            retained_start = self.offset, retained_end = self.offset + self.content.len(),
            last_sent_sequence = self.sent,
            last_read_ms_ago = ?self.last_read.map(|t| t.elapsed().as_millis() as u64),
            last_sent_ms_ago = ?self.last_sent.map(|t| t.elapsed().as_millis() as u64),
            "daemon output sync progress");
        let progressing = self.last_ack.is_some_and(|previous| sequence > previous);
        self.last_ack = Some(sequence);
        if sequence >= self.sent as u64 || progressing {
            self.sync_misses = 0;
            self.reported_gap = false;
            return None;
        }
        self.sync_misses = self.sync_misses.saturating_add(1);
        if self.sync_misses < 3 {
            return None;
        }
        if sequence < self.offset as u64 && !recovery {
            // Old servers ignore the new protobuf field. Do not endlessly
            // resend megabytes they cannot accept; input remains operational.
            if self.reported_gap {
                return None;
            }
            self.reported_gap = true;
            warn!(%id, server_sequence = sequence, retained_start = self.offset,
                retained_end = self.offset + self.content.len(), sent = self.sent,
                "terminal output gap exceeds retention; server upgrade required");
            return Some(format!("Terminal {id} output is out of sync: missing output is no longer retained. Upgrade sshxx-server to recover; the terminal process is still running."));
        }
        warn!(%id, server_sequence = sequence, retained_start = self.offset,
            retained_end = self.offset + self.content.len(), sent = self.sent,
            "replaying terminal output after repeated unacknowledged progress");
        self.sent = (sequence as usize).max(self.offset);
        self.sent_modes = self.advance_modes(self.retained_modes.clone(), self.offset, self.sent);
        self.sync_misses = 0;
        self.reported_gap = false;
        None
    }

    pub fn next_chunk(&self, id: Sid, encrypt: &Encrypt) -> Option<TerminalData> {
        if self.sent >= self.offset + self.content.len() {
            return None;
        }
        let start = prev_char_boundary(&self.content, self.sent.saturating_sub(self.offset));
        let end = prev_char_boundary(&self.content, (start + CHUNK_BYTES).min(self.content.len()));
        let seq = (self.offset + start) as u64;
        let mode = self.advance_modes(
            self.sent_modes.clone(),
            self.offset + start,
            self.offset + end,
        );
        let mode_byte = mode
            .enabled()
            .map_or(0, |enabled| if enabled { 2 } else { 1 });
        Some(TerminalData {
            paste_mode: encrypt
                .segment(
                    0x300000000 | id.0 as u64,
                    (self.offset + end - 1) as u64,
                    &[mode_byte],
                )
                .into(),
            id: id.0,
            data: encrypt
                .segment(
                    0x100000000 | id.0 as u64,
                    seq,
                    &self.content.as_bytes()[start..end],
                )
                .into(),
            seq,
            retained_sequence: Some(self.offset as u64),
        })
    }

    pub fn mark_sent(&mut self, end: u64) {
        self.last_sent = Some(Instant::now());
        self.sent_modes = self.advance_modes(self.sent_modes.clone(), self.sent, end as usize);
        self.sent = end as usize;
        if self.content.len() > PRUNE_BYTES && self.sent.saturating_sub(ROLLING_BYTES) > self.offset
        {
            let pruned = prev_char_boundary(&self.content, self.sent - ROLLING_BYTES - self.offset);
            self.retained_modes = self.advance_modes(
                self.retained_modes.clone(),
                self.offset,
                self.offset + pruned,
            );
            self.offset += pruned;
            self.content.drain(..pruned);
            while self
                .mode_checkpoints
                .front()
                .is_some_and(|(offset, _)| *offset < self.offset)
            {
                self.mode_checkpoints.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_checkpoint_survives_pruning_replay_and_host_reattach() {
        let encrypt = Encrypt::new("synthetic-paste-checkpoint");
        let mut mode = PasteMode::fresh();
        mode.feed(b"\x1b[?2004h".iter().copied());
        let mut buffer = OutputBuffer::default();
        buffer.reset_decoder();
        buffer.restore_modes(&mode.checkpoint());
        buffer.append(&vec![b'x'; PRUNE_BYTES + CHUNK_BYTES], false);
        let original = drain(&mut buffer, &encrypt);
        assert!(buffer.offset > 0);
        assert!(buffer.mode_checkpoints.is_empty());
        for chunk in &original {
            assert_eq!(
                encrypt.segment(
                    0x30000003c,
                    chunk.seq + chunk.data.len() as u64 - 1,
                    &chunk.paste_mode
                ),
                [2]
            );
        }
        for _ in 0..3 {
            buffer.sync(Sid(60), 0, true);
        }
        for chunk in drain(&mut buffer, &encrypt) {
            assert_eq!(
                encrypt.segment(
                    0x30000003c,
                    chunk.seq + chunk.data.len() as u64 - 1,
                    &chunk.paste_mode
                ),
                [2]
            );
        }
        buffer.append(b"\x1b[?2004l", false);
        let off = drain(&mut buffer, &encrypt);
        assert_eq!(
            encrypt.segment(
                0x30000003c,
                off[0].seq + off[0].data.len() as u64 - 1,
                &off[0].paste_mode
            ),
            [1]
        );
        buffer.reset_decoder();
        buffer.append(b"legacy host lost mode", false);
        let unknown = drain(&mut buffer, &encrypt);
        assert_eq!(
            encrypt.segment(
                0x30000003c,
                unknown[0].seq + unknown[0].data.len() as u64 - 1,
                &unknown[0].paste_mode
            ),
            [0]
        );
    }

    #[test]
    fn old_wire_messages_do_not_opt_into_discarding_gaps() {
        use prost::Message;
        use sshx_core::proto::SequenceNumbers;

        let chunk = TerminalData::decode(&[8, 60, 18, 1, b'x', 24, 9][..]).unwrap();
        assert_eq!(
            (chunk.id, chunk.seq, chunk.retained_sequence),
            (60, 9, None)
        );
        let sync = SequenceNumbers::decode(&[10, 4, 8, 60, 16, 9][..]).unwrap();
        assert_eq!(sync.map[&60], 9);
        assert!(!sync.output_recovery);
    }

    fn drain(buffer: &mut OutputBuffer, encrypt: &Encrypt) -> Vec<TerminalData> {
        let mut chunks = Vec::new();
        while let Some(chunk) = buffer.next_chunk(Sid(60), encrypt) {
            buffer.mark_sent(chunk.seq + chunk.data.len() as u64);
            chunks.push(chunk);
        }
        chunks
    }

    fn pruned_buffer(encrypt: &Encrypt) -> OutputBuffer {
        let mut buffer = OutputBuffer::default();
        for _ in 0..240 {
            buffer.append(&vec![b'x'; CHUNK_BYTES], false);
            drain(&mut buffer, encrypt);
        }
        buffer.append(b"\r\nCodex exited\r\n$ ", false);
        drain(&mut buffer, encrypt);
        assert!(buffer.offset > CHUNK_BYTES);
        assert!(buffer.content.len() <= PRUNE_BYTES);
        buffer
    }

    #[test]
    fn expired_gap_replays_retained_tail_at_original_encryption_offsets() {
        let encrypt = Encrypt::new("synthetic-replay");
        let mut buffer = pruned_buffer(&encrypt);
        let end = buffer.sent;
        for _ in 0..3 {
            assert!(buffer.sync(Sid(60), CHUNK_BYTES as u64, true).is_none());
        }
        let chunks = drain(&mut buffer, &encrypt);
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].seq, buffer.offset as u64);
        assert_eq!(chunks[0].retained_sequence, Some(chunks[0].seq));
        let mut plaintext = Vec::new();
        for chunk in chunks {
            plaintext.extend(encrypt.segment(0x10000003c, chunk.seq, &chunk.data));
        }
        assert_eq!(plaintext, buffer.content.as_bytes());
        assert!(plaintext.ends_with(b"Codex exited\r\n$ "));
        buffer.sync(Sid(60), end as u64, true);
        assert!(buffer.next_chunk(Sid(60), &encrypt).is_none());
    }

    #[test]
    fn old_server_gets_one_error_not_an_endless_expired_replay() {
        let encrypt = Encrypt::new("synthetic-legacy");
        let mut buffer = pruned_buffer(&encrypt);
        let mut errors = 0;
        for _ in 0..18 {
            errors += usize::from(buffer.sync(Sid(60), CHUNK_BYTES as u64, false).is_some());
            assert!(buffer.next_chunk(Sid(60), &encrypt).is_none());
        }
        assert_eq!(errors, 1);
        // Capability is re-evaluated on Sync, so a server upgrade can recover.
        buffer.sync(Sid(60), CHUNK_BYTES as u64, true);
        assert!(buffer.next_chunk(Sid(60), &encrypt).is_some());
    }

    #[test]
    fn continued_pty_output_does_not_hide_a_stalled_server_ack() {
        let encrypt = Encrypt::new("synthetic-live");
        let mut buffer = OutputBuffer::default();
        for _ in 0..3 {
            buffer.append(b"new output\r\n", false);
            drain(&mut buffer, &encrypt);
            buffer.sync(Sid(60), 0, false);
        }
        assert_eq!(buffer.next_chunk(Sid(60), &encrypt).unwrap().seq, 0);
    }

    #[test]
    fn advancing_ack_does_not_replay_and_utf8_boundaries_survive_retry() {
        let encrypt = Encrypt::new("synthetic-utf8");
        let mut buffer = OutputBuffer::default();
        for seq in 0..8 {
            buffer.append(b"output", false);
            drain(&mut buffer, &encrypt);
            buffer.sync(Sid(60), seq, false);
            assert!(buffer.next_chunk(Sid(60), &encrypt).is_none());
        }
        buffer.append(&[0xe4, 0xb8], false);
        assert!(buffer.next_chunk(Sid(60), &encrypt).is_none());
        buffer.append(&[0xad], false);
        let original = drain(&mut buffer, &encrypt);
        let seq = original[0].seq;
        for _ in 0..4 {
            buffer.sync(Sid(60), seq + 1, true);
        }
        let replay = drain(&mut buffer, &encrypt);
        assert_eq!(replay, original);
    }
}
