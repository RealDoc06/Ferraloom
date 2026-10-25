#[allow(dead_code)]
mod level_support;
#[allow(dead_code)]
mod redstone_support;
use ferraloom_data::{Block, properties as p};
use ferraloom_parity::{Bot, ObservationLog, Result, RunMode, Session, ensure, run};
use ferraloom_util::{BlockPos, Direction};
use level_support::block;
use redstone_support::step;
use redstone_support::{sample, set, source, state, timeline};

async fn propagation(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    for (case, rail) in ["powered_rail", "activator_rail"].into_iter().enumerate() {
        for (axis, facing) in [
            ("east_west", Direction::East),
            ("north_south", Direction::South),
        ] {
            let origin = BlockPos::new(
                8 + case as i32 * 18,
                -59,
                if facing == Direction::East { 8 } else { 28 },
            );
            let positions = (0..11)
                .map(|i| origin.relative(facing, i))
                .collect::<Vec<_>>();
            for pos in &positions {
                state(bot, *pos, &format!("minecraft:{rail}[shape={axis}]")).await?;
            }
            for (edge, on) in [("rise", true), ("fall", false)] {
                if on {
                    source(bot, origin.below(1), true).await?;
                } else {
                    set(bot, origin.below(1), Block::STONE).await?;
                }
                timeline(
                    bot,
                    log,
                    &format!("rail.{case}.{axis}.{edge}"),
                    &positions,
                    3,
                )
                .await?;
                for (distance, pos) in positions.iter().enumerate() {
                    ensure!(
                        block(bot, *pos)?.get(p::POWERED) == Some(on && distance < 9),
                        "{rail} wrong distance {distance} after {edge}"
                    );
                }
            }
        }
    }
    // A connected ascent consumes propagation depth just like a level segment.
    let origin = BlockPos::new(8, -59, 48);
    let mut positions = Vec::new();
    for i in 0..11 {
        let pos = BlockPos::new(origin.x + i, origin.y + i32::from(i >= 4), origin.z);
        set(bot, pos.below(1), Block::STONE).await?;
        positions.push(pos);
    }
    for (i, pos) in positions.iter().enumerate() {
        let shape = if i == 3 {
            "ascending_east"
        } else {
            "east_west"
        };
        state(bot, *pos, &format!("minecraft:powered_rail[shape={shape}]")).await?;
    }
    for (edge, on) in [("rise", true), ("fall", false)] {
        if on {
            source(bot, origin.below(1), true).await?;
        } else {
            set(bot, origin.below(1), Block::STONE).await?;
        }
        timeline(bot, log, &format!("rail.ascent.{edge}"), &positions, 3).await?;
        for (distance, pos) in positions.iter().enumerate() {
            ensure!(block(bot, *pos)?.get(p::POWERED) == Some(on && distance < 9));
        }
    }
    Ok(())
}
async fn junctions(bot: &mut Bot, log: &mut ObservationLog) -> Result<()> {
    // Every neighbor mask and both construction orders exercise RailState's
    // insertion-order connection choices. Redstone reverses each T junction.
    let center = BlockPos::new(48, -59, 14);
    for mask in 0_u8..16 {
        for reverse in [false, true] {
            redstone_support::clear(bot, BlockPos::new(46, -59, 12), BlockPos::new(50, -59, 16))
                .await?;
            set(bot, center, Block::RAIL).await?;
            let directions = if reverse {
                [
                    Direction::East,
                    Direction::West,
                    Direction::South,
                    Direction::North,
                ]
            } else {
                [
                    Direction::North,
                    Direction::South,
                    Direction::West,
                    Direction::East,
                ]
            };
            for direction in directions {
                let bit = match direction {
                    Direction::North => 0,
                    Direction::South => 1,
                    Direction::West => 2,
                    Direction::East => 3,
                    _ => unreachable!(),
                };
                if mask & (1 << bit) != 0 {
                    set(bot, center.relative(direction, 1), Block::RAIL).await?;
                }
            }
            let positions = [
                center,
                center.north(1),
                center.south(1),
                center.west(1),
                center.east(1),
            ];
            step(bot, 1).await?;
            sample(
                bot,
                log,
                format!("junction.{mask}.{reverse}.unpowered"),
                &positions,
            )?;
            source(bot, center.below(1), true).await?;
            step(bot, 1).await?;
            sample(
                bot,
                log,
                format!("junction.{mask}.{reverse}.powered"),
                &positions,
            )?;
            set(bot, center.below(1), Block::STONE).await?;
            step(bot, 1).await?;
            sample(
                bot,
                log,
                format!("junction.{mask}.{reverse}.released"),
                &positions,
            )?;
        }
    }
    for direction in [
        Direction::North,
        Direction::South,
        Direction::West,
        Direction::East,
    ] {
        redstone_support::clear(bot, BlockPos::new(46, -59, 30), BlockPos::new(50, -57, 34))
            .await?;
        let center = BlockPos::new(48, -59, 32);
        let upper = center.relative(direction, 1).above(1);
        set(bot, upper.below(1), Block::STONE).await?;
        set(bot, upper, Block::RAIL).await?;
        set(bot, center, Block::RAIL).await?;
        step(bot, 1).await?;
        sample(
            bot,
            log,
            format!("junction.ascent.{direction:?}"),
            &[center, upper],
        )?;
        ensure!(matches!(
            block(bot, center)?.get(p::SHAPE),
            Some(
                p::Shape::AscendingNorth
                    | p::Shape::AscendingSouth
                    | p::Shape::AscendingWest
                    | p::Shape::AscendingEast
            )
        ));
    }
    Ok(())
}
async fn record(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    redstone_support::prepare(bot).await?;
    let mut log = ObservationLog::default();
    propagation(bot, &mut log).await?;
    junctions(bot, &mut log).await?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "live Strict both-server rail propagation and connection scenarios"]
async fn parity_rail_limits_junctions_and_slopes() -> Result<()> {
    run(redstone_support::options(), RunMode::Both, record).await
}
