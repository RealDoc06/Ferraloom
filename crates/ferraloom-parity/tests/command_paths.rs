//! Live Strict command paths. Each family shares one disposable boot per backend.
#[path = "command_paths/cases.rs"]
mod cases;
#[path = "command_paths/harness.rs"]
mod harness;
#[path = "command_paths/place_template.rs"]
mod place_template;
#[path = "command_paths/placement_nbt.rs"]
mod placement_nbt;

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
family!(parity_paths_place_template, "place_template");
family!(parity_paths_placement_nbt, "placement_nbt");

family!(parity_paths_give, "give");
family!(parity_paths_data_storage, "data_storage");

family!(parity_paths_data_block, "data_block");

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "BannerBlockEntity factory not ported"]
async fn parity_paths_placement_nbt_banner() -> ferraloom_parity::Result<()> {
    harness::run_family("placement_nbt_banner").await
}
