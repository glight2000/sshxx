use std::{
    net::{IpAddr, SocketAddr},
    process::ExitCode,
};

use anyhow::Result;
use clap::Parser;
use sshx_server::{Server, ServerOptions};
use tracing::{error, info};

#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};

/// The sshxx server CLI interface.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Specify port to listen on.
    #[clap(long, default_value_t = 8051)]
    port: u16,

    /// Which IP address or network interface to listen on.
    #[clap(long, value_parser, default_value = "::1")]
    listen: IpAddr,

    /// Secret used for signing session tokens.
    #[clap(long, env = "SSHXX_SECRET")]
    secret: Option<String>,

    /// Override the origin URL returned by the Open() RPC.
    #[clap(long)]
    override_origin: Option<String>,

    /// URL for optional multi-server coordination (requires redis-mesh build feature).
    #[clap(long, env = "SSHXX_REDIS_URL")]
    redis_url: Option<String>,

    /// Hostname of this server, if running multiple servers.
    #[clap(long)]
    host: Option<String>,

    /// Fixed session name used instead of a random value (unsafe for
    /// production).
    #[clap(long, env = "SSHXX_SESSION_NAME")]
    session_name: Option<String>,
}

#[cfg(unix)]
async fn wait_for_shutdown() -> Result<()> {
    let mut sigterm = signal(SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result?,
        _ = sigterm.recv() => (),
    }
    Ok(())
}

#[cfg(not(unix))]
async fn wait_for_shutdown() -> Result<()> {
    tokio::signal::ctrl_c().await?;
    Ok(())
}

#[tokio::main]
async fn start(args: Args) -> Result<()> {
    let addr = SocketAddr::new(args.listen, args.port);

    let mut options = ServerOptions::default();
    options.secret = args.secret;
    options.override_origin = args.override_origin;
    options.redis_url = args.redis_url;
    options.host = args.host;
    options.session_name = args.session_name;

    let server = Server::new(options)?;

    let serve_task = async {
        info!("server listening at {addr}");
        server.bind(&addr).await
    };

    let signals_task = async {
        wait_for_shutdown().await?;
        info!("gracefully shutting down...");
        server.shutdown();
        Ok(())
    };

    tokio::try_join!(serve_task, signals_task)?;
    Ok(())
}

fn main() -> ExitCode {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or("info".into()))
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
