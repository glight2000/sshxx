use std::collections::{HashMap, HashSet};
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
    TerminalInput, TerminalSize,
};
use sshx_core::{Sid, PRODUCT_ID};
use subtle::ConstantTimeEq;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{error, info_span, warn, Instrument};

use crate::session::Session;
use crate::web::protocol::{WsClient, WsServer};
use crate::ServerState;

const IMAGE_UPLOAD_CHUNK_BYTES: usize = 64 << 10;
const IMAGE_UPLOAD_MAX_BYTES: u64 = 20 << 20;
const FILE_REQUEST_MAX_BYTES: usize = 12 << 20;

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
            PRODUCT_ID.into(),
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
    send(socket, WsServer::Users(session.list_users())).await?;
    for (id, page_id, editor) in session.list_note_editors() {
        send(socket, WsServer::NoteEditing(id, page_id, Some(editor))).await?;
    }

    let mut subscribed = HashSet::new(); // prevent duplicate subscriptions
                                         // Filesystem responses are returned only to the WebSocket that requested
                                         // them, even though the daemon-to-server transport is session-scoped.
    let mut pending_file_requests = HashMap::<String, Instant>::new();
    let (chunks_tx, mut chunks_rx) = mpsc::channel::<(Sid, u32, bool, u64, Vec<Bytes>)>(1);

    let mut shells_stream = session.subscribe_shells();
    let mut notes_stream = session.subscribe_notes();
    let mut file_windows_stream = session.subscribe_file_windows();
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
            Some(pages) = pages_stream.next() => {
                send(socket, WsServer::Pages(pages)).await?;
                continue;
            }
            Some(profiles) = ssh_profiles_stream.next() => {
                send(socket, WsServer::SshProfiles(profiles)).await?;
                continue;
            }
            Some((id, page_id, replay, seqnum, chunks)) = chunks_rx.recv() => {
                send(socket, WsServer::Chunks(id, page_id, replay, seqnum, chunks)).await?;
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
                };
                update_tx
                    .send(ServerMessage::CreateShell(new_shell))
                    .await?;
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
                    .send(ServerMessage::CreateShell(NewShell {
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
                    .send(ServerMessage::CreateShell(NewShell {
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
                    .send(ServerMessage::CreateShell(NewShell {
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
                    .send(ServerMessage::CreateShell(NewShell {
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
                    .send(ServerMessage::CreateShell(NewShell {
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
                    .send(ServerMessage::CreateShell(NewShell {
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
                };
                update_tx
                    .send(ServerMessage::CreateShell(new_shell))
                    .await?;
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
                    .send(ServerMessage::CreateShell(NewShell {
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
                    .send(ServerMessage::CreateShell(NewShell {
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
                    }))
                    .await?;
            }
            WsClient::CloneWindowed(source_id, x, y, width, height, rows, cols, page_id, theme) => {
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
                if let Err(err) = session.check_shell_page(source_id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                let id = session.counter().next_sid();
                session.sync_now();
                update_tx
                    .send(ServerMessage::CreateShell(NewShell {
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
                        working_directory: String::new(),
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
                    || session.check_shell_page(source_id, page_id).is_err()
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
                let id = session.counter().next_sid();
                session.sync_now();
                update_tx
                    .send(ServerMessage::CreateShell(NewShell {
                        id: id.0,
                        x,
                        y,
                        source_id: Some(source_id.0),
                        page_id,
                        rows: rows.into(),
                        cols: cols.into(),
                        width: width.into(),
                        height: height.into(),
                        ssh_profile: None,
                        theme,
                        background: String::new(),
                        working_directory,
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
            WsClient::Subscribe(id, page_id, chunknum) => {
                if let Err(err) = session.check_shell_page(id, page_id) {
                    send(socket, WsServer::Error(err.to_string())).await?;
                    continue;
                }
                if subscribed.contains(&id) {
                    continue;
                }
                subscribed.insert(id);
                let session = Arc::clone(&session);
                let chunks_tx = chunks_tx.clone();
                tokio::spawn(async move {
                    let stream = session.subscribe_chunks(id, chunknum);
                    tokio::pin!(stream);
                    while let Some((replay, seqnum, chunks)) = stream.next().await {
                        if chunks_tx
                            .send((id, page_id, replay, seqnum, chunks))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });
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
    use super::valid_image_upload;

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
