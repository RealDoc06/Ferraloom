//! Ferraloom's engine: the Minecraft Java Edition 26.3 dedicated server (`--dedicated`).
#![forbid(unsafe_code)]
mod game;

use horizon_client::{crash, logging};

/// The mod API level this engine serves, kept in the program so a tool can
/// read it without running it (`horizon_mods::api_level::read_mark`). `main`
/// reads it, so the linker keeps it.
static MOD_API_LEVEL: [u8; 27] = horizon_mods::api_level::MARK;

/// jemalloc instead of glibc malloc on Linux: its per-thread caches suit the tick pool's short-lived
/// allocations and the server's thousand connection threads.
#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// Raises the open-file limit to the hard one, as the JVM does at start (`-XX:+MaxFDLimit`), so vanilla
/// never meets the usual soft limit of 1024. A server holds a socket per player and a file per open
/// region: at 500 players that limit made region and entity saves fail with "Too many open files".
#[cfg(unix)]
fn raise_open_file_limit() {
    use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};
    let limit = getrlimit(Resource::Nofile);
    if limit.current == limit.maximum {
        return;
    }
    let raised = Rlimit {
        current: limit.maximum,
        maximum: limit.maximum,
    };
    if let Err(error) = setrlimit(Resource::Nofile, raised) {
        log::warn!("Open-file limit stays at {:?}: {error}", limit.current);
    }
}

fn main() {
    std::hint::black_box(&MOD_API_LEVEL);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let game_args = args.iter().take_while(|a| *a != "--dedicated");
    let help = game_args.clone().any(|a| a == "--help" || a == "-h");
    let dedicated = args.iter().any(|a| a == "--dedicated");
    let fixture = ["HORIZON_DISPENSE_FIXTURE", "HORIZON_SCULK_FIXTURE"]
        .iter()
        .any(|v| std::env::var_os(v).is_some());
    if !help && !dedicated && !fixture {
        eprintln!("{}", game::USAGE);
        std::process::exit(2);
    }
    // The dedicated server logs in its own folder, as vanilla's does; a
    // fixture (HORIZON_DISPENSE_FIXTURE, HORIZON_SCULK_FIXTURE) in the game home.
    let (home, side, thread) = if dedicated {
        (std::path::PathBuf::from("."), "server", "Server thread")
    } else {
        (logging::game_home(), "fixture", "Main thread")
    };
    if !help {
        logging::init(&home, thread);
        // The log's first line names the build, so a bug report that carries the log says which one it was.
        log::info!("Horizon {}", crash::version());
        crash::install(home.clone(), side);
    }
    #[cfg(unix)]
    if dedicated {
        raise_open_file_limit();
    }
    if let Err(error) = game::run() {
        log::error!("Horizon: {error}");
        if !help {
            crash::report_error(&home, side, &*error);
        }
        std::process::exit(1);
    }
    logging::flush();
}
