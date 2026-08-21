use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinSet;
use tracing::{debug, info};

use crate::protocol::frame::Message;
use crate::protocol::wire::{
    Ack, Error, Frame, HelloAck, TerminalExited, TerminalList, TerminalOutput, WorkingDirectory,
};
use crate::protocol::{frame, read_frame, write_frame};
use crate::session::{
    BufferSnapshot, SessionEvent, SessionMap, TerminalSession, OUTPUT_CHUNK_BYTES,
};
use crate::{HOST_VERSION, PROTOCOL_VERSION};

const CONNECTION_QUEUE_CAPACITY: usize = 256;

#[derive(Clone)]
struct Host {
    authentication_token: Arc<[u8]>,
    sessions: SessionMap,
    shutdown_tx: watch::Sender<bool>,
}

impl Host {
    fn new(authentication_token: Vec<u8>, shutdown_tx: watch::Sender<bool>) -> Result<Self> {
        if authentication_token.len() < 32 {
            bail!("terminal-host authentication token must contain at least 32 bytes");
        }
        Ok(Self {
            authentication_token: authentication_token.into(),
            sessions: Arc::new(tokio::sync::RwLock::new(Default::default())),
            shutdown_tx,
        })
    }

    async fn handle_connection<S>(&self, stream: S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let (mut reader, mut writer) = tokio::io::split(stream);
        let hello = tokio::time::timeout(Duration::from_secs(5), read_frame(&mut reader))
            .await
            .context("terminal-host handshake timed out")??
            .context("terminal-host client disconnected before handshake")?;
        let request_id = hello.request_id;
        let Message::Hello(hello) = hello.message.context("terminal-host handshake is empty")?
        else {
            bail!("first terminal-host message must be hello");
        };
        if hello.minimum_protocol_version > PROTOCOL_VERSION
            || hello.maximum_protocol_version < PROTOCOL_VERSION
        {
            write_frame(
                &mut writer,
                &error_frame(
                    request_id,
                    "INCOMPATIBLE_PROTOCOL",
                    format!(
                        "host supports protocol {PROTOCOL_VERSION}, client supports {}..{}",
                        hello.minimum_protocol_version, hello.maximum_protocol_version
                    ),
                    "",
                ),
            )
            .await?;
            bail!("client and host protocol versions do not overlap");
        }
        if !tokens_equal(&hello.authentication_token, &self.authentication_token) {
            write_frame(
                &mut writer,
                &error_frame(
                    request_id,
                    "AUTHENTICATION_FAILED",
                    "terminal-host authentication failed",
                    "",
                ),
            )
            .await?;
            bail!("terminal-host authentication failed");
        }

        write_frame(
            &mut writer,
            &frame(
                request_id,
                Message::HelloAck(HelloAck {
                    selected_protocol_version: PROTOCOL_VERSION,
                    host_version: HOST_VERSION.into(),
                    host_restart_is_disruptive: true,
                }),
            ),
        )
        .await?;

        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<Frame>(CONNECTION_QUEUE_CAPACITY);
        let writer_task = tokio::spawn(async move {
            while let Some(frame) = outgoing_rx.recv().await {
                write_frame(&mut writer, &frame).await?;
            }
            Result::<()>::Ok(())
        });
        let mut subscriptions = JoinSet::new();

        while let Some(incoming_frame) = read_frame(&mut reader).await? {
            if incoming_frame.protocol_version != PROTOCOL_VERSION {
                send_error(
                    &outgoing_tx,
                    incoming_frame.request_id,
                    "INCOMPATIBLE_PROTOCOL",
                    "message protocol version does not match the negotiated version",
                    "",
                )
                .await;
                continue;
            }
            let request_id = incoming_frame.request_id;
            let Some(message) = incoming_frame.message else {
                send_error(
                    &outgoing_tx,
                    request_id,
                    "INVALID_REQUEST",
                    "request message is empty",
                    "",
                )
                .await;
                continue;
            };
            match message {
                Message::CreateTerminal(request) => {
                    let terminal_id = request.terminal_id.clone();
                    if self.sessions.read().await.contains_key(&terminal_id) {
                        send_error(
                            &outgoing_tx,
                            request_id,
                            "ALREADY_EXISTS",
                            "terminal ID already exists; attach instead of recreating it",
                            &terminal_id,
                        )
                        .await;
                        continue;
                    }
                    let result =
                        tokio::task::spawn_blocking(move || TerminalSession::spawn(request))
                            .await
                            .context("terminal creation task failed")?;
                    match result {
                        Ok(session) => {
                            let mut sessions = self.sessions.write().await;
                            if sessions.contains_key(&terminal_id) {
                                session.close().ok();
                                send_error(
                                    &outgoing_tx,
                                    request_id,
                                    "ALREADY_EXISTS",
                                    "terminal ID was created concurrently",
                                    &terminal_id,
                                )
                                .await;
                            } else {
                                sessions.insert(terminal_id.clone(), session);
                                send_ack(&outgoing_tx, request_id, &terminal_id).await;
                            }
                        }
                        Err(error) => {
                            send_error(
                                &outgoing_tx,
                                request_id,
                                "CREATE_FAILED",
                                error.to_string(),
                                &terminal_id,
                            )
                            .await;
                        }
                    }
                }
                Message::AttachTerminal(request) => {
                    let session = self
                        .sessions
                        .read()
                        .await
                        .get(&request.terminal_id)
                        .cloned();
                    let Some(session) = session else {
                        send_error(
                            &outgoing_tx,
                            request_id,
                            "NOT_FOUND",
                            "terminal does not exist",
                            &request.terminal_id,
                        )
                        .await;
                        continue;
                    };
                    let subscriber = session.subscribe();
                    let snapshot = session.snapshot_after(request.after_sequence);
                    send_ack(&outgoing_tx, request_id, session.id()).await;
                    send_snapshot(&outgoing_tx, session.id(), &snapshot, true).await?;
                    if let Some(event) = session.exit_event() {
                        send_event(&outgoing_tx, session.id(), event, false).await?;
                    } else {
                        let outgoing_tx = outgoing_tx.clone();
                        subscriptions.spawn(async move {
                            stream_session(session, subscriber, snapshot.next_sequence, outgoing_tx)
                                .await
                        });
                    }
                }
                Message::TerminalInput(request) => {
                    let result = match self.sessions.read().await.get(&request.terminal_id) {
                        Some(session) => session.send_input(request.data),
                        None => Err(anyhow::anyhow!("terminal does not exist")),
                    };
                    match result {
                        Ok(()) => send_ack(&outgoing_tx, request_id, &request.terminal_id).await,
                        Err(error) => {
                            send_error(
                                &outgoing_tx,
                                request_id,
                                "INPUT_FAILED",
                                error.to_string(),
                                &request.terminal_id,
                            )
                            .await
                        }
                    }
                }
                Message::ResizeTerminal(request) => {
                    let result = match self.sessions.read().await.get(&request.terminal_id) {
                        Some(session) => session.resize(request.rows, request.columns),
                        None => Err(anyhow::anyhow!("terminal does not exist")),
                    };
                    match result {
                        Ok(()) => send_ack(&outgoing_tx, request_id, &request.terminal_id).await,
                        Err(error) => {
                            send_error(
                                &outgoing_tx,
                                request_id,
                                "RESIZE_FAILED",
                                error.to_string(),
                                &request.terminal_id,
                            )
                            .await
                        }
                    }
                }
                Message::CloseTerminal(request) => {
                    let session = self.sessions.write().await.remove(&request.terminal_id);
                    match session {
                        Some(session) => {
                            session.close().ok();
                            send_ack(&outgoing_tx, request_id, &request.terminal_id).await;
                        }
                        None => {
                            // Closing an already-closed terminal is idempotent.
                            send_ack(&outgoing_tx, request_id, &request.terminal_id).await;
                        }
                    }
                }
                Message::ListTerminals(_) => {
                    let terminals = self
                        .sessions
                        .read()
                        .await
                        .values()
                        .map(|session| session.summary())
                        .collect();
                    outgoing_tx
                        .send(frame(
                            request_id,
                            Message::TerminalList(TerminalList { terminals }),
                        ))
                        .await
                        .ok();
                }
                Message::ShutdownHost(request) => {
                    let active = self
                        .sessions
                        .read()
                        .await
                        .values()
                        .filter(|session| session.is_running())
                        .count();
                    if active > 0 && !request.force {
                        send_error(
                            &outgoing_tx,
                            request_id,
                            "ACTIVE_TERMINALS",
                            format!(
                                "host restart would disconnect {active} active terminal process(es); recover them manually before forcing shutdown"
                            ),
                            "",
                        )
                        .await;
                    } else {
                        if request.force {
                            let sessions = std::mem::take(&mut *self.sessions.write().await);
                            for session in sessions.into_values() {
                                session.close().ok();
                            }
                        }
                        send_ack(&outgoing_tx, request_id, "").await;
                        let shutdown_tx = self.shutdown_tx.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            shutdown_tx.send(true).ok();
                        });
                    }
                }
                Message::GetWorkingDirectory(request) => {
                    let session = self
                        .sessions
                        .read()
                        .await
                        .get(&request.terminal_id)
                        .cloned();
                    match session {
                        Some(session) => {
                            let path = session
                                .working_directory()
                                .map(|path| path.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            outgoing_tx
                                .send(frame(
                                    request_id,
                                    Message::WorkingDirectory(WorkingDirectory {
                                        terminal_id: request.terminal_id,
                                        path,
                                    }),
                                ))
                                .await
                                .ok();
                        }
                        None => {
                            send_error(
                                &outgoing_tx,
                                request_id,
                                "NOT_FOUND",
                                "terminal does not exist",
                                &request.terminal_id,
                            )
                            .await;
                        }
                    }
                }
                Message::Hello(_)
                | Message::HelloAck(_)
                | Message::Ack(_)
                | Message::TerminalOutput(_)
                | Message::TerminalExited(_)
                | Message::TerminalList(_)
                | Message::WorkingDirectory(_)
                | Message::Error(_) => {
                    send_error(
                        &outgoing_tx,
                        request_id,
                        "INVALID_REQUEST",
                        "client sent a response-only message",
                        "",
                    )
                    .await;
                }
            }
        }

        subscriptions.abort_all();
        drop(outgoing_tx);
        writer_task
            .await
            .context("terminal-host writer task failed")??;
        Ok(())
    }
}

async fn stream_session(
    session: Arc<TerminalSession>,
    mut subscriber: broadcast::Receiver<SessionEvent>,
    mut next_sequence: u64,
    outgoing_tx: mpsc::Sender<Frame>,
) -> Result<()> {
    loop {
        match subscriber.recv().await {
            Ok(SessionEvent::Output { sequence, data }) => {
                let end = sequence.saturating_add(data.len() as u64);
                if end <= next_sequence {
                    continue;
                }
                if sequence > next_sequence {
                    let snapshot = session.snapshot_after(next_sequence);
                    send_snapshot(&outgoing_tx, session.id(), &snapshot, true).await?;
                    next_sequence = snapshot.next_sequence;
                    continue;
                }
                let offset = next_sequence.saturating_sub(sequence) as usize;
                let data = data[offset..].to_vec();
                outgoing_tx
                    .send(frame(
                        0,
                        Message::TerminalOutput(TerminalOutput {
                            terminal_id: session.id().into(),
                            sequence: next_sequence,
                            data,
                            replay: false,
                        }),
                    ))
                    .await
                    .context("terminal subscriber disconnected")?;
                next_sequence = end;
            }
            Ok(event @ SessionEvent::Exited { .. }) => {
                send_event(&outgoing_tx, session.id(), event, false).await?;
                return Ok(());
            }
            Err(broadcast::error::RecvError::Lagged(_)) => {
                let snapshot = session.snapshot_after(next_sequence);
                send_snapshot(&outgoing_tx, session.id(), &snapshot, true).await?;
                next_sequence = snapshot.next_sequence;
            }
            Err(broadcast::error::RecvError::Closed) => return Ok(()),
        }
    }
}

async fn send_snapshot(
    outgoing_tx: &mpsc::Sender<Frame>,
    terminal_id: &str,
    snapshot: &BufferSnapshot,
    replay: bool,
) -> Result<()> {
    for (index, data) in snapshot.bytes.chunks(OUTPUT_CHUNK_BYTES).enumerate() {
        outgoing_tx
            .send(frame(
                0,
                Message::TerminalOutput(TerminalOutput {
                    terminal_id: terminal_id.into(),
                    sequence: snapshot.sequence + (index * OUTPUT_CHUNK_BYTES) as u64,
                    data: data.to_vec(),
                    replay,
                }),
            ))
            .await
            .context("terminal subscriber disconnected")?;
    }
    Ok(())
}

async fn send_event(
    outgoing_tx: &mpsc::Sender<Frame>,
    terminal_id: &str,
    event: SessionEvent,
    replay: bool,
) -> Result<()> {
    let message = match event {
        SessionEvent::Output { sequence, data } => Message::TerminalOutput(TerminalOutput {
            terminal_id: terminal_id.into(),
            sequence,
            data: data.to_vec(),
            replay,
        }),
        SessionEvent::Exited { exit_code, signal } => Message::TerminalExited(TerminalExited {
            terminal_id: terminal_id.into(),
            exit_code,
            signal,
        }),
    };
    outgoing_tx
        .send(frame(0, message))
        .await
        .context("terminal subscriber disconnected")
}

async fn send_ack(outgoing_tx: &mpsc::Sender<Frame>, request_id: u64, terminal_id: &str) {
    outgoing_tx
        .send(frame(
            request_id,
            Message::Ack(Ack {
                terminal_id: terminal_id.into(),
            }),
        ))
        .await
        .ok();
}

async fn send_error(
    outgoing_tx: &mpsc::Sender<Frame>,
    request_id: u64,
    code: impl Into<String>,
    message: impl Into<String>,
    terminal_id: impl Into<String>,
) {
    outgoing_tx
        .send(error_frame(request_id, code, message, terminal_id))
        .await
        .ok();
}

fn error_frame(
    request_id: u64,
    code: impl Into<String>,
    message: impl Into<String>,
    terminal_id: impl Into<String>,
) -> Frame {
    frame(
        request_id,
        Message::Error(Error {
            code: code.into(),
            message: message.into(),
            terminal_id: terminal_id.into(),
        }),
    )
}

fn tokens_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(
            left.get(index).copied().unwrap_or_default()
                ^ right.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

/// Serve the local terminal-host endpoint until an authenticated client asks
/// for shutdown. The host never exits merely because all daemon connections
/// have disconnected.
pub async fn serve(endpoint: &str, authentication_token: Vec<u8>) -> Result<()> {
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let host = Host::new(authentication_token, shutdown_tx)?;
    serve_transport(endpoint, host, shutdown_rx).await
}

#[cfg(unix)]
async fn serve_transport(
    endpoint: &str,
    host: Host,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use tokio::net::{UnixListener, UnixStream};

    let path = Path::new(endpoint);
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .context("terminal-host endpoint needs a parent directory")?;
    tokio::fs::create_dir_all(parent).await?;
    tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).await?;
    if tokio::fs::try_exists(path).await? {
        if UnixStream::connect(path).await.is_ok() {
            bail!("terminal host is already listening at {endpoint}");
        }
        tokio::fs::remove_file(path)
            .await
            .context("failed to remove stale terminal-host socket")?;
    }
    let listener = UnixListener::bind(path)
        .with_context(|| format!("failed to bind terminal-host socket {endpoint}"))?;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).await?;
    let _socket_guard = SocketGuard(PathBuf::from(path));
    info!(endpoint, "terminal host is ready");

    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted.context("failed to accept terminal-host client")?;
                let host = host.clone();
                tokio::spawn(async move {
                    if let Err(error) = host.handle_connection(stream).await {
                        debug!(?error, "terminal-host client disconnected");
                    }
                });
            }
        }
    }
    info!("terminal host stopped");
    Ok(())
}

#[cfg(unix)]
struct SocketGuard(std::path::PathBuf);

#[cfg(unix)]
impl Drop for SocketGuard {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).ok();
    }
}

#[cfg(windows)]
async fn serve_transport(
    endpoint: &str,
    host: Host,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let mut first_instance = true;
    loop {
        let server = ServerOptions::new()
            .first_pipe_instance(first_instance)
            .create(endpoint)
            .with_context(|| format!("failed to create terminal-host pipe {endpoint}"))?;
        first_instance = false;
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow() {
                    break;
                }
            }
            connected = server.connect() => {
                connected.context("failed to accept terminal-host pipe client")?;
                let host = host.clone();
                tokio::spawn(async move {
                    if let Err(error) = host.handle_connection(server).await {
                        debug!(?error, "terminal-host client disconnected");
                    }
                });
            }
        }
    }
    info!("terminal host stopped");
    Ok(())
}
