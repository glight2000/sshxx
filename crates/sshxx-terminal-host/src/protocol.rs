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
