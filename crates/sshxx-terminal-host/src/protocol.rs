use std::io::ErrorKind;

use anyhow::{bail, Context, Result};
use prost::Message;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub mod wire {
    include!(concat!(env!("OUT_DIR"), "/sshxx.terminal_host.v1.rs"));
}

pub use wire::frame;
pub use wire::Frame;

/// Keep malformed or unauthenticated local clients from forcing large
/// allocations. PTY data is chunked well below this limit.
pub const MAX_FRAME_BYTES: usize = 1 << 20;

/// Cancellation-safe framed reader.
///
/// `AsyncReadExt::read_exact` does not retain partial progress when its future
/// is cancelled by `select!`. Keep incomplete bytes on this object so terminal
/// input and resize events cannot desynchronize the host transport.
pub struct FrameReader<R> {
    inner: R,
    buffer: Vec<u8>,
    frame_length: Option<usize>,
}

impl<R> FrameReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buffer: Vec::new(),
            frame_length: None,
        }
    }
}

impl<R> FrameReader<R>
where
    R: AsyncRead + Unpin,
{
    pub async fn read_frame(&mut self) -> Result<Option<Frame>> {
        loop {
            if self.frame_length.is_none() && self.buffer.len() >= 4 {
                let length = u32::from_be_bytes([
                    self.buffer[0],
                    self.buffer[1],
                    self.buffer[2],
                    self.buffer[3],
                ]) as usize;
                if length == 0 || length > MAX_FRAME_BYTES {
                    bail!("invalid terminal-host frame length: {length}");
                }
                self.frame_length = Some(length);
            }

            if let Some(length) = self.frame_length {
                let frame_end = 4 + length;
                if self.buffer.len() >= frame_end {
                    let frame = Frame::decode(&self.buffer[4..frame_end])
                        .context("failed to decode terminal-host frame")?;
                    self.buffer.drain(..frame_end);
                    self.frame_length = None;
                    return Ok(Some(frame));
                }
            }

            let mut chunk = [0; 8 << 10];
            let read = self
                .inner
                .read(&mut chunk)
                .await
                .context("failed to read terminal-host frame")?;
            if read == 0 {
                if self.buffer.is_empty() {
                    return Ok(None);
                }
                bail!("terminal host disconnected during a frame");
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }
}

pub async fn read_frame<R>(reader: &mut R) -> Result<Option<Frame>>
where
    R: AsyncRead + Unpin,
{
    let mut length = [0u8; 4];
    match reader.read_exact(&mut length).await {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error).context("failed to read frame length"),
    }
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        bail!("invalid terminal-host frame length: {length}");
    }
    let mut payload = vec![0; length];
    reader
        .read_exact(&mut payload)
        .await
        .context("failed to read frame payload")?;
    Frame::decode(payload.as_slice())
        .context("failed to decode terminal-host frame")
        .map(Some)
}

pub async fn write_frame<W>(writer: &mut W, frame: &Frame) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let length = frame.encoded_len();
    if length == 0 || length > MAX_FRAME_BYTES {
        bail!("invalid terminal-host frame length: {length}");
    }
    writer
        .write_all(&(length as u32).to_be_bytes())
        .await
        .context("failed to write frame length")?;
    let mut payload = Vec::with_capacity(length);
    frame
        .encode(&mut payload)
        .context("failed to encode terminal-host frame")?;
    writer
        .write_all(&payload)
        .await
        .context("failed to write frame payload")?;
    writer.flush().await.context("failed to flush frame")
}

pub fn frame(request_id: u64, message: frame::Message) -> Frame {
    Frame {
        protocol_version: crate::PROTOCOL_VERSION,
        request_id,
        message: Some(message),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::protocol::wire::ListTerminals;

    #[tokio::test]
    async fn framed_reader_retains_partial_bytes_when_cancelled() {
        let expected = frame(42, frame::Message::ListTerminals(ListTerminals {}));
        let mut encoded = Vec::new();
        encoded.extend_from_slice(&(expected.encoded_len() as u32).to_be_bytes());
        expected.encode(&mut encoded).unwrap();

        let (mut writer, reader) = tokio::io::duplex(128);
        let mut reader = FrameReader::new(reader);
        writer.write_all(&encoded[..2]).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(10), reader.read_frame())
                .await
                .is_err()
        );

        writer.write_all(&encoded[2..]).await.unwrap();
        let actual = reader.read_frame().await.unwrap().unwrap();
        assert_eq!(actual, expected);
    }
}
