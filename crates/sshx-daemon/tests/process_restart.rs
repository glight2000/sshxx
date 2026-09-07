#![cfg(unix)]

use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use sshx_core::proto::sshx_service_server::{SshxService, SshxServiceServer};
use sshx_core::proto::{self, client_update::ClientMessage, server_update::ServerMessage};
use sshx_daemon::terminal_host::TerminalHostConfig;
use sshxx_terminal_host::{client::Client, protocol::frame::Message};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

type Channel = (
    Streaming<proto::ClientUpdate>,
    mpsc::Sender<Result<proto::ServerUpdate, Status>>,
);

#[derive(Clone)]
struct Bridge {
    opens: mpsc::Sender<proto::OpenRequest>,
    channels: mpsc::Sender<Channel>,
}

#[tonic::async_trait]
impl SshxService for Bridge {
    type ChannelStream = ReceiverStream<Result<proto::ServerUpdate, Status>>;

    async fn open(
        &self,
        request: Request<proto::OpenRequest>,
    ) -> Result<Response<proto::OpenResponse>, Status> {
        self.opens.send(request.into_inner()).await.unwrap();
        Ok(Response::new(proto::OpenResponse {
            name: "restart-test".into(),
            token: "test-token".into(),
            url: "http://localhost/s/restart-test".into(),
            process_restart_supported: true,
        }))
    }

    async fn channel(
        &self,
        request: Request<Streaming<proto::ClientUpdate>>,
    ) -> Result<Response<Self::ChannelStream>, Status> {
        let (tx, rx) = mpsc::channel(16);
        self.channels
            .send((request.into_inner(), tx))
            .await
            .unwrap();
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn close(
        &self,
        _: Request<proto::CloseRequest>,
    ) -> Result<Response<proto::CloseResponse>, Status> {
        Err(Status::internal(
            "restart must not close the server session",
        ))
    }
}

async fn send(
    tx: &mpsc::Sender<Result<proto::ServerUpdate, Status>>,
    message: ServerMessage,
) -> Result<()> {
    tx.send(Ok(proto::ServerUpdate {
        server_message: Some(message),
    }))
    .await?;
    Ok(())
}

async fn update(stream: &mut Streaming<proto::ClientUpdate>) -> Result<ClientMessage> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(message) = stream
                .message()
                .await?
                .context("daemon disconnected")?
                .client_message
            {
                return Ok(message);
            }
        }
    })
    .await?
}

async fn terminals(
    client: &mut Client,
) -> Result<Vec<sshxx_terminal_host::protocol::wire::TerminalSummary>> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let id = client.list_terminals().await?;
            let response = client.receive().await?.context("host disconnected")?;
            if response.request_id == id {
                if let Some(Message::TerminalList(list)) = response.message {
                    if !list.terminals.is_empty() {
                        return Ok(list.terminals);
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn daemon_process_restart_preserves_identity_workspace_and_pty() -> Result<()> {
    let state = tempfile::tempdir()?;
    let host_directory = state.path().join("cache/terminal-host");
    std::fs::create_dir_all(&host_directory)?;
    std::fs::write(host_directory.join("host.token"), [0x5a; 32])?;
    let host_config = TerminalHostConfig::load(&host_directory)?;
    let config = host_config.clone();
    let host = tokio::spawn(async move {
        sshxx_terminal_host::server::serve(&config.endpoint, config.authentication_token).await
    });
    let mut host_client = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(client) = Client::connect(
                &host_config.endpoint,
                host_config.authentication_token.clone(),
                "test",
            )
            .await
            {
                break client;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let (incoming_tx, incoming_rx) = mpsc::channel(8);
    let accept = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            if incoming_tx
                .send(Ok::<_, std::io::Error>(stream))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let (opens_tx, mut opens) = mpsc::channel(8);
    let (channels_tx, mut channels) = mpsc::channel(8);
    let server = tokio::spawn(
        tonic::transport::Server::builder()
            .add_service(SshxServiceServer::new(Bridge {
                opens: opens_tx,
                channels: channels_tx,
            }))
            .serve_with_incoming(ReceiverStream::new(incoming_rx)),
    );

    let mut daemon = tokio::process::Command::new(env!("CARGO_BIN_EXE_sshxx-daemon"))
        .args([
            "--server",
            &origin,
            "--enable-readers",
            "--shell",
            "/bin/bash",
        ])
        .current_dir(state.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let original = tokio::time::timeout(Duration::from_secs(15), opens.recv())
        .await?
        .context("daemon did not open session")?;
    assert!(original.resume_name.is_empty());
    let (mut input, output) = tokio::time::timeout(Duration::from_secs(5), channels.recv())
        .await?
        .unwrap();
    assert!(matches!(update(&mut input).await?, ClientMessage::Hello(_)));
    let shell = proto::NewShell {
        id: 11,
        rows: 24,
        cols: 80,
        page_id: 1,
        ..Default::default()
    };
    send(&output, ServerMessage::CreateShell(Box::new(shell.clone()))).await?;
    while !matches!(update(&mut input).await?, ClientMessage::CreatedShell(_)) {}
    let before = terminals(&mut host_client).await?;
    assert_eq!(before.len(), 1);
    let workspace = proto::WorkspaceState {
        format_version: sshx_core::WORKSPACE_FORMAT_VERSION,
        pages: vec![proto::WorkspacePage {
            id: 1,
            name: "Page 1".into(),
        }],
        shells: vec![proto::WorkspaceShell {
            id: 11,
            rows: 24,
            cols: 80,
            page_id: 1,
            ..Default::default()
        }],
        ..Default::default()
    };
    send(&output, ServerMessage::Workspace(workspace.clone())).await?;
    send(
        &output,
        ServerMessage::SystemAction(proto::SystemActionRequest {
            request_id: "0123456789abcdef0123456789abcdef".into(),
            action: proto::SystemAction::RestartDaemon.into(),
        }),
    )
    .await?;
    loop {
        if let ClientMessage::SystemActionResponse(response) = update(&mut input).await? {
            assert!(response.ok, "{}", response.message);
            break;
        }
    }
    let restored = tokio::time::timeout(Duration::from_secs(15), opens.recv())
        .await?
        .context("replacement daemon did not start")?;
    assert_eq!(restored.resume_name, "restart-test");
    assert_eq!(restored.resume_token, "test-token");
    assert_eq!(restored.encrypted_zeros, original.encrypted_zeros);
    assert_eq!(restored.write_password_hash, original.write_password_hash);
    assert_eq!(restored.workspace, Some(workspace));
    let (mut input, output) = tokio::time::timeout(Duration::from_secs(5), channels.recv())
        .await?
        .unwrap();
    assert!(matches!(update(&mut input).await?, ClientMessage::Hello(_)));
    assert!(!state.path().join(".sshx-restart").exists());
    send(&output, ServerMessage::CreateShell(Box::new(shell))).await?;
    while !matches!(update(&mut input).await?, ClientMessage::CreatedShell(_)) {}
    let after = terminals(&mut host_client).await?;
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].process_id, before[0].process_id);
    assert!(after[0].running);

    daemon.kill().await?;
    host_client.shutdown(true).await?;
    tokio::time::timeout(Duration::from_secs(5), host).await???;
    server.abort();
    accept.abort();
    Ok(())
}
