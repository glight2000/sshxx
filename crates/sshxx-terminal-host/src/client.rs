use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncRead, AsyncWrite, ReadHalf, WriteHalf};

use crate::protocol::frame::Message;
use crate::protocol::wire::{
    AttachTerminal, CloseTerminal, CreateTerminal, Frame, GetWorkingDirectory, Hello,
    ListTerminals, ResizeTerminal, ShutdownHost, TerminalInput,
};
use crate::protocol::{frame, write_frame, FrameReader};
use crate::PROTOCOL_VERSION;

trait LocalIo: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T> LocalIo for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

type BoxedLocalIo = Box<dyn LocalIo>;

/// A low-level protocol client. sshxx-daemon owns routing and acknowledgement
/// policy; this type deliberately exposes asynchronous host events unchanged.
pub struct Client {
    reader: FrameReader<ReadHalf<BoxedLocalIo>>,
    writer: WriteHalf<BoxedLocalIo>,
    next_request_id: u64,
    host_version: String,
    host_restart_is_disruptive: bool,
}

impl Client {
    pub async fn connect(
        endpoint: &str,
        authentication_token: Vec<u8>,
        client_version: impl Into<String>,
    ) -> Result<Self> {
        let stream = connect_transport(endpoint).await?;
        let (reader, writer) = tokio::io::split(stream);
        let mut client = Self {
            reader: FrameReader::new(reader),
            writer,
            next_request_id: 1,
            host_version: String::new(),
            host_restart_is_disruptive: true,
        };
        let request_id = client.next_request_id();
        client
            .send_frame(frame(
                request_id,
                Message::Hello(Hello {
                    minimum_protocol_version: PROTOCOL_VERSION,
                    maximum_protocol_version: PROTOCOL_VERSION,
                    client_version: client_version.into(),
                    authentication_token,
                }),
            ))
            .await?;
        let response = tokio::time::timeout(Duration::from_secs(5), client.receive())
            .await
            .context("terminal-host handshake timed out")??
            .context("terminal host disconnected during handshake")?;
        match response.message {
            Some(Message::HelloAck(ack))
                if response.request_id == request_id
                    && ack.selected_protocol_version == PROTOCOL_VERSION =>
            {
                client.host_version = ack.host_version;
                client.host_restart_is_disruptive = ack.host_restart_is_disruptive;
            }
            Some(Message::Error(error)) => bail!("{}: {}", error.code, error.message),
            _ => bail!("terminal host returned an invalid handshake response"),
        }
        Ok(client)
    }

    pub fn host_version(&self) -> &str {
        &self.host_version
    }

    pub fn host_restart_is_disruptive(&self) -> bool {
        self.host_restart_is_disruptive
    }

    pub async fn create_terminal(&mut self, request: CreateTerminal) -> Result<u64> {
        self.send(Message::CreateTerminal(request)).await
    }

    pub async fn attach_terminal(
        &mut self,
        terminal_id: impl Into<String>,
        after_sequence: u64,
    ) -> Result<u64> {
        self.send(Message::AttachTerminal(AttachTerminal {
            terminal_id: terminal_id.into(),
            after_sequence,
        }))
        .await
    }

    pub async fn input(&mut self, terminal_id: impl Into<String>, data: Vec<u8>) -> Result<u64> {
        self.send(Message::TerminalInput(TerminalInput {
            terminal_id: terminal_id.into(),
            data,
        }))
        .await
    }

    pub async fn resize(
        &mut self,
        terminal_id: impl Into<String>,
        rows: u32,
        columns: u32,
    ) -> Result<u64> {
        self.send(Message::ResizeTerminal(ResizeTerminal {
            terminal_id: terminal_id.into(),
            rows,
            columns,
        }))
        .await
    }

    pub async fn close_terminal(&mut self, terminal_id: impl Into<String>) -> Result<u64> {
        self.send(Message::CloseTerminal(CloseTerminal {
            terminal_id: terminal_id.into(),
        }))
        .await
    }

    pub async fn list_terminals(&mut self) -> Result<u64> {
        self.send(Message::ListTerminals(ListTerminals {})).await
    }

    pub async fn get_working_directory(&mut self, terminal_id: impl Into<String>) -> Result<u64> {
        self.send(Message::GetWorkingDirectory(GetWorkingDirectory {
            terminal_id: terminal_id.into(),
        }))
        .await
    }

    pub async fn shutdown(&mut self, force: bool) -> Result<u64> {
        self.send(Message::ShutdownHost(ShutdownHost {
            force,
            restart: false,
        }))
        .await
    }

    pub async fn restart(&mut self, force: bool) -> Result<u64> {
        self.send(Message::ShutdownHost(ShutdownHost {
            force,
            restart: true,
        }))
        .await
    }

    pub async fn receive(&mut self) -> Result<Option<Frame>> {
        self.reader.read_frame().await
    }

    async fn send(&mut self, message: Message) -> Result<u64> {
        let request_id = self.next_request_id();
        self.send_frame(frame(request_id, message)).await?;
        Ok(request_id)
    }

    async fn send_frame(&mut self, frame: Frame) -> Result<()> {
        write_frame(&mut self.writer, &frame).await
    }

    fn next_request_id(&mut self) -> u64 {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1).max(1);
        request_id
    }
}

#[cfg(unix)]
async fn connect_transport(endpoint: &str) -> Result<BoxedLocalIo> {
    let stream = tokio::net::UnixStream::connect(endpoint)
        .await
        .with_context(|| format!("failed to connect to terminal host at {endpoint}"))?;
    Ok(Box::new(stream))
}

#[cfg(windows)]
async fn connect_transport(endpoint: &str) -> Result<BoxedLocalIo> {
    use std::io::ErrorKind;

    use tokio::net::windows::named_pipe::ClientOptions;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match ClientOptions::new().open(endpoint) {
            Ok(client) => return Ok(Box::new(client)),
            Err(error)
                if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::NotFound)
                    && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to connect to terminal host at {endpoint}"));
            }
        }
    }
}
