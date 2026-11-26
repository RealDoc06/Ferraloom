//! Ferraloom's engine: the Minecraft Java Edition 26.3 dedicated server (`--dedicated`).
#![forbid(unsafe_code)]
mod game;

use horizon_client::{crash, logging};

/// The mod API level this game serves, kept in the program for the launcher
/// to read without running it (`horizon_mods::api_level::MARK`). `main`
/// reads it, so the linker keeps it.
static MOD_API_LEVEL: [u8; 27] = horizon_mods::api_level::MARK;

/// jemalloc instead of glibc malloc on Linux: its per-thread caches suit the tick pool's short-lived
/// allocations and the server's thousand connection threads.
#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

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
    // component harness in the game home.
    let (home, side, thread) = if dedicated {
        (std::path::PathBuf::from("."), "server", "Server thread")
    } else {
        (logging::game_home(), "client", "Render thread")
    };
    if !help {
        logging::init(&home, thread);
        // The log's first line names the build, so a bug report that carries the log says which one it was.
        log::info!("Horizon {}", crash::version());
        crash::install(home.clone(), side);
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
