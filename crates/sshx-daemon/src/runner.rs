//! Defines tasks that control the behavior of a single shell in the client.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use encoding_rs::{CoderResult, UTF_8};
use sshx_core::proto::{client_update::ClientMessage, SshAuthMethod, SshProfile, TerminalData};
use sshx_core::Sid;
use sshxx_terminal_host::client::Client as TerminalHostClient;
use sshxx_terminal_host::protocol::frame::Message as HostMessage;
use sshxx_terminal_host::protocol::wire::CreateTerminal;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, oneshot},
    time::{self, Duration},
};
use tracing::warn;

use crate::encrypt::Encrypt;
use crate::terminal::Terminal;
use crate::terminal_host::TerminalHostConfig;

const CONTENT_CHUNK_SIZE: usize = 1 << 16; // Send at most this many bytes at a time.
const CONTENT_ROLLING_BYTES: usize = 8 << 20; // Store at least this much content.
const CONTENT_PRUNE_BYTES: usize = 12 << 20; // Prune when we exceed this length.
const INITIAL_DIRECTORY_ENV: &str = "SSHXX_INITIAL_DIRECTORY";
const BASH_INITIAL_DIRECTORY_COMMAND: &str = "if [ -n \"${SSHXX_INITIAL_DIRECTORY+x}\" ]; then builtin cd -- \"$SSHXX_INITIAL_DIRECTORY\"; unset SSHXX_INITIAL_DIRECTORY; fi";

/// Variants of terminal behavior that are used by the controller.
#[derive(Debug, Clone)]
pub enum Runner {
    /// Spawns the specified shell as a subprocess, forwarding PTYs.
    Shell(String),

    /// Runs shells in an independent host so daemon restarts do not close them.
    HostedShell {
        /// Local shell program used for newly created terminals.
        shell: String,
        /// Authenticated local host connection and history policy.
        host: TerminalHostConfig,
    },

    /// Mock runner that only echos its input, useful for testing.
    Echo,
}

#[derive(Debug)]
pub(crate) struct ShellOptions {
    pub working_directory: Option<PathBuf>,
    pub ssh_profile: Option<SshProfile>,
    pub ssh_profile_id: String,
    /// Whether this request may attach to an existing PTY with the same stable
    /// host ID. Source-derived creation actions disable this behavior.
    pub reattach_existing: bool,
    pub rows: u16,
    pub cols: u16,
    pub theme: String,
    pub background: String,
    pub width: u16,
    pub height: u16,
}

/// Internal message routed to shell runners.
pub enum ShellData {
    /// Sequence of input bytes from the server.
    Data(Vec<u8>),
    /// Information about the server's current sequence number.
    Sync(u64),
    /// Resize the shell to a different number of rows and columns.
    Size(u32, u32),
    /// Request the shell's current working directory for terminal duplication.
    WorkingDirectory(oneshot::Sender<Option<PathBuf>>),
    /// Explicitly close the shell and its hosted process.
    Close,
}

impl Runner {
    /// Asynchronous task to run a single shell with process I/O.
    pub(crate) async fn run(
        &self,
        id: Sid,
        encrypt: Encrypt,
        shell_rx: mpsc::Receiver<ShellData>,
        output_tx: mpsc::Sender<ClientMessage>,
        mut options: ShellOptions,
    ) -> Result<()> {
        match self {
            Self::Shell(shell) => {
                let (program, args) = launch_command(shell, &mut options)?;
                shell_task(id, encrypt, &program, &args, shell_rx, output_tx, options).await
            }
            Self::HostedShell { shell, host } => {
                let local_shell = options.ssh_profile.is_none();
                let (program, args) = launch_command(shell, &mut options)?;
                hosted_shell_task(
                    id,
                    encrypt,
                    &program,
                    args,
                    local_shell,
                    host,
                    shell_rx,
                    output_tx,
                    options,
                )
                .await
            }
            Self::Echo => echo_task(id, encrypt, shell_rx, output_tx).await,
        }
    }

    /// Copy a daemon-owned history snapshot when duplicating a local terminal.
    pub(crate) async fn clone_history(&self, source_id: Sid, target_id: Sid) -> Result<bool> {
        match self {
            Self::HostedShell { host, .. } => host.clone_history(source_id.0, target_id.0).await,
            Self::Shell(_) | Self::Echo => Ok(false),
        }
    }

    /// Restart the independent terminal-host runtime, explicitly terminating
    /// every PTY it owns. The host process keeps supervising its local endpoint
    /// so this works without OS service-manager privileges.
    pub(crate) async fn restart_terminal_host(&self) -> Result<()> {
        let Self::HostedShell { host, .. } = self else {
            bail!("terminal-host is unavailable for the active runner");
        };
        let mut client = TerminalHostClient::connect(
            &host.endpoint,
            host.authentication_token.clone(),
            env!("CARGO_PKG_VERSION"),
        )
        .await?;
        let request_id = client.restart(true).await?;
        match receive_host_response(&mut client, request_id).await? {
            HostMessage::Ack(_) => {}
            HostMessage::Error(error) => {
                bail!("terminal host {}: {}", error.code, error.message)
            }
            _ => bail!("terminal host returned an invalid restart response"),
        }
        // The acknowledgement precedes listener teardown so the response is
        // not lost with the control connection. Confirm the replacement
        // runtime is actually accepting authenticated connections.
        time::sleep(Duration::from_millis(150)).await;
        let deadline = time::Instant::now() + Duration::from_secs(5);
        loop {
            match TerminalHostClient::connect(
                &host.endpoint,
                host.authentication_token.clone(),
                env!("CARGO_PKG_VERSION"),
            )
            .await
            {
                Ok(_) => return Ok(()),
                Err(error) if time::Instant::now() < deadline => {
                    warn!(?error, "waiting for restarted terminal host");
                    time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => {
                    return Err(error).context("terminal host did not recover after restart")
                }
            }
        }
    }
}

fn launch_command(shell: &str, options: &mut ShellOptions) -> Result<(String, Vec<String>)> {
    match &options.ssh_profile {
        Some(profile) => {
            let (program, mut args) = ssh_command(profile)?;
            if let Some(directory) = options.working_directory.take() {
                let quoted_directory = shell_quote(&directory.to_string_lossy());
                let quoted_prompt_command = shell_quote(BASH_INITIAL_DIRECTORY_COMMAND);
                let host_index = args.len().saturating_sub(1);
                args.insert(host_index, "-t".into());
                args.push(format!(
                    "export {INITIAL_DIRECTORY_ENV}={quoted_directory}; export \
                     PROMPT_COMMAND={quoted_prompt_command}\"${{PROMPT_COMMAND:+; \
                     $PROMPT_COMMAND}}\"; cd -- {quoted_directory} && exec \
                     \"${{SHELL:-/bin/sh}}\" -l"
                ));
            }
            Ok((program, args))
        }
        None => Ok((shell.to_owned(), Vec::new())),
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// Asynchronous task handling a single shell within the session.
async fn shell_task(
    id: Sid,
    encrypt: Encrypt,
    program: &str,
    args: &[String],
    mut shell_rx: mpsc::Receiver<ShellData>,
    output_tx: mpsc::Sender<ClientMessage>,
    options: ShellOptions,
) -> Result<()> {
    let mut term = Terminal::new(program, args, options.working_directory.as_deref()).await?;
    term.set_winsize(options.rows, options.cols)?;

    let mut content = String::new(); // content from the terminal
    let mut content_offset = 0; // bytes before the first character of `content`
    let mut decoder = UTF_8.new_decoder(); // UTF-8 streaming decoder
    let mut seq = 0; // our log of the server's sequence number
    let mut seq_outdated = 0; // number of times seq has been outdated
    let mut buf = [0u8; 4096]; // buffer for reading
    let mut finished = false; // set when this is done

    while !finished {
        tokio::select! {
            result = term.read(&mut buf) => {
                let n = result?;
                if n == 0 {
                    finished = true;
                } else {
                    content.reserve(decoder.max_utf8_buffer_length(n).unwrap());
                    let (result, _, _) = decoder.decode_to_string(&buf[..n], &mut content, false);
                    debug_assert!(result == CoderResult::InputEmpty);
                }
            }
            item = shell_rx.recv() => {
                match item {
                    Some(ShellData::Data(data)) => {
                        term.write_all(&data).await?;
                    }
                    Some(ShellData::Sync(seq2)) => {
                        if seq2 < seq as u64 {
                            seq_outdated += 1;
                            if seq_outdated >= 3 {
                                seq = seq2 as usize;
                            }
                        }
                    }
                    Some(ShellData::Size(rows, cols)) => {
                        term.set_winsize(rows as u16, cols as u16)?;
                    }
                    Some(ShellData::WorkingDirectory(sender)) => {
                        sender.send(term.working_directory().await).ok();
                    }
                    Some(ShellData::Close) => finished = true,
                    None => finished = true, // Server closed this shell.
                }
            }
        }

        if finished {
            content.reserve(decoder.max_utf8_buffer_length(0).unwrap());
            let (result, _, _) = decoder.decode_to_string(&[], &mut content, true);
            debug_assert!(result == CoderResult::InputEmpty);
        }

        // Send data if the server has fallen behind.
        if content_offset + content.len() > seq {
            let start = prev_char_boundary(&content, seq - content_offset);
            let end = prev_char_boundary(&content, (start + CONTENT_CHUNK_SIZE).min(content.len()));
            let data = encrypt.segment(
                0x100000000 | id.0 as u64, // stream number
                (content_offset + start) as u64,
                &content.as_bytes()[start..end],
            );
            let data = TerminalData {
                id: id.0,
                data: data.into(),
                seq: (content_offset + start) as u64,
            };
            output_tx.send(ClientMessage::Data(data)).await?;
            seq = content_offset + end;
            seq_outdated = 0;
        }

        if content.len() > CONTENT_PRUNE_BYTES && seq - CONTENT_ROLLING_BYTES > content_offset {
            let pruned = (seq - CONTENT_ROLLING_BYTES) - content_offset;
            let pruned = prev_char_boundary(&content, pruned);
            content_offset += pruned;
            content.drain(..pruned);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn hosted_shell_task(
    id: Sid,
    encrypt: Encrypt,
    program: &str,
    mut args: Vec<String>,
    local_shell: bool,
    host: &TerminalHostConfig,
    mut shell_rx: mpsc::Receiver<ShellData>,
    output_tx: mpsc::Sender<ClientMessage>,
    options: ShellOptions,
) -> Result<()> {
    let terminal_id = host.terminal_id(id.0);
    let mut environment = terminal_environment();
    if local_shell {
        environment.extend(history_launch_policy(
            host,
            &terminal_id,
            program,
            &mut args,
            options.working_directory.as_deref(),
        ));
    }
    let durable_ssh_preset = !options.ssh_profile_id.is_empty();
    let mut recovering = false;
    loop {
        let mut client = match TerminalHostClient::connect(
            &host.endpoint,
            host.authentication_token.clone(),
            env!("CARGO_PKG_VERSION"),
        )
        .await
        {
            Ok(client) => client,
            Err(error) if recovering => {
                warn!(%id, ?error, "waiting for terminal host to recover SSH preset");
                time::sleep(Duration::from_millis(500)).await;
                continue;
            }
            Err(error) => return Err(error).context("failed to connect to sshxx-terminal-host"),
        };

        if let Err(error) = attach_or_create_hosted_terminal(
            &mut client,
            &terminal_id,
            program,
            args.clone(),
            Some(environment.clone()),
            &options,
        )
        .await
        {
            if recovering {
                warn!(%id, ?error, "retrying SSH preset after terminal host restart");
                time::sleep(Duration::from_millis(500)).await;
                continue;
            }
            return Err(error);
        }

        match forward_hosted_terminal(
            id,
            &terminal_id,
            &encrypt,
            &mut client,
            &mut shell_rx,
            &output_tx,
        )
        .await
        {
            Ok(HostedShellEnd::Detached) => return Ok(()),
            Ok(HostedShellEnd::Closed | HostedShellEnd::Exited) => {
                host.remove_history(id.0).await?;
                return Ok(());
            }
            Err(error) if durable_ssh_preset => {
                warn!(%id, ?error, "terminal host state was lost; recreating SSH preset");
                output_tx
                    .send(ClientMessage::RestartedShell(id.0))
                    .await
                    .context("failed to reset recovered SSH terminal output")?;
                recovering = true;
                time::sleep(Duration::from_millis(500)).await;
            }
            Err(error) => return Err(error),
        }
    }
}

enum HostedShellEnd {
    Closed,
    Detached,
    Exited,
}

async fn forward_hosted_terminal(
    id: Sid,
    terminal_id: &str,
    encrypt: &Encrypt,
    client: &mut TerminalHostClient,
    shell_rx: &mut mpsc::Receiver<ShellData>,
    output_tx: &mpsc::Sender<ClientMessage>,
) -> Result<HostedShellEnd> {
    let mut content = String::new();
    let mut content_offset = 0usize;
    let mut decoder = UTF_8.new_decoder();
    let mut host_sequence = 0u64;
    let mut seq = 0usize;
    let mut seq_outdated = 0usize;
    let mut pending_working_directories = HashMap::<u64, oneshot::Sender<Option<PathBuf>>>::new();
    let mut finished = false;

    while !finished {
        tokio::select! {
            frame = client.receive() => {
                let frame = frame?.context("terminal host disconnected")?;
                match frame.message {
                    Some(HostMessage::TerminalOutput(output)) if output.terminal_id == terminal_id => {
                        let end = output.sequence.saturating_add(output.data.len() as u64);
                        if end > host_sequence {
                            if output.sequence > host_sequence {
                                decoder = UTF_8.new_decoder();
                                host_sequence = output.sequence;
                            }
                            let start = host_sequence.saturating_sub(output.sequence) as usize;
                            let bytes = &output.data[start.min(output.data.len())..];
                            content.reserve(decoder.max_utf8_buffer_length(bytes.len()).unwrap());
                            let (result, _, _) = decoder.decode_to_string(bytes, &mut content, false);
                            debug_assert!(result == CoderResult::InputEmpty);
                            host_sequence = end;
                        }
                    }
                    Some(HostMessage::TerminalExited(exit)) if exit.terminal_id == terminal_id => {
                        if exit.host_shutdown {
                            bail!("terminal host shut down");
                        }
                        finished = true;
                    }
                    Some(HostMessage::WorkingDirectory(directory)) => {
                        if let Some(sender) = pending_working_directories.remove(&frame.request_id) {
                            let path = (!directory.path.is_empty()).then(|| PathBuf::from(directory.path));
                            sender.send(path).ok();
                        }
                    }
                    Some(HostMessage::Error(error)) => {
                        if let Some(sender) = pending_working_directories.remove(&frame.request_id) {
                            sender.send(None).ok();
                        } else {
                            bail!("terminal host {}: {}", error.code, error.message);
                        }
                    }
                    _ => {}
                }
            }
            item = shell_rx.recv() => {
                match item {
                    Some(ShellData::Data(data)) => {
                        client.input(terminal_id, data).await?;
                    }
                    Some(ShellData::Sync(seq2)) => {
                        if seq2 < seq as u64 {
                            seq_outdated += 1;
                            if seq_outdated >= 3 {
                                seq = seq2 as usize;
                            }
                        }
                    }
                    Some(ShellData::Size(rows, cols)) => {
                        client.resize(terminal_id, rows, cols).await?;
                    }
                    Some(ShellData::WorkingDirectory(sender)) => {
                        let request_id = client.get_working_directory(terminal_id).await?;
                        pending_working_directories.insert(request_id, sender);
                    }
                    Some(ShellData::Close) => {
                        let request_id = client.close_terminal(terminal_id).await?;
                        match receive_host_response(client, request_id).await? {
                            HostMessage::Ack(_) => {}
                            HostMessage::Error(error) => {
                                bail!("terminal host {}: {}", error.code, error.message)
                            }
                            _ => bail!("terminal host returned an invalid close response"),
                        }
                        return Ok(HostedShellEnd::Closed);
                    }
                    None => return Ok(HostedShellEnd::Detached), // Daemon/controller stopped; leave the hosted PTY alive.
                }
            }
        }

        if finished {
            content.reserve(decoder.max_utf8_buffer_length(0).unwrap());
            let (result, _, _) = decoder.decode_to_string(&[], &mut content, true);
            debug_assert!(result == CoderResult::InputEmpty);
        }

        while content_offset + content.len() > seq {
            let start = prev_char_boundary(&content, seq.saturating_sub(content_offset));
            let end = prev_char_boundary(&content, (start + CONTENT_CHUNK_SIZE).min(content.len()));
            let data = encrypt.segment(
                0x100000000 | id.0 as u64,
                (content_offset + start) as u64,
                &content.as_bytes()[start..end],
            );
            output_tx
                .send(ClientMessage::Data(TerminalData {
                    id: id.0,
                    data: data.into(),
                    seq: (content_offset + start) as u64,
                }))
                .await?;
            seq = content_offset + end;
            seq_outdated = 0;
        }

        if content.len() > CONTENT_PRUNE_BYTES
            && seq.saturating_sub(CONTENT_ROLLING_BYTES) > content_offset
        {
            let pruned = (seq - CONTENT_ROLLING_BYTES) - content_offset;
            let pruned = prev_char_boundary(&content, pruned);
            content_offset += pruned;
            content.drain(..pruned);
        }
    }
    // Natural process exit is terminal: remove the retained host entry so a
    // future shell reusing this server ID cannot attach to a dead process.
    let request_id = client.close_terminal(terminal_id).await?;
    match receive_host_response(client, request_id).await? {
        HostMessage::Ack(_) => {}
        HostMessage::Error(error) => bail!("terminal host {}: {}", error.code, error.message),
        _ => bail!("terminal host returned an invalid close response"),
    }
    Ok(HostedShellEnd::Exited)
}

async fn attach_or_create_hosted_terminal(
    client: &mut TerminalHostClient,
    terminal_id: &str,
    program: &str,
    args: Vec<String>,
    environment: Option<HashMap<String, String>>,
    options: &ShellOptions,
) -> Result<()> {
    if options.reattach_existing {
        let attach_id = client.attach_terminal(terminal_id, 0).await?;
        match receive_host_response(client, attach_id).await? {
            HostMessage::Ack(_) => return Ok(()),
            HostMessage::Error(error) if error.code == "NOT_FOUND" => {}
            HostMessage::Error(error) => {
                bail!("terminal host {}: {}", error.code, error.message)
            }
            _ => bail!("terminal host returned an invalid attach response"),
        }
    }

    let request = CreateTerminal {
        terminal_id: terminal_id.into(),
        program: program.into(),
        args,
        working_directory: options
            .working_directory
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        environment: environment.unwrap_or_default(),
        rows: options.rows.into(),
        columns: options.cols.into(),
    };
    let create_id = client.create_terminal(request.clone()).await?;
    match receive_host_response(client, create_id).await? {
        HostMessage::Ack(_) => {}
        HostMessage::Error(error)
            if error.code == "ALREADY_EXISTS" && !options.reattach_existing =>
        {
            // A new server-side shell ID is unique in this workspace. A host
            // collision is therefore stale and must not silently discard this
            // request's working directory or SSH profile.
            let close_id = client.close_terminal(terminal_id).await?;
            match receive_host_response(client, close_id).await? {
                HostMessage::Ack(_) => {}
                HostMessage::Error(error) => {
                    bail!("terminal host {}: {}", error.code, error.message)
                }
                _ => bail!("terminal host returned an invalid close response"),
            }
            let create_id = client.create_terminal(request).await?;
            match receive_host_response(client, create_id).await? {
                HostMessage::Ack(_) => {}
                HostMessage::Error(error) => {
                    bail!("terminal host {}: {}", error.code, error.message)
                }
                _ => bail!("terminal host returned an invalid create response"),
            }
        }
        HostMessage::Error(error) if error.code == "ALREADY_EXISTS" => {}
        HostMessage::Error(error) => bail!("terminal host {}: {}", error.code, error.message),
        _ => bail!("terminal host returned an invalid create response"),
    }

    let attach_id = client.attach_terminal(terminal_id, 0).await?;
    match receive_host_response(client, attach_id).await? {
        HostMessage::Ack(_) => Ok(()),
        HostMessage::Error(error) => bail!("terminal host {}: {}", error.code, error.message),
        _ => bail!("terminal host returned an invalid attach response"),
    }
}

async fn receive_host_response(
    client: &mut TerminalHostClient,
    request_id: u64,
) -> Result<HostMessage> {
    loop {
        let frame = client
            .receive()
            .await?
            .context("terminal host disconnected before acknowledging request")?;
        if frame.request_id == request_id {
            return frame
                .message
                .context("terminal host response message is empty");
        }
    }
}

fn history_launch_policy(
    host: &TerminalHostConfig,
    terminal_id: &str,
    program: &str,
    args: &mut Vec<String>,
    initial_directory: Option<&std::path::Path>,
) -> HashMap<String, String> {
    let history_path = host
        .history_directory
        .join(format!("{terminal_id}.history"));
    let shell_name = std::path::Path::new(program)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase();
    let mut environment = HashMap::from([
        (
            "HISTFILE".into(),
            history_path.to_string_lossy().into_owned(),
        ),
        ("fish_history".into(), terminal_id.replace('-', "_")),
    ]);
    if shell_name == "bash" {
        let append_history = "history -a";
        let inherited_prompt = std::env::var("PROMPT_COMMAND")
            .ok()
            .filter(|command| !command.trim().is_empty());
        let mut prompt_parts = Vec::new();
        if let Some(directory) = initial_directory {
            environment.insert(
                INITIAL_DIRECTORY_ENV.into(),
                directory.to_string_lossy().into_owned(),
            );
            prompt_parts.push(BASH_INITIAL_DIRECTORY_COMMAND.to_owned());
        }
        if let Some(command) = inherited_prompt {
            prompt_parts.push(command);
        }
        prompt_parts.push(append_history.into());
        environment.insert("PROMPT_COMMAND".into(), prompt_parts.join("; "));
    } else if shell_name == "fish" {
        if let Some(directory) = initial_directory {
            args.extend([
                "--init-command".into(),
                format!("cd -- {}", shell_quote(&directory.to_string_lossy())),
            ]);
        }
    } else if matches!(shell_name.as_str(), "pwsh" | "powershell") {
        let quoted_path = history_path.to_string_lossy().replace('\'', "''");
        let mut commands = vec![format!(
            "Set-PSReadLineOption -HistorySavePath '{quoted_path}'"
        )];
        if let Some(directory) = initial_directory {
            let directory = directory.to_string_lossy().replace('\'', "''");
            commands.push(format!("Set-Location -LiteralPath '{directory}'"));
        }
        args.extend(["-NoExit".into(), "-Command".into(), commands.join("; ")]);
    }
    environment
}

fn terminal_environment() -> HashMap<String, String> {
    HashMap::from([
        ("TERM".into(), "xterm-256color".into()),
        ("COLORTERM".into(), "truecolor".into()),
        ("TERM_PROGRAM".into(), "sshxx-daemon".into()),
    ])
}

pub(crate) fn ssh_command(profile: &SshProfile) -> Result<(String, Vec<String>)> {
    let host = profile.host.trim();
    if host.is_empty()
        || host.starts_with('-')
        || host
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        bail!("SSH host is invalid");
    }
    let username = profile.username.trim();
    if username.starts_with('-')
        || username
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        bail!("SSH username is invalid");
    }
    let port: u16 = profile
        .port
        .try_into()
        .context("SSH port is out of range")?;
    if port == 0 {
        bail!("SSH port must be positive");
    }

    let auth = SshAuthMethod::try_from(profile.auth_method)
        .map_err(|_| anyhow::anyhow!("SSH authentication method is unsupported"))?;
    let mut args = vec![
        "-p".into(),
        port.to_string(),
        "-o".into(),
        "ServerAliveInterval=30".into(),
        "-o".into(),
        "ServerAliveCountMax=3".into(),
    ];
    if !username.is_empty() {
        args.extend(["-l".into(), username.into()]);
    }
    match auth {
        SshAuthMethod::SshAuthDefault => {}
        SshAuthMethod::SshAuthAgent => {
            args.extend(["-o".into(), "PreferredAuthentications=publickey".into()]);
        }
        SshAuthMethod::SshAuthKeyFile => {
            if profile.key_path.trim().is_empty() {
                bail!("SSH private key path is required");
            }
            args.extend([
                "-i".into(),
                profile.key_path.clone(),
                "-o".into(),
                "IdentitiesOnly=yes".into(),
            ]);
        }
        SshAuthMethod::SshAuthPassword => {
            args.extend([
                "-o".into(),
                "PreferredAuthentications=password,keyboard-interactive".into(),
                "-o".into(),
                "PubkeyAuthentication=no".into(),
            ]);
        }
    }
    if profile.accept_new_host_key {
        args.extend(["-o".into(), "StrictHostKeyChecking=accept-new".into()]);
    }
    args.push(host.into());
    Ok(("ssh".into(), args))
}

/// Find the last char boundary before an index in O(1) time.
fn prev_char_boundary(s: &str, i: usize) -> usize {
    (0..=i)
        .rev()
        .find(|&j| s.is_char_boundary(j))
        .expect("no previous char boundary")
}

async fn echo_task(
    id: Sid,
    encrypt: Encrypt,
    mut shell_rx: mpsc::Receiver<ShellData>,
    output_tx: mpsc::Sender<ClientMessage>,
) -> Result<()> {
    let mut seq = 0;
    while let Some(item) = shell_rx.recv().await {
        match item {
            ShellData::Data(data) => {
                let msg = String::from_utf8_lossy(&data);
                let term_data = TerminalData {
                    id: id.0,
                    data: encrypt
                        .segment(0x100000000 | id.0 as u64, seq, msg.as_bytes())
                        .into(),
                    seq,
                };
                output_tx.send(ClientMessage::Data(term_data)).await?;
                seq += msg.len() as u64;
            }
            ShellData::Sync(_) => (),
            ShellData::Size(_, _) => (),
            ShellData::WorkingDirectory(sender) => {
                sender.send(None).ok();
            }
            ShellData::Close => break,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::time::Duration;

    use sshx_core::proto::{SshAuthMethod, SshProfile};
    #[cfg(unix)]
    use sshx_core::Sid;
    #[cfg(unix)]
    use sshxx_terminal_host::client::Client as TerminalHostClient;
    #[cfg(unix)]
    use sshxx_terminal_host::protocol::frame::Message as HostMessage;
    #[cfg(unix)]
    use tokio::sync::mpsc;

    #[cfg(unix)]
    use super::{attach_or_create_hosted_terminal, Runner, ShellData, ShellOptions};
    use super::{
        history_launch_policy, shell_quote, ssh_command, terminal_environment,
        BASH_INITIAL_DIRECTORY_COMMAND, INITIAL_DIRECTORY_ENV,
    };
    #[cfg(unix)]
    use crate::encrypt::Encrypt;
    use crate::terminal_host::TerminalHostConfig;

    #[test]
    fn quotes_remote_working_directories_for_the_shell() {
        assert_eq!(
            shell_quote("/srv/team's files"),
            "'/srv/team'\"'\"'s files'"
        );
    }

    #[test]
    fn builds_open_ssh_arguments_without_a_shell_command() -> anyhow::Result<()> {
        let profile = SshProfile {
            id: "prod".into(),
            name: "Production".into(),
            host: "server.example.test".into(),
            port: 2222,
            username: "deploy".into(),
            auth_method: SshAuthMethod::SshAuthKeyFile.into(),
            key_path: "/home/deploy/.ssh/id key".into(),
            accept_new_host_key: true,
            theme: String::new(),
            background_enabled: false,
            background: String::new(),
        };
        let (program, args) = ssh_command(&profile)?;
        assert_eq!(program, "ssh");
        assert_eq!(args.last().map(String::as_str), Some("server.example.test"));
        assert!(args.windows(2).any(|pair| pair == ["-p", "2222"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-i", "/home/deploy/.ssh/id key"]));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn builds_remote_working_directory_command_after_the_destination() -> anyhow::Result<()> {
        let profile = SshProfile {
            id: "prod".into(),
            name: "Production".into(),
            host: "server.example.test".into(),
            port: 22,
            username: "deploy".into(),
            auth_method: SshAuthMethod::SshAuthAgent.into(),
            key_path: String::new(),
            accept_new_host_key: true,
            theme: String::new(),
            background_enabled: false,
            background: String::new(),
        };
        let mut options = test_shell_options();
        options.ssh_profile = Some(profile);
        options.working_directory = Some(std::path::PathBuf::from("/srv/team's files"));

        let (program, args) = super::launch_command("/bin/bash", &mut options)?;

        assert_eq!(program, "ssh");
        assert_eq!(args[args.len() - 2], "server.example.test");
        let remote_command = args.last().map(String::as_str).unwrap();
        assert!(remote_command
            .contains("cd -- '/srv/team'\"'\"'s files' && exec \"${SHELL:-/bin/sh}\" -l"));
        assert!(remote_command.contains("SSHXX_INITIAL_DIRECTORY"));
        assert!(remote_command.contains("PROMPT_COMMAND"));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-t", "server.example.test"]));
        assert!(options.working_directory.is_none());
        Ok(())
    }

    #[test]
    fn rejects_a_host_that_can_be_parsed_as_an_option() {
        let profile = SshProfile {
            host: "-oProxyCommand=bad".into(),
            port: 22,
            ..Default::default()
        };
        assert!(ssh_command(&profile).is_err());
    }

    #[test]
    fn isolates_posix_fish_and_powershell_history() {
        let host = TerminalHostConfig {
            endpoint: String::new(),
            authentication_token: Vec::new(),
            instance_id: "sshxx-history-test".into(),
            history_directory: std::path::PathBuf::from("cache/history"),
        };
        let mut args = Vec::new();
        let environment = history_launch_policy(
            &host,
            "sshxx-history-test-9",
            "powershell.exe",
            &mut args,
            Some(std::path::Path::new("C:\\work dir")),
        );
        assert!(environment
            .get("HISTFILE")
            .unwrap()
            .ends_with("sshxx-history-test-9.history"));
        assert_eq!(
            environment.get("fish_history").map(String::as_str),
            Some("sshxx_history_test_9")
        );
        assert!(args
            .last()
            .unwrap()
            .contains("sshxx-history-test-9.history"));
        assert!(args
            .last()
            .unwrap()
            .contains("Set-Location -LiteralPath 'C:\\work dir'"));

        let mut fish_args = Vec::new();
        history_launch_policy(
            &host,
            "sshxx-history-test-fish",
            "fish",
            &mut fish_args,
            Some(std::path::Path::new("/srv/team's files")),
        );
        assert_eq!(
            fish_args.first().map(String::as_str),
            Some("--init-command")
        );
        assert!(fish_args
            .last()
            .unwrap()
            .contains("/srv/team'\"'\"'s files"));

        let mut bash_args = Vec::new();
        let bash_environment = history_launch_policy(
            &host,
            "sshxx-history-test-10",
            "bash",
            &mut bash_args,
            Some(std::path::Path::new("/srv/work")),
        );
        assert!(bash_environment
            .get("PROMPT_COMMAND")
            .unwrap()
            .contains("history -a"));
        assert!(bash_environment
            .get("PROMPT_COMMAND")
            .unwrap()
            .contains(BASH_INITIAL_DIRECTORY_COMMAND));
        assert_eq!(
            bash_environment
                .get(INITIAL_DIRECTORY_ENV)
                .map(String::as_str),
            Some("/srv/work")
        );
    }

    #[test]
    fn hosted_terminals_declare_xterm_capabilities() {
        let environment = terminal_environment();
        assert_eq!(
            environment.get("TERM").map(String::as_str),
            Some("xterm-256color")
        );
        assert_eq!(
            environment.get("COLORTERM").map(String::as_str),
            Some("truecolor")
        );
        assert_eq!(
            environment.get("TERM_PROGRAM").map(String::as_str),
            Some("sshxx-daemon")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn hosted_shell_survives_runner_restart_until_explicit_close() -> anyhow::Result<()> {
        let state = tempfile::tempdir()?;
        let endpoint = state
            .path()
            .join("host.sock")
            .to_string_lossy()
            .into_owned();
        let token = vec![23; 32];
        let server_endpoint = endpoint.clone();
        let server_token = token.clone();
        let server = tokio::spawn(async move {
            sshxx_terminal_host::server::serve(&server_endpoint, server_token).await
        });
        wait_for_host(&endpoint, &token).await?;

        let history_directory = state.path().join("history");
        std::fs::create_dir(&history_directory)?;
        let host = TerminalHostConfig {
            endpoint: endpoint.clone(),
            authentication_token: token.clone(),
            instance_id: "sshxx-runner-integration".into(),
            history_directory,
        };
        let runner = Runner::HostedShell {
            shell: "/bin/bash".into(),
            host: host.clone(),
        };

        let (first_tx, first_rx) = mpsc::channel(4);
        let (first_output_tx, _first_output_rx) = mpsc::channel(16);
        let first_runner = runner.clone();
        let first_task = tokio::spawn(async move {
            first_runner
                .run(
                    Sid(7),
                    Encrypt::new("hosted-runner-test"),
                    first_rx,
                    first_output_tx,
                    test_shell_options(),
                )
                .await
        });
        let first_pid = wait_for_terminal_pid(&endpoint, &token).await?;

        drop(first_tx);
        tokio::time::timeout(Duration::from_secs(3), first_task).await???;
        let detached_pid = wait_for_terminal_pid(&endpoint, &token).await?;
        assert_eq!(first_pid, detached_pid);

        let (second_tx, second_rx) = mpsc::channel(4);
        let (second_output_tx, mut second_output_rx) = mpsc::channel(16);
        let second_task = tokio::spawn(async move {
            runner
                .run(
                    Sid(7),
                    Encrypt::new("hosted-runner-test"),
                    second_rx,
                    second_output_tx,
                    test_shell_options(),
                )
                .await
        });
        second_tx
            .send(ShellData::Data(
                b"printf 'RUNNER_REATTACHED TERM=%s COLORTERM=%s\\n' \"$TERM\" \"$COLORTERM\"\r"
                    .to_vec(),
            ))
            .await?;
        let output = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(sshx_core::proto::client_update::ClientMessage::Data(data)) =
                    second_output_rx.recv().await
                {
                    let plaintext = Encrypt::new("hosted-runner-test").segment(
                        0x100000000 | 7,
                        data.seq,
                        &data.data,
                    );
                    if String::from_utf8_lossy(&plaintext)
                        .contains("RUNNER_REATTACHED TERM=xterm-256color COLORTERM=truecolor")
                    {
                        break;
                    }
                }
            }
        })
        .await;
        assert!(
            output.is_ok(),
            "reattached runner did not receive PTY output"
        );
        assert_eq!(wait_for_terminal_pid(&endpoint, &token).await?, first_pid);
        let history_path = host.history_path(7);
        let saved = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if tokio::fs::read_to_string(&history_path)
                    .await
                    .is_ok_and(|history| history.contains("RUNNER_REATTACHED"))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(saved.is_ok(), "interactive Bash history was not persisted");

        second_tx.send(ShellData::Close).await?;
        tokio::time::timeout(Duration::from_secs(3), second_task).await???;
        wait_for_no_terminals(&endpoint, &token).await?;
        assert!(
            !history_path.exists(),
            "closed terminal history was retained"
        );

        let mut client =
            TerminalHostClient::connect(&endpoint, token, env!("CARGO_PKG_VERSION")).await?;
        let shutdown = client.shutdown(false).await?;
        wait_for_ack(&mut client, shutdown).await?;
        tokio::time::timeout(Duration::from_secs(3), server).await???;
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ssh_preset_restarts_after_terminal_host_state_loss() -> anyhow::Result<()> {
        let state = tempfile::tempdir()?;
        let endpoint = state
            .path()
            .join("host.sock")
            .to_string_lossy()
            .into_owned();
        let token = vec![31; 32];
        let first_endpoint = endpoint.clone();
        let first_token = token.clone();
        let first_server = tokio::spawn(async move {
            sshxx_terminal_host::server::serve(&first_endpoint, first_token).await
        });
        wait_for_host(&endpoint, &token).await?;

        let history_directory = state.path().join("history");
        std::fs::create_dir(&history_directory)?;
        let runner = Runner::HostedShell {
            shell: "/bin/bash".into(),
            host: TerminalHostConfig {
                endpoint: endpoint.clone(),
                authentication_token: token.clone(),
                instance_id: "sshxx-preset-recovery".into(),
                history_directory,
            },
        };
        let (shell_tx, shell_rx) = mpsc::channel(8);
        let (output_tx, mut output_rx) = mpsc::channel(32);
        let mut options = test_shell_options();
        // The durable profile identity is what distinguishes an SSH preset
        // from a default local terminal during host-loss recovery.
        options.ssh_profile_id = "saved-ssh-profile".into();
        let task = tokio::spawn(async move {
            runner
                .run(
                    Sid(11),
                    Encrypt::new("host-recovery-test"),
                    shell_rx,
                    output_tx,
                    options,
                )
                .await
        });
        let first_pid = wait_for_terminal_pid(&endpoint, &token).await?;

        let mut admin =
            TerminalHostClient::connect(&endpoint, token.clone(), env!("CARGO_PKG_VERSION"))
                .await?;
        let shutdown = admin.shutdown(true).await?;
        wait_for_ack(&mut admin, shutdown).await?;
        tokio::time::timeout(Duration::from_secs(3), first_server).await???;

        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if matches!(
                    output_rx.recv().await,
                    Some(sshx_core::proto::client_update::ClientMessage::RestartedShell(11))
                ) {
                    break;
                }
            }
        })
        .await?;

        let second_endpoint = endpoint.clone();
        let second_token = token.clone();
        let second_server = tokio::spawn(async move {
            sshxx_terminal_host::server::serve(&second_endpoint, second_token).await
        });
        wait_for_host(&endpoint, &token).await?;
        let second_pid = wait_for_terminal_pid(&endpoint, &token).await?;
        assert_ne!(first_pid, second_pid);

        shell_tx
            .send(ShellData::Data(
                b"printf 'SSH_PRESET_RECOVERED\\n'\r".to_vec(),
            ))
            .await?;
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(sshx_core::proto::client_update::ClientMessage::Data(data)) =
                    output_rx.recv().await
                {
                    let plaintext = Encrypt::new("host-recovery-test").segment(
                        0x100000000 | 11,
                        data.seq,
                        &data.data,
                    );
                    if String::from_utf8_lossy(&plaintext).contains("SSH_PRESET_RECOVERED") {
                        break;
                    }
                }
            }
        })
        .await?;

        shell_tx.send(ShellData::Close).await?;
        tokio::time::timeout(Duration::from_secs(3), task).await???;
        let mut admin =
            TerminalHostClient::connect(&endpoint, token, env!("CARGO_PKG_VERSION")).await?;
        let shutdown = admin.shutdown(false).await?;
        wait_for_ack(&mut admin, shutdown).await?;
        tokio::time::timeout(Duration::from_secs(3), second_server).await???;
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn explicit_hosted_shell_replaces_a_stale_id_and_honors_cwd() -> anyhow::Result<()> {
        let state = tempfile::tempdir()?;
        let endpoint = state
            .path()
            .join("host.sock")
            .to_string_lossy()
            .into_owned();
        let token = vec![29; 32];
        let server_endpoint = endpoint.clone();
        let server_token = token.clone();
        let server = tokio::spawn(async move {
            sshxx_terminal_host::server::serve(&server_endpoint, server_token).await
        });
        wait_for_host(&endpoint, &token).await?;

        let first_directory = state.path().join("first");
        let requested_directory = state.path().join("requested");
        std::fs::create_dir(&first_directory)?;
        std::fs::create_dir(&requested_directory)?;
        let mut client =
            TerminalHostClient::connect(&endpoint, token.clone(), env!("CARGO_PKG_VERSION"))
                .await?;
        let mut first = test_shell_options();
        first.working_directory = Some(first_directory);
        first.reattach_existing = false;
        attach_or_create_hosted_terminal(
            &mut client,
            "open-terminal-here",
            "/bin/bash",
            Vec::new(),
            None,
            &first,
        )
        .await?;
        let first_pid = wait_for_terminal_pid(&endpoint, &token).await?;

        let mut replacement = test_shell_options();
        replacement.working_directory = Some(requested_directory.clone());
        replacement.reattach_existing = false;
        attach_or_create_hosted_terminal(
            &mut client,
            "open-terminal-here",
            "/bin/bash",
            Vec::new(),
            None,
            &replacement,
        )
        .await?;
        let replacement_pid = wait_for_terminal_pid(&endpoint, &token).await?;
        assert_ne!(replacement_pid, first_pid);
        assert_eq!(
            std::fs::read_link(format!("/proc/{replacement_pid}/cwd"))?,
            requested_directory
        );

        let close_id = client.close_terminal("open-terminal-here").await?;
        wait_for_ack(&mut client, close_id).await?;
        wait_for_no_terminals(&endpoint, &token).await?;
        let shutdown = client.shutdown(false).await?;
        wait_for_ack(&mut client, shutdown).await?;
        tokio::time::timeout(Duration::from_secs(3), server).await???;
        Ok(())
    }

    #[cfg(unix)]
    fn test_shell_options() -> ShellOptions {
        ShellOptions {
            working_directory: None,
            ssh_profile: None,
            ssh_profile_id: String::new(),
            reattach_existing: true,
            rows: 24,
            cols: 80,
            theme: String::new(),
            background: String::new(),
            width: 0,
            height: 0,
        }
    }

    #[cfg(unix)]
    async fn wait_for_host(endpoint: &str, token: &[u8]) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if TerminalHostClient::connect(endpoint, token.to_vec(), env!("CARGO_PKG_VERSION"))
                    .await
                    .is_ok()
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        Ok(())
    }

    #[cfg(unix)]
    async fn wait_for_terminal_pid(endpoint: &str, token: &[u8]) -> anyhow::Result<u32> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let mut client = TerminalHostClient::connect(
                    endpoint,
                    token.to_vec(),
                    env!("CARGO_PKG_VERSION"),
                )
                .await?;
                let request_id = client.list_terminals().await?;
                loop {
                    let frame = client.receive().await?.unwrap();
                    if frame.request_id != request_id {
                        continue;
                    }
                    if let Some(HostMessage::TerminalList(list)) = frame.message {
                        if let Some(terminal) = list.terminals.first() {
                            return Ok(terminal.process_id);
                        }
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?
    }

    #[cfg(unix)]
    async fn wait_for_no_terminals(endpoint: &str, token: &[u8]) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let mut client = TerminalHostClient::connect(
                    endpoint,
                    token.to_vec(),
                    env!("CARGO_PKG_VERSION"),
                )
                .await?;
                let request_id = client.list_terminals().await?;
                loop {
                    let frame = client.receive().await?.unwrap();
                    if frame.request_id != request_id {
                        continue;
                    }
                    if let Some(HostMessage::TerminalList(list)) = frame.message {
                        if list.terminals.is_empty() {
                            return Ok::<(), anyhow::Error>(());
                        }
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await??;
        Ok(())
    }

    #[cfg(unix)]
    async fn wait_for_ack(client: &mut TerminalHostClient, request_id: u64) -> anyhow::Result<()> {
        loop {
            let frame = client.receive().await?.unwrap();
            if frame.request_id == request_id {
                assert!(matches!(frame.message, Some(HostMessage::Ack(_))));
                return Ok(());
            }
        }
    }
}
