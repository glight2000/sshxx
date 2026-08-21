//! Network gRPC client allowing server control of terminals.

use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::pin;
use std::sync::Arc;

use anyhow::{Context, Result};
use sshx_core::proto::{
    client_update::ClientMessage, server_update::ServerMessage,
    sshx_service_client::SshxServiceClient, ClientUpdate, CloseRequest, FileResponse, NewShell,
    OpenRequest, WorkspacePage, WorkspaceState,
};
use sshx_core::{rand_alphanumeric, Sid, WORKSPACE_FORMAT_VERSION};
use tokio::sync::{mpsc, oneshot, watch, Semaphore};
use tokio::task;
use tokio::time::{self, Duration, Instant, MissedTickBehavior};
use tokio_stream::{wrappers::ReceiverStream, StreamExt};
use tonic::transport::Channel;
use tracing::{debug, error, warn};

use crate::encrypt::Encrypt;
use crate::runner::{Runner, ShellData, ShellOptions};
use crate::{ssh_profiles, workspace};

/// Interval for sending empty heartbeat messages to the server.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);

/// Interval to automatically reestablish connections.
const RECONNECT_INTERVAL: Duration = Duration::from_secs(60);

/// Returns the host portion of an HTTP(S) origin without adding a URL parser
/// dependency to the daemon. Tonic performs the full URI validation when it
/// creates the channel.
fn server_hostname(origin: &str) -> Option<&str> {
    let (_, remainder) = origin.split_once("://")?;
    let authority = remainder.split(['/', '?', '#']).next()?;
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, hostname)| hostname);
    if let Some(ipv6) = authority.strip_prefix('[') {
        return ipv6.split_once(']').map(|(hostname, _)| hostname);
    }
    Some(authority.split(':').next().unwrap_or(authority))
}

/// Returns whether an origin targets the upstream project's public service.
pub fn is_upstream_sshx_origin(origin: &str) -> bool {
    let Some(hostname) = server_hostname(origin) else {
        return false;
    };
    let hostname = hostname.trim_end_matches('.').to_ascii_lowercase();
    hostname == "sshx.io" || hostname.ends_with(".sshx.io")
}

struct PersistencePaths {
    workspace: std::path::PathBuf,
    ssh_profiles: std::path::PathBuf,
    uploads: std::path::PathBuf,
}

/// Handles a single session's communication with the remote server.
pub struct Controller {
    origin: String,
    runner: Runner,
    encrypt: Encrypt,
    encryption_key: String,

    name: String,
    token: String,
    url: String,
    write_url: Option<String>,

    /// Channels with backpressure routing messages to each shell task.
    shells_tx: HashMap<Sid, mpsc::Sender<ShellData>>,
    /// Channel shared with tasks to allow them to output client messages.
    output_tx: mpsc::Sender<ClientMessage>,
    /// Owned receiving end of the `output_tx` channel.
    output_rx: mpsc::Receiver<ClientMessage>,

    workspace_path: Option<std::path::PathBuf>,
    workspace_tx: Option<watch::Sender<WorkspaceState>>,
    ssh_profiles_path: Option<std::path::PathBuf>,
    ssh_profiles_encrypt: Option<Encrypt>,
    uploads: Option<crate::uploads::UploadManager>,
    remote_profiles: HashMap<Sid, sshx_core::proto::SshProfile>,
    file_tasks: Arc<Semaphore>,
}

impl Controller {
    /// Construct a new controller, connecting to the remote server.
    pub async fn new(
        origin: &str,
        name: &str,
        runner: Runner,
        enable_readers: bool,
    ) -> Result<Self> {
        Self::new_inner(origin, name, runner, enable_readers, None, None).await
    }

    /// Construct a controller with an optional fixed encryption key for local
    /// testing.
    pub async fn new_with_encryption_key(
        origin: &str,
        name: &str,
        runner: Runner,
        enable_readers: bool,
        encryption_key: Option<&str>,
    ) -> Result<Self> {
        Self::new_inner(origin, name, runner, enable_readers, encryption_key, None).await
    }

    /// Construct a persistent controller using the default workspace file in
    /// the daemon's current working directory.
    pub async fn new_persistent_with_encryption_key(
        origin: &str,
        name: &str,
        runner: Runner,
        enable_readers: bool,
        encryption_key: Option<&str>,
    ) -> Result<Self> {
        let workspace_path = workspace::path_in_current_dir()?;
        let ssh_profiles_path = ssh_profiles::path_in_current_dir()?;
        let uploads_path = crate::uploads::path_in_current_dir()?;
        let persistence = PersistencePaths {
            workspace: workspace_path,
            ssh_profiles: ssh_profiles_path,
            uploads: uploads_path,
        };
        Self::new_inner(
            origin,
            name,
            runner,
            enable_readers,
            encryption_key,
            Some(persistence),
        )
        .await
    }

    async fn new_inner(
        origin: &str,
        name: &str,
        runner: Runner,
        enable_readers: bool,
        encryption_key: Option<&str>,
        persistence: Option<PersistencePaths>,
    ) -> Result<Self> {
        let (workspace_path, ssh_profiles_path, uploads_path) = match persistence {
            Some(paths) => (
                Some(paths.workspace),
                Some(paths.ssh_profiles),
                Some(paths.uploads),
            ),
            None => (None, None, None),
        };
        debug!(%origin, "connecting to server");
        let encryption_key = encryption_key
            .map(str::to_owned)
            .unwrap_or_else(|| rand_alphanumeric(14)); // 83.3 bits of entropy by default

        let kdf_task = {
            let encryption_key = encryption_key.clone();
            task::spawn_blocking(move || Encrypt::new(&encryption_key))
        };

        let (write_password, kdf_write_password_task) = if enable_readers {
            let write_password = rand_alphanumeric(14); // 83.3 bits of entropy
            let task = {
                let write_password = write_password.clone();
                task::spawn_blocking(move || Encrypt::new(&write_password))
            };
            (Some(write_password), Some(task))
        } else {
            (None, None)
        };

        let mut client = Self::connect(origin).await?;
        let encrypt = kdf_task.await?;
        let write_password_hash = if let Some(task) = kdf_write_password_task {
            Some(task.await?.zeros().into())
        } else {
            None
        };

        let workspace_state = if let Some(path) = &workspace_path {
            match workspace::load(path).await {
                Ok(workspace) => workspace,
                Err(err) => {
                    warn!(?err, path = %path.display(), "isolating invalid workspace file");
                    match workspace::quarantine(path).await {
                        Ok(destination) => {
                            warn!(path = %destination.display(), "preserved invalid workspace file")
                        }
                        Err(quarantine_err) => warn!(
                            ?quarantine_err,
                            path = %path.display(),
                            "failed to isolate invalid workspace file"
                        ),
                    }
                    None
                }
            }
        } else {
            None
        };

        let (ssh_profiles_encrypt, ssh_profile_state) = if let Some(path) = &ssh_profiles_path {
            match ssh_profiles::load_or_create_encryptor(path).await {
                Ok(profile_encrypt) => {
                    let profiles = match ssh_profiles::load(path, &profile_encrypt).await {
                        Ok(Some(profiles)) => profiles,
                        Ok(None) => ssh_profiles::empty(),
                        Err(err) => {
                            warn!(?err, path = %path.display(), "isolating invalid SSH profile file");
                            match ssh_profiles::quarantine(path).await {
                                Ok(destination) => {
                                    warn!(path = %destination.display(), "preserved invalid SSH profile file")
                                }
                                Err(quarantine_err) => warn!(
                                    ?quarantine_err,
                                    "failed to isolate invalid SSH profile file"
                                ),
                            }
                            ssh_profiles::empty()
                        }
                    };
                    (Some(profile_encrypt), Some(profiles))
                }
                Err(err) => {
                    warn!(?err, path = %path.display(), "replacing invalid SSH profile key");
                    match ssh_profiles::replace_invalid_encryptor(path).await {
                        Ok(profile_encrypt) => (Some(profile_encrypt), Some(ssh_profiles::empty())),
                        Err(recovery_err) => {
                            warn!(?recovery_err, path = %path.display(), "SSH profile persistence is unavailable");
                            (None, Some(ssh_profiles::empty()))
                        }
                    }
                }
            }
        } else {
            (None, None)
        };
        let uploads = match uploads_path {
            Some(path) => Some(crate::uploads::UploadManager::new(path).await?),
            None => None,
        };

        let req = OpenRequest {
            origin: origin.into(),
            encrypted_zeros: encrypt.zeros().into(),
            name: name.into(),
            write_password_hash,
            daemon_version: env!("CARGO_PKG_VERSION").into(),
            workspace: workspace_state.clone(),
            ssh_profiles: ssh_profile_state,
        };
        let mut resp = client.open(req).await?.into_inner();
        resp.url = resp.url + "#" + &encryption_key;

        let write_url = if let Some(write_password) = write_password {
            Some(resp.url.clone() + "," + &write_password)
        } else {
            None
        };

        let (output_tx, output_rx) = mpsc::channel(64);
        let workspace_tx = workspace_path.as_ref().map(|path| {
            let initial = workspace_state.unwrap_or(WorkspaceState {
                format_version: WORKSPACE_FORMAT_VERSION,
                shells: Vec::new(),
                notes: Vec::new(),
                file_windows: Vec::new(),
                pages: vec![WorkspacePage {
                    id: 1,
                    name: "Page 1".into(),
                }],
            });
            let (tx, rx) = watch::channel(initial);
            tokio::spawn(workspace::writer(path.clone(), rx));
            tx
        });
        Ok(Self {
            origin: origin.into(),
            runner,
            encrypt,
            encryption_key,
            name: resp.name,
            token: resp.token,
            url: resp.url,
            write_url,
            shells_tx: HashMap::new(),
            output_tx,
            output_rx,
            workspace_path,
            workspace_tx,
            ssh_profiles_path,
            ssh_profiles_encrypt,
            uploads,
            remote_profiles: HashMap::new(),
            file_tasks: Arc::new(Semaphore::new(4)),
        })
    }

    /// Create a new gRPC client to the HTTP(S) origin.
    ///
    /// This is used on reconnection to the server, since some replicas may be
    /// gracefully shutting down, which means connected clients need to start a
    /// new TCP handshake.
    async fn connect(origin: &str) -> Result<SshxServiceClient<Channel>, tonic::transport::Error> {
        SshxServiceClient::connect(String::from(origin)).await
    }

    /// Returns the name of the session.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the URL of the session.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the write URL of the session, if it exists.
    pub fn write_url(&self) -> Option<&str> {
        self.write_url.as_deref()
    }

    /// Returns the encryption key for this session, hidden from the server.
    pub fn encryption_key(&self) -> &str {
        &self.encryption_key
    }

    /// Run the controller forever, listening for requests from the server.
    pub async fn run(&mut self) -> ! {
        let mut last_retry = Instant::now();
        let mut retries = 0;
        loop {
            if let Err(err) = self.try_channel().await {
                if last_retry.elapsed() >= Duration::from_secs(10) {
                    retries = 0;
                }
                let secs = 2_u64.pow(retries.min(4));
                error!(%err, "disconnected, retrying in {secs}s...");
                time::sleep(Duration::from_secs(secs)).await;
                retries += 1;
            }
            last_retry = Instant::now();
        }
    }

    /// Helper function used by `run()` that can return errors.
    async fn try_channel(&mut self) -> Result<()> {
        let (tx, rx) = mpsc::channel(16);

        let hello = ClientMessage::Hello(format!("{},{}", self.name, self.token));
        send_msg(&tx, hello).await?;

        let mut client = Self::connect(&self.origin).await?;
        let resp = client.channel(ReceiverStream::new(rx)).await?;
        let mut messages = resp.into_inner(); // A stream of server messages.

        let mut interval = time::interval(HEARTBEAT_INTERVAL);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut reconnect = pin!(time::sleep(RECONNECT_INTERVAL));
        loop {
            let message = tokio::select! {
                _ = interval.tick() => {
                    tx.send(ClientUpdate::default()).await?;
                    continue;
                }
                msg = self.output_rx.recv() => {
                    let msg = msg.context("unreachable: output_tx was closed?")?;
                    send_msg(&tx, msg).await?;
                    continue;
                }
                item = messages.next() => {
                    item.context("server closed connection")??
                        .server_message
                        .context("server message is missing")?
                }
                _ = &mut reconnect => {
                    return Ok(()); // Reconnect to the server.
                }
            };

            match message {
                ServerMessage::Input(input) => {
                    let data = self.encrypt.segment(0x200000000, input.offset, &input.data);
                    if let Some(sender) = self.shells_tx.get(&Sid(input.id)) {
                        // This line applies backpressure if the shell task is overloaded.
                        sender.send(ShellData::Data(data)).await.ok();
                    } else {
                        warn!(%input.id, "received data for non-existing shell");
                    }
                }
                ServerMessage::ImageUpload(upload) => {
                    let id = Sid(upload.id);
                    if self.remote_profiles.contains_key(&id) {
                        if upload.offset == 0 {
                            send_msg(
                                &tx,
                                ClientMessage::Error(format!(
                                    "Terminal {id} is an SSH connection; browser images can only \
                                     be uploaded to local terminals."
                                )),
                            )
                            .await?;
                        }
                        continue;
                    }
                    let Some(sender) = self.shells_tx.get(&id).cloned() else {
                        if upload.offset == 0 {
                            send_msg(
                                &tx,
                                ClientMessage::Error(format!(
                                    "Cannot upload an image to missing terminal {id}."
                                )),
                            )
                            .await?;
                        }
                        continue;
                    };
                    let Some(uploads) = &mut self.uploads else {
                        if upload.offset == 0 {
                            send_msg(
                                &tx,
                                ClientMessage::Error(
                                    "Image uploads are unavailable for this daemon session.".into(),
                                ),
                            )
                            .await?;
                        }
                        continue;
                    };
                    match uploads.accept(&self.encrypt, &upload).await {
                        Ok(Some(path)) => {
                            let input = format!("{} ", path.display()).into_bytes();
                            sender.send(ShellData::Data(input)).await.ok();
                        }
                        Ok(None) => {}
                        Err(err) => {
                            uploads.abort(id, &upload.upload_id).await;
                            send_msg(
                                &tx,
                                ClientMessage::Error(format!("Image upload failed: {err}")),
                            )
                            .await?;
                        }
                    }
                }
                ServerMessage::FileRequest(file_request) => {
                    let id = Sid(file_request.id);
                    let plaintext =
                        self.encrypt
                            .segment(file_request.request_stream, 0, &file_request.data);
                    let request =
                        serde_json::from_slice::<crate::file_browser::FileOperationRequest>(
                            &plaintext,
                        )
                        .context("invalid file request");
                    let sender = self.shells_tx.get(&id).cloned();
                    let remote_profile = self.remote_profiles.get(&id).cloned();
                    let working_directory = if remote_profile.is_some() {
                        None
                    } else if let Some(sender) = sender.as_ref() {
                        let (working_tx, working_rx) = oneshot::channel();
                        if sender
                            .send(ShellData::WorkingDirectory(working_tx))
                            .await
                            .is_ok()
                        {
                            working_rx.await.ok().flatten()
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    let output_tx = self.output_tx.clone();
                    let encrypt = self.encrypt.clone();
                    let limiter = Arc::clone(&self.file_tasks);
                    task::spawn(async move {
                        let response = match (request, sender) {
                            (Ok(request), Some(_)) => {
                                let operation = match limiter.acquire_owned().await {
                                    Ok(_permit) => tokio::time::timeout(
                                        Duration::from_secs(30),
                                        crate::file_browser::execute(
                                            &request,
                                            working_directory,
                                            remote_profile.as_ref(),
                                        ),
                                    )
                                    .await
                                    .map_err(|_| anyhow::anyhow!("filesystem operation timed out"))
                                    .and_then(|result| result),
                                    Err(_) => Err(anyhow::anyhow!(
                                        "filesystem operations are unavailable"
                                    )),
                                };
                                match operation {
                                    Ok(response) => serde_json::to_vec(&response),
                                    Err(error) => serde_json::to_vec(
                                        &crate::file_browser::FileOperationResponse::failure(
                                            &request, error,
                                        ),
                                    ),
                                }
                            }
                            (Err(error), _) => serde_json::to_vec(&serde_json::json!({
                                "ok": false,
                                "operation": "list",
                                "path": "",
                                "error": error.to_string(),
                            })),
                            (Ok(_), None) => serde_json::to_vec(&serde_json::json!({
                                "ok": false,
                                "operation": "list",
                                "path": "",
                                "error": format!("terminal {id} does not exist"),
                            })),
                        };
                        let Ok(response) = response else { return };
                        let data = encrypt.segment(file_request.response_stream, 0, &response);
                        output_tx
                            .send(ClientMessage::FileResponse(FileResponse {
                                request_id: file_request.request_id,
                                stream_num: file_request.response_stream,
                                data: data.into(),
                            }))
                            .await
                            .ok();
                    });
                }
                ServerMessage::CreateShell(new_shell) => {
                    let id = Sid(new_shell.id);
                    let center = (new_shell.x, new_shell.y);
                    let page_id = new_shell.page_id.max(1);
                    let rows = u16::try_from(new_shell.rows)
                        .ok()
                        .filter(|value| (8..=500).contains(value))
                        .unwrap_or(24);
                    let cols = u16::try_from(new_shell.cols)
                        .ok()
                        .filter(|value| (32..=500).contains(value))
                        .unwrap_or(80);
                    let width = u16::try_from(new_shell.width)
                        .ok()
                        .filter(|value| *value == 0 || (240..=4_000).contains(value))
                        .unwrap_or(0);
                    let height = u16::try_from(new_shell.height)
                        .ok()
                        .filter(|value| *value == 0 || (160..=4_000).contains(value))
                        .unwrap_or(0);
                    let source_id = new_shell.source_id.map(Sid);
                    let explicit_working_directory = (!new_shell.working_directory.is_empty())
                        .then(|| PathBuf::from(new_shell.working_directory));
                    // CloneWindowed has a source without an explicit path. CreateAt also
                    // has a source, but opens a directory and must start with fresh history.
                    let copies_source_history =
                        source_id.is_some() && explicit_working_directory.is_none();
                    let ssh_profile = new_shell.ssh_profile.or_else(|| {
                        source_id
                            .and_then(|source_id| self.remote_profiles.get(&source_id).cloned())
                    });
                    let remote_profile = ssh_profile.clone();
                    let is_remote = ssh_profile.is_some();
                    let theme = new_shell.theme;
                    let background = new_shell.background;
                    if !self.shells_tx.contains_key(&id) {
                        let working_directory = if explicit_working_directory.is_some() {
                            explicit_working_directory
                        } else if ssh_profile.is_none() {
                            if let Some(source_id) = source_id {
                                if let Some(sender) = self.shells_tx.get(&source_id).cloned() {
                                    let (tx, rx) = oneshot::channel();
                                    if sender.send(ShellData::WorkingDirectory(tx)).await.is_ok() {
                                        rx.await.ok().flatten()
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        };
                        if !is_remote && copies_source_history {
                            if let Some(source_id) = source_id {
                                match self.runner.clone_history(source_id, id).await {
                                    Ok(true) => debug!(%source_id, %id, "copied terminal history"),
                                    Ok(false) => {
                                        debug!(%source_id, %id, "source terminal history is not persisted yet")
                                    }
                                    Err(err) => {
                                        warn!(%source_id, %id, ?err, "failed to copy terminal history")
                                    }
                                }
                            }
                        }
                        self.spawn_shell_task(
                            id,
                            center,
                            page_id,
                            ShellOptions {
                                working_directory,
                                ssh_profile,
                                rows,
                                cols,
                                theme,
                                background,
                                width,
                                height,
                            },
                        );
                        if is_remote {
                            if let Some(profile) = remote_profile {
                                self.remote_profiles.insert(id, profile);
                            }
                        }
                    } else {
                        warn!(%id, "server asked to create duplicate shell");
                    }
                }
                ServerMessage::CloseShell(id) => {
                    // Explicit close is distinct from dropping the controller: hosted PTYs
                    // survive daemon restarts and are only killed by this message.
                    if let Some(sender) = self.shells_tx.remove(&Sid(id)) {
                        sender.send(ShellData::Close).await.ok();
                    }
                    self.remote_profiles.remove(&Sid(id));
                    send_msg(&tx, ClientMessage::ClosedShell(id)).await?;
                }
                ServerMessage::Sync(seqnums) => {
                    for (id, seq) in seqnums.map {
                        if let Some(sender) = self.shells_tx.get(&Sid(id)) {
                            sender.send(ShellData::Sync(seq)).await.ok();
                        } else {
                            warn!(%id, "received sequence number for non-existing shell");
                            send_msg(&tx, ClientMessage::ClosedShell(id)).await?;
                        }
                    }
                }
                ServerMessage::Resize(msg) => {
                    if let Some(sender) = self.shells_tx.get(&Sid(msg.id)) {
                        sender.send(ShellData::Size(msg.rows, msg.cols)).await.ok();
                    } else {
                        warn!(%msg.id, "received resize for non-existing shell");
                    }
                }
                ServerMessage::Workspace(workspace) => {
                    if workspace.format_version != WORKSPACE_FORMAT_VERSION {
                        warn!(
                            version = workspace.format_version,
                            "ignoring unsupported workspace format"
                        );
                    } else if let Some(tx) = &self.workspace_tx {
                        tx.send_replace(workspace);
                    }
                }
                ServerMessage::SshProfiles(profiles) => {
                    if let (Some(path), Some(encrypt)) =
                        (&self.ssh_profiles_path, &self.ssh_profiles_encrypt)
                    {
                        if let Err(err) = ssh_profiles::save(path, encrypt, &profiles).await {
                            warn!(?err, path = %path.display(), "failed to persist SSH profiles");
                            self.output_tx
                                .send(ClientMessage::Error(
                                    "Failed to save SSH connection profiles.".into(),
                                ))
                                .await
                                .ok();
                        }
                    }
                }
                ServerMessage::Ping(ts) => {
                    // Echo back the timestamp, for stateless latency measurement.
                    send_msg(&tx, ClientMessage::Pong(ts)).await?;
                }
                ServerMessage::Error(err) => {
                    error!(?err, "error received from server");
                }
            }
        }
    }

    /// Entry point to start a new terminal task on the client.
    fn spawn_shell_task(
        &mut self,
        id: Sid,
        center: (i32, i32),
        page_id: u32,
        options: ShellOptions,
    ) {
        let (shell_tx, shell_rx) = mpsc::channel(16);
        let opt = self.shells_tx.insert(id, shell_tx);
        debug_assert!(opt.is_none(), "shell ID cannot be in existing tasks");

        let runner = self.runner.clone();
        let encrypt = self.encrypt.clone();
        let output_tx = self.output_tx.clone();
        tokio::spawn(async move {
            let rows = options.rows;
            let cols = options.cols;
            let theme = options.theme.clone();
            let options_background = options.background.clone();
            let width = options.width;
            let height = options.height;
            debug!(%id, "spawning new shell");
            let new_shell = NewShell {
                id: id.0,
                x: center.0,
                y: center.1,
                source_id: None,
                page_id,
                rows: rows.into(),
                cols: cols.into(),
                ssh_profile: None,
                theme,
                width: width.into(),
                height: height.into(),
                background: options_background,
                working_directory: options
                    .working_directory
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            };
            if let Err(err) = output_tx
                .send(ClientMessage::CreatedShell(Box::new(new_shell)))
                .await
            {
                error!(%id, ?err, "failed to send shell creation message");
                return;
            }
            if let Err(err) = runner
                .run(id, encrypt, shell_rx, output_tx.clone(), options)
                .await
            {
                let err = ClientMessage::Error(err.to_string());
                output_tx.send(err).await.ok();
            }
            output_tx.send(ClientMessage::ClosedShell(id.0)).await.ok();
        });
    }

    /// Terminate this session gracefully.
    pub async fn close(&self) -> Result<()> {
        debug!("closing session");
        if let (Some(path), Some(tx)) = (&self.workspace_path, &self.workspace_tx) {
            let workspace = tx.borrow().clone();
            workspace::save(path, &workspace).await?;
        }
        let req = CloseRequest {
            name: self.name.clone(),
            token: self.token.clone(),
        };
        let mut client = Self::connect(&self.origin).await?;
        client.close(req).await?;
        Ok(())
    }
}

/// Attempt to send a client message over an update channel.
async fn send_msg(tx: &mpsc::Sender<ClientUpdate>, message: ClientMessage) -> Result<()> {
    let update = ClientUpdate {
        client_message: Some(message),
    };
    tx.send(update)
        .await
        .context("failed to send message to server")
}

#[cfg(test)]
mod tests {
    use super::{is_upstream_sshx_origin, server_hostname};

    #[test]
    fn extracts_server_hostnames() {
        assert_eq!(
            server_hostname("https://example.com:8051/path"),
            Some("example.com")
        );
        assert_eq!(server_hostname("http://[fd00::25]:8051"), Some("fd00::25"));
        assert_eq!(server_hostname("not-an-origin"), None);
    }

    #[test]
    fn identifies_upstream_public_service_origins() {
        for origin in [
            "https://sshx.io",
            "https://SSHX.IO.",
            "https://sshx.io:443/path",
            "https://edge.sshx.io",
        ] {
            assert!(is_upstream_sshx_origin(origin), "accepted {origin}");
        }
    }

    #[test]
    fn allows_self_hosted_origins() {
        for origin in [
            "http://localhost:8051",
            "http://192.168.1.25:8051",
            "http://[fd00::25]:8051",
            "https://sshxx.example",
            "https://sshx.io.example",
        ] {
            assert!(!is_upstream_sshx_origin(origin), "rejected {origin}");
        }
    }
}
