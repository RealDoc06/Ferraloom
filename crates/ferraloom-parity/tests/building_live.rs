//! Strict live comparisons for bound workstation openers and supported building
//! placement/hydration. Falling entity execution is deliberately a separate slice.
#[path = "building_live/hydration.rs"]
mod hydration;
#[allow(dead_code)]
mod level_support;
#[allow(dead_code)]
mod redstone_support;
#[path = "building_live/workstations.rs"]
mod workstations;

use ferraloom_data::{Block, BlockState};
use ferraloom_parity::{
    Bot, Result,
    protocol::play::{BlockHitResult, InteractionHand},
};
use ferraloom_util::{BlockPos, Direction};

const POS: BlockPos = BlockPos::new(8, -59, 8);

fn state(block: Block, properties: &[(&str, &str)]) -> BlockState {
    BlockState::from_properties(block, properties.iter().copied()).expect("native fixture state")
}

fn argument(state: BlockState) -> String {
    let properties = state
        .properties()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}[{properties}]", state.block().name())
}

async fn expect(bot: &mut Bot, pos: BlockPos, wanted: BlockState) -> Result<()> {
    level_support::expect_blocks(bot, &[(pos, wanted)]).await
}

async fn put(bot: &mut Bot, pos: BlockPos, wanted: BlockState) -> Result<()> {
    if wanted == wanted.block().default_state() {
        redstone_support::set(bot, pos, wanted.block()).await?;
    } else {
        // Native no-change feedback is authoritative. Clearing first makes the
        // property-bearing placement a real change, without trusting a snapshot.
        redstone_support::set(bot, pos, Block::AIR).await?;
        redstone_support::state(bot, pos, &argument(wanted)).await?;
    }
    expect(bot, pos, wanted).await
}

async fn use_block(bot: &mut Bot, pos: BlockPos, face: Direction) -> Result<()> {
    let mut offset = [0.5; 3];
    match face {
        Direction::Down => offset[1] = 0.0,
        Direction::Up => offset[1] = 1.0,
        Direction::North => offset[2] = 0.0,
        Direction::South => offset[2] = 1.0,
        Direction::West => offset[0] = 0.0,
        Direction::East => offset[0] = 1.0,
    }
    bot.use_item_on(
        InteractionHand::MainHand,
        BlockHitResult {
            block_pos: pos,
            direction: face,
            offset_x: offset[0],
            offset_y: offset[1],
            offset_z: offset[2],
            inside: false,
            world_border_hit: false,
        },
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK; live Strict workstation and anvil placement"]
async fn all_workstation_openers_anvil_placement_and_bound_menu_validity_match_vanilla()
-> Result<()> {
    ferraloom_parity::run(
        redstone_support::options(),
        ferraloom_parity::RunMode::Both,
        workstations::record,
    )
    .await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK; live Strict powder placement and neighbor hydration"]
async fn concrete_powder_hydration_and_neighbor_conversion_match_vanilla() -> Result<()> {
    ferraloom_parity::run(
        redstone_support::options(),
        ferraloom_parity::RunMode::Both,
        hydration::record,
    )
    .await
}
