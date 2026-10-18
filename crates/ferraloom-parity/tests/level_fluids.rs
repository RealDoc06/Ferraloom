mod level_support;
use ferraloom_data::{Block, properties};
use ferraloom_parity::{ObservationLog, Result, RunMode, Session, ensure, run};
use ferraloom_util::BlockPos;
use level_support::*;

async fn water_spreading(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    let source = BlockPos::new(15, -60, 8);
    set(bot, source, Block::WATER).await?;
    step(bot, 30).await?;
    let mut states = Vec::new();
    for dx in -7_i32..=7 {
        for dz in -7_i32..=7 {
            let pos = source.offset(dx, 0, dz);
            let distance = dx.abs() + dz.abs();
            let expected = if distance <= 6 {
                Block::WATER
                    .default_state()
                    .with(properties::LEVEL, distance as u8)
                    .unwrap()
            } else {
                Block::AIR.default_state()
            };
            let actual = block(bot, pos)?;
            ensure!(
                actual == expected,
                "water after 30 ticks at {pos:?}: {actual:?} != {expected:?}"
            );
            states.push((dx, dz, actual.id()));
        }
    }
    let mut log = ObservationLog::default();
    log.record("water.after_30_ticks", states)?;
    Ok(log)
}
async fn lava_meeting_water(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    let source = BlockPos::new(12, -60, 8);
    set(bot, source.east(1), Block::WATER).await?;
    set(bot, source, Block::LAVA).await?;
    let flowing = BlockPos::new(20, -60, 8);
    set(bot, flowing.east(1), Block::WATER).await?;
    command(
        bot,
        &format!(
            "setblock {} {} {} minecraft:lava[level=1]",
            flowing.x, flowing.y, flowing.z
        ),
        "commands.setblock.success",
    )
    .await?;
    let expected = [
        (source, Block::OBSIDIAN.default_state()),
        (flowing, Block::COBBLESTONE.default_state()),
    ];
    expect_blocks(bot, &expected).await?;
    let mut log = ObservationLog::default();
    log.record("source_product", block(bot, source)?.id())?;
    log.record("flowing_product", block(bot, flowing)?.id())?;
    Ok(log)
}
async fn falling_sand_column(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    command(
        bot,
        "fill 8 -55 8 8 -53 8 minecraft:sand",
        "commands.fill.success",
    )
    .await?;
    step(bot, 40).await?;
    let mut expected = Vec::new();
    for y in -60..=-58 {
        expected.push((BlockPos::new(8, y, 8), Block::SAND.default_state()));
    }
    for y in -55..=-53 {
        expected.push((BlockPos::new(8, y, 8), Block::AIR.default_state()));
    }
    expect_blocks(bot, &expected).await?;
    let mut log = ObservationLog::default();
    log.record(
        "column",
        expected
            .iter()
            .map(|(pos, _)| Ok((pos.y, block(bot, *pos)?.id())))
            .collect::<Result<Vec<_>>>()?,
    )?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK and integrated flat-world, commands and live phases"]
async fn parity_water_spreads_over_flat_ground_for_30_ticks() -> Result<()> {
    run(options(), RunMode::from_env()?, water_spreading).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK and integrated flat-world, commands and live phases"]
async fn parity_lava_meets_water() -> Result<()> {
    run(options(), RunMode::from_env()?, lava_meeting_water).await
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires builder vanilla/JDK and falling-entity hook integration"]
async fn parity_falling_sand_column() -> Result<()> {
    run(options(), RunMode::from_env()?, falling_sand_column).await
}
