use std::process::ExitCode;

use ansi_term::Color::{Cyan, Fixed, Green};
use anyhow::Result;
use clap::Parser;
use sshx_daemon::{
    controller::{is_upstream_sshx_origin, Controller},
    runner::Runner,
    terminal::get_default_shell,
};
use tokio::signal;
use tracing::{error, warn};

/// A self-hosted, persistent, collaborative terminal daemon.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
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

fn print_greeting(shell: &str, controller: &Controller) {
    let version_str = match option_env!("CARGO_PKG_VERSION") {
        Some(version) => format!("v{version}"),
        None => String::from("[dev]"),
    };
    if let Some(write_url) = controller.write_url() {
        println!(
            r#"
  {sshx} {version}

  {arr}  Read-only link: {link_v}
  {arr}  Writable link:  {link_e}
  {arr}  Shell:          {shell_v}
"#,
            sshx = Green.bold().paint("sshxx-daemon"),
            version = Green.paint(&version_str),
            arr = Green.paint("➜"),
            link_v = Cyan.underline().paint(controller.url()),
            link_e = Cyan.underline().paint(write_url),
            shell_v = Fixed(8).paint(shell),
        );
    } else {
        println!(
            r#"
  {sshx} {version}

  {arr}  Link:  {link_v}
  {arr}  Shell: {shell_v}
"#,
            sshx = Green.bold().paint("sshxx-daemon"),
            version = Green.paint(&version_str),
            arr = Green.paint("➜"),
            link_v = Cyan.underline().paint(controller.url()),
            shell_v = Fixed(8).paint(shell),
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

    let runner = Runner::Shell(shell.clone());
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

    let exit_signal = signal::ctrl_c();
    tokio::pin!(exit_signal);
    tokio::select! {
        _ = controller.run() => unreachable!(),
        Ok(()) = &mut exit_signal => (),
    };
    controller.close().await?;

    Ok(())
}

fn main() -> ExitCode {
    let args = Args::parse();

    let default_level = if args.quiet { "error" } else { "info" };

    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or(default_level.into()))
        .with_writer(std::io::stderr)
        .init();

    match start(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            error!("{err:?}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validate_server_selection;

    #[test]
    fn upstream_service_requires_explicit_consent() {
        assert!(validate_server_selection("https://sshx.io", false).is_err());
        assert!(validate_server_selection("https://sshx.io", true).unwrap());
        assert!(!validate_server_selection("http://localhost:8051", false).unwrap());
    }
}
