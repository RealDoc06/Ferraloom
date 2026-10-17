use std::{net::SocketAddr, num::NonZeroUsize, process::ExitCode, thread};

use anyhow::{Context, Result};
use clap::Parser;
use ferraloom::server::{Server, ServerConfig};
use tokio::sync::watch;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Ferraloom independent Minecraft Java server (in development)"
)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:25565")]
    bind: SocketAddr,
    /// Explicitly enable unauthenticated development connections.
    #[arg(long)]
    offline: bool,
    /// Network runtime workers; defaults to available CPUs.
    #[arg(long)]
    workers: Option<NonZeroUsize>,
    #[arg(long, default_value_t = NonZeroUsize::new(256).unwrap())]
    max_connections: NonZeroUsize,
    #[arg(long, default_value = "Ferraloom independent engine (in development)")]
    motd: String,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Ferraloom: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<()> {
    let workers = args
        .workers
        .unwrap_or_else(|| thread::available_parallelism().unwrap_or(NonZeroUsize::MIN));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers.get())
        .thread_name("Ferraloom")
        .enable_all()
        .build()
        .context("cannot start runtime")?;
    runtime.block_on(async {
        let config = ServerConfig {
            offline: args.offline,
            max_connections: args.max_connections.get(),
            motd: args.motd,
            ..ServerConfig::default()
        };
        let server = Server::bind(args.bind, config).await?;
        println!("Ferraloom {} | Java {} | independent engine | {workers} workers | {}", env!("CARGO_PKG_VERSION"), ferraloom::MINECRAFT_VERSION, server.local_addr()?);
        let (shutdown, receiver) = watch::channel(false);
        let signals = tokio::spawn(async move {
            #[cfg(unix)]
            {
                use tokio::signal::unix::{SignalKind, signal};
                let mut terminate = signal(SignalKind::terminate())?;
                tokio::select! { result = tokio::signal::ctrl_c() => result?, _ = terminate.recv() => {} }
            }
            #[cfg(not(unix))]
            tokio::signal::ctrl_c().await?;
            let _ = shutdown.send(true);
            Ok::<(), std::io::Error>(())
        });
        let result = server.run(receiver).await;
        signals.abort();
        result.context("server stopped with an error")
    })
}
