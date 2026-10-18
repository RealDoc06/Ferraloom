mod level_support;

use ferraloom_data::{Block, properties as p};
use ferraloom_parity::{Bot, ObservationLog, Result, RunMode, Session, ensure, run};
use ferraloom_util::BlockPos;
use level_support::*;

async fn build_piston(bot: &mut Bot, pos: BlockPos, sticky: bool, length: i32) -> Result<()> {
    let block = if sticky { "sticky_piston" } else { "piston" };
    command(
        bot,
        &format!(
            "setblock {} {} {} minecraft:{block}[facing=east]",
            pos.x, pos.y, pos.z
        ),
        "commands.setblock.success",
    )
    .await?;
    command(
        bot,
        &format!(
            "fill {} {} {} {} {} {} minecraft:stone",
            pos.x + 1,
            pos.y,
            pos.z,
            pos.x + length,
            pos.y,
            pos.z
        ),
        "commands.fill.success",
    )
    .await?;
    Ok(())
}

fn record_rows(bot: &Bot, log: &mut ObservationLog, name: String, rows: &[BlockPos]) -> Result<()> {
    let states = rows
        .iter()
        .map(|pos| {
            (0..=15)
                .map(|x| Ok(block(bot, BlockPos::new(pos.x + x, pos.y, pos.z))?.id()))
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    log.record(name, states)
}

async fn record(session: &mut Session) -> Result<ObservationLog> {
    let bot_id = session.join("ParityBot").await?;
    let bot = session.bot(bot_id)?;
    freeze(bot).await?;
    command(
        bot,
        "fill 4 -60 4 32 -60 24 minecraft:stone strict",
        "commands.fill.success",
    )
    .await?;
    let mut log = ObservationLog::default();
    let limit = [BlockPos::new(8, -59, 8), BlockPos::new(8, -59, 12)];
    for (pos, length) in limit.into_iter().zip([12, 13]) {
        build_piston(bot, pos, false, length).await?;
        set(bot, pos.below(1), Block::REDSTONE_BLOCK).await?;
    }
    for tick in 1..=7 {
        step(bot, 1).await?;
        record_rows(bot, &mut log, format!("push-limit.{tick}"), &limit)?;
    }
    ensure!(
        block(bot, limit[0])?.get(p::EXTENDED) == Some(true),
        "twelve blocks must move"
    );
    ensure!(
        block(bot, limit[0].relative(ferraloom_util::Direction::East, 1))?.block()
            == Block::PISTON_HEAD
    );
    ensure!(
        block(bot, limit[0].relative(ferraloom_util::Direction::East, 13))?.block() == Block::STONE
    );
    ensure!(
        block(bot, limit[1])?.get(p::EXTENDED) == Some(false),
        "thirteen blocks must stay"
    );
    ensure!(
        block(bot, limit[1].relative(ferraloom_util::Direction::East, 14))?.block() == Block::AIR
    );

    let quasi = BlockPos::new(8, -59, 16);
    let wake = BlockPos::new(7, -59, 16);
    build_piston(bot, quasi, true, 1).await?;
    set(bot, quasi.above(2), Block::REDSTONE_BLOCK).await?;
    for tick in 1..=3 {
        step(bot, 1).await?;
        record_rows(bot, &mut log, format!("quasi.no-update.{tick}"), &[quasi])?;
    }
    ensure!(
        block(bot, quasi)?.get(p::EXTENDED) == Some(false),
        "quasi power needs a neighbor update"
    );
    set(bot, wake, Block::STONE).await?;
    for tick in 1..=6 {
        step(bot, 1).await?;
        record_rows(bot, &mut log, format!("quasi.extend.{tick}"), &[quasi])?;
    }
    ensure!(block(bot, quasi)?.get(p::EXTENDED) == Some(true));
    set(bot, quasi.above(2), Block::AIR).await?;
    for tick in 1..=3 {
        step(bot, 1).await?;
        record_rows(bot, &mut log, format!("quasi.unpowered.{tick}"), &[quasi])?;
    }
    ensure!(
        block(bot, quasi)?.get(p::EXTENDED) == Some(true),
        "quasi removal also needs a neighbor update"
    );
    set(bot, wake, Block::AIR).await?;
    for tick in 1..=6 {
        step(bot, 1).await?;
        record_rows(bot, &mut log, format!("quasi.retract.{tick}"), &[quasi])?;
    }
    ensure!(block(bot, quasi)?.get(p::EXTENDED) == Some(false));
    ensure!(
        block(bot, quasi.relative(ferraloom_util::Direction::East, 1))?.block() == Block::STONE
    );

    let pulses = [BlockPos::new(8, -59, 20), BlockPos::new(8, -59, 24)];
    for pos in pulses {
        build_piston(bot, pos, true, 1).await?;
        set(bot, pos.below(1), Block::REDSTONE_BLOCK).await?;
    }
    // The second extension event is queued and loses power before its phase.
    set(bot, pulses[1].below(1), Block::AIR).await?;
    step(bot, 1).await?;
    record_rows(bot, &mut log, "short-pulse.extend.1".into(), &pulses)?;
    set(bot, pulses[0].below(1), Block::AIR).await?;
    for tick in 1..=8 {
        step(bot, 1).await?;
        record_rows(
            bot,
            &mut log,
            format!("short-pulse.retract.{tick}"),
            &pulses,
        )?;
    }
    for pos in pulses {
        ensure!(block(bot, pos)?.get(p::EXTENDED) == Some(false));
    }
    ensure!(
        block(bot, pulses[1].relative(ferraloom_util::Direction::East, 1))?.block() == Block::STONE
    );
    ensure!(
        block(bot, pulses[1].relative(ferraloom_util::Direction::East, 2))?.block() == Block::AIR
    );
    Ok(log)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires dedicated builder vanilla/JDK and integrated live piston phases"]
async fn parity_piston_push_limits_quasi_updates_and_short_pulses() -> Result<()> {
    run(options(), RunMode::from_env()?, record).await
}
