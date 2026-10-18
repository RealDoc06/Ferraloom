mod level_support;
use ferraloom_data::{Block, properties as p};
use ferraloom_parity::{ObservationLog, Result, RunMode, Session, ensure, run};
use ferraloom_util::BlockPos;
use level_support::*;

async fn record(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    freeze(bot).await?;
    command(
        bot,
        "fill 4 -60 4 30 -60 20 minecraft:stone strict",
        "commands.fill.success",
    )
    .await?;
    command(
        bot,
        "fill 4 -59 4 30 -56 20 minecraft:air strict",
        "commands.fill.success",
    )
    .await?;
    let mut log = ObservationLog::default();
    command(
        bot,
        "fill 8 -59 8 22 -59 8 minecraft:redstone_wire",
        "commands.fill.success",
    )
    .await?;
    command(
        bot,
        "fill 12 -59 9 12 -59 13 minecraft:redstone_wire",
        "commands.fill.success",
    )
    .await?;
    let wires: Vec<_> = (8..=22)
        .map(|x| BlockPos::new(x, -59, 8))
        .chain((9..=13).map(|z| BlockPos::new(12, -59, z)))
        .collect();
    for (phase, power) in [
        ("on", Block::REDSTONE_BLOCK),
        ("off", Block::AIR),
        ("on-again", Block::REDSTONE_BLOCK),
    ] {
        set(bot, BlockPos::new(7, -59, 8), power).await?;
        step(bot, 1).await?;
        let values = wires
            .iter()
            .map(|pos| Ok((pos.x, pos.y, pos.z, block(bot, *pos)?.id())))
            .collect::<Result<Vec<_>>>()?;
        ensure!(
            block(bot, wires[0])?.get(p::POWER) == Some(if power == Block::AIR { 0 } else { 15 }),
            "wire failed {phase}"
        );
        log.record(format!("wire.{phase}"), values)?;
    }
    command(
        bot,
        "setblock 8 -59 16 minecraft:repeater[facing=west,delay=2]",
        "commands.setblock.success",
    )
    .await?;
    command(
        bot,
        "setblock 9 -59 16 minecraft:redstone_lamp",
        "commands.setblock.success",
    )
    .await?;
    set(bot, BlockPos::new(7, -59, 16), Block::REDSTONE_BLOCK).await?;
    for tick in 1..=6 {
        step(bot, 1).await?;
        log.record(
            format!("repeater-rise.{tick}"),
            [
                block(bot, BlockPos::new(8, -59, 16))?.id(),
                block(bot, BlockPos::new(9, -59, 16))?.id(),
            ],
        )?;
    }
    set(bot, BlockPos::new(7, -59, 16), Block::AIR).await?;
    for tick in 1..=10 {
        step(bot, 1).await?;
        log.record(
            format!("repeater-fall.{tick}"),
            [
                block(bot, BlockPos::new(8, -59, 16))?.id(),
                block(bot, BlockPos::new(9, -59, 16))?.id(),
            ],
        )?;
    }
    command(
        bot,
        "setblock 16 -59 16 minecraft:observer[facing=west]",
        "commands.setblock.success",
    )
    .await?;
    command(
        bot,
        "setblock 17 -59 16 minecraft:redstone_lamp",
        "commands.setblock.success",
    )
    .await?;
    set(bot, BlockPos::new(15, -59, 16), Block::STONE).await?;
    for tick in 1..=8 {
        step(bot, 1).await?;
        log.record(
            format!("observer.{tick}"),
            [
                block(bot, BlockPos::new(16, -59, 16))?.id(),
                block(bot, BlockPos::new(17, -59, 16))?.id(),
            ],
        )?;
    }
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires dedicated builder vanilla/JDK and integrated live block phases"]
async fn parity_redstone_wire_repeater_lamp_and_observer_timelines() -> Result<()> {
    run(options(), RunMode::from_env()?, record).await
}
