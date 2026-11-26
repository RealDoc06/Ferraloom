//! Every horizon-client parity probe in one binary: `probe <name> [arguments]` runs `<name>.rs` as its own
//! example did (tools/test_*_parity.py, tools/oracle_harness.py). Linking ~80 examples,
//! each with the whole client in it, was most of the parity job's probe build; one binary builds it once.
//!
//! This file only lists the probes, so that adding one reruns no harness: tools/parity.py leaves it out of
//! their dependencies. How a probe runs is in dispatch.rs.
#[macro_use]
mod dispatch;

use dispatch::args;

mod arrow_probe;
mod block_entity_tickers_fixture;
mod block_transform_fixture;
mod cauldron_fixture;
mod command_parse_fixture;
mod creative_mining_probe;
mod cushion_placement_probe;
mod enchanting_fixture;
mod entity_components_probe;
mod entity_death_loot_fixture;
mod environment_fixture;
mod finalize_spawn_fixture;
mod fire_starter_sound_probe;
mod game_loop_probe;
mod light_parity;
mod menu_fixture;
mod mob_ai_probe;
mod mob_attributes_fixture;
mod natural_spawn_fixture;
mod pick_block_data_fixture;
mod place_recipe_fixture;
mod raid_fixture;
mod random_tick_sections_probe;
mod recipe_book_fixture;
mod recipe_fixture;
mod scoreboard_fixture;
mod server_files_probe;
mod spawn_rules_fixture;
mod team_rules_fixture;
mod trading_fixture;
mod world_options_fixture;

probes! {
    arrow_probe,
    block_entity_tickers_fixture,
    block_transform_fixture,
    creative_mining_probe,
    cushion_placement_probe,
    cauldron_fixture,
    command_parse_fixture,
    enchanting_fixture,
    entity_components_probe,
    entity_death_loot_fixture,
    environment_fixture,
    finalize_spawn_fixture,
    fire_starter_sound_probe,
    game_loop_probe,
    light_parity,
    menu_fixture,
    mob_ai_probe,
    mob_attributes_fixture,
    natural_spawn_fixture,
    pick_block_data_fixture,
    place_recipe_fixture,
    raid_fixture,
    random_tick_sections_probe,
    recipe_book_fixture,
    recipe_fixture,
    scoreboard_fixture,
    server_files_probe,
    spawn_rules_fixture,
    team_rules_fixture,
    trading_fixture,
    world_options_fixture,
}
