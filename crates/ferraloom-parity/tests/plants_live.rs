//! Live Strict plant mechanics, sharing one disposable world per server.
#[path = "plants_live/growth.rs"]
mod growth;
mod level_support;
#[path = "plants_live/soil.rs"]
mod soil;
#[path = "plants_live/support.rs"]
mod support;
#[path = "plants_live/uses.rs"]
mod uses;
#[path = "plants_live/weather.rs"]
mod weather;

use anyhow::Context;
use ferraloom_data::{Block, BlockState, properties as p};
use ferraloom_parity::{
    Bot, ObservationLog, Result, RunMode, Session, WorldPreset, ensure,
    protocol::play::{BlockHitResult, InteractionHand},
};
use ferraloom_util::{BlockPos, Direction};
use level_support::{block, command, expect_blocks, freeze, options, step};

const Y: i32 = -60;
fn at(x: i32, z: i32) -> BlockPos {
    BlockPos::new(x, Y, z)
}
fn state(block: Block, properties: &[(&'static str, &'static str)]) -> BlockState {
    BlockState::from_properties(block, properties.iter().copied()).expect("native test state")
}
fn block_argument(state: BlockState) -> String {
    let properties = state
        .properties()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}[{properties}]", state.block().name())
}
async fn put(bot: &mut Bot, pos: BlockPos, state: BlockState) -> Result<()> {
    if block(bot, pos)? == state {
        return Ok(());
    }
    if state == state.block().default_state() {
        level_support::set(bot, pos, state.block()).await?;
    } else {
        command(
            bot,
            &format!(
                "setblock {} {} {} {}",
                pos.x,
                pos.y,
                pos.z,
                block_argument(state)
            ),
            "commands.setblock.success",
        )
        .await?;
    }
    expect_blocks(bot, &[(pos, state)]).await
}
async fn fill(bot: &mut Bot, from: BlockPos, to: BlockPos, state: BlockState) -> Result<()> {
    // fill.failed is native's zero-changes result, valid for arena cleanup;
    // the endpoint observations also serve as a chunk-broadcast barrier.
    command(
        bot,
        &format!(
            "fill {} {} {} {} {} {} {} strict",
            from.x,
            from.y,
            from.z,
            to.x,
            to.y,
            to.z,
            block_argument(state)
        ),
        "commands.fill.",
    )
    .await?;
    expect_blocks(bot, &[(from, state), (to, state)]).await
}
async fn random_ticks(bot: &mut Bot, speed: i32) -> Result<()> {
    command(
        bot,
        &format!("gamerule minecraft:random_tick_speed {speed}"),
        "commands.gamerule.set",
    )
    .await?;
    Ok(())
}
async fn reset(bot: &mut Bot) -> Result<()> {
    random_ticks(bot, 0).await?;
    bot.teleport([8.5, -60.0, 8.5]).await?;
    fill(
        bot,
        at(-8, -8),
        BlockPos::new(23, -32, 23),
        Block::AIR.default_state(),
    )
    .await?;
    fill(
        bot,
        at(-8, -8).below(1),
        at(23, 23).below(1),
        Block::STONE.default_state(),
    )
    .await?;
    step(bot, 2).await?;
    Ok(())
}
async fn until(
    bot: &mut Bot,
    description: &str,
    rounds: u32,
    ticks: i64,
    matches: impl Fn(&Bot) -> Result<bool>,
) -> Result<()> {
    for _ in 0..rounds {
        if matches(bot)? {
            return Ok(());
        }
        step(bot, ticks).await?;
    }
    ensure!(matches(bot)?, "{description}");
    Ok(())
}
fn record(log: &mut ObservationLog, bot: &Bot, label: &str, positions: &[BlockPos]) -> Result<()> {
    log.record(
        label,
        positions
            .iter()
            .map(|pos| block(bot, *pos).map(BlockState::id))
            .collect::<Result<Vec<_>>>()?,
    )
}

async fn plants(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    command(bot, "tick rate 1000", "commands.tick.rate.success").await?;
    command(bot, "time set noon", "commands.time.set").await?;
    command(bot, "weather clear 100000", "commands.weather.set.clear").await?;
    let mut log = ObservationLog::default();
    support::survival(bot, &mut log)
        .await
        .context("support updates")?;
    support::leaves(bot, &mut log)
        .await
        .context("leaf distance and decay")?;
    growth::crops(bot, &mut log)
        .await
        .context("crop growth and light")?;
    soil::spreading(bot, &mut log)
        .await
        .context("grass and mycelium")?;
    growth::columns(bot, &mut log)
        .await
        .context("bamboo, cactus and cane")?;
    soil::farmland(bot, &mut log)
        .await
        .context("farmland hydration and trampling")?;
    uses::bonemeal(bot, &mut log)
        .await
        .context("bone meal and saplings")?;
    uses::dripleaf(bot, &mut log)
        .await
        .context("dripleaf contact and power")?;
    growth::vines_and_kelp(bot, &mut log)
        .await
        .context("vines and kelp")?;
    weather::melting(bot, &mut log)
        .await
        .context("snow and ice melting")?;
    weather::precipitation(bot, &mut log)
        .await
        .context("snow and ice formation")?;
    command(bot, "save-all flush", "commands.save.success").await?;
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK and released live item/block integration"]
async fn parity_live_plant_mechanics() -> Result<()> {
    let mut launch = options();
    // Clear weather and lit irrigation isolate earlier groups; the final group
    // uses this same world's real cold biome precipitation without a second boot.
    launch.preset = WorldPreset::Flat {
        layers: vec![("minecraft:stone".into(), 4)],
        biome: "minecraft:snowy_plains".into(),
    };
    launch
        .game_rules
        .insert("minecraft:block_drops".into(), "false".into());
    launch
        .properties
        .insert("spawn-protection".into(), "0".into());
    ferraloom_parity::run(launch, RunMode::from_env()?, plants).await
}
