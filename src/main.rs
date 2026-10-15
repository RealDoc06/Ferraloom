use std::{
    backtrace::{Backtrace, BacktraceStatus},
    fs,
    net::SocketAddr,
    num::NonZeroUsize,
    path::PathBuf,
    process,
    sync::atomic::Ordering,
    thread,
};

use anyhow::{Context, Result};
use clap::Parser;
use pumpkin_core::{
    CRASH_REPORT, PumpkinServer, SERVER_EXIT_CODE,
    crash::{CrashReport, FullBacktrace},
    data::VanillaData,
};
use pumpkin_data::packet::CURRENT_MC_VERSION;

mod commands;
mod config;
mod registry;
mod world_lock;

/// A Minecraft Java server powered by the multithreaded Pumpkin Rust engine.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Directory containing pumpkin.toml, worlds, player data, and logs.
    #[arg(long, default_value = ".")]
    data_dir: PathBuf,

    /// Override the Java listener, for example 127.0.0.1:25565 or [::]:25565.
    #[arg(long)]
    bind: Option<SocketAddr>,

    /// Disable account authentication for an isolated local test server.
    #[arg(long)]
    offline: bool,

    /// Workers in each of the Tokio and Rayon pools; defaults to available CPUs.
    #[arg(long)]
    workers: Option<NonZeroUsize>,

    /// Export the engine's block and state registry to JSON without starting a server.
    #[arg(long, value_name = "JSON_PATH")]
    export_registry: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();
    if let Err(error) = run(args) {
        eprintln!("Ferraloom: {error:#}");
        process::exit(1);
    }
    // Upstream's stdin console uses a blocking reader. Exit after its graceful
    // server shutdown rather than waiting indefinitely for that reader.
    process::exit(SERVER_EXIT_CODE.load(Ordering::Acquire));
}

fn run(args: Args) -> Result<()> {
    if let Some(path) = &args.export_registry {
        return registry::export(path);
    }
    fs::create_dir_all(&args.data_dir)
        .with_context(|| format!("Cannot create data directory {}", args.data_dir.display()))?;
    let data_dir = args
        .data_dir
        .canonicalize()
        .context("Cannot resolve data directory")?;
    let mut config = config::load(&data_dir)?;
    config::configure_runtime(&mut config, &args)?;
    // Lock the world before the engine touches it. Held until `run` returns,
    // after the final save.
    let _world_lock = world_lock::acquire(&data_dir.join(&config.basic.default_level_name))?;

    let workers = args
        .workers
        .unwrap_or_else(|| thread::available_parallelism().unwrap_or(NonZeroUsize::MIN))
        .get();
    let _ = rustls::crypto::ring::default_provider().install_default();
    rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .thread_name(|index| format!("Ferraloom-CPU-{index}"))
        // Detached Rayon jobs otherwise abort, bypassing graceful world saving.
        .panic_handler(|_| {
            SERVER_EXIT_CODE.store(1, Ordering::Release);
            pumpkin_core::stop_server();
        })
        .build_global()
        .context("Cannot initialize CPU workers")?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .thread_name("Ferraloom-IO")
        .enable_all()
        .build()
        .context("Cannot initialize network workers")?;

    // Pumpkin resolves its persistent files relative to the working directory.
    // Set it once before starting any server tasks.
    std::env::set_current_dir(&data_dir).context("Cannot enter data directory")?;
    pumpkin_core::init_logger(&config.advanced);
    install_panic_handler();

    let result = runtime.block_on(async move {
        let signal_task = install_signal_handler()?;
        tracing::info!(
            "Ferraloom {} | Minecraft Java {} (protocol {}) | {} workers per pool | data {}",
            env!("CARGO_PKG_VERSION"),
            CURRENT_MC_VERSION,
            CURRENT_MC_VERSION.protocol_version(),
            workers,
            data_dir.display()
        );
        tracing::info!(
            "Engine base: Pumpkin 5b53f064af8708ba33fcaa533b81ca29a53a3310; Ferraloom compatibility patches included"
        );
        if !config.advanced.networking.java.online_mode {
            tracing::warn!("Offline mode enabled: player identities are not authenticated");
        }
        let server = PumpkinServer::new(
            config.basic,
            config.advanced,
            config.telemetry,
            VanillaData::load(),
            Vec::new(),
        )
        .await
        .context("Cannot initialize Minecraft worlds")?;
        commands::install_console_chat_logging(&server.server)?;
        if let Some(listener) = &server.tcp_listener {
            tracing::info!("Java listener ready at {}", listener.local_addr()?);
        }
        server.start().await;
        signal_task.abort();
        Ok(())
    });
    runtime.shutdown_timeout(std::time::Duration::from_secs(1));
    result
}

/// Panics in detached engine tasks must stop the server and produce a failure exit status.
fn install_panic_handler() {
    let main_thread = thread::current().id();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        SERVER_EXIT_CODE.store(1, Ordering::Release);
        let captured = Backtrace::capture();
        let full = if captured.status() == BacktraceStatus::Captured {
            FullBacktrace::Captured
        } else {
            FullBacktrace::ForceCaptured(Backtrace::force_capture())
        };
        let first = CRASH_REPORT
            .set(CrashReport::new(info, captured, full))
            .is_ok();
        pumpkin_core::stop_server();
        if thread::current().id() == main_thread {
            if first && let Some(report) = CRASH_REPORT.get() {
                report.print_to_console();
                report.save_and_log();
            }
            process::exit(1);
        }
    }));
}

#[cfg(unix)]
fn install_signal_handler() -> std::io::Result<tokio::task::JoinHandle<()>> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    Ok(tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = interrupt.recv() => {},
                _ = terminate.recv() => {},
                _ = hangup.recv() => {},
            }
            request_stop();
        }
    }))
}

#[cfg(not(unix))]
fn install_signal_handler() -> std::io::Result<tokio::task::JoinHandle<()>> {
    Ok(tokio::spawn(async {
        loop {
            if let Err(error) = tokio::signal::ctrl_c().await {
                tracing::error!("Cannot listen for shutdown signals: {error}");
                SERVER_EXIT_CODE.store(1, Ordering::Release);
                pumpkin_core::stop_server();
                break;
            }
            request_stop();
        }
    }))
}

/// The first signal stops the server gracefully. A signal during shutdown force-exits
/// with a failure status, since the final save may be incomplete.
fn request_stop() {
    if pumpkin_core::SERVER_IS_STOPPING.load(Ordering::Acquire) {
        SERVER_EXIT_CODE.store(1, Ordering::Release);
        pumpkin_core::stop_or_exit_server();
    } else {
        pumpkin_core::stop_server();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_workers_and_invalid_listener() {
        assert!(Args::try_parse_from(["ferraloom", "--workers", "0"]).is_err());
        assert!(Args::try_parse_from(["ferraloom", "--bind", "localhost"]).is_err());
        assert!(Args::try_parse_from(["ferraloom", "--workers", "1", "--bind", "[::1]:0"]).is_ok());
    }
}
