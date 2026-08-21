//! Defines tasks that control the behavior of a single shell in the client.

use anyhow::{bail, Context, Result};
use encoding_rs::{CoderResult, UTF_8};
use sshx_core::proto::{client_update::ClientMessage, SshAuthMethod, SshProfile, TerminalData};
use sshx_core::Sid;
use std::path::PathBuf;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, oneshot},
};

use crate::encrypt::Encrypt;
use crate::terminal::Terminal;

const CONTENT_CHUNK_SIZE: usize = 1 << 16; // Send at most this many bytes at a time.
const CONTENT_ROLLING_BYTES: usize = 8 << 20; // Store at least this much content.
const CONTENT_PRUNE_BYTES: usize = 12 << 20; // Prune when we exceed this length.

/// Variants of terminal behavior that are used by the controller.
#[derive(Debug, Clone)]
pub enum Runner {
    /// Spawns the specified shell as a subprocess, forwarding PTYs.
    Shell(String),

    /// Mock runner that only echos its input, useful for testing.
    Echo,
}

#[derive(Debug)]
pub(crate) struct ShellOptions {
    pub working_directory: Option<PathBuf>,
    pub ssh_profile: Option<SshProfile>,
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
                let (program, args) = match &options.ssh_profile {
                    Some(profile) => {
                        let (program, mut args) = ssh_command(profile)?;
                        if let Some(directory) = options.working_directory.take() {
                            let host_index = args.len().saturating_sub(1);
                            args.insert(host_index, "-t".into());
                            args.push(format!(
                                "cd -- {} && exec \"${{SHELL:-/bin/sh}}\" -l",
                                shell_quote(&directory.to_string_lossy())
                            ));
                        }
                        (program, args)
                    }
                    None => (shell.clone(), Vec::new()),
                };
                shell_task(id, encrypt, &program, &args, shell_rx, output_tx, options).await
            }
            Self::Echo => echo_task(id, encrypt, shell_rx, output_tx).await,
        }
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
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sshx_core::proto::{SshAuthMethod, SshProfile};

    use super::{shell_quote, ssh_command};

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

    #[test]
    fn rejects_a_host_that_can_be_parsed_as_an_option() {
        let profile = SshProfile {
            host: "-oProxyCommand=bad".into(),
            port: 22,
            ..Default::default()
        };
        assert!(ssh_command(&profile).is_err());
    }
}
