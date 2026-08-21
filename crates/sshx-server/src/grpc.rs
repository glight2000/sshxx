//! Defines gRPC routes and application request logic.

use std::sync::Arc;
use std::time::{Duration, SystemTime};

use base64::prelude::{Engine as _, BASE64_STANDARD};
use hmac::Mac;
use sshx_core::proto::{
    client_update::ClientMessage, server_update::ServerMessage, sshx_service_server::SshxService,
    ClientUpdate, CloseRequest, CloseResponse, OpenRequest, OpenResponse, ServerUpdate,
    TerminalSize,
};
use sshx_core::{rand_alphanumeric, Sid};
use tokio::sync::mpsc;
use tokio::time::{self, MissedTickBehavior};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::{Request, Response, Status, Streaming};
use tracing::{error, info, warn};

use crate::session::{Metadata, Session};
use crate::ServerState;

/// Interval for synchronizing sequence numbers with the client.
pub const SYNC_INTERVAL: Duration = Duration::from_secs(5);

/// Interval for measuring client latency.
pub const PING_INTERVAL: Duration = Duration::from_secs(2);

/// Server that handles gRPC requests from sshxx-daemon.
#[derive(Clone)]
pub struct GrpcServer(Arc<ServerState>);

impl GrpcServer {
    /// Construct a new [`GrpcServer`] instance with associated state.
    pub fn new(state: Arc<ServerState>) -> Self {
        Self(state)
    }
}

type RR<T> = Result<Response<T>, Status>;

#[tonic::async_trait]
impl SshxService for GrpcServer {
    type ChannelStream = ReceiverStream<Result<ServerUpdate, Status>>;

    async fn open(&self, request: Request<OpenRequest>) -> RR<OpenResponse> {
        let request = request.into_inner();
        let origin = self.0.override_origin().unwrap_or(request.origin);
        if origin.is_empty() {
            return Err(Status::invalid_argument("origin is empty"));
        }
        let fixed_session = self.0.session_name().is_some();
        let name = self
            .0
            .session_name()
            .map(str::to_owned)
            .unwrap_or_else(|| rand_alphanumeric(10));
        info!(%name, "creating new session");

        if self.0.lookup(&name).is_some() {
            if !fixed_session {
                return Err(Status::already_exists("generated duplicate ID"));
            }
            info!(%name, "replacing fixed session for a reconnecting daemon");
        }
        let metadata = Metadata {
            encrypted_zeros: request.encrypted_zeros,
            name: request.name,
            write_password_hash: request.write_password_hash,
            daemon_version: request.daemon_version,
        };
        let session = Arc::new(Session::new(metadata));
        if let Some(profiles) = request.ssh_profiles {
            if let Err(err) = session.restore_ssh_profiles(profiles) {
                warn!(?err, "ignoring invalid SSH connection profiles");
            }
        }
        let restored_shells = match request.workspace {
            Some(workspace) => session
                .restore_workspace(workspace)
                .map_err(|err| Status::invalid_argument(err.to_string()))?,
            None => Vec::new(),
        };
        for shell in restored_shells {
            session
                .update_tx()
                .send(ServerMessage::CreateShell(shell))
                .await
                .map_err(|err| Status::internal(err.to_string()))?;
        }
        self.0.insert(&name, session);
        let token = self.0.mac().chain_update(&name).finalize();
        let url = format!("{origin}/s/{name}");
        Ok(Response::new(OpenResponse {
            name,
            token: BASE64_STANDARD.encode(token.into_bytes()),
            url,
        }))
    }

    async fn channel(&self, request: Request<Streaming<ClientUpdate>>) -> RR<Self::ChannelStream> {
        let mut stream = request.into_inner();
        let first_update = match stream.next().await {
            Some(result) => result?,
            None => return Err(Status::invalid_argument("missing first message")),
        };
        let session_name = match first_update.client_message {
            Some(ClientMessage::Hello(hello)) => {
                let (name, token) = hello
                    .split_once(',')
                    .ok_or_else(|| Status::invalid_argument("missing name and token"))?;
                validate_token(self.0.mac(), name, token)?;
                name.to_string()
            }
            _ => return Err(Status::invalid_argument("invalid first message")),
        };
        let session = match self.0.backend_connect(&session_name).await {
            Ok(Some(session)) => session,
            Ok(None) => return Err(Status::not_found("session not found")),
            Err(err) => {
                error!(?err, "failed to connect to backend session");
                return Err(Status::internal(err.to_string()));
            }
        };

        // We now spawn an asynchronous task that sends updates to the client. Note that
        // when this task finishes, the sender end is dropped, so the receiver is
        // automatically closed.
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            if let Err(err) = handle_streaming(&tx, &session, stream).await {
                warn!(?err, "connection exiting early due to an error");
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn close(&self, request: Request<CloseRequest>) -> RR<CloseResponse> {
        let request = request.into_inner();
        validate_token(self.0.mac(), &request.name, &request.token)?;
        info!("closing session {}", request.name);
        if let Err(err) = self.0.close_session(&request.name).await {
            error!(?err, "failed to close session {}", request.name);
            return Err(Status::internal(err.to_string()));
        }
        Ok(Response::new(CloseResponse {}))
    }
}

/// Validate the client token for a session.
#[allow(clippy::result_large_err)]
fn validate_token(mac: impl Mac, name: &str, token: &str) -> tonic::Result<()> {
    if let Ok(token) = BASE64_STANDARD.decode(token) {
        if mac.chain_update(name).verify_slice(&token).is_ok() {
            return Ok(());
        }
    }
    Err(Status::unauthenticated("invalid token"))
}

type ServerTx = mpsc::Sender<Result<ServerUpdate, Status>>;

/// Handle bidirectional streaming messages RPC messages.
async fn handle_streaming(
    tx: &ServerTx,
    session: &Session,
    mut stream: Streaming<ClientUpdate>,
) -> Result<(), &'static str> {
    let mut sync_interval = time::interval(SYNC_INTERVAL);
    sync_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut ping_interval = time::interval(PING_INTERVAL);
    ping_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut workspace = session.subscribe_workspace();

    loop {
        tokio::select! {
            // Send periodic sync messages to the client.
            _ = sync_interval.tick() => {
                let msg = ServerMessage::Sync(session.sequence_numbers());
                if !send_msg(tx, msg).await {
                    return Err("failed to send sync message");
                }
            }
            // Send periodic pings to the client.
            _ = ping_interval.tick() => {
                send_msg(tx, ServerMessage::Ping(get_time_ms())).await;
            }
            // Send the latest persistable workspace after connection and on changes.
            Some(_) = workspace.next() => {
                if !send_msg(tx, ServerMessage::Workspace(session.workspace_state())).await {
                    return Err("failed to send workspace state");
                }
            }
            // Send buffered server updates to the client.
            Ok(msg) = session.update_rx().recv() => {
                if !send_msg(tx, msg).await {
                    return Err("failed to send update message");
                }
            }
            // Handle incoming client messages.
            maybe_update = stream.next() => {
                if let Some(Ok(update)) = maybe_update {
                    if !handle_update(tx, session, update).await {
                        return Err("error responding to client update");
                    }
                } else {
                    // The client has hung up on their end.
                    return Ok(());
                }
            }
            // Exit on a session shutdown signal.
            _ = session.terminated() => {
                let msg = String::from("disconnecting because session is closed");
                send_msg(tx, ServerMessage::Error(msg)).await;
                return Ok(());
            }
        };
    }
}

/// Handles a singe update from the client. Returns `true` on success.
async fn handle_update(tx: &ServerTx, session: &Session, update: ClientUpdate) -> bool {
    session.access();
    match update.client_message {
        Some(ClientMessage::Hello(_)) => {
            return send_err(tx, "unexpected hello".into()).await;
        }
        Some(ClientMessage::Data(data)) => {
            if let Err(err) = session.add_data(Sid(data.id), data.data, data.seq) {
                return send_err(tx, format!("add data: {:?}", err)).await;
            }
        }
        Some(ClientMessage::CreatedShell(new_shell)) => {
            let new_shell = *new_shell;
            let id = Sid(new_shell.id);
            let center = (new_shell.x, new_shell.y);
            let rows = u16::try_from(new_shell.rows).unwrap_or(0);
            let cols = u16::try_from(new_shell.cols).unwrap_or(0);
            let width = u16::try_from(new_shell.width).unwrap_or(0);
            let height = u16::try_from(new_shell.height).unwrap_or(0);
            match session.add_shell(
                id,
                center,
                new_shell.page_id.max(1),
                (rows, cols),
                (width, height),
                (new_shell.theme, new_shell.background),
            ) {
                Ok(Some(winsize)) => {
                    let resize = TerminalSize {
                        id: id.0,
                        rows: winsize.rows.into(),
                        cols: winsize.cols.into(),
                    };
                    if !send_msg(tx, ServerMessage::Resize(resize)).await {
                        return false;
                    }
                }
                Ok(None) => {}
                Err(err) => return send_err(tx, format!("add shell: {:?}", err)).await,
            }
        }
        Some(ClientMessage::ClosedShell(id)) => {
            if let Err(err) = session.close_shell(Sid(id)) {
                return send_err(tx, format!("close shell: {:?}", err)).await;
            }
        }
        Some(ClientMessage::FileResponse(response)) => {
            session.send_file_response(response.request_id, response.stream_num, response.data);
        }
        Some(ClientMessage::Pong(ts)) => {
            let latency = get_time_ms().saturating_sub(ts);
            session.send_latency_measurement(latency);
        }
        Some(ClientMessage::Error(err)) => {
            error!(?err, "error received from client");
            session.send_error(err);
        }
        None => (), // Heartbeat message, ignored.
    }
    true
}

/// Attempt to send a server message to the client.
async fn send_msg(tx: &ServerTx, message: ServerMessage) -> bool {
    let update = Ok(ServerUpdate {
        server_message: Some(message),
    });
    tx.send(update).await.is_ok()
}

/// Attempt to send an error string to the client.
async fn send_err(tx: &ServerTx, err: String) -> bool {
    send_msg(tx, ServerMessage::Error(err)).await
}

fn get_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system time is before the UNIX epoch")
        .as_millis() as u64
}
