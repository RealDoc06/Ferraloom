#[allow(dead_code)] // Each parity target uses a subset of the shared level fixtures.
mod level_support;
#[allow(dead_code)]
mod redstone_support;
use ferraloom_data::{Block, properties as p};
use ferraloom_parity::{ObservationLog, Result, RunMode, Session, ensure, run};
use ferraloom_util::BlockPos;
use level_support::*;
use redstone_support::set;

async fn record_torch_burnout(
    bot: &mut ferraloom_parity::Bot,
    log: &mut ObservationLog,
) -> Result<()> {
    let support = BlockPos::new(12, -59, 5);
    let torch = BlockPos::new(12, -58, 5);
    let wire = BlockPos::new(11, -59, 5);
    let input = BlockPos::new(10, -59, 5);
    set(bot, support, Block::STONE).await?;
    set(bot, wire, Block::REDSTONE_WIRE).await?;
    set(bot, torch, Block::REDSTONE_TORCH).await?;
    for cycle in 0..8 {
        for (edge, source) in [("rise", Block::REDSTONE_BLOCK), ("fall", Block::AIR)] {
            set(bot, input, source).await?;
            for tick in 1..=2 {
                step(bot, 1).await?;
                log.record(
                    format!("torch.{cycle}.{edge}.{tick}"),
                    block(bot, torch)?.id(),
                )?;
            }
            let expected = edge == "fall" && cycle < 7;
            ensure!(
                block(bot, torch)?.get(p::LIT) == Some(expected),
                "torch cycle {cycle} after {edge}"
            );
        }
    }
    // Burnout schedules recovery 160 ticks after the eighth falling torch
    // edge. Removing its input above has already consumed two of those ticks.
    for tick in 1..=160 {
        step(bot, 1).await?;
        let state = block(bot, torch)?;
        log.record(format!("torch.recovery.{tick}"), state.id())?;
        ensure!(
            state.get(p::LIT) == Some(tick >= 158),
            "torch recovery tick {tick}"
        );
    }
    Ok(())
}

async fn record(session: &mut Session) -> Result<ObservationLog> {
    let id = session.join("ParityBot").await?;
    let bot = session.bot(id)?;
    redstone_support::prepare(bot).await?;
    command(
        bot,
        "fill 4 -60 4 30 -60 20 minecraft:stone strict",
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
        if tick == 1 {
            set(bot, BlockPos::new(15, -59, 16), Block::DIRT).await?;
        }
        if tick == 3 {
            set(bot, BlockPos::new(15, -59, 16), Block::STONE).await?;
        }
        step(bot, 1).await?;
        log.record(
            format!("observer.{tick}"),
            [
                block(bot, BlockPos::new(16, -59, 16))?.id(),
                block(bot, BlockPos::new(17, -59, 16))?.id(),
            ],
        )?;
    }
    command(
        bot,
        "setblock 20 -59 16 minecraft:observer[facing=east]",
        "commands.setblock.success",
    )
    .await?;
    command(
        bot,
        "setblock 21 -59 16 minecraft:observer[facing=west]",
        "commands.setblock.success",
    )
    .await?;
    set(bot, BlockPos::new(19, -59, 16), Block::REDSTONE_LAMP).await?;
    set(bot, BlockPos::new(22, -59, 16), Block::REDSTONE_LAMP).await?;
    let mut powered = [0_u8; 2];
    for tick in 1..=24 {
        step(bot, 1).await?;
        let a = block(bot, BlockPos::new(20, -59, 16))?;
        let b = block(bot, BlockPos::new(21, -59, 16))?;
        powered[0] += u8::from(a.get(p::POWERED) == Some(true));
        powered[1] += u8::from(b.get(p::POWERED) == Some(true));
        log.record(
            format!("observer-clock.{tick}"),
            [
                a.id(),
                b.id(),
                block(bot, BlockPos::new(19, -59, 16))?.id(),
                block(bot, BlockPos::new(22, -59, 16))?.id(),
            ],
        )?;
    }
    ensure!(
        powered.into_iter().all(|ticks| ticks >= 2),
        "both observers must pulse in the clock"
    );
    record_wire_shapes(bot, &mut log).await?;
    record_diodes(bot, &mut log).await?;
    record_comparators(bot, &mut log).await?;
    record_bulbs_and_rods(bot, &mut log).await?;
    record_torch_burnout(bot, &mut log).await?;
    Ok(log)
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires dedicated builder vanilla/JDK and integrated live block phases"]
async fn parity_redstone_wire_repeater_lamp_and_observer_timelines() -> Result<()> {
    run(redstone_support::options(), RunMode::Both, record).await
}

async fn record_wire_shapes(
    bot: &mut ferraloom_parity::Bot,
    log: &mut ObservationLog,
) -> Result<()> {
    use ferraloom_util::Direction as D;
    use redstone_support::{click, hand, sample, source, timeline};
    let wire = BlockPos::new(34, -58, 6);
    // A wire on a conductor climbs one block and descends into a branch.
    set(bot, wire.below(1), Block::STONE).await?;
    set(bot, wire.east(1), Block::STONE).await?;
    for pos in [
        wire,
        wire.east(1).above(1),
        wire.east(2).above(1),
        wire.south(1).below(1),
    ] {
        set(bot, pos.below(1), Block::STONE).await?;
        set(bot, pos, Block::REDSTONE_WIRE).await?;
    }
    let positions = [
        wire,
        wire.east(1).above(1),
        wire.east(2).above(1),
        wire.south(1).below(1),
    ];
    for (phase, powered) in [("on", true), ("off", false)] {
        source(bot, wire.west(1), powered).await?;
        timeline(bot, log, &format!("wire.slope.{phase}"), &positions, 2).await?;
    }
    let dot = BlockPos::new(34, -58, 12);
    source(bot, dot.below(1), true).await?;
    set(bot, dot, Block::REDSTONE_WIRE).await?;
    let mut dot_positions = vec![dot];
    for direction in [D::North, D::South, D::East, D::West, D::Up] {
        let lamp = dot.relative(direction, 1);
        set(bot, lamp, Block::REDSTONE_LAMP).await?;
        dot_positions.push(lamp);
    }
    hand(bot, ferraloom_data::Item::AIR).await?;
    sample(bot, log, "wire.cross", &dot_positions)?;
    for toggle in 0..2 {
        let before = block(bot, dot)?;
        click(bot, dot, D::Up).await?;
        ensure!(
            block(bot, dot)? != before,
            "dot/cross click did not change the wire"
        );
        timeline(
            bot,
            log,
            &format!("wire.toggle.{toggle}"),
            &dot_positions,
            5,
        )
        .await?;
    }
    // Weak lamp power does not relay through stone. A repeater's direct output
    // does, and wire powers its supporting conductor into the lamp below.
    let relay = BlockPos::new(34, -58, 18);
    set(bot, relay.below(1), Block::STONE).await?;
    redstone_support::state(bot, relay, "minecraft:repeater[facing=west]").await?;
    set(bot, relay.east(1), Block::STONE).await?;
    set(bot, relay.east(2), Block::REDSTONE_LAMP).await?;
    source(bot, relay.west(1), true).await?;
    timeline(bot, log, "power.direct-relay", &[relay, relay.east(2)], 4).await?;
    let weak = BlockPos::new(34, -57, 23);
    set(bot, weak.below(1), Block::STONE).await?;
    set(bot, weak.below(2), Block::REDSTONE_LAMP).await?;
    set(bot, weak, Block::REDSTONE_WIRE).await?;
    source(bot, weak.west(1), true).await?;
    timeline(bot, log, "power.support-relay", &[weak, weak.below(2)], 4).await
}

async fn record_diodes(bot: &mut ferraloom_parity::Bot, log: &mut ObservationLog) -> Result<()> {
    use redstone_support::{source, state, timeline};
    for delay in 1..=4 {
        let pos = BlockPos::new(44, -59, 6 + delay * 6);
        state(
            bot,
            pos,
            &format!("minecraft:repeater[facing=west,delay={delay}]"),
        )
        .await?;
        set(bot, pos.east(1), Block::REDSTONE_LAMP).await?;
        // The side diode locks an initially off repeater before input rises.
        state(bot, pos.north(1), "minecraft:repeater[facing=north]").await?;
        source(bot, pos.north(2), true).await?;
        step(bot, 3).await?;
        ensure!(
            block(bot, pos)?.get(p::LOCKED) == Some(true),
            "side diode did not lock"
        );
        source(bot, pos.west(1), true).await?;
        timeline(
            bot,
            log,
            &format!("diode.{delay}.locked"),
            &[pos, pos.east(1), pos.north(1)],
            10,
        )
        .await?;
        ensure!(
            block(bot, pos)?.get(p::POWERED) == Some(false),
            "locked repeater changed output"
        );
        source(bot, pos.north(2), false).await?;
        timeline(
            bot,
            log,
            &format!("diode.{delay}.unlock"),
            &[pos, pos.east(1), pos.north(1)],
            12,
        )
        .await?;
        ensure!(block(bot, pos)?.get(p::POWERED) == Some(true));
        source(bot, pos.west(1), false).await?;
        timeline(
            bot,
            log,
            &format!("diode.{delay}.fall"),
            &[pos, pos.east(1)],
            12,
        )
        .await?;
        // Two edges before the first pending tick must not replace that tick.
        source(bot, pos.west(1), true).await?;
        source(bot, pos.west(1), false).await?;
        timeline(
            bot,
            log,
            &format!("diode.{delay}.short"),
            &[pos, pos.east(1)],
            12,
        )
        .await?;
    }
    Ok(())
}
fn filled_slots(slots: i32) -> String {
    (0..slots)
        .map(|slot| format!("{{Slot:{slot}b,id:\"minecraft:stone\",count:64}}"))
        .collect::<Vec<_>>()
        .join(",")
}
async fn record_comparators(
    bot: &mut ferraloom_parity::Bot,
    log: &mut ObservationLog,
) -> Result<()> {
    use redstone_support::{data, state, timeline};
    for (case, (main, side)) in [(24, 8), (8, 24), (8, 8), (0, 0)].into_iter().enumerate() {
        for (mode_index, mode) in ["compare", "subtract"].into_iter().enumerate() {
            let pos = BlockPos::new(7 + case as i32 * 7, -59, 34 + mode_index as i32 * 7);
            state(
                bot,
                pos,
                &format!("minecraft:comparator[facing=west,mode={mode}]"),
            )
            .await?;
            state(bot, pos.north(1), "minecraft:comparator[facing=north]").await?;
            state(
                bot,
                pos.west(1),
                &format!("minecraft:barrel{{Items:[{}]}}", filled_slots(main)),
            )
            .await?;
            state(
                bot,
                pos.north(2),
                &format!("minecraft:barrel{{Items:[{}]}}", filled_slots(side)),
            )
            .await?;
            for x in 1..=3 {
                set(bot, pos.east(x), Block::REDSTONE_WIRE).await?;
            }
            timeline(
                bot,
                log,
                &format!("comparator.{case}.{mode}"),
                &[pos, pos.north(1), pos.east(1), pos.east(2), pos.east(3)],
                6,
            )
            .await?;
            let output = data(bot, pos, "OutputSignal").await?;
            let main = if main == 0 { 0 } else { 1 + main * 14 / 27 };
            let side = if side == 0 { 0 } else { 1 + side * 14 / 27 };
            let expected = if mode == "subtract" {
                (main - side).max(0)
            } else if main >= side {
                main
            } else {
                0
            };
            ensure!(
                output == ferraloom_nbt::Tag::Int(expected),
                "wrong {mode} comparator output: {output:?}"
            );
            log.record(format!("comparator.{case}.{mode}.output"), expected)?;
        }
    }
    for (case, output) in [-7, 0, 5, 23].into_iter().enumerate() {
        let pos = BlockPos::new(7 + case as i32 * 7, -59, 50);
        state(bot, pos, "minecraft:comparator[facing=west]").await?;
        step(bot, 3).await?;
        if output == 0 {
            redstone_support::merge(bot, pos, "{OutputSignal:1}").await?;
        }
        redstone_support::merge(bot, pos, &format!("{{OutputSignal:{output}}}")).await?;
        set(bot, pos.east(1), Block::STONE).await?;
        set(bot, pos.east(2), Block::REDSTONE_LAMP).await?;
        let saved = data(bot, pos, "OutputSignal").await?;
        ensure!(saved == ferraloom_nbt::Tag::Int(output));
        log.record(
            format!("comparator.stored.{case}"),
            ferraloom_nbt::to_text(&saved, "", false),
        )?;
        redstone_support::sample(
            bot,
            log,
            format!("comparator.stored.{case}.relay"),
            &[pos, pos.east(2)],
        )?;
    }
    Ok(())
}
async fn record_bulbs_and_rods(
    bot: &mut ferraloom_parity::Bot,
    log: &mut ObservationLog,
) -> Result<()> {
    use redstone_support::{source, state, timeline};
    let bulb = BlockPos::new(40, -59, 40);
    set(bot, bulb, Block::COPPER_BULB).await?;
    state(bot, bulb.east(1), "minecraft:comparator[facing=west]").await?;
    set(bot, bulb.east(2), Block::REDSTONE_WIRE).await?;
    for edge in 0..6 {
        source(bot, bulb.west(1), edge % 2 == 0).await?;
        timeline(
            bot,
            log,
            &format!("bulb.{edge}"),
            &[bulb, bulb.east(1), bulb.east(2)],
            4,
        )
        .await?;
        ensure!(block(bot, bulb)?.get(p::LIT) == Some(matches!(edge, 0 | 1 | 4 | 5)));
    }
    for (case, rod) in [
        "lightning_rod",
        "exposed_lightning_rod",
        "weathered_lightning_rod",
        "oxidized_lightning_rod",
        "waxed_lightning_rod",
        "waxed_exposed_lightning_rod",
        "waxed_weathered_lightning_rod",
        "waxed_oxidized_lightning_rod",
    ]
    .into_iter()
    .enumerate()
    {
        for facing in ["down", "up", "north", "south", "west", "east"] {
            let pos = BlockPos::new(50, -55, 48);
            // Restore lamp states between fixtures; the center rod is always a
            // new placement and must schedule its own eight-tick reset.
            redstone_support::clear(bot, BlockPos::new(48, -57, 46), BlockPos::new(52, -53, 50))
                .await?;
            let mut positions = vec![pos];
            for (dx, dy, dz) in [
                (0, -1, 0),
                (0, 1, 0),
                (0, 0, -1),
                (0, 0, 1),
                (-1, 0, 0),
                (1, 0, 0),
            ] {
                let conductor = BlockPos::new(pos.x + dx, pos.y + dy, pos.z + dz);
                let lamp = BlockPos::new(pos.x + dx * 2, pos.y + dy * 2, pos.z + dz * 2);
                set(bot, conductor, Block::STONE).await?;
                set(bot, lamp, Block::REDSTONE_LAMP).await?;
                positions.push(lamp);
            }
            state(
                bot,
                pos,
                &format!("minecraft:{rod}[facing={facing},powered=true]"),
            )
            .await?;
            timeline(bot, log, &format!("rod.{case}.{facing}"), &positions, 12).await?;
            ensure!(block(bot, pos)?.get(p::POWERED) == Some(false));
        }
    }
    Ok(())
}
