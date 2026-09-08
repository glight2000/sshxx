use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::task::{Context as TaskContext, Poll};

use prost::Message as _;
use tokio::io::{AsyncWriteExt, DuplexStream, ReadBuf};
use tokio::task::JoinHandle;

use super::*;
use crate::protocol::wire::{Hello, ListTerminals};

const TOKEN: [u8; 32] = [0x52; 32];

// Fault only the host's writes; its read half remains connected. This
// reproduces the previously invisible, half-dead output connection.
struct ControlledIo {
    inner: DuplexStream,
    mode: Arc<AtomicU8>,
    dropped: Arc<AtomicBool>,
}

impl AsyncRead for ControlledIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for ControlledIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.mode.load(Ordering::SeqCst) {
            1 => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "injected writer failure",
            ))),
            2 => Poll::Pending,
            _ => Pin::new(&mut self.inner).poll_write(cx, bytes),
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl Drop for ControlledIo {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

struct TestConnection {
    peer: DuplexStream,
    mode: Arc<AtomicU8>,
    dropped: Arc<AtomicBool>,
    task: JoinHandle<Result<()>>,
}

async fn connect(host: &Host) -> TestConnection {
    let (mut peer, inner) = tokio::io::duplex(4096);
    let mode = Arc::new(AtomicU8::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let io = ControlledIo {
        inner,
        mode: mode.clone(),
        dropped: dropped.clone(),
    };
    let host = host.clone();
    let task = tokio::spawn(async move { host.handle_connection(io).await });
    write_frame(
        &mut peer,
        &frame(
            1,
            Message::Hello(Hello {
                minimum_protocol_version: PROTOCOL_VERSION,
                maximum_protocol_version: PROTOCOL_VERSION,
                client_version: "fault-test".into(),
                authentication_token: TOKEN.to_vec(),
            }),
        ),
    )
    .await
    .unwrap();
    assert!(matches!(
        read_frame(&mut peer).await.unwrap().unwrap().message,
        Some(Message::HelloAck(_))
    ));
    TestConnection {
        peer,
        mode,
        dropped,
        task,
    }
}

fn host() -> Host {
    Host::new(TOKEN.to_vec(), watch::channel(None).0).unwrap()
}

#[tokio::test]
async fn writer_failure_closes_only_that_connection() {
    let host = host();
    let mut faulty = connect(&host).await;
    let mut healthy = connect(&host).await;
    faulty.mode.store(1, Ordering::SeqCst);
    write_frame(
        &mut faulty.peer,
        &frame(2, Message::ListTerminals(ListTerminals {})),
    )
    .await
    .unwrap();
    let error = tokio::time::timeout(Duration::from_secs(1), &mut faulty.task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(format!("{error:#}").contains("injected writer failure"));
    assert!(faulty.dropped.load(Ordering::SeqCst));
    assert!(read_frame(&mut faulty.peer).await.unwrap().is_none());
    write_frame(
        &mut healthy.peer,
        &frame(2, Message::ListTerminals(ListTerminals {})),
    )
    .await
    .unwrap();
    assert!(matches!(
        read_frame(&mut healthy.peer)
            .await
            .unwrap()
            .unwrap()
            .message,
        Some(Message::TerminalList(_))
    ));
    drop(healthy.peer);
    tokio::time::timeout(Duration::from_secs(1), healthy.task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn failed_or_cancelled_reader_does_not_leave_a_detached_writer() {
    for abort in [false, true] {
        let mut connection = connect(&host()).await;
        connection.mode.store(2, Ordering::SeqCst);
        write_frame(
            &mut connection.peer,
            &frame(2, Message::ListTerminals(ListTerminals {})),
        )
        .await
        .unwrap();
        tokio::task::yield_now().await;
        if abort {
            connection.task.abort();
        } else {
            connection
                .peer
                .write_all(&0u32.to_be_bytes())
                .await
                .unwrap();
        }
        let result = tokio::time::timeout(Duration::from_secs(1), connection.task)
            .await
            .unwrap();
        if abort {
            assert!(result.unwrap_err().is_cancelled());
        } else {
            assert!(result.unwrap().is_err());
        }
        assert!(connection.dropped.load(Ordering::SeqCst));
    }
}

#[tokio::test]
async fn subscription_errors_and_panics_are_observed_without_requests() {
    for panic in [false, true] {
        let (_peer, inner) = tokio::io::duplex(128);
        let mut reader = FrameReader::new(inner);
        let mut subscriptions = JoinSet::new();
        subscriptions.spawn(async move {
            assert!(!panic, "injected subscription panic");
            bail!("injected subscription failure");
        });
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            next_request(&mut reader, &mut subscriptions),
        )
        .await
        .unwrap();
        assert!(result.is_err());
        assert!(subscriptions.is_empty());
    }
}

#[tokio::test]
async fn successful_subscription_exit_preserves_partial_control_frame() {
    let (mut peer, inner) = tokio::io::duplex(128);
    let mut reader = FrameReader::new(inner);
    let mut subscriptions = JoinSet::new();
    let expected = frame(2, Message::ListTerminals(ListTerminals {}));
    let mut bytes = (expected.encoded_len() as u32).to_be_bytes().to_vec();
    expected.encode(&mut bytes).unwrap();
    peer.write_all(&bytes[..2]).await.unwrap();
    assert!(tokio::time::timeout(
        Duration::from_millis(10),
        next_request(&mut reader, &mut subscriptions)
    )
    .await
    .is_err());
    subscriptions.spawn(async { Ok(()) });
    assert!(tokio::time::timeout(
        Duration::from_millis(10),
        next_request(&mut reader, &mut subscriptions)
    )
    .await
    .is_err());
    assert!(subscriptions.is_empty());
    peer.write_all(&bytes[2..]).await.unwrap();
    assert_eq!(
        next_request(&mut reader, &mut subscriptions).await.unwrap(),
        Some(expected)
    );
}

#[cfg(unix)]
#[tokio::test]
async fn broken_output_connection_preserves_the_owned_pty_and_replay() {
    use crate::protocol::wire::{AttachTerminal, CreateTerminal};
    let host = host();
    let state = tempfile::tempdir().unwrap();
    let session = TerminalSession::spawn(CreateTerminal {
        terminal_id: "fault-test-terminal".into(),
        program: "/bin/sh".into(),
        args: vec![
            "-c".into(),
            "read value; printf '\\033[?2004hSTILL_RUNNING\\n'; read value".into(),
        ],
        working_directory: state.path().to_string_lossy().into_owned(),
        environment: Default::default(),
        rows: 24,
        columns: 80,
    })
    .unwrap();
    let cleanup = PendingTerminal(Some(session.clone()));
    let pid = session.summary().process_id;
    host.sessions
        .write()
        .await
        .insert(session.id().into(), session.clone());
    let mut broken = connect(&host).await;
    broken.mode.store(1, Ordering::SeqCst);
    write_frame(
        &mut broken.peer,
        &frame(
            2,
            Message::AttachTerminal(AttachTerminal {
                terminal_id: session.id().into(),
                after_sequence: 0,
            }),
        ),
    )
    .await
    .unwrap();
    assert!(tokio::time::timeout(Duration::from_secs(1), broken.task)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert!(session.is_running());
    assert_eq!(session.summary().process_id, pid);
    session.send_input(b"continue\n".to_vec()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while !session
            .snapshot_after(0)
            .bytes
            .windows(13)
            .any(|bytes| bytes == b"STILL_RUNNING")
        {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let replay_from = session
        .snapshot_after(0)
        .bytes
        .windows(13)
        .position(|bytes| bytes == b"STILL_RUNNING")
        .unwrap() as u64;
    let mut reattached = connect(&host).await;
    write_frame(
        &mut reattached.peer,
        &frame(
            2,
            Message::AttachTerminal(AttachTerminal {
                terminal_id: session.id().into(),
                after_sequence: replay_from,
            }),
        ),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(Message::TerminalOutput(output)) = read_frame(&mut reattached.peer)
                .await
                .unwrap()
                .unwrap()
                .message
            {
                if output
                    .data
                    .windows(13)
                    .any(|bytes| bytes == b"STILL_RUNNING")
                {
                    assert_eq!(
                        crate::paste_mode::PasteMode::restore(&output.paste_checkpoint)
                            .unwrap()
                            .enabled(),
                        Some(true)
                    );
                    assert!(!output.data.windows(8).any(|bytes| bytes == b"\x1b[?2004h"));
                    break;
                }
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(session.summary().process_id, pid);
    assert!(session.is_running());
    drop(cleanup); // Only the disposable test PTY is closed, even on assertion failure.
    drop(reattached.peer);
    let _ = tokio::time::timeout(Duration::from_secs(1), reattached.task)
        .await
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn cancelled_creation_closes_only_the_unregistered_pty() {
    use crate::protocol::wire::CreateTerminal;
    let state = tempfile::tempdir().unwrap();
    let session = TerminalSession::spawn(CreateTerminal {
        terminal_id: "cancelled-create".into(),
        program: "/bin/sh".into(),
        args: vec!["-c".into(), "read value".into()],
        working_directory: state.path().to_string_lossy().into_owned(),
        environment: Default::default(),
        rows: 24,
        columns: 80,
    })
    .unwrap();
    let pending = PendingTerminal(Some(session.clone()));
    let (release, ready) = std::sync::mpsc::channel();
    let mut worker = tokio::task::spawn_blocking(move || {
        ready.recv().unwrap();
        pending
    });
    // A completed creation result must be cleaned up if its awaiter vanished.
    assert!(tokio::time::timeout(Duration::from_millis(10), &mut worker)
        .await
        .is_err());
    drop(worker);
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        while session.is_running() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}
