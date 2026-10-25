//! Live Strict command paths. Each family shares one disposable boot per backend.
#[path = "command_paths/cases.rs"]
mod cases;
#[path = "command_paths/harness.rs"]
mod harness;

macro_rules! family {
    ($name:ident, $family:literal) => {
        #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
        #[ignore = "requires both disposable servers on the builder; run with --ignored --nocapture"]
        async fn $name() -> ferraloom_parity::Result<()> {
            harness::run_family($family).await
        }
    };
}
family!(parity_paths_gamerule, "gamerule");
family!(parity_paths_clocks, "clocks");
family!(parity_paths_chat, "chat");
family!(parity_paths_players, "players");
family!(parity_paths_admin, "admin");
family!(parity_paths_placement, "placement");

family!(parity_paths_give, "give");
