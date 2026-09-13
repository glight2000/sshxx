//! Opaque checkpoint routing and byte-fence to chunk-index translation.

use super::*;

impl Session {
    pub(crate) fn valid_chunk_cursor(&self, id: Sid, generation: u32, chunk: u64) -> bool {
        if self.shell_generation(id) != Some(generation) {
            return false;
        }
        let shells = self.shells.read();
        shells.get(&id).is_some_and(|shell| {
            !shell.closed
                && (chunk == 0
                    || (chunk >= shell.chunk_offset
                        && chunk <= shell.chunk_offset + shell.data.len() as u64))
        })
    }

    pub(crate) fn send_terminal_checkpoint(
        &self,
        response: sshx_core::proto::TerminalCheckpointResponse,
    ) {
        if response.data.len() <= (4 << 20) + 65536
            && (response.nonce.len() == 12 || response.data.is_empty())
        {
            self.broadcast
                .send(WsServer::TerminalCheckpoint(response))
                .ok();
        }
    }

    pub(crate) fn chunk_at_sequence(
        &self,
        id: Sid,
        generation: u32,
        sequence: u64,
    ) -> Result<Option<u64>> {
        if self.shell_generation(id) != Some(generation) {
            bail!("terminal generation changed");
        }
        let shells = self.shells.read();
        let shell = shells
            .get(&id)
            .filter(|shell| !shell.closed)
            .context("terminal unavailable")?;
        if sequence < shell.byte_offset {
            bail!("checkpoint output fence is no longer retained");
        }
        if sequence > shell.seqnum {
            return Ok(None);
        }
        let mut position = shell.byte_offset;
        for (index, data) in shell.data.iter().enumerate() {
            if sequence < position + data.len() as u64 {
                return Ok(Some(shell.chunk_offset + index as u64));
            }
            position += data.len() as u64;
        }
        Ok(Some(shell.chunk_offset + shell.data.len() as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::tests::session;

    #[tokio::test]
    async fn snapshot_fences_resolve_at_inside_and_after_chunk_boundaries() {
        let session = session();
        let id = Sid(60);
        session
            .add_shell(id, (0, 0), 1, (24, 80), (640, 480), Default::default())
            .unwrap();
        session
            .add_data(id, Bytes::from_static(b"first"), 0)
            .unwrap();
        session
            .add_data(id, Bytes::from_static(b"second"), 5)
            .unwrap();
        assert_eq!(session.chunk_at_sequence(id, 0, 0).unwrap(), Some(0));
        assert_eq!(session.chunk_at_sequence(id, 0, 3).unwrap(), Some(0));
        assert_eq!(session.chunk_at_sequence(id, 0, 5).unwrap(), Some(1));
        assert_eq!(session.chunk_at_sequence(id, 0, 11).unwrap(), Some(2));
        assert_eq!(session.chunk_at_sequence(id, 0, 12).unwrap(), None);
        assert!(session.chunk_at_sequence(id, 1, 0).is_err());
        session
            .add_data(id, Bytes::from(vec![b'x'; SHELL_STORED_BYTES as usize]), 11)
            .unwrap();
        assert!(session.chunk_at_sequence(id, 0, 0).is_err());
    }
}
