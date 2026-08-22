use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitCode};

use anstyle::{Ansi256Color, AnsiColor};
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use sshx_daemon::{
    controller::{is_upstream_sshx_origin, Controller},
    runner::Runner,
    terminal::get_default_shell,
    terminal_host::{TerminalHostConfig, DEFAULT_STATE_DIRECTORY},
};
use sshxx_terminal_host::client::Client as TerminalHostClient;
use tokio::signal;
use tracing::{error, warn};

#[cfg(unix)]
use tokio::signal::unix::{signal as unix_signal, SignalKind};

/// A self-hosted, persistent, collaborative terminal daemon.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<DaemonCommand>,

    /// Address of the remote sshxx server.
    #[clap(long, default_value = "http://localhost:8051", env = "SSHXX_SERVER")]
    server: String,

    /// Explicitly allow use of the upstream sshx public service. This service
    /// is not selected by default and compatibility is not supported by sshxx.
    #[clap(long, env = "SSHXX_ALLOW_UPSTREAM_SERVICE")]
    allow_upstream_service: bool,

    /// Local shell command to run in the terminal.
    #[clap(long)]
    shell: Option<String>,

    /// Quiet mode, only prints the URL to stdout.
    #[clap(short, long)]
    quiet: bool,

    /// Session name displayed in the title (defaults to user@hostname).
    #[clap(long)]
    name: Option<String>,

    /// Enable read-only access mode - generates separate URLs for viewers and
    /// editors.
    #[clap(long)]
    enable_readers: bool,

    /// Fixed encryption key for local testing (unsafe for production).
    #[clap(long, env = "SSHXX_TEST_ENCRYPTION_KEY")]
    test_encryption_key: Option<String>,
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// Manage the independent process that owns persistent terminals.
    TerminalHost {
        #[command(subcommand)]
        command: TerminalHostCommand,
    },
}

#[derive(Debug, Subcommand)]
enum TerminalHostCommand {
    /// Start the terminal host in the background.
    Start {
        #[arg(long, default_value = "cache/terminal-host")]
        state_dir: PathBuf,
    },
    /// Show the terminal host version and active terminals.
    Status {
        #[arg(long, default_value = "cache/terminal-host")]
        state_dir: PathBuf,
    },
    /// Stop the terminal host, refusing if terminals are active by default.
    Stop {
        #[arg(long, default_value = "cache/terminal-host")]
        state_dir: PathBuf,
        /// Disconnect every hosted terminal process.
        #[arg(long)]
        force: bool,
    },
    /// Stop and start the host; active terminals require explicit --force.
    Restart {
        #[arg(long, default_value = "cache/terminal-host")]
        state_dir: PathBuf,
        /// Disconnect every hosted terminal process.
        #[arg(long)]
        force: bool,
    },
}

impl TerminalHostCommand {
    fn run(&self) -> Result<()> {
        match self {
            Self::Start { state_dir } => run_terminal_host(&["start"], state_dir),
            Self::Status { state_dir } => run_terminal_host(&["status"], state_dir),
            Self::Stop { state_dir, force } => {
                warn_if_destructive(*force);
                run_terminal_host(stop_arguments(*force), state_dir)
            }
            Self::Restart { state_dir, force } => {
                warn_if_destructive(*force);
                run_terminal_host(stop_arguments(*force), state_dir)?;
                run_terminal_host(&["start"], state_dir)
            }
        }
    }
}

fn stop_arguments(force: bool) -> &'static [&'static str] {
    if force {
        &["stop", "--force"]
    } else {
        &["stop"]
    }
}

fn warn_if_destructive(force: bool) {
    if force {
        eprintln!(
            "WARNING: --force disconnects every process owned by sshxx-terminal-host; shell and application state may be lost."
        );
    }
}

fn run_terminal_host(arguments: &[&str], state_dir: &Path) -> Result<()> {
    let executable = terminal_host_executable()?;
    let status = ProcessCommand::new(&executable)
        .args(arguments)
        .arg("--state-dir")
        .arg(state_dir)
        .status()
        .with_context(|| {
            format!(
                "failed to run terminal host executable {}",
                executable.display()
            )
        })?;
    if status.success() {
        Ok(())
    } else {
        bail!("sshxx-terminal-host exited with {status}")
    }
}

fn run_terminal_host_silently(arguments: &[&str], state_dir: &Path) -> Result<()> {
    let executable = terminal_host_executable()?;
    let output = ProcessCommand::new(&executable)
        .args(arguments)
        .arg("--state-dir")
        .arg(state_dir)
        .output()
        .with_context(|| {
            format!(
                "failed to run terminal host executable {}",
                executable.display()
            )
        })?;
    if output.status.success() {
        return Ok(());
    }

    let message = String::from_utf8_lossy(&output.stderr);
    let separator = if message.trim().is_empty() { "" } else { ": " };
    bail!(
        "sshxx-terminal-host exited with {}{separator}{}",
        output.status,
        message.trim()
    )
}

fn terminal_host_executable() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("SSHXX_TERMINAL_HOST_BIN") {
        return Ok(path.into());
    }

    let current = std::env::current_exe().context("failed to locate sshxx-daemon executable")?;
    let sibling = current.with_file_name(format!(
        "sshxx-terminal-host{}",
        std::env::consts::EXE_SUFFIX
    ));
    if sibling.is_file() {
        return Ok(sibling);
    }

    Ok(PathBuf::from(format!(
        "sshxx-terminal-host{}",
        std::env::consts::EXE_SUFFIX
    )))
}

fn print_greeting(shell: &str, controller: &Controller) {
    let version_str = match option_env!("CARGO_PKG_VERSION") {
        Some(version) => format!("v{version}"),
        None => String::from("[dev]"),
    };
    let green = AnsiColor::Green.on_default();
    let green_bold = green.bold();
    let cyan_underlined = AnsiColor::Cyan.on_default().underline();
    let dim = Ansi256Color(8).on_default();
    if let Some(write_url) = controller.write_url() {
        println!(
            r#"
  {green_bold}sshxx-daemon{green_bold:#} {green}{version_str}{green:#}

  {green}➜{green:#}  Read-only link: {cyan_underlined}{}{cyan_underlined:#}
  {green}➜{green:#}  Writable link:  {cyan_underlined}{write_url}{cyan_underlined:#}
  {green}➜{green:#}  Shell:          {dim}{shell}{dim:#}
"#,
            controller.url(),
        );
    } else {
        println!(
            r#"
  {green_bold}sshxx-daemon{green_bold:#} {green}{version_str}{green:#}

  {green}➜{green:#}  Link:  {cyan_underlined}{}{cyan_underlined:#}
  {green}➜{green:#}  Shell: {dim}{shell}{dim:#}
"#,
            controller.url(),
        );
    }
}

fn validate_server_selection(server: &str, allow_upstream_service: bool) -> Result<bool> {
    let uses_upstream_service = is_upstream_sshx_origin(server);
    anyhow::ensure!(
        !uses_upstream_service || allow_upstream_service,
        "connecting to the upstream sshx public service requires explicit consent; add --allow-upstream-service after reviewing the compatibility and support notice"
    );
    Ok(uses_upstream_service)
}

#[tokio::main]
async fn start(args: Args) -> Result<()> {
    let uses_upstream_service =
        validate_server_selection(&args.server, args.allow_upstream_service)?;
    if uses_upstream_service {
        warn!(
            server = %args.server,
            "using the upstream sshx public service by explicit request; sshxx does not guarantee compatibility or provide support for this connection"
        );
    }

    let shell = match args.shell {
        Some(shell) => shell,
        None => get_default_shell().await,
    };
    let terminal_host = prepare_terminal_host().await?;

    let name = args.name.unwrap_or_else(|| {
        let mut name = whoami::username().unwrap_or_else(|_| "unknown".to_owned());
        if let Ok(host) = whoami::hostname() {
            // Trim domain information like .lan or .local
            let host = host.split('.').next().unwrap_or(&host);
            name += "@";
            name += host;
        }
        name
    });

    let runner = Runner::HostedShell {
        shell: shell.clone(),
        host: terminal_host,
    };
    let mut controller = Controller::new_persistent_with_encryption_key(
        &args.server,
        &name,
        runner,
        args.enable_readers,
        args.test_encryption_key.as_deref(),
    )
    .await?;
    if args.quiet {
        if let Some(write_url) = controller.write_url() {
            println!("{}", write_url);
        } else {
            println!("{}", controller.url());
        }
    } else {
        print_greeting(&shell, &controller);
    }

    let exit_signal = wait_for_shutdown();
    tokio::pin!(exit_signal);
    tokio::select! {
        _ = controller.run() => unreachable!(),
        Ok(()) = &mut exit_signal => (),
    };
    controller.close().await?;

    Ok(())
}

#[cfg(unix)]
async fn wait_for_shutdown() -> Result<()> {
    let mut sigterm = unix_signal(SignalKind::terminate())?;
    tokio::select! {
        result = signal::ctrl_c() => result?,
        _ = sigterm.recv() => (),
    }
    Ok(())
}

#[cfg(not(unix))]
async fn wait_for_shutdown() -> Result<()> {
    signal::ctrl_c().await?;
    Ok(())
}

async fn prepare_terminal_host() -> Result<TerminalHostConfig> {
    let state_directory = PathBuf::from(DEFAULT_STATE_DIRECTORY);
    if let Ok(config) = TerminalHostConfig::load(&state_directory) {
        if TerminalHostClient::connect(
            &config.endpoint,
            config.authentication_token.clone(),
            env!("CARGO_PKG_VERSION"),
        )
        .await
        .is_ok()
        {
            return Ok(config);
        }
    }

    if std::env::var_os("INVOCATION_ID").is_some() {
        bail!(
            "sshxx-terminal-host is not running. Under systemd it must use a separate service so restarting sshxx-daemon does not kill hosted terminals; start that service before sshxx-daemon"
        );
    }

    run_terminal_host_silently(&["start"], &state_directory)?;
    let config = TerminalHostConfig::load(&state_directory)?;
    TerminalHostClient::connect(
        &config.endpoint,
        config.authentication_token.clone(),
        env!("CARGO_PKG_VERSION"),
    )
    .await
    .context("terminal host did not become available after startup")?;
    Ok(config)
}

fn main() -> ExitCode {
    let args = Args::parse();

    let default_level = if args.quiet { "error" } else { "info" };

    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or(default_level.into()))
        .with_writer(std::io::stderr)
        .init();

    let result = match args.command.as_ref() {
        Some(DaemonCommand::TerminalHost { command }) => command.run(),
        None => start(args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            error!("{err:?}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{stop_arguments, validate_server_selection};

    #[test]
    fn upstream_service_requires_explicit_consent() {
        assert!(validate_server_selection("https://sshx.io", false).is_err());
        assert!(validate_server_selection("https://sshx.io", true).unwrap());
        assert!(!validate_server_selection("http://localhost:8051", false).unwrap());
    }

    #[test]
    fn terminal_host_stop_is_non_destructive_by_default() {
        assert_eq!(stop_arguments(false), ["stop"]);
        assert_eq!(stop_arguments(true), ["stop", "--force"]);
    }
}
