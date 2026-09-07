//! Continuous encrypted output and explicit recovery beyond daemon retention.

use anyhow::{ensure, Context, Result};
use bytes::Bytes;
use sshx_core::Sid;
use tracing::{debug, warn};

use super::{Session, SHELL_STORED_BYTES};

impl Session {
    /// Legacy senders cannot authorize discarding a gap.
    pub fn add_data(&self, id: Sid, data: Bytes, seq: u64) -> Result<()> {
        self.add_output(id, data, seq, None)
    }

    /// Only the authenticated daemon's earliest retained chunk can rebase a
    /// stream. Never renumber ciphertext or close/recreate the underlying PTY.
    pub fn add_output(&self, id: Sid, data: Bytes, seq: u64, retained: Option<u64>) -> Result<()> {
        let end = seq
            .checked_add(data.len() as u64)
            .context("terminal output sequence overflow")?;
        ensure!(
            retained.is_none_or(|start| start <= seq),
            "terminal retention starts after output chunk"
        );
        let mut shells = self.shells.write();
        let shell = shells.get_mut(&id).context("terminal does not exist")?;
        if shell.closed || data.is_empty() {
            return Ok(());
        }
        shell.last_received_sequence = Some(seq);

        let mut recovered = None;
        if seq > shell.seqnum {
            shell.gap_chunks = shell.gap_chunks.saturating_add(1);
            if retained != Some(seq) {
                if shell.gap_chunks == 1 || shell.gap_chunks.is_power_of_two() {
                    warn!(%id, expected = shell.seqnum, received = seq, ?retained,
                        rejected_chunks = shell.gap_chunks, "terminal output gap; awaiting daemon replay");
                }
                return Ok(());
            }
            recovered = Some(shell.seqnum);
            warn!(%id, expected = shell.seqnum, retained_start = seq,
                rejected_chunks = shell.gap_chunks, "recovering terminal output beyond daemon retention");
            shell.data.clear();
            shell.seqnum = seq;
            shell.byte_offset = seq;
            shell.chunk_offset = 0;
        }

        if end > shell.seqnum {
            let segment = data.slice((shell.seqnum - seq) as usize..);
            debug!(%id, bytes = segment.len(), previous = shell.seqnum, next = end,
                "adding data to shell");
            shell.seqnum = end;
            shell.last_accepted = Some(tokio::time::Instant::now());
            shell.gap_chunks = 0;
            shell.data.push(segment);
            let mut stored_bytes = shell.seqnum - shell.byte_offset;
            let mut offset = 0;
            while offset < shell.data.len() && stored_bytes > SHELL_STORED_BYTES {
                let bytes = shell.data[offset].len() as u64;
                shell.chunk_offset += 1;
                shell.byte_offset += bytes;
                stored_bytes -= bytes;
                offset += 1;
            }
            shell.data.drain(..offset);
            shell.notify.notify_waiters();
        }
        let notify = shell.notify.clone();
        drop(shells);

        if let Some(previous) = recovered {
            // Reuse the existing renderer-generation invalidation. Encryption
            // still uses the original absolute offsets, not this generation.
            self.source.send_modify(|source| {
                if let Some((_, window)) = source.iter_mut().find(|(sid, _)| *sid == id) {
                    window.generation = window.generation.wrapping_add(1);
                }
            });
            notify.notify_waiters();
            self.sync_now();
            self.send_error(format!("Terminal {id}: output stream recovered after a gap of {} bytes. Some earlier output is unavailable; replaying retained output. The terminal process was not restarted.", seq - previous));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::tests::session;
    use crate::web::protocol::WsServer;
    use tokio_stream::StreamExt;

    fn add_shell(session: &Session, id: Sid) {
        session
            .add_shell(id, (120, 240), 1, (24, 80), (640, 480), Default::default())
            .unwrap();
    }

    #[tokio::test]
    async fn expired_gap_resets_only_its_renderer_and_preserves_absolute_offsets() {
        let session = session();
        let id = Sid(60);
        add_shell(&session, id);
        add_shell(&session, Sid(61));
        session.add_data(id, Bytes::from_static(b"old"), 0).unwrap();
        {
            let shells = session.shells.read();
            assert_eq!(shells[&id].last_received_sequence, Some(0));
            assert!(shells[&id].last_accepted.is_some());
        }
        session
            .add_data(Sid(61), Bytes::from_static(b"unaffected"), 0)
            .unwrap();
        let old_stream = session.subscribe_chunks(id, 0, 0);
        tokio::pin!(old_stream);
        assert_eq!(old_stream.next().await.unwrap().1, 0);
        let mut notices = session.subscribe_broadcast();
        let base = 20 << 20;
        let data = Bytes::from_static(b"retained tail");

        // A future chunk is not permission to skip recoverable history.
        session
            .add_output(id, data.clone(), base + 64, Some(base))
            .unwrap();
        assert_eq!(session.sequence_numbers().map[&60], 3);
        session
            .add_output(id, data.clone(), base, Some(base))
            .unwrap();
        assert_eq!(
            session.sequence_numbers().map[&60],
            base + data.len() as u64
        );
        assert_eq!(session.shell_generation(id), Some(1));
        assert_eq!(session.shell_generation(Sid(61)), Some(0));
        assert_eq!(session.sequence_numbers().map[&61], 10);
        assert!(old_stream.next().await.is_none());
        assert!(
            matches!(notices.next().await.unwrap().unwrap(), WsServer::Error(message) if message.contains("process was not restarted"))
        );

        let stream = session.subscribe_chunks(id, 1, 0);
        tokio::pin!(stream);
        let (_, sequence, chunks) = stream.next().await.unwrap();
        assert_eq!(sequence, base);
        assert_eq!(chunks, vec![data.clone()]);
        // Duplicate recovery messages cannot clear output or bump generation.
        session
            .add_output(id, data.clone(), base, Some(base))
            .unwrap();
        assert_eq!(session.shell_generation(id), Some(1));
        session
            .add_output(
                id,
                Bytes::from_static(b"$ "),
                base + data.len() as u64,
                Some(base),
            )
            .unwrap();
        assert_eq!(
            stream.next().await.unwrap().2,
            vec![Bytes::from_static(b"$ ")]
        );
        let window = session.source.borrow()[0].1.clone();
        assert_eq!(
            (window.x, window.y, window.width, window.height),
            (120, 240, 640, 480)
        );
        assert!(!session.shells.read()[&id].closed);

        let restored = Session::restore(&session.snapshot().unwrap()).unwrap();
        assert_eq!(
            restored.sequence_numbers().map[&60],
            base + data.len() as u64 + 2
        );
        assert_eq!(restored.shells.read()[&id].byte_offset, base);
    }

    #[test]
    fn legacy_overlap_and_invalid_ranges_never_discard_valid_output() {
        let session = session();
        let id = Sid(60);
        add_shell(&session, id);
        session.add_data(id, Bytes::from_static(b"ab"), 0).unwrap();
        for _ in 0..18 {
            session
                .add_data(id, Bytes::from_static(b"future"), 20 << 20)
                .unwrap();
        }
        assert_eq!(session.sequence_numbers().map[&60], 2);
        session
            .add_output(id, Bytes::from_static(b"bcd"), 1, Some(0))
            .unwrap();
        assert_eq!(session.sequence_numbers().map[&60], 4);
        assert_eq!(session.shell_generation(id), Some(0));
        assert!(session
            .add_output(id, Bytes::from_static(b"x"), 4, Some(5))
            .is_err());
        assert!(session
            .add_data(id, Bytes::from_static(b"x"), u64::MAX)
            .is_err());
        session
            .add_output(id, Bytes::new(), 100, Some(100))
            .unwrap();
        assert_eq!(session.sequence_numbers().map[&60], 4);
        session.shells.write().get_mut(&id).unwrap().closed = true;
        session
            .add_output(id, Bytes::from_static(b"x"), 100, Some(100))
            .unwrap();
        assert_eq!(session.shells.read()[&id].seqnum, 4);
        assert_eq!(session.shell_generation(id), Some(0));
    }

    #[tokio::test]
    async fn final_output_wakes_an_idle_subscription() {
        let session = session();
        let id = Sid(60);
        add_shell(&session, id);
        // The notification must be captured while inspecting state, before
        // awaiting. notify_waiters does not save permits for later futures.
        let notified = session.shells.read()[&id].notify.clone().notified_owned();
        session.add_data(id, Bytes::from_static(b"$ "), 0).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), notified)
            .await
            .unwrap();
        let stream = session.subscribe_chunks(id, 0, 0);
        tokio::pin!(stream);
        assert_eq!(
            stream.next().await.unwrap().2,
            vec![Bytes::from_static(b"$ ")]
        );
        let next = stream.next();
        tokio::pin!(next);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(10), &mut next)
                .await
                .is_err()
        );
        session
            .add_data(id, Bytes::from_static(b"ready"), 2)
            .unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(1), next)
                .await
                .unwrap()
                .unwrap()
                .2,
            vec![Bytes::from_static(b"ready")]
        );
    }
}
