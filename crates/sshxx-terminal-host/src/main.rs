use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use sshxx_terminal_host::client::Client;
use sshxx_terminal_host::endpoint_for_state_directory;
use sshxx_terminal_host::protocol::frame::Message;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    #[command(subcommand)]
    command: HostCommand,
}

#[derive(Debug, Subcommand)]
enum HostCommand {
    /// Start the host as an independent background process.
    Start {
        #[arg(long, default_value = "cache/terminal-host")]
        state_dir: PathBuf,
    },
    /// Run the host in the foreground (for OS service managers).
    Serve {
        #[arg(long, default_value = "cache/terminal-host")]
        state_dir: PathBuf,
    },
    /// List hosted terminals without exposing terminal contents.
    Status {
        #[arg(long, default_value = "cache/terminal-host")]
        state_dir: PathBuf,
    },
    /// Stop the host. Active terminals make this destructive.
    Stop {
        #[arg(long, default_value = "cache/terminal-host")]
        state_dir: PathBuf,
        /// Confirm that all active terminal processes may be disconnected.
        #[arg(long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    match Args::parse().command {
        HostCommand::Start { state_dir } => start(&state_dir).await,
        HostCommand::Serve { state_dir } => serve(&state_dir).await,
        HostCommand::Status { state_dir } => status(&state_dir).await,
        HostCommand::Stop { state_dir, force } => stop(&state_dir, force).await,
    }
}

async fn start(state_dir: &Path) -> Result<()> {
    let token = ensure_token(state_dir)?;
    let endpoint = endpoint_for_state_directory(state_dir);
    if let Ok(client) = Client::connect(&endpoint, token.clone(), env!("CARGO_PKG_VERSION")).await {
        if client.host_version() == env!("CARGO_PKG_VERSION") {
            println!("sshxx-terminal-host is already running");
            return Ok(());
        }
        bail!(
            "sshxx-terminal-host {} is still running while executable {} is installed. It was not restarted because a host restart disconnects every hosted terminal process. Inspect `status`, recover or close those processes, then explicitly run `stop` (or `stop --force`) before starting the new host.",
            client.host_version(),
            env!("CARGO_PKG_VERSION"),
        );
    }

    let executable =
        std::env::current_exe().context("failed to locate terminal-host executable")?;
    let mut command = Command::new(executable);
    command
        .arg("serve")
        .arg("--state-dir")
        .arg(state_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    configure_detached_process(&mut command);
    command.spawn().context("failed to launch terminal host")?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        match Client::connect(&endpoint, token.clone(), env!("CARGO_PKG_VERSION")).await {
            Ok(_) => {
                println!("sshxx-terminal-host started");
                return Ok(());
            }
            Err(error) if tokio::time::Instant::now() < deadline => {
                tracing::debug!(?error, "waiting for terminal host");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(error) => return Err(error).context("terminal host did not become ready"),
        }
    }
}

async fn serve(state_dir: &Path) -> Result<()> {
    let token = ensure_token(state_dir)?;
    sshxx_terminal_host::server::serve(&endpoint_for_state_directory(state_dir), token).await
}

async fn status(state_dir: &Path) -> Result<()> {
    let token = read_token(state_dir)?;
    let mut client = Client::connect(
        &endpoint_for_state_directory(state_dir),
        token,
        env!("CARGO_PKG_VERSION"),
    )
    .await?;
    println!(
        "sshxx-terminal-host {} (restart disruptive: {})",
        client.host_version(),
        client.host_restart_is_disruptive(),
    );
    let request_id = client.list_terminals().await?;
    loop {
        let response = client
            .receive()
            .await?
            .context("terminal host disconnected before returning status")?;
        if response.request_id != request_id {
            continue;
        }
        match response.message {
            Some(Message::TerminalList(list)) => {
                if list.terminals.is_empty() {
                    println!("sshxx-terminal-host is running with no terminals");
                } else {
                    println!("ID\tPID\tSTATE\tSIZE\tRETAINED..NEXT");
                    for terminal in list.terminals {
                        println!(
                            "{}\t{}\t{}\t{}x{}\t{}..{}",
                            terminal.terminal_id,
                            terminal.process_id,
                            if terminal.running {
                                "running"
                            } else {
                                "exited"
                            },
                            terminal.columns,
                            terminal.rows,
                            terminal.retained_sequence,
                            terminal.next_sequence,
                        );
                    }
                }
                return Ok(());
            }
            Some(Message::Error(error)) => bail!("{}: {}", error.code, error.message),
            _ => bail!("terminal host returned an invalid status response"),
        }
    }
}

async fn stop(state_dir: &Path, force: bool) -> Result<()> {
    let token = read_token(state_dir)?;
    let mut client = Client::connect(
        &endpoint_for_state_directory(state_dir),
        token,
        env!("CARGO_PKG_VERSION"),
    )
    .await?;
    let request_id = client.shutdown(force).await?;
    loop {
        let response = client
            .receive()
            .await?
            .context("terminal host disconnected before acknowledging shutdown")?;
        if response.request_id != request_id {
            continue;
        }
        match response.message {
            Some(Message::Ack(_)) => {
                println!("sshxx-terminal-host stopped");
                return Ok(());
            }
            Some(Message::Error(error)) if error.code == "ACTIVE_TERMINALS" => {
                bail!(
                    "{}\nThe host was not stopped. Recover or close the listed processes first; use --force only when their loss is acceptable.",
                    error.message
                );
            }
            Some(Message::Error(error)) => bail!("{}: {}", error.code, error.message),
            _ => bail!("terminal host returned an invalid shutdown response"),
        }
    }
}

fn ensure_token(state_dir: &Path) -> Result<Vec<u8>> {
    std::fs::create_dir_all(state_dir).context("failed to create terminal-host state directory")?;
    set_private_directory_permissions(state_dir)?;
    let path = state_dir.join("host.token");
    let token = rand::random::<[u8; 32]>();
    match private_token_file(&path) {
        Ok(mut file) => {
            file.write_all(&token)
                .context("failed to write terminal-host authentication token")?;
            file.sync_all()
                .context("failed to persist terminal-host authentication token")?;
            Ok(token.to_vec())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read_token(state_dir),
        Err(error) => Err(error).context("failed to create terminal-host authentication token"),
    }
}

fn read_token(state_dir: &Path) -> Result<Vec<u8>> {
    let token = std::fs::read(state_dir.join("host.token"))
        .context("failed to read terminal-host authentication token")?;
    if token.len() < 32 {
        bail!("terminal-host authentication token is invalid");
    }
    Ok(token)
}

fn private_token_file(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(windows)]
fn set_private_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn configure_detached_process(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(windows)]
fn configure_detached_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}
