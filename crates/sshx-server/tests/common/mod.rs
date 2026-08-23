use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{ensure, Result};
use axum::serve::ListenerExt;
use futures_util::{SinkExt, StreamExt};
use http::StatusCode;
use sshx_core::proto::sshx_service_client::SshxServiceClient;
use sshx_core::{Sid, Uid};
use sshx_daemon::encrypt::Encrypt;
use sshx_server::{
    state::ServerState,
    web::protocol::{
        WsClient, WsCustomWindow, WsFileWindow, WsNote, WsPage, WsServer, WsTerminalChunks, WsUser,
        WsWinsize,
    },
    Server, ServerOptions,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::time;
use tokio_tungstenite::{tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tonic::transport::Channel;

/// An ephemeral, isolated server that is created for each test.
pub struct TestServer {
    local_addr: SocketAddr,
    server: Arc<Server>,
}

impl TestServer {
    /// Create a fresh server listening on an unused local port for testing.
    ///
    /// Returns an object with the local address, as well as a custom [`Drop`]
    /// implementation that gracefully shuts down the server.
    pub async fn new() -> Self {
        Self::new_with_options(ServerOptions::default()).await
    }

    /// Create a fresh server using custom options.
    pub async fn new_with_options(options: ServerOptions) -> Self {
        let listener = TcpListener::bind("[::1]:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap();

        let server = Arc::new(Server::new(options).unwrap());
        {
            let server = Arc::clone(&server);
            let listener = listener.tap_io(|tcp_stream| {
                _ = tcp_stream.set_nodelay(true);
            });
            tokio::spawn(async move {
                server.listen(listener).await.unwrap();
            });
        }

        TestServer { local_addr, server }
    }

    /// Returns the local TCP address of this server.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Returns the HTTP/2 base endpoint URI for this server.
    pub fn endpoint(&self) -> String {
        format!("http://{}", self.local_addr)
    }

    /// Returns the WebSocket endpoint for streaming connections to a session.
    pub fn ws_endpoint(&self, name: &str) -> String {
        format!("ws://{}/api/s/{}", self.local_addr, name)
    }

    /// Creates a gRPC client connected to this server.
    pub async fn grpc_client(&self) -> SshxServiceClient<Channel> {
        SshxServiceClient::connect(self.endpoint()).await.unwrap()
    }

    /// Return the current server state object.
    pub fn state(&self) -> Arc<ServerState> {
        self.server.state()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.server.shutdown();
    }
}

/// A WebSocket client that interacts with the server, used for testing.
pub struct ClientSocket {
    inner: WebSocketStream<MaybeTlsStream<TcpStream>>,
    encrypt: Encrypt,
    write_encrypt: Option<Encrypt>,

    pub user_id: Uid,
    pub server_version: String,
    pub daemon_version: String,
    pub users: BTreeMap<Uid, WsUser>,
    pub shells: BTreeMap<Sid, WsWinsize>,
    pub notes: BTreeMap<Sid, WsNote>,
    pub file_windows: BTreeMap<Sid, WsFileWindow>,
    pub custom_windows: BTreeMap<Sid, WsCustomWindow>,
    pub pages: Vec<WsPage>,
    pub note_editors: BTreeMap<Sid, (u32, Uid)>,
    pub data: HashMap<Sid, String>,
    pub chunk_replays: Vec<(Sid, bool)>,
    pub messages: Vec<(Uid, String, String)>,
    pub system_action_results: Vec<(String, String, bool, String)>,
    pub errors: Vec<String>,
}

impl ClientSocket {
    /// Connect to a WebSocket endpoint.
    pub async fn connect(uri: &str, key: &str, write_password: Option<&str>) -> Result<Self> {
        let (stream, resp) = tokio_tungstenite::connect_async(uri).await?;
        ensure!(resp.status() == StatusCode::SWITCHING_PROTOCOLS);

        let mut this = Self {
            inner: stream,
            encrypt: Encrypt::new(key),
            write_encrypt: write_password.map(Encrypt::new),
            user_id: Uid(0),
            server_version: String::new(),
            daemon_version: String::new(),
            users: BTreeMap::new(),
            shells: BTreeMap::new(),
            notes: BTreeMap::new(),
            file_windows: BTreeMap::new(),
            custom_windows: BTreeMap::new(),
            pages: Vec::new(),
            note_editors: BTreeMap::new(),
            data: HashMap::new(),
            chunk_replays: Vec::new(),
            messages: Vec::new(),
            system_action_results: Vec::new(),
            errors: Vec::new(),
        };
        this.authenticate().await;
        Ok(this)
    }

    async fn authenticate(&mut self) {
        let encrypted_zeros = self.encrypt.zeros().into();
        let write_zeros = self.write_encrypt.as_ref().map(|e| e.zeros().into());

        self.send(WsClient::Authenticate(encrypted_zeros, write_zeros))
            .await;
    }

    pub async fn send(&mut self, msg: WsClient) {
        let mut buf = Vec::new();
        ciborium::ser::into_writer(&msg, &mut buf).unwrap();
        self.inner.send(Message::Binary(buf.into())).await.unwrap();
    }

    pub async fn send_input(&mut self, id: Sid, data: &[u8]) {
        let offset = 42; // arbitrary, don't reuse the offset in real code though
        let data = self.encrypt.segment(0x200000000, offset, data);
        let page_id = self.shells.get(&id).unwrap().page_id;
        self.send(WsClient::Data(id, page_id, data.into(), offset))
            .await;
    }

    async fn recv(&mut self) -> Option<WsServer> {
        loop {
            match self.inner.next().await.transpose().unwrap() {
                Some(Message::Text(_)) => panic!("unexpected text message over WebSocket"),
                Some(Message::Binary(msg)) => {
                    break Some(ciborium::de::from_reader(&*msg).unwrap())
                }
                Some(_) => (), // ignore other message types, keep looping
                None => break None,
            }
        }
    }

    pub async fn expect_close(&mut self, code: u16) {
        let msg = self.inner.next().await.unwrap().unwrap();
        match msg {
            Message::Close(Some(frame)) => assert!(frame.code == code.into()),
            _ => panic!("unexpected non-close message over WebSocket: {:?}", msg),
        }
    }

    pub async fn flush(&mut self) {
        const FLUSH_DURATION: Duration = Duration::from_millis(50);
        let flush_task = async {
            while let Some(msg) = self.recv().await {
                match msg {
                    WsServer::Hello(user_id, _, server_version, daemon_version) => {
                        self.user_id = user_id;
                        self.server_version = server_version;
                        self.daemon_version = daemon_version;
                    }
                    WsServer::Capabilities(_) => {}
                    WsServer::InvalidAuth() => panic!("invalid authentication"),
                    WsServer::Users(users) => self.users = BTreeMap::from_iter(users),
                    WsServer::UserDiff(id, maybe_user) => {
                        self.users.remove(&id);
                        if let Some(user) = maybe_user {
                            self.users.insert(id, user);
                        }
                    }
                    WsServer::Shells(shells) => self.shells = BTreeMap::from_iter(shells),
                    WsServer::Notes(notes) => self.notes = BTreeMap::from_iter(notes),
                    WsServer::FileWindows(windows) => {
                        self.file_windows = BTreeMap::from_iter(windows)
                    }
                    WsServer::CustomWindows(windows) => {
                        self.custom_windows = BTreeMap::from_iter(windows)
                    }
                    WsServer::Pages(pages) => self.pages = pages,
                    WsServer::SshProfiles(_) => {}
                    WsServer::NoteEditing(id, page_id, editor) => {
                        self.note_editors.remove(&id);
                        if let Some(editor) = editor {
                            self.note_editors.insert(id, (page_id, editor));
                        }
                    }
                    WsServer::NoteText(id, page_id, text) => {
                        assert_eq!(self.notes.get(&id).unwrap().page_id, page_id);
                        let note = self.notes.get_mut(&id).unwrap();
                        note.paragraphs = text.split('\n').map(str::to_owned).collect();
                        note.text = text;
                    }
                    WsServer::NoteParagraphs(id, page_id, paragraphs) => {
                        assert_eq!(self.notes.get(&id).unwrap().page_id, page_id);
                        let note = self.notes.get_mut(&id).unwrap();
                        note.text = paragraphs.join("\n");
                        note.paragraphs = paragraphs;
                    }
                    WsServer::Chunks(WsTerminalChunks::Legacy(
                        id,
                        page_id,
                        replay,
                        seqnum,
                        chunks,
                    ))
                    | WsServer::Chunks(WsTerminalChunks::Generation(
                        id,
                        page_id,
                        _,
                        replay,
                        seqnum,
                        chunks,
                    ))
                    | WsServer::ChunksGeneration(id, page_id, _, replay, seqnum, chunks) => {
                        assert_eq!(self.shells.get(&id).unwrap().page_id, page_id);
                        self.chunk_replays.push((id, replay));
                        let value = self.data.entry(id).or_default();
                        assert_eq!(seqnum, value.len() as u64);
                        for buf in chunks {
                            let plaintext = self.encrypt.segment(
                                0x100000000 | id.0 as u64,
                                value.len() as u64,
                                &buf,
                            );
                            value.push_str(std::str::from_utf8(&plaintext).unwrap());
                        }
                    }
                    WsServer::Hear(id, name, msg) => {
                        self.messages.push((id, name, msg));
                    }
                    WsServer::ShellLatency(_) => {}
                    WsServer::FileResponse(_, _, _) => {}
                    WsServer::SystemActionResult(request_id, action, ok, message) => {
                        self.system_action_results
                            .push((request_id, action, ok, message));
                    }
                    WsServer::Pong(_) => {}
                    WsServer::Error(err) => self.errors.push(err),
                }
            }
        };
        time::timeout(FLUSH_DURATION, flush_task).await.ok();
    }

    pub fn read(&self, id: Sid) -> &str {
        self.data.get(&id).map(|s| &**s).unwrap_or("")
    }
}
