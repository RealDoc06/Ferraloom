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
    // An unlimited hard limit (macOS reports one) is refused as the soft one: ask for macOS's
    // `OPEN_MAX` then. macOS also refuses a finite hard limit above `OPEN_MAX`.
    let hard = limit.maximum.unwrap_or(10240);
    let wanted = if cfg!(target_os = "macos") {
        hard.min(10240)
    } else {
        hard
    };
    if limit.current.is_none_or(|current| current >= wanted) {
        return;
    }
    let raised = Rlimit {
        current: Some(wanted),
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
    // The engine runs on a thread of its own, as the `java` launcher runs `main`: HotSpot does not
    // control the process's first thread's stack, so a plugin recursing too deep there could crash
    // the process instead of getting a StackOverflowError. 8 MiB is what the first thread has.
    let engine = std::thread::Builder::new()
        .name(thread.into())
        .stack_size(8 << 20)
        .spawn(move || {
            if let Err(error) = game::run() {
                log::error!("Horizon: {error}");
                if !help {
                    crash::report_error(&home, side, &*error);
                }
                logging::flush();
                std::process::exit(1);
            }
        });
    match engine.map(std::thread::JoinHandle::join) {
        Ok(Ok(())) => logging::flush(),
        // A panic was reported by the crash hook; 101 as a panic on the first thread exits.
        Ok(Err(_)) => {
            logging::flush();
            std::process::exit(101)
        }
        Err(error) => {
            eprintln!("Horizon: could not start the {thread}: {error}");
            logging::flush();
            std::process::exit(1);
        }
    }
}
