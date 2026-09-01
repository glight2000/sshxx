use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use axum::extract::{
    ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
    Path, State,
};
use axum::response::IntoResponse;
use bytes::Bytes;
use futures_util::SinkExt;
use sshx_core::proto::{
    server_update::ServerMessage, FileRequest as ProtoFileRequest, ImageUploadChunk, NewShell,
    SystemAction, SystemActionRequest, TerminalInput, TerminalSize,
};
use sshx_core::Sid;
use subtle::ConstantTimeEq;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{error, info_span, warn, Instrument};

use crate::session::Session;
use crate::web::protocol::{WsClient, WsServer, WsTerminalChunks, WsTerminalSubscription};
use crate::ServerState;

const IMAGE_UPLOAD_CHUNK_BYTES: usize = 64 << 10;
const IMAGE_UPLOAD_MAX_BYTES: u64 = 20 << 20;
const FILE_REQUEST_MAX_BYTES: usize = 12 << 20;
const TERMINAL_RENDER_ACK_CAPABILITY: &str = "terminal-render-ack-v1";
// A 256 KiB server batch can become four 64 KiB browser writes. Keep this
// above the client's per-write recovery deadline plus scheduling overhead.
const TERMINAL_RENDER_ACK_TIMEOUT: Duration = Duration::from_secs(75);
const TERMINAL_GENERATION_CAPABILITY: &str = "terminal-generation-v1";
const SYSTEM_ACTION_CAPABILITY: &str = "system-action-v1";
const CUSTOM_COMPONENT_CAPABILITY: &str = "custom-component-v1";
const CUSTOM_CLICK_MIN_INTERVAL: Duration = Duration::from_millis(40);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalChunkProtocol {
    Legacy,
    Transitional,
    Generation,
}

type TerminalChunks = (Sid, u32, u32, TerminalChunkProtocol, bool, u64, Vec<Bytes>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderAckWait {
    Received,
    Closed,
    TimedOut,
}

async fn wait_for_render_ack(
    receiver: &mut mpsc::Receiver<()>,
    timeout: Duration,
) -> RenderAckWait {
    match tokio::time::timeout(timeout, receiver.recv()).await {
        Ok(Some(())) => RenderAckWait::Received,
        Ok(None) => RenderAckWait::Closed,
        Err(_) => RenderAckWait::TimedOut,
    }
}

fn resolve_terminal_subscription(
    session: &Session,
    subscription: WsTerminalSubscription,
) -> Option<(Sid, u32, u32, TerminalChunkProtocol, u64)> {
    match subscription {
        WsTerminalSubscription::Legacy(id, page_id, chunknum) => {
            session.shell_generation(id).map(|generation| {
                (
                    id,
                    page_id,
                    generation,
                    TerminalChunkProtocol::Legacy,
                    chunknum,
                )
            })
        }
        WsTerminalSubscription::Generation(id, page_id, generation, chunknum) => Some((
            id,
            page_id,
            generation,
            TerminalChunkProtocol::Transitional,
            chunknum,
        )),
    }
}

fn spawn_chunk_forwarder(
    session: Arc<Session>,
    id: Sid,
    generation: u32,
    protocol: TerminalChunkProtocol,
    chunknum: u64,
    chunks_tx: mpsc::Sender<TerminalChunks>,
    mut rendered_rx: Option<mpsc::Receiver<()>>,
) {
    tokio::spawn(async move {
        let stream = session.subscribe_chunks(id, generation, chunknum);
        tokio::pin!(stream);
        while let Some((replay, seqnum, chunks)) = stream.next().await {
            let Some(page_id) = session.shell_page(id) else {
                break;
            };
            if chunks_tx
                .send((id, page_id, generation, protocol, replay, seqnum, chunks))
                .await
                .is_err()
            {
                break;
            }
            if let Some(receiver) = rendered_rx.as_mut() {
                match wait_for_render_ack(receiver, TERMINAL_RENDER_ACK_TIMEOUT).await {
                    RenderAckWait::Received => {}
                    RenderAckWait::Closed => break,
                    RenderAckWait::TimedOut => {
                        warn!(
                            terminal_id = id.0,
                            generation,
                            "terminal renderer acknowledgement timed out; stopping the flow-controlled subscription"
                        );
                        break;
                    }
                }
            }
        }
    });
}

fn create_shell(message: NewShell) -> ServerMessage {
    ServerMessage::CreateShell(Box::new(message))
}

fn same_ssh_host(initial: &str, current: &str) -> bool {
    initial
        .trim()
        .trim_matches(['[', ']'])
        .trim_end_matches('.')
        .eq_ignore_ascii_case(
            current
                .trim()
                .trim_matches(['[', ']'])
                .trim_end_matches('.'),
        )
}

fn valid_file_request(
    request_id: &str,
    request_stream: u64,
    response_stream: u64,
    len: usize,
) -> bool {
    request_id.len() == 32
        && request_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && request_stream & (1 << 63) != 0
        && response_stream & (1 << 63) != 0
        && request_stream != response_stream
        && (1..=FILE_REQUEST_MAX_BYTES).contains(&len)
}

fn valid_request_id(request_id: &str) -> bool {
    request_id.len() == 32
        && request_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_image_upload(
    upload_id: &str,
    media_type: &str,
    total_size: u64,
    stream_num: u64,
    offset: u64,
    data_len: usize,
    complete: bool,
) -> bool {
    upload_id.len() == 32
        && upload_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        && matches!(
            media_type,
            "image/png" | "image/jpeg" | "image/webp" | "image/gif"
        )
        && (1..=IMAGE_UPLOAD_MAX_BYTES).contains(&total_size)
        && stream_num & (1 << 63) != 0
        && data_len > 0
        && data_len <= IMAGE_UPLOAD_CHUNK_BYTES
        && offset
            .checked_add(data_len as u64)
            .is_some_and(|end| end <= total_size && (!complete || end == total_size))
}

pub async fn get_session_ws(
    Path(name): Path<String>,
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |mut socket| {
        let span = info_span!("ws", %name);
        async move {
            match state.frontend_connect(&name).await {
                Ok(Ok(session)) => {
                    if let Err(err) = handle_socket(&mut socket, session).await {
                        warn!(?err, "websocket exiting early");
                    } else {
                        socket.close().await.ok();
                    }
                }
                Ok(Err(Some(host))) => {
                    if let Err(err) = proxy_redirect(&mut socket, &host, &name).await {
                        error!(?err, "failed to proxy websocket");
                        let frame = CloseFrame {
                            code: 4500,
                            reason: format!("proxy redirect: {err}").into(),
                        };
                        socket.send(Message::Close(Some(frame))).await.ok();
                    } else {
                        socket.close().await.ok();
                    }
                }
                Ok(Err(None)) => {
                    let frame = CloseFrame {
                        code: 4404,
                        reason: "could not find the requested session".into(),
                    };
                    socket.send(Message::Close(Some(frame))).await.ok();
                }
                Err(err) => {
                    error!(?err, "failed to connect to frontend session");
                    let frame = CloseFrame {
                        code: 4500,
                        reason: format!("session connect: {err}").into(),
                    };
                    socket.send(Message::Close(Some(frame))).await.ok();
                }
            }
        }
        .instrument(span)
    })
}

/// Handle an incoming live WebSocket connection to a given session.
async fn handle_socket(socket: &mut WebSocket, session: Arc<Session>) -> Result<()> {
    /// Send a message to the client over WebSocket.
    async fn send(socket: &mut WebSocket, msg: WsServer) -> Result<()> {
        let mut buf = Vec::new();
        ciborium::ser::into_writer(&msg, &mut buf)?;
        socket.send(Message::Binary(Bytes::from(buf))).await?;
        Ok(())
    }

    /// Receive a message from the client over WebSocket.
    async fn recv(socket: &mut WebSocket) -> Result<Option<WsClient>> {
        Ok(loop {
            match socket.recv().await.transpose()? {
                Some(Message::Text(_)) => warn!("ignoring text message over WebSocket"),
                Some(Message::Binary(msg)) => break Some(ciborium::de::from_reader(&*msg)?),
                Some(_) => (), // ignore other message types, keep looping
                None => break None,
            }
        })
    }

    let metadata = session.metadata();
    let user_id = session.counter().next_uid();
    session.sync_now();
    send(
        socket,
        WsServer::Hello(
            user_id,
            metadata.name.clone(),
            env!("CARGO_PKG_VERSION").into(),
            metadata.daemon_version.clone(),
            metadata.terminal_host_version.clone(),
        ),
    )
    .await?;

    let can_write = match recv(socket).await? {
        Some(WsClient::Authenticate(bytes, write_password_bytes)) => {
            // Constant-time comparison of bytes, converting Choice to bool
            if !bool::from(bytes.ct_eq(metadata.encrypted_zeros.as_ref())) {
                send(socket, WsServer::InvalidAuth()).await?;
                return Ok(());
            }

            match (write_password_bytes, &metadata.write_password_hash) {
                // No password needed, so all users can write (default).
                (_, None) => true,

                // Password stored but not provided, user is read-only.
                (None, Some(_)) => false,

                // Password stored and provided, compare them.
                (Some(provided), Some(stored)) => {
                    if !bool::from(provided.ct_eq(stored)) {
                        send(socket, WsServer::InvalidAuth()).await?;
                        return Ok(());
                    }
                    true
                }
            }
        }
        _ => {
            send(socket, WsServer::InvalidAuth()).await?;
            return Ok(());
        }
    };

    let _user_guard = session.user_scope(user_id, can_write)?;

    let update_tx = session.update_tx(); // start listening for updates before any state reads
    let mut broadcast_stream = session.subscribe_broadcast();
    let mut capabilities = vec![
        TERMINAL_RENDER_ACK_CAPABILITY.into(),
        TERMINAL_GENERATION_CAPABILITY.into(),
    ];
    if metadata
        .daemon_capabilities
        .iter()
        .any(|capability| capability == SYSTEM_ACTION_CAPABILITY)
    {
        capabilities.push(SYSTEM_ACTION_CAPABILITY.into());
    }
    if metadata
        .daemon_capabilities
        .iter()
        .any(|capability| capability == CUSTOM_COMPONENT_CAPABILITY)
    {
        capabilities.push(CUSTOM_COMPONENT_CAPABILITY.into());
    }
    send(socket, WsServer::Capabilities(capabilities)).await?;
    send(socket, WsServer::Users(session.list_users())).await?;
    for (id, page_id, editor) in session.list_note_editors() {
        send(socket, WsServer::NoteEditing(id, page_id, Some(editor))).await?;
    }

    let mut subscribed = HashMap::<Sid, (u32, TerminalChunkProtocol)>::new();
    let mut render_acks = HashMap::<Sid, mpsc::Sender<()>>::new();
    // Filesystem responses are returned only to the WebSocket that requested
    // them, even though the daemon-to-server transport is session-scoped.
    let mut pending_file_requests = HashMap::<String, Instant>::new();
    let mut pending_system_actions = HashMap::<String, Instant>::new();
    let mut last_custom_click = Instant::now() - CUSTOM_CLICK_MIN_INTERVAL;
    let (chunks_tx, mut chunks_rx) = mpsc::channel::<TerminalChunks>(1);

    let mut shells_stream = session.subscribe_shells();
    let mut notes_stream = session.subscribe_notes();
    let mut file_windows_stream = session.subscribe_file_windows();
    let mut custom_windows_stream = session.subscribe_custom_windows();
    let mut pages_stream = session.subscribe_pages();
    let mut ssh_profiles_stream = session.subscribe_ssh_profiles();
    loop {
        let msg = tokio::select! {
            _ = session.terminated() => break,
            Some(result) = broadcast_stream.next() => {
                let msg = result.context("client fell behind on broadcast stream")?;
                if let WsServer::FileResponse(request_id, _, _) = &msg {
                    if pending_file_requests.remove(request_id).is_none() {
                        continue;
                    }
                }
                if let WsServer::SystemActionResult(request_id, _, _, _) = &msg {
                    if pending_system_actions.remove(request_id).is_none() {
                        continue;
                    }
                }
                send(socket, msg).await?;
                continue;
            }
            Some(shells) = shells_stream.next() => {
                send(socket, WsServer::Shells(shells)).await?;
                continue;
            }
            Some(notes) = notes_stream.next() => {
                send(socket, WsServer::Notes(notes)).await?;
                continue;
            }
            Some(file_windows) = file_windows_stream.next() => {
                send(socket, WsServer::FileWindows(file_windows)).await?;
                continue;
            }
            Some(custom_windows) = custom_windows_stream.next() => {
                send(socket, WsServer::CustomWindows(custom_windows)).await?;
                continue;
            }
            Some(pages) = pages_stream.next() => {
                send(socket, WsServer::Pages(pages)).await?;
                continue;
            }
            Some(profiles) = ssh_profiles_stream.next() => {
                send(socket, WsServer::SshProfiles(profiles)).await?;
                continue;
            }
            Some((id, page_id, generation, protocol, replay, seqnum, chunks)) = chunks_rx.recv() => {
                let message = match protocol {
                    TerminalChunkProtocol::Legacy => WsServer::Chunks(WsTerminalChunks::Legacy(
                        id, page_id, replay, seqnum, chunks,
                    )),
                    TerminalChunkProtocol::Transitional => WsServer::Chunks(WsTerminalChunks::Generation(
                        id, page_id, generation, replay, seqnum, chunks,
                    )),
                    TerminalChunkProtocol::Generation => {
                        WsServer::ChunksGeneration(id, page_id, generation, replay, seqnum, chunks)
                    }
                };
                send(socket, message).await?;
                continue;
            }
            result = recv(socket) => {
                match result? {
                    Some(msg) => msg,
                    None => break,
                }
            }
        };

        match msg {
            WsClient::Authenticate(_, _) => {}
            WsClient::SetName(name) => {
                if !name.is_empty() {
                    session.update_user(user_id, |user| user.name = name)?;
                }
            }
            WsClient::SetCursor(page_id, cursor) => {
                if !session.page_exists(page_id) {
                    send(socket, WsServer::Error("Page does not exist.".into())).await?;
                    continue;
                }
                session.update_user(user_id, |user| {
                    user.cursor = cursor;
                    user.page_id = page_id;
                })?;
            }
            WsClient::CustomClick(id, page_id, x, y) => {
                if last_custom_click.elapsed() < CUSTOM_CLICK_MIN_INTERVAL {
                    continue;
                }
                last_custom_click = Instant::now();
                if let Err(error) = session.send_custom_click(user_id, id, page_id, x, y) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                }
            }
            WsClient::SetFocus(focus) => {
                if let Some((id, page_id)) = focus {
                    if let Err(err) = session.check_shell_page(id, page_id) {
                        send(socket, WsServer::Error(err.to_string())).await?;
                        continue;
                    }
                    session.update_user(user_id, |user| {
                        user.focus = Some(id);
                        user.page_id = page_id;
                    })?;
                } else {
                    session.update_user(user_id, |user| user.focus = None)?;
                }
            }
            WsClient::Create(x, y, page_id) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id) {
                    send(socket, WsServer::Error("Page does not exist.".into())).await?;
                    continue;
                }
                let id = session.counter().next_sid();
                session.sync_now();
                let new_shell = NewShell {
                    id: id.0,
                    x,
                    y,
                    source_id: None,
                    page_id,
                    rows: 0,
                    cols: 0,
                    width: 0,
                    height: 0,
                    ssh_profile: None,
                    theme: String::new(),
                    background: String::new(),
                    working_directory: String::new(),
                    ssh_profile_id: String::new(),
                    copy_history: false,
                };
                update_tx.send(create_shell(new_shell)).await?;
            }
            WsClient::CreateSized(x, y, rows, cols, page_id) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id)
                    || !(8..=500).contains(&rows)
                    || !(32..=500).contains(&cols)
                {
                    send(
                        socket,
                        WsServer::Error("Invalid page or terminal dimensions.".into()),
                    )
                    .await?;
                    continue;
                }
                let id = session.counter().next_sid();
                session.sync_now();
                update_tx
                    .send(create_shell(NewShell {
                        id: id.0,
                        x,
                        y,
                        source_id: None,
                        page_id,
                        rows: rows.into(),
                        cols: cols.into(),
                        width: 0,
                        height: 0,
                        ssh_profile: None,
                        theme: String::new(),
                        background: String::new(),
                        working_directory: String::new(),
                        ssh_profile_id: String::new(),
                        copy_history: false,
                    }))
                    .await?;
            }
            WsClient::CreateStyled(x, y, rows, cols, page_id, theme) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id)
                    || !(8..=500).contains(&rows)
                    || !(32..=500).contains(&cols)
                    || theme.len() > 100
                    || theme.chars().any(char::is_control)
                {
                    send(
                        socket,
                        WsServer::Error("Invalid terminal creation settings.".into()),
                    )
                    .await?;
                    continue;
                }
                let id = session.counter().next_sid();
                session.sync_now();
                update_tx
                    .send(create_shell(NewShell {
                        id: id.0,
                        x,
                        y,
                        source_id: None,
                        page_id,
                        rows: rows.into(),
                        cols: cols.into(),
                        width: 0,
                        height: 0,
                        ssh_profile: None,
                        theme,
                        background: String::new(),
                        working_directory: String::new(),
                        ssh_profile_id: String::new(),
                        copy_history: false,
                    }))
                    .await?;
            }
            WsClient::CreateWindowed(x, y, width, height, rows, cols, page_id, theme) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id)
                    || !(240..=4_000).contains(&width)
                    || !(160..=4_000).contains(&height)
                    || !(8..=500).contains(&rows)
                    || !(32..=500).contains(&cols)
                    || theme.len() > 100
                    || theme.chars().any(char::is_control)
                {
                    send(
                        socket,
                        WsServer::Error("Invalid terminal creation settings.".into()),
                    )
                    .await?;
                    continue;
                }
                let id = session.counter().next_sid();
                session.sync_now();
                update_tx
                    .send(create_shell(NewShell {
                        id: id.0,
                        x,
                        y,
                        source_id: None,
                        page_id,
                        rows: rows.into(),
                        cols: cols.into(),
                        ssh_profile: None,
                        theme,
                        width: width.into(),
                        height: height.into(),
                        background: String::new(),
                        working_directory: String::new(),
                        ssh_profile_id: String::new(),
                        copy_history: false,
                    }))
                    .await?;
            }
            WsClient::CreateSsh(profile_id, x, y, rows, cols, page_id) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id)
                    || !(8..=500).contains(&rows)
                    || !(32..=500).contains(&cols)
                {
                    send(
                        socket,
                        WsServer::Error("Invalid page or terminal dimensions.".into()),
                    )
                    .await?;
                    continue;
                }
                let profile = match session.ssh_profile(&profile_id) {
                    Ok(profile) => profile,
                    Err(err) => {
                        send(socket, WsServer::Error(err.to_string())).await?;
                        continue;
                    }
                };
                let id = session.counter().next_sid();
                session.sync_now();
                let theme = profile.theme.clone();
                let background = if profile.background_enabled {
                    profile.background.clone()
                } else {
                    String::new()
                };
                update_tx
                    .send(create_shell(NewShell {
                        id: id.0,
                        x,
                        y,
                        source_id: None,
                        page_id,
                        rows: rows.into(),
                        cols: cols.into(),
                        width: 0,
                        height: 0,
                        ssh_profile: Some(profile),
                        theme,
                        background,
                        working_directory: String::new(),
                        ssh_profile_id: profile_id,
                        copy_history: false,
                    }))
                    .await?;
            }
            WsClient::CreateSshStyled(profile_id, x, y, rows, cols, page_id, theme) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id)
                    || !(8..=500).contains(&rows)
                    || !(32..=500).contains(&cols)
                    || theme.len() > 100
                    || theme.chars().any(char::is_control)
                {
                    send(
                        socket,
                        WsServer::Error("Invalid terminal creation settings.".into()),
                    )
                    .await?;
                    continue;
                }
                let profile = match session.ssh_profile(&profile_id) {
                    Ok(profile) => profile,
                    Err(err) => {
                        send(socket, WsServer::Error(err.to_string())).await?;
                        continue;
                    }
                };
                let id = session.counter().next_sid();
                session.sync_now();
                let theme = if profile.theme.is_empty() {
                    theme
                } else {
                    profile.theme.clone()
                };
                let background = if profile.background_enabled {
                    profile.background.clone()
                } else {
                    String::new()
                };
                update_tx
                    .send(create_shell(NewShell {
                        id: id.0,
                        x,
                        y,
                        source_id: None,
                        page_id,
                        rows: rows.into(),
                        cols: cols.into(),
                        width: 0,
                        height: 0,
                        ssh_profile: Some(profile),
                        theme,
                        background,
                        working_directory: String::new(),
                        ssh_profile_id: profile_id,
                        copy_history: false,
                    }))
                    .await?;
            }
            WsClient::CreateSshWindowed(
                profile_id,
                x,
                y,
                width,
                height,
                rows,
                cols,
                page_id,
                theme,
            ) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id)
                    || !(240..=4_000).contains(&width)
                    || !(160..=4_000).contains(&height)
                    || !(8..=500).contains(&rows)
                    || !(32..=500).contains(&cols)
                    || theme.len() > 100
                    || theme.chars().any(char::is_control)
                {
                    send(
                        socket,
                        WsServer::Error("Invalid terminal creation settings.".into()),
                    )
                    .await?;
                    continue;
                }
                let profile = match session.ssh_profile(&profile_id) {
                    Ok(profile) => profile,
                    Err(err) => {
                        send(socket, WsServer::Error(err.to_string())).await?;
                        continue;
                    }
                };
                let id = session.counter().next_sid();
                session.sync_now();
                let theme = if profile.theme.is_empty() {
                    theme
                } else {
                    profile.theme.clone()
                };
                let background = if profile.background_enabled {
                    profile.background.clone()
                } else {
                    String::new()
                };
                update_tx
                    .send(create_shell(NewShell {
                        id: id.0,
                        x,
                        y,
                        source_id: None,
                        page_id,
                        rows: rows.into(),
                        cols: cols.into(),
                        ssh_profile: Some(profile),
                        theme,
                        width: width.into(),
                        height: height.into(),
                        background,
                        working_directory: String::new(),
                        ssh_profile_id: profile_id,
                        copy_history: false,
                    }))
                    .await?;
            }
            WsClient::Clone(source_id, x, y, page_id) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id) {
                    send(socket, WsServer::Error("Page does not exist.".into())).await?;
                    continue;
                }
                if let Err(err) = session.check_shell_page(source_id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                let id = session.counter().next_sid();
                session.sync_now();
                let new_shell = NewShell {
                    id: id.0,
                    x,
                    y,
                    source_id: Some(source_id.0),
                    page_id,
                    rows: 0,
                    cols: 0,
                    width: 0,
                    height: 0,
                    ssh_profile: None,
                    theme: String::new(),
                    background: String::new(),
                    working_directory: String::new(),
                    ssh_profile_id: session.shell_ssh_profile_id(source_id).unwrap_or_default(),
                    copy_history: true,
                };
                update_tx.send(create_shell(new_shell)).await?;
            }
            WsClient::CloneSized(source_id, x, y, rows, cols, page_id) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id)
                    || !(8..=500).contains(&rows)
                    || !(32..=500).contains(&cols)
                {
                    send(
                        socket,
                        WsServer::Error("Invalid page or terminal dimensions.".into()),
                    )
                    .await?;
                    continue;
                }
                if let Err(err) = session.check_shell_page(source_id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                let id = session.counter().next_sid();
                session.sync_now();
                update_tx
                    .send(create_shell(NewShell {
                        id: id.0,
                        x,
                        y,
                        source_id: Some(source_id.0),
                        page_id,
                        rows: rows.into(),
                        cols: cols.into(),
                        width: 0,
                        height: 0,
                        ssh_profile: None,
                        theme: String::new(),
                        background: String::new(),
                        working_directory: String::new(),
                        ssh_profile_id: session.shell_ssh_profile_id(source_id).unwrap_or_default(),
                        copy_history: true,
                    }))
                    .await?;
            }
            WsClient::CloneStyled(source_id, x, y, rows, cols, page_id, theme) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id)
                    || !(8..=500).contains(&rows)
                    || !(32..=500).contains(&cols)
                    || theme.len() > 100
                    || theme.chars().any(char::is_control)
                {
                    send(
                        socket,
                        WsServer::Error("Invalid terminal creation settings.".into()),
                    )
                    .await?;
                    continue;
                }
                if let Err(err) = session.check_shell_page(source_id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                let id = session.counter().next_sid();
                session.sync_now();
                update_tx
                    .send(create_shell(NewShell {
                        id: id.0,
                        x,
                        y,
                        source_id: Some(source_id.0),
                        page_id,
                        rows: rows.into(),
                        cols: cols.into(),
                        width: 0,
                        height: 0,
                        ssh_profile: None,
                        theme,
                        background: String::new(),
                        working_directory: String::new(),
                        ssh_profile_id: session.shell_ssh_profile_id(source_id).unwrap_or_default(),
                        copy_history: true,
                    }))
                    .await?;
            }
            clone_message @ (WsClient::CloneWindowed(..) | WsClient::CloneWindowedAt(..)) => {
                let (
                    source_id,
                    working_directory,
                    working_directory_host,
                    initial_working_directory_host,
                    x,
                    y,
                    width,
                    height,
                    rows,
                    cols,
                    page_id,
                    theme,
                ) = match clone_message {
                    WsClient::CloneWindowed(
                        source_id,
                        x,
                        y,
                        width,
                        height,
                        rows,
                        cols,
                        page_id,
                        theme,
                    ) => (
                        source_id,
                        String::new(),
                        String::new(),
                        String::new(),
                        x,
                        y,
                        width,
                        height,
                        rows,
                        cols,
                        page_id,
                        theme,
                    ),
                    WsClient::CloneWindowedAt(
                        source_id,
                        working_directory,
                        working_directory_host,
                        initial_working_directory_host,
                        x,
                        y,
                        width,
                        height,
                        rows,
                        cols,
                        page_id,
                        theme,
                    ) => (
                        source_id,
                        working_directory,
                        working_directory_host,
                        initial_working_directory_host,
                        x,
                        y,
                        width,
                        height,
                        rows,
                        cols,
                        page_id,
                        theme,
                    ),
                    _ => unreachable!(),
                };
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id)
                    || !(240..=4_000).contains(&width)
                    || !(160..=4_000).contains(&height)
                    || !(8..=500).contains(&rows)
                    || !(32..=500).contains(&cols)
                    || theme.len() > 100
                    || theme.chars().any(char::is_control)
                    || working_directory.len() > 16_384
                    || working_directory.chars().any(char::is_control)
                    || working_directory_host.len() > 1_024
                    || working_directory_host.chars().any(char::is_control)
                    || initial_working_directory_host.len() > 1_024
                    || initial_working_directory_host.chars().any(char::is_control)
                {
                    send(
                        socket,
                        WsServer::Error("Invalid terminal creation settings.".into()),
                    )
                    .await?;
                    continue;
                }
                if let Err(err) = session.check_shell_page(source_id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                let ssh_profile_id = session.shell_ssh_profile_id(source_id).unwrap_or_default();
                let working_directory = if ssh_profile_id.is_empty() || working_directory.is_empty()
                {
                    String::new()
                } else {
                    match session.ssh_profile(&ssh_profile_id) {
                        Ok(_)
                            if same_ssh_host(
                                &initial_working_directory_host,
                                &working_directory_host,
                            ) =>
                        {
                            working_directory
                        }
                        // A changed OSC 7 host indicates a manually nested SSH
                        // session. Reusing its path on the first hop would be
                        // incorrect, so fall back to that host's home.
                        Ok(_) => String::new(),
                        Err(error) => {
                            send(
                                socket,
                                WsServer::Error(format!(
                                    "Cannot duplicate this SSH connection: {error}"
                                )),
                            )
                            .await?;
                            continue;
                        }
                    }
                };
                let id = session.counter().next_sid();
                session.sync_now();
                update_tx
                    .send(create_shell(NewShell {
                        id: id.0,
                        x,
                        y,
                        source_id: Some(source_id.0),
                        page_id,
                        rows: rows.into(),
                        cols: cols.into(),
                        ssh_profile: None,
                        theme,
                        width: width.into(),
                        height: height.into(),
                        background: String::new(),
                        working_directory,
                        ssh_profile_id,
                        copy_history: true,
                    }))
                    .await?;
            }
            WsClient::CreateAt(
                source_id,
                working_directory,
                x,
                y,
                width,
                height,
                rows,
                cols,
                page_id,
                theme,
            ) => {
                if let Err(error) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                if session.shell_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 terminals.".into()),
                    )
                    .await?;
                    continue;
                }
                if !session.page_exists(page_id)
                    || session.check_shell_exists(source_id).is_err()
                    || !(240..=4_000).contains(&width)
                    || !(160..=4_000).contains(&height)
                    || !(8..=500).contains(&rows)
                    || !(32..=500).contains(&cols)
                    || working_directory.is_empty()
                    || working_directory.len() > 16_384
                    || working_directory.chars().any(char::is_control)
                    || theme.len() > 100
                    || theme.chars().any(char::is_control)
                {
                    send(
                        socket,
                        WsServer::Error("Invalid terminal creation settings.".into()),
                    )
                    .await?;
                    continue;
                }
                let ssh_profile_id = session.shell_ssh_profile_id(source_id).unwrap_or_default();
                let ssh_profile = if ssh_profile_id.is_empty() {
                    None
                } else {
                    match session.ssh_profile(&ssh_profile_id) {
                        Ok(profile) => Some(profile),
                        Err(error) => {
                            send(
                                socket,
                                WsServer::Error(format!(
                                    "Cannot open a terminal for this SSH connection: {error}"
                                )),
                            )
                            .await?;
                            continue;
                        }
                    }
                };
                let id = session.counter().next_sid();
                session.sync_now();
                update_tx
                    .send(create_shell(NewShell {
                        id: id.0,
                        x,
                        y,
                        source_id: Some(source_id.0),
                        page_id,
                        rows: rows.into(),
                        cols: cols.into(),
                        width: width.into(),
                        height: height.into(),
                        ssh_profile,
                        theme,
                        background: String::new(),
                        working_directory,
                        ssh_profile_id,
                        copy_history: false,
                    }))
                    .await?;
            }
            WsClient::Close(id, page_id) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.check_shell_page(id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                update_tx.send(ServerMessage::CloseShell(id.0)).await?;
            }
            WsClient::Move(id, page_id, winsize) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.move_shell(id, page_id, winsize.clone()) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                if let Some(winsize) = winsize {
                    let msg = ServerMessage::Resize(TerminalSize {
                        id: id.0,
                        rows: winsize.rows as u32,
                        cols: winsize.cols as u32,
                    });
                    session.update_tx().send(msg).await?;
                }
            }
            WsClient::MoveCanvasItems(
                source_page_id,
                target_page_id,
                terminals,
                notes,
                file_windows,
            ) => {
                if let Err(error) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                if let Err(error) = session.move_canvas_items(
                    source_page_id,
                    target_page_id,
                    terminals,
                    notes,
                    file_windows,
                    Vec::new(),
                ) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                }
            }
            WsClient::MoveCanvasItemsWithCustoms(
                source_page_id,
                target_page_id,
                terminals,
                notes,
                file_windows,
                custom_windows,
            ) => {
                if let Err(error) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                if let Err(error) = session.move_canvas_items(
                    source_page_id,
                    target_page_id,
                    terminals,
                    notes,
                    file_windows,
                    custom_windows,
                ) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                }
            }
            WsClient::CreateNote(x, y, page_id) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.note_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 notes.".into()),
                    )
                    .await?;
                    continue;
                }
                let id = session.counter().next_sid();
                if let Err(err) = session.add_note(id, (x, y), page_id, None) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                }
            }
            WsClient::CreateNoteSized(x, y, width, height, page_id) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if session.note_count() >= 100 {
                    send(
                        socket,
                        WsServer::Error("You can only create up to 100 notes.".into()),
                    )
                    .await?;
                    continue;
                }
                let id = session.counter().next_sid();
                if let Err(err) = session.add_note(id, (x, y), page_id, Some((width, height))) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                }
            }
            WsClient::CloseNote(id, page_id) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.close_note(id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                }
            }
            WsClient::UpdateNote(id, page_id, note) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.update_note(id, page_id, note) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                }
            }
            WsClient::SetNoteEditing(id, page_id, editing) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.set_note_editing(id, page_id, user_id, editing) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                }
            }
            WsClient::UpdateNoteText(id, page_id, text) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.update_note_text(id, page_id, user_id, text) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                }
            }
            WsClient::UpdateNoteParagraphs(id, page_id, paragraphs) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.update_note_paragraphs(id, page_id, user_id, paragraphs) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                }
            }
            WsClient::CreateFileWindow(shell_id, page_id, path, title, x, y, width, height) => {
                if let Err(error) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                let id = session.counter().next_sid();
                if let Err(error) = session
                    .open_file_window(id, shell_id, page_id, path, title, x, y, width, height)
                {
                    send(socket, WsServer::Error(error.to_string())).await?;
                }
            }
            WsClient::CloseFileWindow(id, page_id) => {
                if let Err(error) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                if let Err(error) = session.close_file_window(id, page_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                }
            }
            WsClient::UpdateFileWindow(id, page_id, window) => {
                if let Err(error) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                if let Err(error) = session.update_file_window(id, page_id, window) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                }
            }
            WsClient::CreateCustomWindow(x, y, width, height, page_id) => {
                if let Err(error) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                if !metadata
                    .daemon_capabilities
                    .iter()
                    .any(|capability| capability == CUSTOM_COMPONENT_CAPABILITY)
                {
                    send(
                        socket,
                        WsServer::Error(
                            "The connected daemon does not support custom components.".into(),
                        ),
                    )
                    .await?;
                    continue;
                }
                let id = session.counter().next_sid();
                if let Err(error) = session.add_custom_window(id, (x, y), (width, height), page_id)
                {
                    send(socket, WsServer::Error(error.to_string())).await?;
                }
            }
            WsClient::CloseCustomWindow(id, page_id) => {
                if let Err(error) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                if let Err(error) = session.close_custom_window(id, page_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                }
            }
            WsClient::UpdateCustomWindow(id, page_id, window) => {
                if let Err(error) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                if let Err(error) = session.update_custom_window(id, page_id, window) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                }
            }
            WsClient::CreatePage(name) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.create_page(name) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                }
            }
            WsClient::RenamePage(id, name) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.rename_page(id, name) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                }
            }
            WsClient::UpsertSshProfile(profile) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.upsert_ssh_profile(profile) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                }
            }
            WsClient::DeleteSshProfile(id) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.delete_ssh_profile(&id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                }
            }
            WsClient::Data(id, page_id, data, offset) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.check_shell_page(id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                let input = TerminalInput {
                    id: id.0,
                    data,
                    offset,
                };
                update_tx.send(ServerMessage::Input(input)).await?;
            }
            WsClient::UploadImage(
                id,
                page_id,
                upload_id,
                media_type,
                total_size,
                stream_num,
                offset,
                data,
                complete,
            ) => {
                if let Err(e) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(e.to_string())).await?;
                    continue;
                }
                if let Err(err) = session.check_shell_page(id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                if !valid_image_upload(
                    &upload_id,
                    &media_type,
                    total_size,
                    stream_num,
                    offset,
                    data.len(),
                    complete,
                ) {
                    send(
                        socket,
                        WsServer::Error("Invalid image upload chunk.".into()),
                    )
                    .await?;
                    continue;
                }
                update_tx
                    .send(ServerMessage::ImageUpload(ImageUploadChunk {
                        id: id.0,
                        upload_id,
                        media_type,
                        total_size,
                        stream_num,
                        offset,
                        data,
                        complete,
                    }))
                    .await?;
            }
            WsClient::FileRequest(
                id,
                page_id,
                request_id,
                request_stream,
                response_stream,
                data,
            ) => {
                if let Err(error) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                if let Err(error) = session.check_shell_page(id, page_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                if !valid_file_request(&request_id, request_stream, response_stream, data.len()) {
                    send(
                        socket,
                        WsServer::Error("Invalid filesystem request.".into()),
                    )
                    .await?;
                    continue;
                }
                pending_file_requests
                    .retain(|_, created| created.elapsed() < Duration::from_secs(40));
                if pending_file_requests.len() >= 8 {
                    send(
                        socket,
                        WsServer::Error("Too many filesystem requests are still pending.".into()),
                    )
                    .await?;
                    continue;
                }
                pending_file_requests.insert(request_id.clone(), Instant::now());
                update_tx
                    .send(ServerMessage::FileRequest(ProtoFileRequest {
                        id: id.0,
                        request_id,
                        request_stream,
                        response_stream,
                        data,
                    }))
                    .await?;
            }
            WsClient::SystemAction(request_id, action) => {
                if let Err(error) = session.check_write_permission(user_id) {
                    send(socket, WsServer::Error(error.to_string())).await?;
                    continue;
                }
                if !metadata
                    .daemon_capabilities
                    .iter()
                    .any(|capability| capability == SYSTEM_ACTION_CAPABILITY)
                {
                    send(
                        socket,
                        WsServer::Error(
                            "The connected daemon does not support runtime controls.".into(),
                        ),
                    )
                    .await?;
                    continue;
                }
                let action_value = match action.as_str() {
                    "restartDaemon" => SystemAction::RestartDaemon,
                    "restartTerminalHost" => SystemAction::RestartTerminalHost,
                    _ => {
                        send(socket, WsServer::Error("Invalid system action.".into())).await?;
                        continue;
                    }
                };
                pending_system_actions
                    .retain(|_, created| created.elapsed() < Duration::from_secs(20));
                if !valid_request_id(&request_id) || pending_system_actions.len() >= 2 {
                    send(
                        socket,
                        WsServer::Error("Invalid or excessive system action request.".into()),
                    )
                    .await?;
                    continue;
                }
                pending_system_actions.insert(request_id.clone(), Instant::now());
                update_tx
                    .send(ServerMessage::SystemAction(SystemActionRequest {
                        request_id,
                        action: action_value.into(),
                    }))
                    .await?;
            }
            WsClient::Subscribe(subscription) => {
                let Some((id, page_id, generation, protocol, chunknum)) =
                    resolve_terminal_subscription(&session, subscription)
                else {
                    continue;
                };
                if let Err(err) = session.check_shell_page(id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                if subscribed.get(&id) == Some(&(generation, protocol)) {
                    continue;
                }
                subscribed.insert(id, (generation, protocol));
                render_acks.remove(&id);
                spawn_chunk_forwarder(
                    Arc::clone(&session),
                    id,
                    generation,
                    protocol,
                    chunknum,
                    chunks_tx.clone(),
                    None,
                );
            }
            WsClient::SubscribeFlowControlled(subscription) => {
                let Some((id, page_id, generation, protocol, chunknum)) =
                    resolve_terminal_subscription(&session, subscription)
                else {
                    continue;
                };
                if let Err(err) = session.check_shell_page(id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                if subscribed.get(&id) == Some(&(generation, protocol)) {
                    continue;
                }
                subscribed.insert(id, (generation, protocol));
                let (rendered_tx, rendered_rx) = mpsc::channel(1);
                render_acks.insert(id, rendered_tx);
                spawn_chunk_forwarder(
                    Arc::clone(&session),
                    id,
                    generation,
                    protocol,
                    chunknum,
                    chunks_tx.clone(),
                    Some(rendered_rx),
                );
            }
            WsClient::SubscribeGeneration(id, page_id, generation, chunknum) => {
                if let Err(err) = session.check_shell_page(id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                if subscribed.get(&id) == Some(&(generation, TerminalChunkProtocol::Generation)) {
                    continue;
                }
                subscribed.insert(id, (generation, TerminalChunkProtocol::Generation));
                render_acks.remove(&id);
                spawn_chunk_forwarder(
                    Arc::clone(&session),
                    id,
                    generation,
                    TerminalChunkProtocol::Generation,
                    chunknum,
                    chunks_tx.clone(),
                    None,
                );
            }
            WsClient::SubscribeFlowControlledGeneration(id, page_id, generation, chunknum) => {
                if let Err(err) = session.check_shell_page(id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                if subscribed.get(&id) == Some(&(generation, TerminalChunkProtocol::Generation)) {
                    continue;
                }
                subscribed.insert(id, (generation, TerminalChunkProtocol::Generation));
                let (rendered_tx, rendered_rx) = mpsc::channel(1);
                render_acks.insert(id, rendered_tx);
                spawn_chunk_forwarder(
                    Arc::clone(&session),
                    id,
                    generation,
                    TerminalChunkProtocol::Generation,
                    chunknum,
                    chunks_tx.clone(),
                    Some(rendered_rx),
                );
            }
            WsClient::RenderedChunks(id) => {
                if let Some(sender) = render_acks.get(&id) {
                    sender.try_send(()).ok();
                }
            }
            WsClient::Chat(msg) => {
                session.send_chat(user_id, &msg)?;
            }
            WsClient::Ping(ts) => {
                send(socket, WsServer::Pong(ts)).await?;
            }
        }
    }
    Ok(())
}

/// Transparently reverse-proxy a WebSocket connection to a different host.
async fn proxy_redirect(socket: &mut WebSocket, host: &str, name: &str) -> Result<()> {
    use tokio_tungstenite::{
        connect_async,
        tungstenite::protocol::{CloseFrame as TCloseFrame, Message as TMessage},
    };

    let (mut upstream, _) = connect_async(format!("ws://{host}/api/s/{name}")).await?;
    loop {
        // Due to axum having its own WebSocket API types, we need to manually translate
        // between it and tungstenite's message type.
        tokio::select! {
            Some(client_msg) = socket.recv() => {
                let msg = match client_msg {
                    Ok(Message::Text(s)) => Some(TMessage::Text(s.as_str().into())),
                    Ok(Message::Binary(b)) => Some(TMessage::Binary(b)),
                    Ok(Message::Close(frame)) => {
                        let frame = frame.map(|frame| TCloseFrame {
                            code: frame.code.into(),
                            reason: frame.reason.as_str().into(),
                        });
                        Some(TMessage::Close(frame))
                    }
                    Ok(_) => None,
                    Err(_) => break,
                };
                if let Some(msg) = msg {
                    if upstream.send(msg).await.is_err() {
                        break;
                    }
                }
            }
            Some(server_msg) = upstream.next() => {
                let msg = match server_msg {
                    Ok(TMessage::Text(s)) => Some(Message::Text(s.as_str().into())),
                    Ok(TMessage::Binary(b)) => Some(Message::Binary(b)),
                    Ok(TMessage::Close(frame)) => {
                        let frame = frame.map(|frame| CloseFrame {
                            code: frame.code.into(),
                            reason: frame.reason.as_str().into(),
                        });
                        Some(Message::Close(frame))
                    }
                    Ok(_) => None,
                    Err(_) => break,
                };
                if let Some(msg) = msg {
                    if socket.send(msg).await.is_err() {
                        break;
                    }
                }
            }
            else => break,
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sshx_core::Sid;
    use tokio::sync::mpsc;

    use crate::web::protocol::{WsClient, WsTerminalSubscription};

    use super::{
        same_ssh_host, valid_image_upload, valid_request_id, wait_for_render_ack, RenderAckWait,
    };

    #[tokio::test]
    async fn bounds_terminal_renderer_ack_waits() {
        let (sender, mut receiver) = mpsc::channel(1);
        sender.send(()).await.expect("ack channel should be open");
        assert_eq!(
            wait_for_render_ack(&mut receiver, Duration::from_secs(1)).await,
            RenderAckWait::Received
        );

        assert_eq!(
            wait_for_render_ack(&mut receiver, Duration::from_millis(10)).await,
            RenderAckWait::TimedOut
        );

        drop(sender);
        assert_eq!(
            wait_for_render_ack(&mut receiver, Duration::from_secs(1)).await,
            RenderAckWait::Closed
        );
    }

    #[test]
    fn accepts_only_bounded_hex_system_request_ids() {
        assert!(valid_request_id("0123456789abcdef0123456789abcdef"));
        assert!(!valid_request_id("0123456789ABCDEF0123456789ABCDEF"));
        assert!(!valid_request_id("../../../../../../../../../../../../etc"));
    }

    #[test]
    fn round_trips_system_action_messages() {
        let action = WsClient::SystemAction(
            "0123456789abcdef0123456789abcdef".into(),
            "restartDaemon".into(),
        );
        let mut encoded = Vec::new();
        ciborium::into_writer(&action, &mut encoded).expect("system action should encode");
        assert!(matches!(
            ciborium::from_reader::<WsClient, _>(encoded.as_slice())
                .expect("system action should decode"),
            WsClient::SystemAction(request_id, action)
                if request_id == "0123456789abcdef0123456789abcdef"
                    && action == "restartDaemon"
        ));
    }

    #[test]
    fn matches_only_the_configured_ssh_host_for_clone_paths() {
        assert!(same_ssh_host("Build.Example.Test.", "build.example.test"));
        assert!(same_ssh_host("[2001:db8::1]", "2001:db8::1"));
        assert!(!same_ssh_host("jump.example.test", "target.example.test"));
    }

    #[test]
    fn accepts_legacy_and_location_aware_clone_messages() {
        let round_trip = |message: &WsClient| {
            let mut encoded = Vec::new();
            ciborium::into_writer(message, &mut encoded).expect("clone message should encode");
            ciborium::from_reader::<WsClient, _>(encoded.as_slice())
                .expect("clone message should decode")
        };
        let legacy = round_trip(&WsClient::CloneWindowed(
            Sid(7),
            10,
            20,
            700,
            500,
            24,
            80,
            1,
            "Tokyo Night".into(),
        ));
        assert!(matches!(legacy, WsClient::CloneWindowed(Sid(7), ..)));

        let current = round_trip(&WsClient::CloneWindowedAt(
            Sid(7),
            "/work".into(),
            "host.example".into(),
            "host.example".into(),
            10,
            20,
            700,
            500,
            24,
            80,
            1,
            "Tokyo Night".into(),
        ));
        assert!(matches!(
            current,
            WsClient::CloneWindowedAt(Sid(7), path, host, ..)
                if path == "/work" && host == "host.example"
        ));
    }

    #[test]
    fn accepts_legacy_and_generation_aware_terminal_subscriptions() {
        let round_trip = |message: &WsClient| {
            let mut encoded = Vec::new();
            ciborium::into_writer(message, &mut encoded).expect("subscription should encode");
            ciborium::from_reader::<WsClient, _>(encoded.as_slice())
                .expect("subscription should decode")
        };

        assert!(matches!(
            round_trip(&WsClient::Subscribe(WsTerminalSubscription::Legacy(
                Sid(7),
                1,
                12,
            ))),
            WsClient::Subscribe(WsTerminalSubscription::Legacy(Sid(7), 1, 12))
        ));
        assert!(matches!(
            round_trip(&WsClient::Subscribe(WsTerminalSubscription::Generation(
                Sid(7),
                1,
                3,
                12
            ),)),
            WsClient::Subscribe(WsTerminalSubscription::Generation(Sid(7), 1, 3, 12))
        ));
        assert!(matches!(
            round_trip(&WsClient::SubscribeGeneration(Sid(7), 1, 3, 12)),
            WsClient::SubscribeGeneration(Sid(7), 1, 3, 12)
        ));
    }

    #[test]
    fn accepts_the_frontend_render_ack_shape() {
        // Produced by cbor-x for `{ renderedChunks: 23 }`. The variant is a
        // newtype, so its value must be a scalar rather than a one-item array.
        let encoded = [
            0xb9, 0x00, 0x01, 0x6e, b'r', b'e', b'n', b'd', b'e', b'r', b'e', b'd', b'C', b'h',
            b'u', b'n', b'k', b's', 0x17,
        ];
        assert!(matches!(
            ciborium::from_reader::<WsClient, _>(encoded.as_slice())
                .expect("frontend render acknowledgement should decode"),
            WsClient::RenderedChunks(Sid(23))
        ));
    }

    #[test]
    fn validates_image_upload_boundaries() {
        let id = "0123456789abcdef0123456789abcdef";
        let stream = 0x8000_0000_0000_0001;
        assert!(valid_image_upload(
            id,
            "image/png",
            100,
            stream,
            0,
            64,
            false
        ));
        assert!(valid_image_upload(
            id,
            "image/png",
            100,
            stream,
            64,
            36,
            true
        ));
        assert!(!valid_image_upload(
            "../../unsafe",
            "image/png",
            100,
            stream,
            0,
            64,
            false
        ));
        assert!(!valid_image_upload(
            id,
            "image/svg+xml",
            100,
            stream,
            0,
            64,
            false
        ));
        assert!(!valid_image_upload(
            id,
            "image/png",
            100,
            stream,
            0,
            64,
            true
        ));
    }
}
