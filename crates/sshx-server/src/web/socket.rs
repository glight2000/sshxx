use std::collections::HashSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::{
    ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade},
    Path, State,
};
use axum::response::IntoResponse;
use bytes::Bytes;
use futures_util::SinkExt;
use sshx_core::proto::{server_update::ServerMessage, NewShell, TerminalInput, TerminalSize};
use sshx_core::Sid;
use subtle::ConstantTimeEq;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{error, info_span, warn, Instrument};

use crate::session::Session;
use crate::web::protocol::{WsClient, WsServer};
use crate::ServerState;

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
    let (chunks_tx, mut chunks_rx) = mpsc::channel::<(Sid, u32, bool, u64, Vec<Bytes>)>(1);

    let mut shells_stream = session.subscribe_shells();
    let mut notes_stream = session.subscribe_notes();
    let mut pages_stream = session.subscribe_pages();
    loop {
        let msg = tokio::select! {
            _ = session.terminated() => break,
            Some(result) = broadcast_stream.next() => {
                let msg = result.context("client fell behind on broadcast stream")?;
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
            Some(pages) = pages_stream.next() => {
                send(socket, WsServer::Pages(pages)).await?;
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
                };
                update_tx
                    .send(ServerMessage::CreateShell(new_shell))
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
                };
                update_tx
                    .send(ServerMessage::CreateShell(new_shell))
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
                if let Err(err) = session.add_note(id, (x, y), page_id) {
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
