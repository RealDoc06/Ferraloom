mod level_support;
use ferraloom_data::{Block, properties};
use ferraloom_parity::{
    ObservationLog, Result, RunMode, Session, WorldPreset, ensure,
    protocol::play::{BlockHitResult, InteractionHand},
    run,
};
use ferraloom_util::{BlockPos, Direction};
use level_support::*;

async fn random_ticks(bot: &mut ferraloom_parity::Bot) -> Result<()> {
    command(
        bot,
        "gamerule minecraft:random_tick_speed 4096",
        "commands.gamerule.set",
    )
    .await?;
    Ok(())
}
async fn crops_grow(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    random_ticks(bot).await?;
    let pos = BlockPos::new(8, -60, 8);
    set(bot, pos.below(1), Block::FARMLAND).await?;
    set(bot, pos.east(1).below(1), Block::WATER).await?;
    set(bot, pos, Block::WHEAT).await?;
    for _ in 0..20 {
        step(bot, 100).await?;
        if block(bot, pos)?.get(properties::AGE) == Some(7) {
            break;
        }
    }
    let age = block(bot, pos)?.get(properties::AGE);
    ensure!(age == Some(7), "wheat did not mature under /tick: {age:?}");
    let mut log = ObservationLog::default();
    log.record("wheat.mature", block(bot, pos)?.id())?;
    Ok(log)
}
async fn leaves_decay(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    let log_pos = BlockPos::new(8, -60, 8);
    let leaves = log_pos.east(1);
    set(bot, log_pos, Block::OAK_LOG).await?;
    set(bot, leaves, Block::OAK_LEAVES).await?;
    step(bot, 2).await?;
    ensure!(
        block(bot, leaves)?.get(properties::DISTANCE) == Some(1),
        "leaves failed to find adjacent log"
    );
    random_ticks(bot).await?;
    set(bot, log_pos, Block::AIR).await?;
    for _ in 0..10 {
        step(bot, 20).await?;
        if block(bot, leaves)?.is_air() {
            break;
        }
    }
    ensure!(
        block(bot, leaves)?.is_air(),
        "disconnected leaves did not decay"
    );
    let mut log = ObservationLog::default();
    log.record("leaves.after_log_removed", block(bot, leaves)?.id())?;
    Ok(log)
}
async fn support_removed(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    let flower = BlockPos::new(8, -60, 8);
    let double = BlockPos::new(10, -60, 8);
    let cane = BlockPos::new(12, -60, 8);
    for pos in [flower, double, cane] {
        set(bot, pos.below(1), Block::DIRT).await?;
    }
    set(bot, cane.east(1).below(1), Block::WATER).await?;
    set(bot, flower, Block::DANDELION).await?;
    set(bot, double, Block::LARGE_FERN).await?;
    command(
        bot,
        "setblock 10 -59 8 minecraft:large_fern[half=upper]",
        "commands.setblock.success",
    )
    .await?;
    set(bot, cane, Block::SUGAR_CANE).await?;
    for pos in [flower, double, cane] {
        set(bot, pos.below(1), Block::AIR).await?;
    }
    step(bot, 2).await?;
    let positions = [flower, double, double.above(1), cane];
    let expected = positions.map(|p| (p, Block::AIR.default_state()));
    expect_blocks(bot, &expected).await?;
    let mut log = ObservationLog::default();
    log.record(
        "plants.without_support",
        positions
            .map(|p| block(bot, p).map(|s| s.id()))
            .into_iter()
            .collect::<Result<Vec<_>>>()?,
    )?;
    Ok(log)
}
async fn bone_meal(bot: &mut ferraloom_parity::Bot, pos: BlockPos) -> Result<()> {
    bot.use_item_on(
        InteractionHand::MainHand,
        BlockHitResult {
            block_pos: pos,
            direction: Direction::Up,
            offset_x: 0.5,
            offset_y: 1.0,
            offset_z: 0.5,
            inside: false,
            world_border_hit: false,
        },
    )
    .await?;
    bot.synchronize().await
}
async fn bonemeal_grass_and_sapling(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    command(
        bot,
        "tp ParityBot 8.5 -60 8.5",
        "commands.teleport.success.entity.single",
    )
    .await?;
    command(
        bot,
        "give ParityBot minecraft:bone_meal 64",
        "commands.give.success.single",
    )
    .await?;
    let grass = BlockPos::new(10, -61, 8);
    command(
        bot,
        "fill 7 -61 5 15 -61 13 minecraft:grass_block",
        "commands.fill.success",
    )
    .await?;
    bone_meal(bot, grass).await?;
    let mut grew_grass = false;
    for x in 7..=13 {
        for z in 5..=11 {
            let state = block(bot, BlockPos::new(x, -60, z))?;
            grew_grass |= !state.is_air();
        }
    }
    ensure!(grew_grass, "bone meal did not place vegetation on grass");
    // Clear the flower/grass volume before asking a sapling to grow a tree.
    command(
        bot,
        "fill 7 -60 5 15 -45 13 minecraft:air strict",
        "commands.fill.success",
    )
    .await?;
    let sapling = BlockPos::new(10, -60, 10);
    set(bot, sapling, Block::OAK_SAPLING).await?;
    for _ in 0..40 {
        bone_meal(bot, sapling).await?;
        if block(bot, sapling)?.block() == Block::OAK_LOG {
            break;
        }
    }
    ensure!(
        block(bot, sapling)?.block() == Block::OAK_LOG,
        "bone meal did not grow the oak sapling"
    );
    let mut log = ObservationLog::default();
    log.record("grass.vegetation", grew_grass)?;
    log.record("sapling.base_log", block(bot, sapling)?.id())?;
    Ok(log)
}
async fn cold_snow_and_ice(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    random_ticks(bot).await?;
    command(bot, "weather rain 1200", "commands.weather.set.rain").await?;
    let water = BlockPos::new(8, -60, 8);
    let snow = BlockPos::new(12, -59, 8);
    set(bot, water, Block::WATER).await?;
    set(bot, snow.below(1), Block::STONE).await?;
    for _ in 0..40 {
        step(bot, 20).await?;
        if block(bot, water)?.block() == Block::ICE && block(bot, snow)?.block() == Block::SNOW {
            break;
        }
    }
    ensure!(
        block(bot, water)?.block() == Block::ICE,
        "water failed to freeze in snowy plains"
    );
    ensure!(
        block(bot, snow)?.block() == Block::SNOW,
        "snow failed to accumulate in snowy plains"
    );
    step(bot, 100).await?;
    let mut log = ObservationLog::default();
    log.record("cold.water", block(bot, water)?.id())?;
    log.record("cold.snow", block(bot, snow)?.id())?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK and complete live plant/command integration"]
async fn parity_crops_grow_under_tick() -> Result<()> {
    run(options(), RunMode::from_env()?, crops_grow).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK and integrated leaf/drop hooks"]
async fn parity_leaves_decay_after_log_removed() -> Result<()> {
    run(options(), RunMode::from_env()?, leaves_decay).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK and live support updates"]
async fn parity_plants_break_when_support_removed() -> Result<()> {
    run(options(), RunMode::from_env()?, support_removed).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "pending item bone meal and live worldgen tree integration"]
async fn parity_bone_meal_on_grass_and_saplings() -> Result<()> {
    run(options(), RunMode::from_env()?, bonemeal_grass_and_sapling).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK and integrated precipitation environment"]
async fn parity_snow_and_ice_in_cold_biome() -> Result<()> {
    let mut launch = options();
    launch.preset = WorldPreset::Flat {
        layers: vec![("minecraft:stone".into(), 4)],
        biome: "minecraft:snowy_plains".into(),
    };
    run(launch, RunMode::from_env()?, cold_snow_and_ice).await
}
